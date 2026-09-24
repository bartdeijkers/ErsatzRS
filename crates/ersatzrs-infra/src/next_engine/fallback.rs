//! Session-owned filler for upstream `DynamicPlayoutItemService` gap callbacks.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{ChannelRepository as _, RepositoryError, Result};
use ersatzrs_domain::{Channel, PlayoutItem, StreamingEngine};
use tokio::time::{timeout_at, Instant};
use uuid::Uuid;

use super::{
    dynamic_item, dynamic_projection, host::NextHost, protocol, schedule, selection, source,
};
use crate::{fallback_playback, playback, repositories::make_repositories};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Gap {
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
}

impl Gap {
    fn id(self) -> String {
        format!(
            "fallback-{}-{}-{}-{}",
            self.start.timestamp(),
            self.start.timestamp_subsec_nanos(),
            self.finish.timestamp(),
            self.finish.timestamp_subsec_nanos()
        )
    }
}

// Match the persisted schedule's 100-nanosecond precision in callback bounds.
fn timestamp_ticks(time: DateTime<Utc>) -> Result<i64> {
    if !time.timestamp_subsec_nanos().is_multiple_of(100) {
        return Err(unavailable());
    }
    time.timestamp()
        .checked_mul(10_000_000)
        .and_then(|seconds| seconds.checked_add(i64::from(time.timestamp_subsec_nanos() / 100)))
        .ok_or_else(unavailable)
}

fn from_timestamp_ticks(ticks: i64) -> Result<DateTime<Utc>> {
    let nanos = u32::try_from(ticks.rem_euclid(10_000_000) * 100).map_err(|_| unavailable())?;
    DateTime::from_timestamp(ticks.div_euclid(10_000_000), nanos).ok_or_else(unavailable)
}

pub(super) struct Occurrence {
    gap: Gap,
    item: PlayoutItem,
    audio: Option<usize>,
    descriptor: String,
    concrete: Option<protocol::PlayoutItem>,
}

struct Projection {
    snapshot: dynamic_projection::Snapshot,
    credits: Option<String>,
    remote: bool,
}

/// Fill only internal gaps, retaining overlapping programmes and outer bounds.
pub(super) fn insert_gaps(
    relay: &super::media::MediaRelay,
    channel_id: i64,
    mut items: Vec<protocol::PlayoutItem>,
) -> Result<Vec<protocol::PlayoutItem>> {
    items.sort_by_key(|item| item.start);
    let mut result = Vec::with_capacity(items.len());
    let mut last_finish = None;
    for item in items {
        if let Some(finish) = last_finish {
            if finish < item.start {
                let gap = Gap {
                    start: finish,
                    finish: item.start,
                };
                result.push(protocol::PlayoutItem {
                    id: gap.id(),
                    start: gap.start,
                    finish: gap.finish,
                    source: Some(relay.fallback_source(
                        channel_id,
                        timestamp_ticks(gap.start)?,
                        timestamp_ticks(gap.finish)?,
                    )?),
                    tracks: None,
                    watermark: None,
                });
            }
        }
        last_finish = Some(last_finish.map_or(item.finish, |finish| finish.max(item.finish)));
        result.push(item);
    }
    Ok(result)
}

impl NextHost {
    /// Resolve a gap only for its admitted session and current persisted bounds.
    /// Authentication is enforced by the HTTP transport before this call.
    /// # Errors
    /// Refuses changed gaps, expired sessions, unavailable media and preparation failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_fallback(
        &self,
        channel_id: i64,
        start: i64,
        finish: i64,
        number: &str,
        instance: Uuid,
        dynamic_id: &str,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<protocol::PlayoutItem> {
        let gap = Gap {
            start: from_timestamp_ticks(start)?,
            finish: from_timestamp_ticks(finish)?,
        };
        if gap.id() != dynamic_id || now < gap.start || now >= gap.finish || until != gap.finish {
            return Err(unavailable());
        }
        let (request, state) = self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .filter(|context| {
                context.request.channel_id == channel_id && context.request.number == number
            })
            .map(|context| (context.request.clone(), context.fallback.clone()))
            .ok_or_else(unavailable)?;
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        timeout_at(
            deadline,
            Box::pin(async {
                let mut state = state.lock().await;
                let channel = make_repositories(self.pool.clone())
                    .channel
                    .get_with_profile(channel_id)
                    .await?;
                if channel.streaming_engine != StreamingEngine::Next {
                    return Err(unavailable());
                }
                let locked = self.validate_gap(&channel, gap).await?;
                let candidates =
                    fallback_playback::load_candidates(&self.pool, &channel, now, gap.finish)
                        .await?
                        .ok_or_else(unavailable)?;
                if let Some(current) = state.as_ref().filter(|current| {
                    current.gap == gap && now >= current.item.start && now < current.item.finish
                }) {
                    if !candidate_matches(&candidates, &current.item) {
                        return Err(unavailable());
                    }
                    drop(locked);
                    let resolved = Box::pin(self.resolve_fallback_occurrence(
                        &channel, current, &request, instance, now, deadline,
                    ))
                    .await?;
                    return Ok(resolved);
                }
                let seed = usize::try_from(ersatzrs_core::scheduling::next_playout_seed())
                    .map_err(|_| unavailable())?;
                let item = candidates
                    .materialize(seed % candidates.items.len())
                    .ok_or_else(unavailable)?;
                let projected = self
                    .project_fallback(
                        &channel,
                        &item,
                        &request,
                        selection::AudioDirective::Select,
                        gap,
                    )
                    .await?;
                let audio = selected_audio(&projected.snapshot.projection.item)?;
                let descriptor = projected.snapshot.identity.descriptor.clone();
                let remote = projected.remote;
                drop(locked);
                let concrete = Box::pin(self.prepare_fallback(
                    projected, &channel, &item, &request, gap, instance, audio, now, deadline,
                ))
                .await?;
                *state = Some(Occurrence {
                    gap,
                    item,
                    audio,
                    descriptor,
                    concrete: (!remote).then(|| concrete.clone()),
                });
                Ok(concrete)
            }),
        )
        .await
        .map_err(|_| unavailable())?
    }

    async fn resolve_fallback_occurrence(
        &self,
        channel: &Channel,
        current: &Occurrence,
        request: &super::host::NextStart,
        instance: Uuid,
        now: DateTime<Utc>,
        deadline: Instant,
    ) -> Result<protocol::PlayoutItem> {
        let projected = self
            .project_fallback(
                channel,
                &current.item,
                request,
                selection::AudioDirective::Published(current.audio),
                current.gap,
            )
            .await?;
        if projected.snapshot.identity.descriptor != current.descriptor
            || !self.dynamic_contexts.lock().await.contains_key(&instance)
        {
            return Err(unavailable());
        }
        if let Some(concrete) = &current.concrete {
            if !self
                .fallback_unchanged(
                    channel,
                    &current.item,
                    request,
                    current.gap,
                    instance,
                    current.audio,
                    &current.descriptor,
                )
                .await
            {
                return Err(unavailable());
            }
            return dynamic_item::resolve_at(concrete, now).map_err(|_| unavailable());
        }
        Box::pin(self.prepare_fallback(
            projected,
            channel,
            &current.item,
            request,
            current.gap,
            instance,
            current.audio,
            now,
            deadline,
        ))
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_fallback(
        &self,
        projected: Projection,
        channel: &Channel,
        item: &PlayoutItem,
        request: &super::host::NextStart,
        gap: Gap,
        instance: Uuid,
        audio: Option<usize>,
        now: DateTime<Utc>,
        deadline: Instant,
    ) -> Result<protocol::PlayoutItem> {
        let mut snapshot = super::remote_projection::materialize_prepared(
            &self.pool,
            &self.locker,
            projected.snapshot,
            audio,
            &crate::file_system_layout::addon_processed_media_folder(),
        )
        .await?;
        super::live_subtitles::prepare(&self.burn_cache, &mut snapshot, now, deadline).await?;
        if !Box::pin(self.fallback_preparation_unchanged(
            channel,
            item,
            request,
            gap,
            instance,
            audio,
            &snapshot.identity,
        ))
        .await
        {
            return Err(unavailable());
        }
        if let Some(context) = snapshot.retained_remote.take() {
            return Box::pin(
                self.resolve_retained(snapshot, context, request, instance, audio, now, deadline),
            )
            .await;
        }
        if let Some(context) = snapshot.producer_remote.take() {
            return Box::pin(
                self.resolve_producer(snapshot, context, request, instance, audio, now, deadline),
            )
            .await;
        }
        Box::pin(self.prepare_fallback_tracks(
            &mut snapshot.projection,
            &snapshot.identity,
            projected.credits,
            channel,
            item,
            request,
            gap,
            instance,
            audio,
            deadline,
        ))
        .await?;
        if !Box::pin(self.fallback_preparation_unchanged(
            channel,
            item,
            request,
            gap,
            instance,
            audio,
            &snapshot.identity,
        ))
        .await
        {
            return Err(unavailable());
        }
        dynamic_item::resolve_at(&snapshot.projection.item, now).map_err(|_| unavailable())
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_fallback_tracks(
        &self,
        projected: &mut selection::DeferredProjection,
        identity: &dynamic_projection::Identity,
        credits: Option<String>,
        channel: &Channel,
        item: &PlayoutItem,
        request: &super::host::NextStart,
        gap: Gap,
        instance: Uuid,
        audio: Option<usize>,
        deadline: Instant,
    ) -> Result<()> {
        if let Some(content) = credits {
            let key = dynamic_projection::digest(&serde_json::json!({"credits": content}))?;
            let path = self
                .burn_cache
                .prepare_text(&key, "ass", &content, item.finish, deadline, || async {
                    Box::pin(self.fallback_preparation_unchanged(
                        channel, item, request, gap, instance, audio, identity,
                    ))
                    .await
                })
                .await
                .map_err(|_| unavailable())?;
            projected
                .item
                .tracks
                .as_mut()
                .ok_or_else(unavailable)?
                .subtitle = Some(protocol::Track {
                source: Some(protocol::Source::Local {
                    path: path.to_str().ok_or_else(unavailable)?.to_owned(),
                    in_point_ms: Some(
                        u64::try_from(item.in_point.num_milliseconds())
                            .map_err(|_| unavailable())?,
                    ),
                    out_point_ms: Some(
                        u64::try_from(item.out_point.num_milliseconds())
                            .map_err(|_| unavailable())?,
                    ),
                }),
                stream_index: Some(0),
            });
        }
        if let Some(pending) = projected.pending_burn.as_ref() {
            let path = self
                .burn_cache
                .prepare(
                    &identity.cache_key,
                    &pending.source,
                    u32::try_from(pending.subtitle.stream_index).map_err(|_| unavailable())?,
                    pending.subtitle.codec.as_deref().unwrap_or("subrip"),
                    &request.ffmpeg,
                    item.finish,
                    deadline - std::time::Duration::from_secs(6),
                    || async {
                        Box::pin(self.fallback_preparation_unchanged(
                            channel, item, request, gap, instance, audio, identity,
                        ))
                        .await
                    },
                )
                .await
                .map_err(|_| unavailable())?;
            projected.item =
                super::dynamic_host::install_prepared_track(projected.item.clone(), &path)?;
        }
        Ok(())
    }

    async fn validate_gap(&self, channel: &Channel, gap: Gap) -> Result<schedule::LockedSchedule> {
        validate_gap(&self.pool, &self.locker, channel, gap).await
    }

    async fn project_fallback(
        &self,
        channel: &Channel,
        item: &PlayoutItem,
        request: &super::host::NextStart,
        audio: selection::AudioDirective,
        gap: Gap,
    ) -> Result<Projection> {
        let media = schedule::bind_media(&self.pool, item.clone()).await?;
        let profile = playback::load_channel_playback_profile(&self.pool, channel.id)
            .await?
            .ok_or_else(unavailable)?;
        if media.remote_stream_id.is_some() {
            return Ok(Projection {
                snapshot: super::remote_projection::project_publishable_for_origin(
                    &self.pool,
                    channel,
                    &media,
                    &profile.resolution,
                    audio,
                    super::remote_projection::Origin::Fallback(gap),
                )
                .await?,
                credits: None,
                remote: true,
            });
        }
        let source =
            source::resolve_published(&self.pool, &media, &request.replacements, Some(&self.media))
                .await?;
        let selection_time = crate::channel_playout::ChannelPlayoutContext::for_channel(channel)
            .source_time(item.start)?;
        let watermarks =
            playback::select_fallback_watermarks(&self.pool, channel, item, selection_time).await?;
        if !playback::select_fallback_graphics(&self.pool, item, selection_time)
            .await?
            .is_empty()
        {
            return Err(unavailable());
        }
        let mut projected = selection::project_deferred_item_with_audio(
            &self.pool,
            channel,
            item,
            media.media_version_id,
            source,
            &profile.resolution,
            audio,
        )
        .await?;
        let credits = if item.subtitle_mode.unwrap_or(channel.subtitle_mode)
            != ersatzrs_domain::ChannelSubtitleMode::None
            && item.pinned_stream_selection.is_none()
            && !(channel.stream_selector_mode == ersatzrs_domain::ChannelStreamSelectorMode::Custom
                && projected.selected_subtitle.is_some())
        {
            crate::music_video_credits::render_music_video_credits_subtitle(
                &self.pool,
                item.media_item_id,
                channel,
                &profile.resolution,
                item.in_point.num_seconds(),
            )
            .await?
        } else {
            None
        };
        if credits.is_some() {
            projected.pending_burn = None;
            projected.selected_subtitle = None;
            projected
                .item
                .tracks
                .as_mut()
                .ok_or_else(unavailable)?
                .subtitle = Some(protocol::Track {
                source: None,
                stream_index: Some(0),
            });
        }
        projected.item.watermark = super::watermark::project(&watermarks)?;
        let mut identity =
            dynamic_projection::fingerprint(&self.pool, &media, &projected, channel).await?;
        if let Some(content) = &credits {
            identity.descriptor = dynamic_projection::digest(
                &serde_json::json!({"projection": identity.descriptor, "credits": content}),
            )?;
        }
        Ok(Projection {
            snapshot: dynamic_projection::Snapshot {
                projection: projected,
                identity,
                prepared_remote: None,
                producer_remote: None,
                retained_remote: None,
            },
            credits,
            remote: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn fallback_preparation_unchanged(
        &self,
        channel: &Channel,
        item: &PlayoutItem,
        request: &super::host::NextStart,
        gap: Gap,
        instance: Uuid,
        audio: Option<usize>,
        identity: &dynamic_projection::Identity,
    ) -> bool {
        if !self
            .fallback_unchanged(
                channel,
                item,
                request,
                gap,
                instance,
                audio,
                &identity.descriptor,
            )
            .await
        {
            return false;
        }
        let Ok(projected) = self
            .project_fallback(
                channel,
                item,
                request,
                selection::AudioDirective::Published(audio),
                gap,
            )
            .await
        else {
            return false;
        };
        let Ok(current) = super::remote_projection::materialize_prepared(
            &self.pool,
            &self.locker,
            projected.snapshot,
            audio,
            &crate::file_system_layout::addon_processed_media_folder(),
        )
        .await
        else {
            return false;
        };
        current.identity.descriptor == identity.descriptor
            && current.identity.cache_key == identity.cache_key
            && self.dynamic_contexts.lock().await.contains_key(&instance)
    }

    #[allow(clippy::too_many_arguments)]
    async fn fallback_unchanged(
        &self,
        channel: &Channel,
        item: &PlayoutItem,
        request: &super::host::NextStart,
        gap: Gap,
        instance: Uuid,
        audio: Option<usize>,
        descriptor: &str,
    ) -> bool {
        let Ok(current) = make_repositories(self.pool.clone())
            .channel
            .get_with_profile(channel.id)
            .await
        else {
            return false;
        };
        let Ok(_locked) = self.validate_gap(&current, gap).await else {
            return false;
        };
        let Ok(Some(candidates)) =
            fallback_playback::load_candidates(&self.pool, &current, item.start, gap.finish).await
        else {
            return false;
        };
        candidate_matches(&candidates, item)
            && self
                .project_fallback(
                    &current,
                    item,
                    request,
                    selection::AudioDirective::Published(audio),
                    gap,
                )
                .await
                .is_ok_and(|projected| projected.snapshot.identity.descriptor == descriptor)
            && self.dynamic_contexts.lock().await.contains_key(&instance)
    }
}

async fn validate_gap(
    pool: &crate::db::DbPool,
    locker: &ersatzrs_core::locking::EntityLocker,
    channel: &Channel,
    gap: Gap,
) -> Result<schedule::LockedSchedule> {
    let start = gap
        .start
        .checked_sub_signed(Duration::microseconds(1))
        .ok_or_else(unavailable)?;
    let finish = gap
        .finish
        .checked_add_signed(Duration::microseconds(1))
        .ok_or_else(unavailable)?;
    let snapshot = schedule::read_locked(pool, locker, channel, start, finish)
        .await?
        .ok_or_else(unavailable)?;
    let before = snapshot
        .items
        .iter()
        .any(|media| media.item.finish == gap.start);
    let after = snapshot
        .items
        .iter()
        .any(|media| media.item.start == gap.finish);
    if !before
        || !after
        || snapshot
            .items
            .iter()
            .any(|media| media.item.start < gap.finish && media.item.finish > gap.start)
    {
        return Err(unavailable());
    }
    Ok(snapshot)
}

pub(super) async fn remote_snapshot(
    pool: &crate::db::DbPool,
    locker: &ersatzrs_core::locking::EntityLocker,
    channel_id: i64,
    item: &PlayoutItem,
    gap: Gap,
    audio: Option<usize>,
) -> Result<super::remote_projection::Snapshot> {
    let channel = make_repositories(pool.clone())
        .channel
        .get_with_profile(channel_id)
        .await?;
    if channel.streaming_engine != StreamingEngine::Next {
        return Err(unavailable());
    }
    let _locked = validate_gap(pool, locker, &channel, gap).await?;
    let candidates = fallback_playback::load_candidates(pool, &channel, item.start, gap.finish)
        .await?
        .ok_or_else(unavailable)?;
    if !candidate_matches(&candidates, item) {
        return Err(unavailable());
    }
    let media = schedule::bind_media(pool, item.clone()).await?;
    let profile = playback::load_channel_playback_profile(pool, channel_id)
        .await?
        .ok_or_else(unavailable)?;
    super::remote_projection::project_for_origin(
        pool,
        &channel,
        &media,
        &profile.resolution,
        selection::AudioDirective::Published(audio),
        super::remote_projection::Origin::Fallback(gap),
    )
    .await
}

fn selected_audio(item: &protocol::PlayoutItem) -> Result<Option<usize>> {
    item.tracks
        .as_ref()
        .and_then(|tracks| tracks.audio.as_ref())
        .filter(|track| !matches!(track.source, Some(protocol::Source::Lavfi { .. })))
        .and_then(|audio| audio.stream_index)
        .map(usize::try_from)
        .transpose()
        .map_err(|_| unavailable())
}

fn candidate_matches(
    candidates: &fallback_playback::FallbackCandidates,
    item: &PlayoutItem,
) -> bool {
    (0..candidates.items.len())
        .any(|index| candidates.materialize_at(index, item.start).as_ref() == Some(item))
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next fallback",
        "session, gap or selected media is unavailable",
    )
}

#[cfg(test)]
#[path = "fallback_tests.rs"]
mod tests;

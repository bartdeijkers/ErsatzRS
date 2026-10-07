//! Isolated item/window projection for upstream `PrepareTroubleshootingPlaybackHandler`.
//! Row201 `GetNextChannelProcess` retains the historical virtual clock and one
//! session containing every exact occurrence in a complete thirty-second window.
//! No persisted playout is created, edited or published by this adapter.

use std::path::Path;

use ersatzrs_core::{
    path_replacement::PathReplacement,
    repositories::{RepositoryError, Result},
};
use ersatzrs_domain::Channel;
use tokio::time::{timeout_at, Instant};

use crate::playback::{self, PlaybackProfile};

use super::{
    dynamic_host::install_prepared_track,
    dynamic_projection::{self, Identity},
    host::NextHost,
    protocol,
    schedule::ScheduledMedia,
    selection::{self, DeferredProjection, TroubleshootingSelection},
    source,
};

/// Exact transient selection supplied by the troubleshooting host.
/// Deliberately no Debug: bindings may contain private source paths.
pub struct TroubleshootingPreview<'a> {
    pub media: &'a ScheduledMedia,
    pub channel: &'a Channel,
    pub profile: &'a PlaybackProfile,
    /// None uses channel preferences; zero explicitly disables subtitles.
    pub subtitle_id: Option<i64>,
    /// Channel preview selection time; None uses the explicit media-preview overlays.
    pub selection_time: Option<chrono::DateTime<chrono::Utc>>,
    pub watermark_ids: &'a [i64],
    pub graphics_ids: &'a [i64],
    pub replacements: &'a [PathReplacement],
    pub ffmpeg: &'a Path,
    pub deadline: Instant,
    /// Opt-in bounded selection decisions; scheduled conversion stays quiet.
    pub diagnostics: Option<super::diagnostics::SelectionLog>,
}

/// Owned selection for authenticated callbacks; never holds schedule guards.
/// No Debug because source bindings can contain private provider paths.
pub(super) struct Context {
    pub media: ScheduledMedia,
    pub channel: Channel,
    pub profile: PlaybackProfile,
    pub subtitle_id: Option<i64>,
    selection_time: Option<chrono::DateTime<chrono::Utc>>,
    watermark_ids: Vec<i64>,
    graphics_ids: Vec<i64>,
    pub diagnostics: Option<super::diagnostics::SelectionLog>,
    pub(super) fallback_gap: Option<super::fallback::Gap>,
    credits: Option<protocol::Track>,
}

impl Context {
    pub(super) fn new(request: &TroubleshootingPreview<'_>) -> Self {
        Self {
            media: request.media.clone(),
            channel: request.channel.clone(),
            profile: request.profile.clone(),
            subtitle_id: request.subtitle_id,
            selection_time: request.selection_time,
            watermark_ids: request.watermark_ids.to_vec(),
            graphics_ids: request.graphics_ids.to_vec(),
            diagnostics: request.diagnostics.clone(),
            fallback_gap: None,
            credits: None,
        }
    }

    pub(super) fn attach_credits(&self, projection: &mut DeferredProjection) -> Result<()> {
        let Some(credits) = &self.credits else {
            return Ok(());
        };
        if self.subtitle_id.is_some()
            || !super::credits::eligible(
                &self.channel,
                &self.media.item,
                projection.selected_subtitle.is_some(),
            )
        {
            return Ok(());
        }
        let tracks = projection
            .item
            .tracks
            .as_mut()
            .ok_or_else(|| unavailable("credits tracks are unavailable"))?;
        tracks.subtitle = Some(credits.clone());
        projection.pending_burn = None;
        projection.selected_subtitle = None;
        Ok(())
    }

    pub(super) fn credits_at(
        &self,
        projection: &DeferredProjection,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<protocol::Track>> {
        if self.credits.is_none()
            || self.subtitle_id.is_some()
            || !super::credits::eligible(
                &self.channel,
                &self.media.item,
                projection.selected_subtitle.is_some(),
            )
        {
            return Ok(None);
        }
        let Some(installed) = projection
            .item
            .tracks
            .as_ref()
            .and_then(|tracks| tracks.subtitle.as_ref())
        else {
            return Ok(None);
        };
        let expected = self
            .credits
            .as_ref()
            .ok_or_else(|| unavailable("credits ownership is unavailable"))?;
        if serde_json::to_vec(installed).map_err(|_| unavailable("credits ownership is invalid"))?
            != serde_json::to_vec(expected)
                .map_err(|_| unavailable("credits ownership is invalid"))?
        {
            return Ok(None);
        }
        let timed = super::dynamic_item::resolve_at(&projection.item, now)
            .map_err(|_| unavailable("credits timing is invalid"))?;
        Ok(timed.tracks.and_then(|tracks| tracks.subtitle))
    }

    pub(super) async fn graphics(
        &self,
        pool: &crate::db::DbPool,
    ) -> Result<Vec<playback::ResolvedPlaybackGraphicsElement>> {
        self.graphics_at(pool, chrono::TimeDelta::zero()).await
    }

    pub(super) async fn graphics_at(
        &self,
        pool: &crate::db::DbPool,
        offset: chrono::TimeDelta,
    ) -> Result<Vec<playback::ResolvedPlaybackGraphicsElement>> {
        if let Some(source_time) = self.source_time(offset)? {
            if self.fallback_gap.is_some() {
                playback::select_fallback_graphics(pool, &self.media.item, source_time).await
            } else {
                playback::select_playback_graphics(
                    pool,
                    &self.channel,
                    self.media.item.id,
                    source_time,
                )
                .await
            }
        } else {
            playback::resolve_playback_graphics_by_ids(pool, self.graphics_ids.clone()).await
        }
    }

    pub(super) async fn watermarks(
        &self,
        pool: &crate::db::DbPool,
    ) -> Result<Vec<playback::ResolvedPlaybackWatermark>> {
        self.watermarks_at(pool, chrono::TimeDelta::zero()).await
    }

    pub(super) async fn watermarks_at(
        &self,
        pool: &crate::db::DbPool,
        offset: chrono::TimeDelta,
    ) -> Result<Vec<playback::ResolvedPlaybackWatermark>> {
        if let Some(source_time) = self.source_time(offset)? {
            if self.fallback_gap.is_some() {
                return playback::select_fallback_watermarks(
                    pool,
                    &self.channel,
                    &self.media.item,
                    source_time,
                )
                .await;
            }
            playback::select_playback_watermarks(
                pool,
                &self.channel,
                self.media.item.id,
                self.media.item.disable_watermarks,
                source_time,
            )
            .await
        } else {
            playback::resolve_playback_watermarks_by_ids(
                pool,
                self.channel.id,
                self.watermark_ids.clone(),
            )
            .await
        }
    }

    fn source_time(
        &self,
        offset: chrono::TimeDelta,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
        self.selection_time
            .map(|at| {
                let at = at
                    .checked_add_signed(offset)
                    .ok_or_else(|| unavailable("preview decoration clock overflow"))?;
                crate::channel_playout::ChannelPlayoutContext::for_channel(&self.channel)
                    .source_time(at)
            })
            .transpose()
    }
}

/// One process's immutable preview policy and exact occurrence membership.
/// No Debug: individual source bindings and configuration are private.
pub(super) struct Session {
    pub channel: Channel,
    pub profile: PlaybackProfile,
    pub occurrences: Vec<std::sync::Arc<Context>>,
    pub window: Option<super::fallback::Gap>,
}

impl Session {
    pub(super) fn single(context: std::sync::Arc<Context>) -> Self {
        Self {
            channel: context.channel.clone(),
            profile: context.profile.clone(),
            occurrences: vec![context],
            window: None,
        }
    }

    pub(super) fn occurrence(
        &self,
        item: i64,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<std::sync::Arc<Context>> {
        let mut matching = self.occurrences.iter().filter(|context| {
            context.media.item.id == item
                && context.media.item.start <= at
                && context.media.item.finish > at
        });
        let context = matching
            .next()
            .cloned()
            .ok_or_else(|| unavailable("preview occurrence is unavailable"))?;
        if matching.next().is_some() {
            return Err(unavailable("preview occurrence is ambiguous"));
        }
        Ok(context)
    }

    pub(super) fn fallback(
        &self,
        media: ScheduledMedia,
        gap: super::fallback::Gap,
    ) -> Result<std::sync::Arc<Context>> {
        let window = self
            .window
            .ok_or_else(|| unavailable("preview window is unavailable"))?;
        if gap.start < window.start
            || gap.finish > window.finish
            || media.item.start < gap.start
            || media.item.finish > gap.finish
        {
            return Err(unavailable("preview fallback is outside its window"));
        }
        Ok(std::sync::Arc::new(Context {
            selection_time: Some(media.item.start),
            media,
            channel: self.channel.clone(),
            profile: self.profile.clone(),
            subtitle_id: None,
            watermark_ids: Vec::new(),
            graphics_ids: Vec::new(),
            diagnostics: None,
            fallback_gap: Some(gap),
            credits: None,
        }))
    }
}

/// One exact occurrence with its transient channel/track policy.
pub struct TroubleshootingChannelOccurrence {
    pub media: ScheduledMedia,
    pub channel: Channel,
    pub subtitle_id: Option<i64>,
    /// Optional owned generated credits; exact caption eligibility is checked during projection.
    pub credits: Option<protocol::Track>,
}

/// Check basic caption ownership before staging optional generated credits.
/// A Custom selector's actual selected caption is checked during immutable projection.
#[must_use]
pub fn channel_credits_eligible(
    channel: &Channel,
    item: &ersatzrs_domain::PlayoutItem,
    subtitle_id: Option<i64>,
) -> bool {
    subtitle_id.is_none() && super::credits::eligible(channel, item, false)
}

/// Exact historical channel window prepared under one existing preview admission.
/// No Debug: includes private media/provider bindings.
pub struct TroubleshootingChannelWindow<'a> {
    pub channel: &'a Channel,
    pub profile: &'a PlaybackProfile,
    pub occurrences: &'a [TroubleshootingChannelOccurrence],
    pub start: chrono::DateTime<chrono::Utc>,
    pub finish: chrono::DateTime<chrono::Utc>,
    pub replacements: &'a [PathReplacement],
    pub ffmpeg: &'a Path,
    pub deadline: Instant,
    pub diagnostics: Option<super::diagnostics::SelectionLog>,
}

/// Historical tickets remain live for two hours without weakening session checks.
pub(super) fn callback_expiry(
    finish: chrono::DateTime<chrono::Utc>,
) -> Result<chrono::DateTime<chrono::Utc>> {
    finish
        .max(chrono::Utc::now())
        .checked_add_signed(chrono::TimeDelta::hours(2))
        .ok_or_else(|| unavailable("callback expiry overflow"))
}

struct Snapshot {
    projection: DeferredProjection,
    identity: Identity,
}

impl NextHost {
    pub(super) async fn preview_session(
        &self,
        instance: uuid::Uuid,
    ) -> Option<std::sync::Arc<Session>> {
        self.dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .and_then(|context| context.troubleshooting.clone())
    }

    /// Prepare one transient occurrence and admit its exact process
    /// instance. The caller must revoke the session after stopping that process,
    /// including on startup failure, before releasing output or transcode capacity.
    /// # Errors
    /// Rejects mismatched session identity or unavailable source/track selections.
    pub async fn prepare_troubleshooting_session(
        &self,
        request: TroubleshootingPreview<'_>,
        start: super::host::NextStart,
    ) -> Result<(uuid::Uuid, protocol::PlayoutItem)> {
        if Instant::now() >= request.deadline {
            return Err(unavailable("preparation deadline expired"));
        }
        if start.channel_id != request.channel.id || start.number != request.channel.number {
            return Err(unavailable("preview session identity differs"));
        }
        let context = std::sync::Arc::new(Context::new(&request));
        let instance = uuid::Uuid::new_v4();
        let item = self.prepare_context(&request, &context, instance).await?;
        self.dynamic_contexts.lock().await.insert(
            instance,
            super::host::DynamicContext {
                started_at: chrono::Utc::now(),
                request: start,
                troubleshooting: Some(std::sync::Arc::new(Session::single(context))),
                transfers: std::sync::Arc::default(),
                fallback: std::sync::Arc::default(),
                gaps: std::sync::Arc::default(),
            },
        );
        Ok((instance, item))
    }

    async fn prepare_context(
        &self,
        request: &TroubleshootingPreview<'_>,
        context: &std::sync::Arc<Context>,
        instance: uuid::Uuid,
    ) -> Result<protocol::PlayoutItem> {
        Ok(if request.media.remote_stream_id.is_none() {
            let mut projection = timeout_at(
                request.deadline,
                self.prepare_troubleshooting_context(request, context),
            )
            .await
            .map_err(|_| unavailable("preparation deadline expired"))??;
            projection.publish_graphics(&self.media, Some(instance), request.media.item.start)?;
            projection.item
        } else {
            let mut before = timeout_at(
                request.deadline,
                super::remote_projection::project_publishable_for_origin(
                    &self.pool,
                    &context.channel,
                    &context.media,
                    &context.profile.resolution,
                    selection::AudioDirective::Select,
                    super::remote_projection::Origin::Troubleshooting(context.clone()),
                ),
            )
            .await
            .map_err(|_| unavailable("preparation deadline expired"))??;
            before.projection.publish_graphics(
                &self.media,
                Some(instance),
                request.media.item.start,
            )?;
            self.defer_item(
                context.channel.id,
                context.media.item.id,
                &before.identity.descriptor,
                before.projection.item,
            )?
        })
    }

    /// Prepare all intersecting occurrences and bounded gaps without writing SQL.
    /// The caller revokes the one returned session after its producer is reaped.
    /// # Errors
    /// Refuses invalid windows, stale selected bindings or preparation timeout.
    pub async fn prepare_troubleshooting_window(
        &self,
        window: TroubleshootingChannelWindow<'_>,
        start: super::host::NextStart,
    ) -> Result<(uuid::Uuid, protocol::Playout)> {
        if window.finish - window.start != chrono::TimeDelta::seconds(30)
            || start.channel_id != window.channel.id
            || start.number != window.channel.number
        {
            return Err(unavailable("invalid preview window identity"));
        }
        if Instant::now() >= window.deadline {
            return Err(unavailable("preparation deadline expired"));
        }
        let instance = uuid::Uuid::new_v4();
        let mut contexts = Vec::with_capacity(window.occurrences.len());
        let mut items = Vec::with_capacity(window.occurrences.len());
        for occurrence in window.occurrences {
            let media = &occurrence.media;
            if media.item.start >= window.finish
                || media.item.finish <= window.start
                || media.item.finish > window.finish
            {
                return Err(unavailable("preview occurrence is outside its window"));
            }
            let request = TroubleshootingPreview {
                media,
                channel: &occurrence.channel,
                profile: window.profile,
                subtitle_id: occurrence.subtitle_id,
                selection_time: Some(media.item.start),
                watermark_ids: &[],
                graphics_ids: &[],
                replacements: window.replacements,
                ffmpeg: window.ffmpeg,
                deadline: window.deadline,
                diagnostics: window.diagnostics.clone(),
            };
            let mut context = Context::new(&request);
            context.credits = occurrence.credits.clone();
            let context = std::sync::Arc::new(context);
            items.push(self.prepare_context(&request, &context, instance).await?);
            contexts.push(context);
        }
        if Instant::now() >= window.deadline {
            return Err(unavailable("preparation deadline expired"));
        }
        let (items, gaps) = super::fallback::insert_bounded_gaps(
            &self.media,
            window.channel.id,
            window.start,
            window.finish,
            items,
        )?;
        let context = Session {
            channel: window.channel.clone(),
            profile: window.profile.clone(),
            occurrences: contexts,
            window: Some(super::fallback::Gap {
                start: window.start,
                finish: window.finish,
            }),
        };
        self.dynamic_contexts.lock().await.insert(
            instance,
            super::host::DynamicContext {
                started_at: chrono::Utc::now(),
                request: start,
                troubleshooting: Some(std::sync::Arc::new(context)),
                transfers: std::sync::Arc::default(),
                fallback: std::sync::Arc::default(),
                gaps: std::sync::Arc::new(tokio::sync::Mutex::new(gaps)),
            },
        );
        Ok((
            instance,
            protocol::Playout {
                version: protocol::PLAYOUT_VERSION.to_owned(),
                items,
            },
        ))
    }

    /// Read every intersecting occurrence on the requested channel clock.
    /// Build locks are released before any provider or process preparation.
    /// # Errors
    /// Returns schedule read failures, invalid bounds or a busy source schedule.
    pub async fn troubleshooting_channel_window(
        &self,
        channel: &Channel,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<ScheduledMedia>> {
        let at = chrono::DateTime::from_timestamp(at.timestamp(), 0)
            .ok_or_else(|| unavailable("invalid preview start"))?;
        let finish = at
            .checked_add_signed(chrono::TimeDelta::seconds(30))
            .ok_or_else(|| unavailable("invalid preview finish"))?;
        let locked = super::schedule::read_locked(&self.pool, &self.locker, channel, at, finish)
            .await?
            .ok_or_else(|| unavailable("channel schedule is busy"))?;
        let mut items = locked.items;
        for media in &mut items {
            if media.item.finish > finish {
                media.item.finish = finish;
                media.item.out_point = media
                    .item
                    .in_point
                    .checked_add(&(finish - media.item.start))
                    .ok_or_else(|| unavailable("invalid preview source range"))?;
            }
        }
        Ok(items)
    }

    /// Revoke preview callbacks before awaiting all producer cleanup.
    /// # Errors
    /// Unconfirmed cleanup requires the caller to retain capacity and output.
    pub async fn finish_troubleshooting_session(
        &self,
        instance: uuid::Uuid,
    ) -> std::result::Result<(), crate::remote_stream::RemoteStreamError> {
        let mut contexts = self.dynamic_contexts.lock().await;
        if contexts
            .get(&instance)
            .is_none_or(|context| context.troubleshooting.is_none())
        {
            return Ok(());
        }
        let context = contexts.remove(&instance);
        drop(contexts);
        if let Some(context) = context {
            context.transfers.shutdown().await?;
        }
        Ok(())
    }

    /// Read one channel occurrence for a finite preview, releasing scheduler locks
    /// before returning its exact version/file binding to the process adapter.
    /// # Errors
    /// Returns schedule read failures or a busy scheduler; absence means no item.
    pub async fn troubleshooting_channel_media(
        &self,
        channel: &Channel,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<ScheduledMedia>> {
        let until = at
            .checked_add_signed(chrono::Duration::milliseconds(1))
            .ok_or_else(|| unavailable("invalid preview start"))?;
        let locked = super::schedule::read_locked(&self.pool, &self.locker, channel, at, until)
            .await?
            .ok_or_else(|| unavailable("channel schedule is busy"))?;
        Ok(locked
            .items
            .into_iter()
            .rev()
            .find(|media| media.item.start <= at && media.item.finish > at))
    }

    /// Prepare an ad-hoc item using normal provider, audio and subtitle contracts.
    /// Network and extraction work runs without holding scheduler locks. Exact
    /// metadata and track bindings are checked again before returning an artifact.
    ///
    /// # Errors
    /// Returns redacted failures for stale selections, unsupported sources,
    /// unsupported watermark modes or preparation beyond the shared deadline.
    pub async fn prepare_troubleshooting(
        &self,
        request: TroubleshootingPreview<'_>,
    ) -> Result<protocol::PlayoutItem> {
        if Instant::now() >= request.deadline {
            return Err(unavailable("preparation deadline expired"));
        }
        let projection = timeout_at(
            request.deadline,
            self.prepare_troubleshooting_inner(&request),
        )
        .await
        .map_err(|_| unavailable("preparation deadline expired"))??;
        if projection.canvas.is_some() {
            return Err(unavailable("graphics require an owned preview session"));
        }
        Ok(projection.item)
    }

    async fn prepare_troubleshooting_inner(
        &self,
        request: &TroubleshootingPreview<'_>,
    ) -> Result<DeferredProjection> {
        let context = Context::new(request);
        self.prepare_troubleshooting_context(request, &context)
            .await
    }

    async fn prepare_troubleshooting_context(
        &self,
        request: &TroubleshootingPreview<'_>,
        context: &Context,
    ) -> Result<DeferredProjection> {
        let before = self
            .troubleshooting_snapshot_with_context(request, context)
            .await?;
        let Some(pending) = before.projection.pending_burn.as_ref() else {
            return Ok(before.projection);
        };
        let validate = || async {
            self.troubleshooting_snapshot_with_context(request, context)
                .await
                .is_ok_and(|current| {
                    current.identity.descriptor == before.identity.descriptor
                        && current.identity.cache_key == before.identity.cache_key
                })
        };
        let expires = before.projection.item.finish.max(chrono::Utc::now());
        let prepared = if pending.subtitle.subtitle_kind == crate::metadata::subtitle_kind::SIDECAR
        {
            self.burn_cache
                .prepare_provider(
                    &self.pool,
                    (&before.identity.cache_key, pending.subtitle.id),
                    expires,
                    request.deadline,
                    validate,
                )
                .await
        } else {
            let stream = u32::try_from(pending.subtitle.stream_index)
                .map_err(|_| unavailable("subtitle stream is invalid"))?;
            self.burn_cache
                .prepare(
                    &before.identity.cache_key,
                    &pending.source,
                    stream,
                    pending.subtitle.codec.as_deref().unwrap_or("subrip"),
                    request.ffmpeg,
                    expires,
                    request.deadline,
                    validate,
                )
                .await
        }
        .map_err(|_| unavailable("subtitle preparation failed"))?;
        let mut after = self
            .troubleshooting_snapshot_with_context(request, context)
            .await?;
        if after.identity.descriptor != before.identity.descriptor
            || after.identity.cache_key != before.identity.cache_key
        {
            return Err(unavailable("selection changed during preparation"));
        }
        after.projection.item = install_prepared_track(after.projection.item, &prepared)?;
        Ok(after.projection)
    }

    #[cfg(test)]
    async fn troubleshooting_snapshot(
        &self,
        request: &TroubleshootingPreview<'_>,
    ) -> Result<Snapshot> {
        self.troubleshooting_snapshot_with_context(request, &Context::new(request))
            .await
    }

    async fn troubleshooting_snapshot_with_context(
        &self,
        request: &TroubleshootingPreview<'_>,
        context: &Context,
    ) -> Result<Snapshot> {
        let source = source::resolve_published(
            &self.pool,
            request.media,
            request.replacements,
            Some(&self.media),
        )
        .await?;
        let mut projection = selection::project_troubleshooting_item(
            &self.pool,
            TroubleshootingSelection {
                channel: request.channel,
                item: &request.media.item,
                media_version_id: request.media.media_version_id,
                source,
                resolution: &request.profile.resolution,
                subtitle_id: request.subtitle_id,
                diagnostics: request.diagnostics.clone(),
                source_pending: false,
            },
        )
        .await?;
        context.attach_credits(&mut projection)?;
        let (watermarks, graphics) = if super::graphics::enabled(request.profile) {
            (
                context.watermarks(&self.pool).await?,
                context.graphics(&self.pool).await?,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        projection.canvas = super::graphics::project(
            &self.pool,
            request.channel,
            request.media,
            request.profile,
            &mut projection.item,
            &watermarks,
            &graphics,
        )
        .await?;
        let identity = dynamic_projection::fingerprint(
            &self.pool,
            request.media,
            &projection,
            request.channel,
        )
        .await?;
        Ok(Snapshot {
            projection,
            identity,
        })
    }
}

fn unavailable(reason: &str) -> RepositoryError {
    RepositoryError::unavailable("Next troubleshooting", reason)
}

#[cfg(test)]
#[path = "troubleshooting_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "troubleshooting_remote_tests.rs"]
mod remote_tests;

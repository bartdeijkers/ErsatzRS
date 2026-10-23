//! Resolve an authenticated Next item only after bounded, version-bound Burn preparation.

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use tokio::time::{timeout_at, Instant};

use super::{dynamic_item, dynamic_projection, host::NextHost, protocol};

/// Runtime-advertised support, never source or publication authority.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SourceVideoCutSupport {
    /// Legacy callbacks cannot consume a finite-video cutoff.
    #[default]
    Unavailable,
    /// The runtime explicitly understands the version-one cutoff contract.
    Version1,
}

/// Explicit runtime features; these never grant source or publication authority.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SourceRuntimeSupport {
    pub video_cut: SourceVideoCutSupport,
    /// Version-one consume-once timing and first-packet contract.
    pub single_use: bool,
}

/// Existing authenticated dynamic callback values, grouped without new authority.
#[derive(Clone, Copy)]
pub struct DynamicItemRequest<'a> {
    pub channel_id: i64,
    pub item_id: i64,
    pub digest: &'a str,
    pub audio: Option<u32>,
    pub number: &'a str,
    pub instance: uuid::Uuid,
    pub now: DateTime<Utc>,
    pub until: DateTime<Utc>,
}

impl NextHost {
    pub(super) async fn project_session_snapshot(
        &self,
        request: &super::host::NextStart,
        instance: uuid::Uuid,
        item_id: i64,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        audio: Option<usize>,
    ) -> Result<dynamic_projection::Snapshot> {
        let context = self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .ok_or_else(|| unavailable("session is unavailable"))?
            .troubleshooting
            .clone();
        let Some(session) = context else {
            return dynamic_projection::project_snapshot(
                &self.pool,
                &self.locker,
                &self.media,
                request.channel_id,
                item_id,
                now,
                until,
                &request.replacements,
                audio,
            )
            .await;
        };
        let context = session.occurrence(item_id, now)?;
        if item_id != context.media.item.id
            || now < context.media.item.start
            || now >= context.media.item.finish
            || until <= now
        {
            return Err(unavailable("preview occurrence is unavailable"));
        }
        let media = super::schedule::bind_media(&self.pool, context.media.item.clone()).await?;
        if media.media_version_id != context.media.media_version_id
            || media.media_file_id != context.media.media_file_id
            || media.path != context.media.path
        {
            return Err(unavailable("preview source changed"));
        }
        let projected = super::remote_projection::project_publishable_for_origin(
            &self.pool,
            &context.channel,
            &media,
            &context.profile.resolution,
            super::selection::AudioDirective::Published(audio),
            super::remote_projection::Origin::Troubleshooting(context.clone()),
        )
        .await?;
        Box::pin(super::remote_projection::materialize_prepared(
            &self.pool,
            &self.locker,
            projected,
            audio,
            &crate::file_system_layout::addon_processed_media_folder(),
        ))
        .await
    }

    async fn prepare_live_captions(
        &self,
        before: &mut dynamic_projection::Snapshot,
        instance: uuid::Uuid,
        item_id: i64,
        now: DateTime<Utc>,
        deadline: Instant,
    ) -> Result<bool> {
        // Capture immutable generated ownership before natural caption preparation
        // can remove an ended Custom-selected caption.
        let generated = if super::live_subtitles::is_live(&before.projection.item) {
            if let Some(session) = self.preview_session(instance).await {
                session
                    .occurrence(item_id, now)?
                    .credits_at(&before.projection, now)?
            } else {
                None
            }
        } else {
            None
        };
        let changed =
            super::live_subtitles::prepare(&self.burn_cache, before, now, deadline).await?;
        let Some(generated) = generated else {
            return Ok(changed);
        };
        let prepared = super::live_subtitles::prepare_generated(
            &self.burn_cache,
            &generated,
            &before.identity.descriptor,
            before.projection.item.finish,
            deadline,
        )
        .await?;
        super::live_subtitles::install_generated(before, now, prepared)?;
        Ok(true)
    }

    /// Resolve a published item for the currently registered Next session.
    /// Authentication belongs to the internal transport boundary. No caller paths
    /// or executable names are accepted; all bindings are reconstructed from SQL.
    /// # Errors
    /// Returns a redacted failure for expired sessions, stale publication,
    /// unavailable media, or bounded subtitle preparation failure.
    #[allow(clippy::too_many_arguments)]
    pub async fn resolve_dynamic(
        &self,
        channel_id: i64,
        item_id: i64,
        digest: &str,
        audio: Option<u32>,
        number: &str,
        instance: uuid::Uuid,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<protocol::PlayoutItem> {
        self.resolve_dynamic_with_video_cut(
            DynamicItemRequest {
                channel_id,
                item_id,
                digest,
                audio,
                number,
                instance,
                now,
                until,
            },
            SourceVideoCutSupport::Unavailable,
        )
        .await
    }

    /// Resolve the same session-bound callback with explicit runtime support.
    /// Authentication remains the internal transport's responsibility. Support
    /// changes no session, publication, source, selected-track or timing checks.
    /// # Errors
    /// Returns the same redacted resolution failures as `resolve_dynamic`.
    pub async fn resolve_dynamic_with_video_cut(
        &self,
        callback: DynamicItemRequest<'_>,
        video_cut: SourceVideoCutSupport,
    ) -> Result<protocol::PlayoutItem> {
        self.resolve_dynamic_with_source_support(
            callback,
            SourceRuntimeSupport {
                video_cut,
                single_use: false,
            },
        )
        .await
    }

    /// Resolve an authenticated callback with the explicitly negotiated source contracts.
    /// # Errors
    /// Preserves all ordinary source, publication, session and cleanup failures.
    pub async fn resolve_dynamic_with_source_support(
        &self,
        callback: DynamicItemRequest<'_>,
        support: SourceRuntimeSupport,
    ) -> Result<protocol::PlayoutItem> {
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        timeout_at(
            deadline,
            Box::pin(self.resolve_dynamic_inner(callback, support, deadline)),
        )
        .await
        .map_err(|_| unavailable("resolution deadline expired"))?
    }

    async fn resolve_dynamic_inner(
        &self,
        callback: DynamicItemRequest<'_>,
        support: SourceRuntimeSupport,
        deadline: Instant,
    ) -> Result<protocol::PlayoutItem> {
        let video_cut = support.video_cut;
        let DynamicItemRequest {
            channel_id,
            item_id,
            digest,
            audio,
            number,
            instance,
            now,
            until,
        } = callback;
        let audio = audio
            .map(usize::try_from)
            .transpose()
            .map_err(|_| unavailable("audio index is invalid"))?;
        let request = self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .map(|context| context.request.clone())
            .filter(|request| request.channel_id == channel_id && request.number == number)
            .ok_or_else(|| unavailable("session is unavailable"))?;
        let mut before = self
            .project_session_snapshot(&request, instance, item_id, now, until, audio)
            .await?;
        if before.identity.descriptor != digest {
            return Err(unavailable("publication changed"));
        }
        if self
            .prepare_live_captions(&mut before, instance, item_id, now, deadline)
            .await?
        {
            let current = self
                .project_session_snapshot(&request, instance, item_id, now, until, audio)
                .await?;
            if current.identity.descriptor != digest
                || !self.dynamic_contexts.lock().await.contains_key(&instance)
            {
                return Err(unavailable(
                    "publication or session changed during caption preparation",
                ));
            }
        }
        before
            .projection
            .publish_graphics(&self.media, Some(instance), now)?;
        if let Some(retained) = before.retained_remote.take() {
            return Box::pin(
                self.resolve_retained(before, retained, &request, instance, audio, now, deadline),
            )
            .await;
        }
        if let Some(producer) = before.producer_remote.take() {
            return Box::pin(self.resolve_producer(
                before,
                producer,
                &request,
                instance,
                audio,
                now,
                deadline,
                support.single_use,
            ))
            .await;
        }
        let provider = super::provider_source_host::ProviderRequest {
            request: &request,
            instance,
            item_id,
            audio,
            now,
            until,
            deadline,
            video_cut,
        };
        if let Some(resolved) = Box::pin(self.resolve_provider_source(&before, &provider)).await? {
            return Ok(resolved);
        }
        Box::pin(self.resolve_concrete(
            before, &request, item_id, instance, audio, now, until, deadline, true, video_cut,
        ))
        .await
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Resolution forwards the admitted session, selected tracks, and timing bounds."
    )]
    async fn resolve_concrete(
        &self,
        before: dynamic_projection::Snapshot,
        request: &super::host::NextStart,
        item_id: i64,
        instance: uuid::Uuid,
        audio: Option<usize>,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        deadline: Instant,
        include_source_text: bool,
        video_cut: SourceVideoCutSupport,
    ) -> Result<protocol::PlayoutItem> {
        let digest = &before.identity.descriptor;
        let Some(pending) = before.projection.pending_burn.as_ref() else {
            return self
                .resolve_ready_item(instance, &before.projection.item, now)
                .await;
        };
        let stream = u32::try_from(pending.subtitle.stream_index)
            .map_err(|_| unavailable("subtitle stream is invalid"))?;
        let validate = || async {
            self.project_preparation_snapshot(
                request,
                instance,
                item_id,
                now,
                until,
                audio,
                include_source_text,
            )
            .await
            .is_ok_and(|current| {
                &current.identity.descriptor == digest
                    && current.identity.cache_key == before.identity.cache_key
            })
        };
        let preparation_deadline = deadline - std::time::Duration::from_secs(6);
        let mut prepared_video_cut = None;
        let prepared = if let Some(text) = &pending.source_text {
            let prepared = Box::pin(super::source_teletext::prepare(
                &self.pool,
                &self.locker,
                &self.burn_cache,
                super::source_teletext::Preparation {
                    pending: text,
                    source: &pending.source,
                    video: before.projection.selected_video(),
                    key: &before.identity.cache_key,
                    expires: before.projection.item.finish.max(Utc::now()),
                    ffmpeg: &request.ffmpeg,
                    ffprobe: &request.ffprobe,
                    deadline: preparation_deadline,
                    video_cut,
                },
                validate,
            ))
            .await?;
            match prepared {
                super::source_teletext::PreparedText::Ready(caption) => {
                    prepared_video_cut = caption.video_cut;
                    Ok(caption.path)
                }
                super::source_teletext::PreparedText::Absent(absent) => {
                    return Box::pin(self.resolve_absent_source_page(
                        &before, absent, request, instance, item_id, audio, now, until, deadline,
                        video_cut,
                    ))
                    .await;
                }
            }
        } else if pending.subtitle.subtitle_kind == crate::metadata::subtitle_kind::SIDECAR {
            self.burn_cache
                .prepare_provider(
                    &self.pool,
                    (&before.identity.cache_key, pending.subtitle.id),
                    before.projection.item.finish.max(Utc::now()),
                    preparation_deadline,
                    validate,
                )
                .await
        } else {
            self.burn_cache
                .prepare(
                    &before.identity.cache_key,
                    &pending.source,
                    stream,
                    pending.subtitle.codec.as_deref().unwrap_or("subrip"),
                    &request.ffmpeg,
                    before.projection.item.finish.max(Utc::now()),
                    preparation_deadline,
                    validate,
                )
                .await
        }
        .map_err(|_| unavailable("subtitle preparation failed"))?;
        self.finish_concrete(
            &before,
            request,
            instance,
            item_id,
            audio,
            now,
            until,
            include_source_text,
            &prepared,
            prepared_video_cut.as_deref(),
        )
        .await
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Final installation rechecks the exact session, selected policy and original timing bounds."
    )]
    async fn finish_concrete(
        &self,
        before: &dynamic_projection::Snapshot,
        request: &super::host::NextStart,
        instance: uuid::Uuid,
        item_id: i64,
        audio: Option<usize>,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        include_source_text: bool,
        prepared: &std::path::Path,
        video_cut: Option<&super::source_teletext::PreparedVideoCut>,
    ) -> Result<protocol::PlayoutItem> {
        if let Some(cut) = video_cut {
            cut.check_source().await?;
        }
        // Metadata and membership may change while FFmpeg is running. Never
        // attach a completed artifact to a different selection or publication.
        let mut after = self
            .project_preparation_snapshot(
                request,
                instance,
                item_id,
                now,
                until,
                audio,
                include_source_text,
            )
            .await?;
        if after.identity.descriptor != before.identity.descriptor
            || after.identity.cache_key != before.identity.cache_key
            || !self.dynamic_contexts.lock().await.contains_key(&instance)
        {
            return Err(unavailable("publication changed during preparation"));
        }
        after
            .projection
            .publish_graphics(&self.media, Some(instance), now)?;
        if let Some(cut) = video_cut {
            cut.install(&mut after.projection.item)?;
        }
        let item = install_prepared_track(after.projection.item, prepared)?;
        dynamic_item::resolve_at(&item, now)
            .map_err(|_| unavailable("requested item timing is invalid"))
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Ordinary fallback must retain every authenticated session and publication bound."
    )]
    async fn project_preparation_snapshot(
        &self,
        request: &super::host::NextStart,
        instance: uuid::Uuid,
        item_id: i64,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        audio: Option<usize>,
        include_source_text: bool,
    ) -> Result<dynamic_projection::Snapshot> {
        if include_source_text {
            return self
                .project_session_snapshot(request, instance, item_id, now, until, audio)
                .await;
        }
        // Source-text intents are created only for ordinary scheduled local
        // media. Never replace the distinct troubleshooting-origin policy.
        if self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .is_none_or(|context| context.troubleshooting.is_some())
        {
            return Err(unavailable("ordinary caption selection is unavailable"));
        }
        dynamic_projection::project_snapshot_with_source_text(
            &self.pool,
            &self.locker,
            &self.media,
            request.channel_id,
            item_id,
            now,
            until,
            &request.replacements,
            audio,
            false,
        )
        .await
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Absence retains the original identity and admitted source across ordinary preparation."
    )]
    async fn resolve_absent_source_page(
        &self,
        before: &dynamic_projection::Snapshot,
        absent: super::source_teletext::AbsentPage,
        request: &super::host::NextStart,
        instance: uuid::Uuid,
        item_id: i64,
        audio: Option<usize>,
        now: DateTime<Utc>,
        until: DateTime<Utc>,
        deadline: Instant,
        video_cut: SourceVideoCutSupport,
    ) -> Result<protocol::PlayoutItem> {
        let validate = || async {
            let current = self
                .project_session_snapshot(request, instance, item_id, now, until, audio)
                .await?;
            if current.identity.descriptor != before.identity.descriptor
                || current.identity.cache_key != before.identity.cache_key
            {
                return Err(unavailable("publication changed after page absence"));
            }
            absent.check_source(deadline).await
        };
        validate().await?;
        let ordinary = self
            .project_preparation_snapshot(request, instance, item_id, now, until, audio, false)
            .await?;
        // No generic error fallback: only the privately held successful
        // complete-source absence can enter this ordinary selection path.
        let ordinary_identity = ordinary.identity.clone();
        let item = Box::pin(self.resolve_concrete(
            ordinary, request, item_id, instance, audio, now, until, deadline, false, video_cut,
        ))
        .await?;
        validate().await?;
        let current = self
            .project_preparation_snapshot(request, instance, item_id, now, until, audio, false)
            .await?;
        if current.identity.descriptor != ordinary_identity.descriptor
            || current.identity.cache_key != ordinary_identity.cache_key
        {
            return Err(unavailable("ordinary selection changed after page absence"));
        }
        tracing::warn!(
            channel_id = request.channel_id,
            item_id,
            page = absent.page(),
            "Complete finite source lacks selected teletext page; using ordinary subtitle selection"
        );
        Ok(item)
    }

    async fn resolve_ready_item(
        &self,
        instance: uuid::Uuid,
        item: &protocol::PlayoutItem,
        now: DateTime<Utc>,
    ) -> Result<protocol::PlayoutItem> {
        if !self.dynamic_contexts.lock().await.contains_key(&instance) {
            return Err(unavailable("session is unavailable"));
        }
        dynamic_item::resolve_at(item, now)
            .map_err(|_| unavailable("requested item timing is invalid"))
    }
}

pub(super) fn install_prepared_track(
    mut item: protocol::PlayoutItem,
    prepared: &std::path::Path,
) -> Result<protocol::PlayoutItem> {
    let track = item
        .tracks
        .as_mut()
        .and_then(|tracks| tracks.subtitle.as_mut())
        .ok_or_else(|| unavailable("subtitle selection changed"))?;
    let (in_point_ms, out_point_ms) = match track.source.as_ref() {
        Some(
            protocol::Source::Local {
                in_point_ms,
                out_point_ms,
                ..
            }
            | protocol::Source::Http {
                in_point_ms,
                out_point_ms,
                ..
            },
        ) => (*in_point_ms, *out_point_ms),
        _ => return Err(unavailable("subtitle source changed")),
    };
    track.source = Some(protocol::Source::Local {
        path: prepared
            .to_str()
            .ok_or_else(|| unavailable("subtitle cache path is invalid"))?
            .to_owned(),
        in_point_ms,
        out_point_ms,

        probe_hint: None,
    });
    track.stream_index = Some(0);
    Ok(item)
}

fn unavailable(reason: &str) -> RepositoryError {
    RepositoryError::unavailable("Next dynamic item", reason)
}

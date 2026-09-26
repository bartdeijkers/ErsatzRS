//! Isolated item projection for upstream `PrepareTroubleshootingPlaybackHandler`.
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
}

impl Context {
    fn new(request: &TroubleshootingPreview<'_>) -> Self {
        Self {
            media: request.media.clone(),
            channel: request.channel.clone(),
            profile: request.profile.clone(),
            subtitle_id: request.subtitle_id,
            selection_time: request.selection_time,
            watermark_ids: request.watermark_ids.to_vec(),
            graphics_ids: request.graphics_ids.to_vec(),
            diagnostics: request.diagnostics.clone(),
        }
    }

    pub(super) async fn graphics(
        &self,
        pool: &crate::db::DbPool,
    ) -> Result<Vec<playback::ResolvedPlaybackGraphicsElement>> {
        if let Some(at) = self.selection_time {
            let source_time =
                crate::channel_playout::ChannelPlayoutContext::for_channel(&self.channel)
                    .source_time(at)?;
            playback::select_playback_graphics(pool, &self.channel, self.media.item.id, source_time)
                .await
        } else {
            playback::resolve_playback_graphics_by_ids(pool, self.graphics_ids.clone()).await
        }
    }

    pub(super) async fn watermarks(
        &self,
        pool: &crate::db::DbPool,
    ) -> Result<Vec<playback::ResolvedPlaybackWatermark>> {
        if let Some(at) = self.selection_time {
            let source_time =
                crate::channel_playout::ChannelPlayoutContext::for_channel(&self.channel)
                    .source_time(at)?;
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
}

struct Snapshot {
    projection: DeferredProjection,
    identity: Identity,
}

impl NextHost {
    /// Publish one transient Remote Stream occurrence and admit its exact process
    /// instance. The caller must revoke the session after stopping that process,
    /// including on startup failure, before releasing output or transcode capacity.
    /// # Errors
    /// Rejects mismatched session identity or unavailable source/track selections.
    pub async fn prepare_troubleshooting_session(
        &self,
        request: TroubleshootingPreview<'_>,
        start: super::host::NextStart,
    ) -> Result<(uuid::Uuid, protocol::PlayoutItem)> {
        if start.channel_id != request.channel.id || start.number != request.channel.number {
            return Err(unavailable("preview session identity differs"));
        }
        let context = std::sync::Arc::new(Context::new(&request));
        let before = timeout_at(
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
        let item = self.defer_item(
            context.channel.id,
            context.media.item.id,
            &before.identity.descriptor,
            before.projection.item,
        )?;
        let instance = uuid::Uuid::new_v4();
        self.dynamic_contexts.lock().await.insert(
            instance,
            super::host::DynamicContext {
                request: start,
                troubleshooting: Some(context),
                transfers: std::sync::Arc::default(),
                fallback: std::sync::Arc::default(),
                gaps: std::sync::Arc::default(),
            },
        );
        Ok((instance, item))
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
        timeout_at(
            request.deadline,
            self.prepare_troubleshooting_inner(&request),
        )
        .await
        .map_err(|_| unavailable("preparation deadline expired"))?
    }

    async fn prepare_troubleshooting_inner(
        &self,
        request: &TroubleshootingPreview<'_>,
    ) -> Result<protocol::PlayoutItem> {
        let before = self.troubleshooting_snapshot(request).await?;
        let Some(pending) = before.projection.pending_burn.as_ref() else {
            return Ok(before.projection.item);
        };
        let validate = || async {
            self.troubleshooting_snapshot(request)
                .await
                .is_ok_and(|current| {
                    current.identity.descriptor == before.identity.descriptor
                        && current.identity.cache_key == before.identity.cache_key
                })
        };
        let expires = before.projection.item.finish;
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
        let after = self.troubleshooting_snapshot(request).await?;
        if after.identity.descriptor != before.identity.descriptor
            || after.identity.cache_key != before.identity.cache_key
        {
            return Err(unavailable("selection changed during preparation"));
        }
        install_prepared_track(after.projection.item, &prepared)
    }

    async fn troubleshooting_snapshot(
        &self,
        request: &TroubleshootingPreview<'_>,
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
            },
        )
        .await?;
        let context = Context::new(request);
        let watermarks = context.watermarks(&self.pool).await?;
        let graphics = context.graphics(&self.pool).await?;
        projection.item.graphics = super::graphics::project(
            &self.pool,
            request.channel,
            request.media,
            &request.profile.resolution,
            &projection.item,
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

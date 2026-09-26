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
    source, watermark,
};

/// Exact transient selection supplied by the troubleshooting host.
/// Deliberately no Debug: bindings may contain private source paths.
pub struct TroubleshootingPreview<'a> {
    pub media: &'a ScheduledMedia,
    pub channel: &'a Channel,
    pub profile: &'a PlaybackProfile,
    /// None uses channel preferences; zero explicitly disables subtitles.
    pub subtitle_id: Option<i64>,
    pub watermark_ids: &'a [i64],
    pub replacements: &'a [PathReplacement],
    pub ffmpeg: &'a Path,
    pub deadline: Instant,
}

struct Snapshot {
    projection: DeferredProjection,
    identity: Identity,
}

impl NextHost {
    /// Prepare an ad-hoc item using normal provider, audio and subtitle contracts.
    /// Network and extraction work runs without holding scheduler locks. Exact
    /// metadata and track bindings are checked again before returning an artifact.
    ///
    /// # Errors
    /// Returns redacted failures for stale selections, unsupported sources,
    /// multiple watermarks or preparation beyond the shared deadline.
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
        if request.watermark_ids.len() > 1 {
            return Err(unavailable("Next troubleshooting accepts one watermark"));
        }
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
            },
        )
        .await?;
        let watermarks = playback::resolve_playback_watermarks_by_ids(
            &self.pool,
            request.channel.id,
            request.watermark_ids.to_vec(),
        )
        .await?;
        projection.item.watermark = watermark::project(&watermarks)?;
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

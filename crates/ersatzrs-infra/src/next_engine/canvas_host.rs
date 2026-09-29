//! Current occurrence and exact process-session validation for canvas rendering.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{ChannelRepository as _, RepositoryError, Result};
use ersatzrs_domain::{Channel, StreamingEngine};
use uuid::Uuid;

use crate::{
    channel_playout::ChannelPlayoutContext,
    playback::{self, PlaybackProfile, ResolvedPlaybackGraphicsElement, ResolvedPlaybackWatermark},
    repositories::make_repositories,
};

use super::{
    canvas::{CanvasBinding, CanvasRequest, CanvasTimeline},
    host::{NextHost, NextStart},
    schedule::{self, ScheduledMedia},
    session_transfers::SessionTransfers,
};

/// Owned rendering inputs. No scheduler lock survives this snapshot, and no
/// producer has started yet. Deliberately no Debug because media paths are private.
pub struct CanvasSession {
    pub request: NextStart,
    pub started_at: DateTime<Utc>,
    pub video_stream_index: Option<i32>,
    pub timeline: CanvasTimeline,
    pub channel: Channel,
    pub profile: PlaybackProfile,
    pub media: ScheduledMedia,
    pub graphics: Vec<ResolvedPlaybackGraphicsElement>,
    pub watermarks: Vec<ResolvedPlaybackWatermark>,
    pub transfers: Arc<SessionTransfers>,
}

impl NextHost {
    /// Prepare and claim a finite canvas in the owning session's transfer registry.
    /// The HTTP boundary must validate capability, ticket and headers first.
    ///
    /// # Errors
    /// Rejects stale bindings, closed/full sessions and failed or cancelled rendering.
    pub async fn render_canvas(
        &self,
        instance: Uuid,
        binding: &CanvasBinding,
        timing: &CanvasRequest,
    ) -> Result<super::session_transfers::TransferReader> {
        let before = self.canvas_session(instance, binding, timing).await?;
        let transfers = before.transfers.clone();
        let pool = self.pool.clone();
        let request_timing = binding
            .programme_request(timing)
            .map_err(|_| unavailable())?;
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        let transfer_binding = super::session_transfers::TransferBinding {
            media_item_id: binding.media,
            playout_item_id: binding.item,
            media_version_id: binding.version,
            media_file_id: binding.file,
            publication: "graphics canvas".into(),
            start: before.timeline.now,
            finish: before.timeline.chunk_finish,
        };
        let ready = transfers
            .prepare(
                transfer_binding,
                deadline,
                deadline + std::time::Duration::from_secs(5),
                move |cancellation| {
                    super::canvas_render::prepare(
                        pool,
                        before,
                        request_timing,
                        deadline,
                        cancellation,
                    )
                },
            )
            .await
            .map_err(|_| unavailable())?;
        let mut pending = super::retained_host::PendingClaim {
            transfers: transfers.clone(),
            id: Some(ready.id),
        };
        let result = async {
            self.canvas_session(instance, binding, timing).await?;
            self.claim_prepared(instance, ready.id)
                .await
                .map_err(|_| unavailable())
        }
        .await;
        if result.is_err() {
            transfers
                .cancel(ready.id)
                .await
                .map_err(|_| unavailable())?;
        } else {
            pending.id = None;
        }
        result
    }

    /// Resolve an authenticated binding within one exact active process instance.
    /// The HTTP boundary must authenticate its ticket first. Call again after
    /// asynchronous producer preparation before claiming/returning its stream.
    ///
    /// # Errors
    /// Rejects expired, cross-channel, stopped, changed or absent occurrences,
    /// including previews/fallbacks belonging to another process instance.
    pub async fn canvas_session(
        &self,
        instance: Uuid,
        binding: &CanvasBinding,
        timing: &CanvasRequest,
    ) -> Result<CanvasSession> {
        if binding.expires <= Utc::now()
            || binding.session.is_some_and(|expected| expected != instance)
        {
            return Err(unavailable());
        }
        let timeline = binding.timeline(timing).map_err(|_| unavailable())?;
        let (request, started_at, preview, fallback, transfers) = {
            let contexts = self.dynamic_contexts.lock().await;
            let context = contexts
                .get(&instance)
                .filter(|context| context.request.channel_id == binding.channel)
                .ok_or_else(unavailable)?;
            (
                context.request.clone(),
                context.started_at,
                context.troubleshooting.clone(),
                context.fallback.clone(),
                context.transfers.clone(),
            )
        };
        let (channel, profile, media, graphics, watermarks) = if let Some(preview) = preview {
            if binding.session != Some(instance) {
                return Err(unavailable());
            }
            if !binding.matches(preview.channel.id, &preview.media, Utc::now()) {
                return Err(unavailable());
            }
            let media = schedule::bind_media(&self.pool, preview.media.item.clone()).await?;
            let graphics = preview.graphics(&self.pool).await?;
            let watermarks = preview.watermarks(&self.pool).await?;
            (
                preview.channel.clone(),
                preview.profile.clone(),
                media,
                graphics,
                watermarks,
            )
        } else {
            if binding.item == 0 && binding.session != Some(instance) {
                return Err(unavailable());
            }
            let channel = make_repositories(self.pool.clone())
                .channel
                .get_with_profile(binding.channel)
                .await?;
            if channel.streaming_engine != StreamingEngine::Next {
                return Err(unavailable());
            }
            let media = self
                .scheduled_canvas_media(&channel, binding, &fallback)
                .await?;
            if !binding.matches(channel.id, &media, Utc::now()) {
                return Err(unavailable());
            }
            let profile = playback::load_channel_playback_profile(&self.pool, channel.id)
                .await?
                .ok_or_else(unavailable)?;
            let at = ChannelPlayoutContext::for_channel(&channel).source_time(timeline.now)?;
            let (graphics, watermarks) = if binding.item == 0 {
                (
                    playback::select_fallback_graphics(&self.pool, &media.item, at).await?,
                    playback::select_fallback_watermarks(&self.pool, &channel, &media.item, at)
                        .await?,
                )
            } else {
                (
                    playback::select_playback_graphics(&self.pool, &channel, media.item.id, at)
                        .await?,
                    playback::select_playback_watermarks(
                        &self.pool,
                        &channel,
                        media.item.id,
                        media.item.disable_watermarks,
                        at,
                    )
                    .await?,
                )
            };
            (channel, profile, media, graphics, watermarks)
        };
        if !binding.matches(channel.id, &media, Utc::now())
            || !self.dynamic_contexts.lock().await.contains_key(&instance)
        {
            return Err(unavailable());
        }
        Ok(CanvasSession {
            video_stream_index: binding.video_stream_index,
            request,
            started_at,
            timeline,
            channel,
            profile,
            media,
            graphics,
            watermarks,
            transfers,
        })
    }
    async fn scheduled_canvas_media(
        &self,
        channel: &Channel,
        binding: &CanvasBinding,
        fallback: &tokio::sync::Mutex<Option<super::fallback::Occurrence>>,
    ) -> Result<ScheduledMedia> {
        if binding.item == 0 {
            let item = fallback
                .lock()
                .await
                .as_ref()
                .map(|occurrence| occurrence.item.clone())
                .ok_or_else(unavailable)?;
            return schedule::bind_media(&self.pool, item).await;
        }
        let snapshot = schedule::read_locked(
            &self.pool,
            &self.locker,
            channel,
            binding.start,
            binding.finish,
        )
        .await?
        .ok_or_else(unavailable)?;
        let mut matches = snapshot
            .items
            .iter()
            .filter(|media| binding.matches(channel.id, media, Utc::now()));
        let media = matches.next().cloned().ok_or_else(unavailable)?;
        if matches.next().is_some() {
            return Err(unavailable());
        }
        Ok(media)
    }
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable("Next canvas", "session or occurrence is unavailable")
}

#[cfg(test)]
#[path = "canvas_host_tests.rs"]
mod tests;

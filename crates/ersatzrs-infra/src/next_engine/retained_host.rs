//! Session-owned retained-range preparation, followed by fresh track mapping.
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use tokio::{sync::oneshot, time::Instant};

use super::{
    dynamic_projection::Snapshot,
    host::{NextHost, NextStart},
    protocol,
    remote_projection::RetainedContext,
    session_transfers::{PreparedTransferMedia, TransferBinding},
};
use crate::{
    addon_media_list_item_runtime::ItemRuntimeError,
    ffprobe::{FfprobeExtractor, FfprobeMediaVersion},
    playback::{self, PlaybackAudioSelection},
    published_interval_source::IntervalPlaybackContext,
    remote_stream::{RemoteStreamError, RemoteStreamSource, RemoteStreamSpawnContext},
};

pub(super) struct PendingClaim {
    pub(super) transfers: std::sync::Arc<super::session_transfers::SessionTransfers>,
    pub(super) id: Option<uuid::Uuid>,
}

impl Drop for PendingClaim {
    fn drop(&mut self) {
        if let Some(id) = self.id {
            self.transfers.request_cancel(id);
        }
    }
}

impl NextHost {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn resolve_retained(
        &self,
        before: Snapshot,
        context: RetainedContext,
        request: &NextStart,
        instance: uuid::Uuid,
        audio: Option<usize>,
        now: DateTime<Utc>,
        deadline: Instant,
    ) -> Result<protocol::PlayoutItem> {
        if now < before.projection.item.start || now >= before.projection.item.finish {
            return Err(unavailable());
        }
        let programme_ms = u64::try_from(
            (context.media.item.in_point + (now - context.media.item.start)).num_milliseconds(),
        )
        .map_err(|_| unavailable())?;
        let duration_ms = u64::try_from((before.projection.item.finish - now).num_milliseconds())
            .map_err(|_| unavailable())?;
        let transfers = self
            .dynamic_contexts
            .lock()
            .await
            .get(&instance)
            .map(|context| context.transfers.clone())
            .ok_or_else(unavailable)?;
        let binding = TransferBinding {
            media_item_id: context.media.item.media_item_id,
            playout_item_id: context.media.item.id,
            media_version_id: context.media.media_version_id,
            media_file_id: context.media.media_file_id,
            publication: before.identity.descriptor.clone(),
            start: before.projection.item.start,
            finish: before.projection.item.finish,
        };
        let (subtitle_tx, subtitle_rx) = oneshot::channel();
        let preparation_deadline = deadline - std::time::Duration::from_secs(6);
        let preparation = RetainedPreparation {
            pool: self.pool.clone(),
            media_id: context.media.item.media_item_id,
            ffprobe: request.ffprobe.to_str().ok_or_else(unavailable)?.to_owned(),
            number: request.number.clone(),
            selected_subtitle: before.projection.selected_subtitle.clone(),
            programme_ms,
            max_height: context.max_height,
            preparation_deadline,
        };
        let ready = Box::pin(transfers.prepare(
            binding,
            preparation_deadline,
            deadline + std::time::Duration::from_secs(10),
            move |cancellation| preparation.run(cancellation, subtitle_tx),
        ))
        .await
        .map_err(|_| unavailable())?;
        let mut pending = PendingClaim {
            transfers: transfers.clone(),
            id: Some(ready.id),
        };
        let result = async {
            let subtitle = subtitle_rx.await.map_err(|_| unavailable())?;
            let after = Box::pin(context.origin.snapshot(
                &self.pool,
                &self.locker,
                request.channel_id,
                &context.media.item,
                audio,
            ))
            .await?;
            if after.identity.descriptor != before.identity.descriptor
                || !after.retained
                || !self.dynamic_contexts.lock().await.contains_key(&instance)
            {
                return Err(unavailable());
            }
            let hint = super::probe_hint::retained(&ready.probe, duration_ms)
                .map_err(|_| unavailable())?;
            let source = self
                .media
                .prepared_source(instance, ready.id, hint, false)?;
            install_fresh_tracks(
                before.projection.item,
                &ready.probe,
                &context.previous_audio,
                audio,
                source,
                subtitle,
                now,
            )
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
}

struct RetainedPreparation {
    pool: crate::db::DbPool,
    media_id: i64,
    ffprobe: String,
    number: String,
    selected_subtitle: Option<playback::PlaybackSubtitle>,
    programme_ms: u64,
    max_height: Option<u32>,
    preparation_deadline: Instant,
}

impl RetainedPreparation {
    async fn run(
        self,
        mut cancellation: oneshot::Receiver<()>,
        subtitle_tx: oneshot::Sender<Option<String>>,
    ) -> std::result::Result<PreparedTransferMedia, RemoteStreamError> {
        let Self {
            pool,
            media_id,
            ffprobe,
            number,
            selected_subtitle,
            programme_ms,
            max_height,
            preparation_deadline,
        } = self;

        let preflight = async {
            let resolved = crate::addon_media_list_playback::resolve_persisted_remote_stream(
                &pool,
                media_id,
                &crate::addons::AddonRuntimeContext::default(),
                &crate::file_system_layout::addon_processed_media_folder(),
            )
            .await
            .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
            let RemoteStreamSource::RetainedIntervals(source) = resolved.source else {
                return Err(RemoteStreamError::PreparedProducerFailed);
            };
            let subtitle = if let Some(selected) = selected_subtitle {
                crate::addon_media_list_subtitle_playback::prepare_retained_subtitle_input(
                    &pool,
                    media_id,
                    &selected,
                    source.plan(),
                    std::time::Duration::from_millis(programme_ms),
                )
                .await
                .map_err(|_| RemoteStreamError::PreparedProducerFailed)?
                .map(|input| input.path)
            } else {
                None
            };
            Ok::<_, RemoteStreamError>((source, subtitle))
        };
        tokio::pin!(preflight);
        let (source, subtitle) = tokio::select! {
            biased;
            _ = &mut cancellation => return Err(RemoteStreamError::PreparedCancelled),
            () = tokio::time::sleep_until(preparation_deadline) => return Err(RemoteStreamError::PreparedTimeout),
            result = &mut preflight => result?,
        };
        let probe = FfprobeExtractor::new(ffprobe);
        let playback = Box::pin(source.prepare_playback_cancellable(
            IntervalPlaybackContext {
                pool: &pool,
                probe: &probe,
                spawn_context: RemoteStreamSpawnContext::hls(number),
            },
            programme_ms,
            max_height,
            preparation_deadline,
            cancellation,
        ))
        .await
        .map_err(runtime_error)?
        .ok_or(RemoteStreamError::PreparedProducerFailed)?;
        let _ = subtitle_tx.send(subtitle);
        Ok(PreparedTransferMedia {
            input: playback.prepared,
            probe: playback.probe,
        })
    }
}

fn runtime_error(error: ItemRuntimeError) -> RemoteStreamError {
    if matches!(error, ItemRuntimeError::TerminationUnconfirmed) {
        RemoteStreamError::PreparedTerminationUnconfirmed
    } else {
        RemoteStreamError::PreparedProducerFailed
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn install_fresh_tracks(
    mut item: protocol::PlayoutItem,
    probe: &FfprobeMediaVersion,
    previous: &[ersatzrs_ffmpeg::SourceAudioStream],
    selected: Option<usize>,
    source: protocol::Source,
    subtitle: Option<String>,
    now: DateTime<Utc>,
) -> Result<protocol::PlayoutItem> {
    let video = playback::source_video_stream_from_probe(probe)?.ok_or_else(unavailable)?;
    let fresh = playback::source_audio_streams_from_probe(probe)?;
    let selection = selected.map_or(
        PlaybackAudioSelection::Disabled,
        PlaybackAudioSelection::Exact,
    );
    let mapped = playback::remap_pinned_audio_selection(selection, previous, &fresh)?;
    let duration =
        u64::try_from((item.finish - now).num_milliseconds()).map_err(|_| unavailable())?;
    let audio = match mapped {
        PlaybackAudioSelection::Exact(index) => protocol::Track {
            source: Some(source.clone()),
            stream_index: Some(u32::try_from(index).map_err(|_| unavailable())?),
        },
        PlaybackAudioSelection::Disabled => super::selection::silent_audio(duration),
        PlaybackAudioSelection::Automatic => return Err(unavailable()),
    };
    item.start = now;
    item.source = None;
    item.tracks = Some(protocol::Tracks {
        video: Some(protocol::Track {
            source: Some(source),
            stream_index: Some(u32::try_from(video.index).map_err(|_| unavailable())?),
        }),
        audio: Some(audio),
        subtitle: subtitle.map(|path| protocol::Track {
            source: Some(protocol::Source::Local {
                path,
                in_point_ms: None,
                out_point_ms: None,
            }),
            stream_index: Some(0),
        }),
    });
    Ok(item)
}

fn unavailable() -> RepositoryError {
    RepositoryError::unavailable(
        "Next retained playback",
        "retained preparation or publication unavailable",
    )
}

#[cfg(test)]
#[path = "retained_host_tests.rs"]
mod tests;

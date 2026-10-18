//! Explicit initialized carriage command and its actual-child lifetime owner.
use std::{fmt, sync::Arc};

use ersatzrs_domain::SourceTeletextCaptionDelivery;
use ersatzrs_ffmpeg::{
    build_pipeline_command_with_source_bitmap_carriage_plan, FfmpegCommand, InputFile,
    SourceTeletextInputPlan,
};
use tokio::time::Instant;

use super::{
    begin, unavailable, ClosingScope, FfprobeExtractor, LocalProbeCleanupScope, PlaybackSource,
    Result, SourceTeletextConsumerRequest, MAX_STARTUP,
};
use crate::ffprobe::teletext_decode::{DecodedLocalTeletextPage, SourceTeletextBitmapPlaylist};

/// The final command and the optional owner that must accompany its actual child.
pub struct InitializedSourceTeletextConsumer {
    command: FfmpegCommand,
    bitmap: Option<Arc<InitializedSourceTeletextBitmap>>,
}
impl fmt::Debug for InitializedSourceTeletextConsumer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InitializedSourceTeletextConsumer")
            .field("bitmap", &self.bitmap.is_some())
            .finish_non_exhaustive()
    }
}
impl InitializedSourceTeletextConsumer {
    #[must_use]
    pub const fn command(&self) -> &FfmpegCommand {
        &self.command
    }
    #[must_use]
    pub fn into_parts(self) -> (FfmpegCommand, Option<Arc<InitializedSourceTeletextBitmap>>) {
        (self.command, self.bitmap)
    }
}
/// Exact final command seal, private playlist/fragments and original source owner.
/// No untyped locator is exposed. Attach this owner to the actual consuming child.
pub struct InitializedSourceTeletextBitmap {
    playlist: SourceTeletextBitmapPlaylist,
    command: FfmpegCommand,
    _hls_producer_directory: Option<super::HlsProducerDirectory>,
}
impl fmt::Debug for InitializedSourceTeletextBitmap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InitializedSourceTeletextBitmap")
            .finish_non_exhaustive()
    }
}
impl InitializedSourceTeletextBitmap {
    #[must_use]
    pub fn matches_command(&self, command: &FfmpegCommand) -> bool {
        self.command == *command
    }
    pub(crate) fn receipt(&self) -> Arc<DecodedLocalTeletextPage> {
        self.playlist.source()
    }
}

impl PlaybackSource {
    /// Initialize a completed request and retain every private carriage input.
    ///
    /// Burn preserves the existing path. Explicit bitmap carriage reobserves the
    /// actual physical-input-zero AV streams and seek on every attempt. Failure
    /// never returns the original full-artifact command as playable fallback.
    /// Transfer the returned owner to the actual child without an intervening await.
    /// # Errors
    /// Rejects absent/changed seals, unsupported input timing, source drift,
    /// preparation failure, expired deadlines or unconfirmed cleanup.
    pub async fn initialize_consumer(
        &self,
        command: &FfmpegCommand,
        plan: Option<&SourceTeletextInputPlan>,
        request: &SourceTeletextConsumerRequest,
        deadline: Instant,
    ) -> Result<InitializedSourceTeletextConsumer> {
        if self.delivery == SourceTeletextCaptionDelivery::Burn {
            let command = self
                .initialize_consumer_command(command, plan, request, deadline)
                .await?;
            return Ok(InitializedSourceTeletextConsumer {
                command,
                bitmap: None,
            });
        }
        self.check_consumer_command(command, plan)?;
        let initial = plan.ok_or_else(unavailable)?;
        let (rebuilt, seal) = request.build().map_err(|_| unavailable())?;
        if rebuilt != *command || seal.as_ref() != Some(initial) {
            return Err(unavailable());
        }
        if !request.bitmap_output_matches(initial) {
            return Err(unavailable());
        }
        let timing = initial.first_input_timing();
        if !initial.is_bitmap_carriage()
            || initial.video_stream().is_none_or(|(owner, _)| owner != 0)
            || timing.has_opaque_input_options()
            || timing.retained_stream_timestamps()
            || timing.output_seek().is_some()
            || timing
                .duration()
                .and_then(ersatzrs_ffmpeg::parsed_time_limit_microseconds)
                .is_none_or(|duration| duration <= 0)
        {
            return Err(unavailable());
        }
        let seek = timing
            .effective_input_seek_microseconds()
            .ok_or_else(unavailable)?;
        let deadline = deadline.min(Instant::now() + MAX_STARTUP);
        self.recheck_candidate(deadline).await?;
        let work = begin(&self.pool, &self.locker, deadline).await?;
        let scope = ClosingScope(LocalProbeCleanupScope::new());
        let initialized = scope
            .0
            .scope(self.initialize_bitmap_consumer(
                initial,
                request,
                seek,
                deadline,
                work.hold_work(),
            ))
            .await;
        scope.0.close();
        scope.0.wait_cleanup().await.map_err(|_| unavailable())?;
        initialized
    }
    async fn initialize_bitmap_consumer(
        &self,
        initial: &SourceTeletextInputPlan,
        request: &SourceTeletextConsumerRequest,
        seek: i64,
        deadline: Instant,
        hold: ersatzrs_core::locking::TeletextWorkHold,
    ) -> Result<InitializedSourceTeletextConsumer> {
        let artifact = self.artifact.clone().ok_or_else(unavailable)?;
        let mut av = Vec::new();
        for (owner, index) in initial
            .video_stream()
            .into_iter()
            .chain(initial.audio_stream())
        {
            if owner == 0 {
                let index = i32::try_from(index).map_err(|_| unavailable())?;
                if !av.contains(&index) {
                    av.push(index);
                }
            }
        }
        let extractor = FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
        let fragments = extractor
            .isolate_local_teletext_bitmap_artifact_for_consumer(
                artifact,
                &av,
                seek,
                deadline,
                hold.clone(),
            )
            .await
            .map_err(|_| unavailable())?;
        let playlist = extractor
            .prepare_local_teletext_bitmap_playlist(Arc::new(fragments), deadline, hold.clone())
            .await
            .map_err(|_| unavailable())?;
        let command = request.initialized_bitmap_command(initial, playlist.input())?;
        self.recheck_candidate(deadline).await?;
        self.receipt
            .check_source(deadline, hold)
            .await
            .map_err(|_| unavailable())?;
        if Instant::now() >= deadline {
            return Err(unavailable());
        }
        let bitmap = Arc::new(InitializedSourceTeletextBitmap {
            playlist,
            command: command.clone(),
            _hls_producer_directory: request.hls_producer_directory.clone(),
        });
        Ok(InitializedSourceTeletextConsumer {
            command,
            bitmap: Some(bitmap),
        })
    }
}

impl SourceTeletextConsumerRequest {
    fn bitmap_output_matches(&self, plan: &SourceTeletextInputPlan) -> bool {
        match self.request.state.output_format {
            ersatzrs_ffmpeg::OutputFormat::MpegTs => {
                plan.mpegts_segments().is_none() && self.hls_producer_directory.is_none()
            }
            ersatzrs_ffmpeg::OutputFormat::Hls => {
                let Some(directory) = &self.hls_producer_directory else {
                    return false;
                };
                let Some(segments) = plan.mpegts_segments() else {
                    return false;
                };
                let Some(name) = directory
                    .0
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                else {
                    return false;
                };
                segments.producer_playlist == format!("{name}/producer.m3u8")
                    && self
                        .request
                        .state
                        .hls
                        .as_ref()
                        .and_then(|hls| hls.mpegts_segments.as_ref())
                        == Some(segments)
            }
            _ => false,
        }
    }

    fn initialized_bitmap_command(
        &self,
        initial: &SourceTeletextInputPlan,
        playlist: &InputFile,
    ) -> Result<FfmpegCommand> {
        let expected = self.expected_bitmap.as_ref().ok_or_else(unavailable)?;
        if !initial.is_bitmap_carriage() || initial.caption_input() != expected {
            return Err(unavailable());
        }
        let mut request = self.request.as_ref().clone();
        let positions = request
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| (input == expected).then_some(index))
            .collect::<Vec<_>>();
        let [position] = positions.as_slice() else {
            return Err(unavailable());
        };
        let mut input = playlist.clone();
        input.input_instance = request
            .inputs
            .iter()
            .map(|input| input.input_instance)
            .max()
            .unwrap_or_default()
            .checked_add(1)
            .ok_or_else(unavailable)?;
        request.inputs[*position] = input.clone();
        let (command, plan) = build_pipeline_command_with_source_bitmap_carriage_plan(
            self.acceleration,
            request,
            &input,
        )
        .map_err(|_| unavailable())?;
        let plan = plan.ok_or_else(unavailable)?;
        if !plan.matches_command(&command)
            || !plan.is_bitmap_carriage()
            || plan.caption_input() != &input
            || plan.video_stream() != initial.video_stream()
            || plan.audio_stream() != initial.audio_stream()
            || plan.first_input_timing() != initial.first_input_timing()
            || plan.first_input_options() != initial.first_input_options()
            || plan.mpegts_segments() != initial.mpegts_segments()
            || plan.timing().input_seek().is_some()
            || plan.enabled_caption_input_streams() != [0]
        {
            return Err(unavailable());
        }
        Ok(command)
    }
}

#[cfg(test)]
#[path = "playback_source_teletext_bitmap_consumer_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "playback_source_teletext_bitmap_hybrid_tests.rs"]
mod hybrid_tests;

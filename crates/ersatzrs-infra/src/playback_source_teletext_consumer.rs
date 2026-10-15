//! Source-bound initialization of an independent native caption decoder.

use std::sync::Arc;

use ersatzrs_domain::HardwareAccelerationKind;
use ersatzrs_ffmpeg::{
    build_pipeline_command_with_source_teletext_plan, FfmpegCommand, PipelineRequest,
    SourceTeletextInputPlan, UnsupportedSource,
};

use super::{begin, unavailable, ClosingScope, PlaybackSource, MAX_STARTUP};
use crate::ffprobe::{
    teletext_consumer_alignment::align_independent_caption_origin,
    teletext_consumer_origin::NativeSourceConsumerOrigin, FfprobeExtractor, LocalProbeCleanupScope,
};
use ersatzrs_core::repositories::Result;
use tokio::time::Instant;

/// The completed caller request, before the generator normalizes physical inputs.
///
/// This is intent, not a trusted decoder policy. Initialization rebuilds and
/// compares the complete command before using its final selected stream owners.
#[derive(Debug, Clone)]
pub struct SourceTeletextConsumerRequest {
    acceleration: HardwareAccelerationKind,
    request: Arc<PipelineRequest>,
}

impl SourceTeletextConsumerRequest {
    #[must_use]
    pub fn new(acceleration: HardwareAccelerationKind, request: PipelineRequest) -> Self {
        Self {
            acceleration,
            request: Arc::new(request),
        }
    }

    /// Build the ordinary command, preserving existing source refusal behavior.
    /// # Errors
    /// Returns the command generator's unsupported-source error unchanged.
    pub fn build(
        &self,
    ) -> std::result::Result<(FfmpegCommand, Option<SourceTeletextInputPlan>), UnsupportedSource>
    {
        build_pipeline_command_with_source_teletext_plan(
            self.acceleration,
            self.request.as_ref().clone(),
        )
    }

    fn independent_command(
        &self,
        initial: &SourceTeletextInputPlan,
        offset_us: i64,
    ) -> Result<FfmpegCommand> {
        let mut request = self.request.as_ref().clone();
        let expected = initial.caption_input();
        let positions: Vec<_> = request
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(index, input)| (input == expected).then_some(index))
            .collect();
        let [position] = positions.as_slice() else {
            return Err(unavailable());
        };
        let instance = request
            .inputs
            .iter()
            .map(|input| input.input_instance)
            .max()
            .unwrap_or_default()
            .checked_add(1)
            .ok_or_else(unavailable)?;
        let mut independent = expected.clone();
        independent.input_instance = instance;
        independent.input_options.extend(
            ersatzrs_ffmpeg::standard_format_flags()
                .global_options()
                .iter()
                .cloned(),
        );
        // Input options are scoped to the next -i. Never borrow the first
        // input's flags, seek or heartbeat owner for this subtitle-only open.
        independent
            .input_options
            .extend(["-itsoffset".into(), signed_microseconds(offset_us)]);
        request.inputs[*position] = independent.clone();
        let (command, plan) =
            build_pipeline_command_with_source_teletext_plan(self.acceleration, request)
                .map_err(|_| unavailable())?;
        let plan = plan.ok_or_else(unavailable)?;
        if !plan.matches_command(&command)
            || plan.caption_input() != &independent
            || plan.caption_physical_input_index() == 0
            || plan.video_stream() != initial.video_stream()
            || plan.audio_stream() != initial.audio_stream()
            || plan
                .audio_stream()
                .is_some_and(|(physical, _)| physical == plan.caption_physical_input_index())
            || plan.enabled_caption_input_streams()
                != independent
                    .streams
                    .iter()
                    .map(|stream| stream.index)
                    .collect::<Vec<_>>()
            || plan.timing().input_seek().is_some()
            || plan.timing().duration() != initial.timing().duration()
            || plan.timing().output_seek() != initial.timing().output_seek()
            || plan.timing().output_ts_offset() != initial.timing().output_ts_offset()
        {
            return Err(unavailable());
        }
        Ok(command)
    }
}

impl PlaybackSource {
    /// Initialize the exact-page decoder independently of ordinary AV decoding.
    ///
    /// Uses the completed request's actual generated AV indices, the original
    /// held source, existing bounded native work and one absolute deadline.
    /// The fixed unseeked caption input reads initialization packets without
    /// admitting arbitrary caller-provided instances or opaque clock options.
    /// Final source bookends precede return; callers must transfer this source
    /// and their actual transcode reservation to the child without an await.
    ///
    /// # Errors
    /// Rejects changed commands/requests/source, unknown native clock policies,
    /// invalid inventory/alignment, deadline or uncertain cleanup. The existing
    /// ordinary-caption fallback remains responsible for preserving playback.
    pub async fn initialize_consumer_command(
        &self,
        command: &FfmpegCommand,
        plan: Option<&SourceTeletextInputPlan>,
        request: &SourceTeletextConsumerRequest,
        deadline: Instant,
    ) -> Result<FfmpegCommand> {
        self.check_consumer_command(command, plan)?;
        let initial = plan.ok_or_else(unavailable)?;
        let (rebuilt, rebuilt_plan) = request.build().map_err(|_| unavailable())?;
        if rebuilt != *command || rebuilt_plan.as_ref() != Some(initial) {
            return Err(unavailable());
        }
        let timing = initial.timing();
        if initial.caption_physical_input_index() != 0
            || initial
                .video_stream()
                .is_none_or(|(physical, _)| physical != 0)
            || timing.has_opaque_input_options()
            || timing.retained_stream_timestamps()
            || timing.output_seek().is_some()
        {
            return Err(unavailable());
        }
        let seek_us = timing
            .effective_input_seek_microseconds()
            .ok_or_else(unavailable)?;
        // Keep optional initialization state off ordinary streaming futures.
        Box::pin(self.initialize_selected_consumer(initial, request, seek_us, deadline)).await
    }

    async fn initialize_selected_consumer(
        &self,
        initial: &SourceTeletextInputPlan,
        request: &SourceTeletextConsumerRequest,
        seek_us: i64,
        deadline: Instant,
    ) -> Result<FfmpegCommand> {
        let mut av_indices = Vec::with_capacity(2);
        for (physical, index) in initial
            .video_stream()
            .into_iter()
            .chain(initial.audio_stream())
        {
            if physical == 0 {
                let index = i32::try_from(index).map_err(|_| unavailable())?;
                if !av_indices.contains(&index) {
                    av_indices.push(index);
                }
            }
        }
        let caption_index = self.receipt.request().stream_index;
        let deadline = deadline.min(Instant::now() + MAX_STARTUP);
        self.recheck_candidate(deadline).await?;
        let work = begin(&self.pool, &self.locker, deadline).await?;
        let scope = ClosingScope(LocalProbeCleanupScope::new());
        let extractor = FfprobeExtractor::new(ersatzrs_ffmpeg::ffprobe_path());
        let initialized = scope
            .0
            .scope(async {
                let av = extractor
                    .observe_local_teletext_consumer_origin(
                        &self.receipt,
                        &av_indices,
                        deadline,
                        work.hold_work(),
                    )
                    .await
                    .map_err(|_| unavailable())?;
                let caption = extractor
                    .observe_local_teletext_consumer_origin(
                        &self.receipt,
                        &[caption_index],
                        deadline,
                        work.hold_work(),
                    )
                    .await
                    .map_err(|_| unavailable())?;
                self.check_initialization_inventory(initial, &av, &caption)?;
                let alignment = align_independent_caption_origin(&av, &caption, seek_us)
                    .ok_or_else(unavailable)?;
                let command = request
                    .independent_command(initial, alignment.caption_input_offset_microseconds())?;
                self.recheck_candidate(deadline).await?;
                self.receipt
                    .check_source(deadline, work.hold_work())
                    .await
                    .map_err(|_| unavailable())?;
                Ok(command)
            })
            .await;
        scope.0.close();
        scope.0.wait_cleanup().await.map_err(|_| unavailable())?;
        if Instant::now() >= deadline {
            return Err(unavailable());
        }
        initialized
    }

    fn check_initialization_inventory(
        &self,
        plan: &SourceTeletextInputPlan,
        av: &NativeSourceConsumerOrigin,
        caption: &NativeSourceConsumerOrigin,
    ) -> Result<()> {
        let [caption] = caption.streams() else {
            return Err(unavailable());
        };
        let expected = self.receipt.request();
        if caption.index() != expected.stream_index
            || caption.pid() != expected.pid
            || caption.codec_type() != "subtitle"
        {
            return Err(unavailable());
        }
        for (selection, role) in [
            (plan.video_stream(), "video"),
            (plan.audio_stream(), "audio"),
        ] {
            if let Some((0, index)) = selection {
                let index = i32::try_from(index).map_err(|_| unavailable())?;
                if !av
                    .streams()
                    .iter()
                    .any(|stream| stream.index() == index && stream.codec_type() == role)
                {
                    return Err(unavailable());
                }
            }
        }
        Ok(())
    }
}

fn signed_microseconds(value: i64) -> String {
    let magnitude = value.unsigned_abs();
    format!(
        "{}{}.{:06}",
        if value < 0 { "-" } else { "" },
        magnitude / 1_000_000,
        magnitude % 1_000_000
    )
}

#[cfg(test)]
#[path = "playback_source_teletext_consumer_tests.rs"]
mod tests;

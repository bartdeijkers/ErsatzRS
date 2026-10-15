//! Generated native caption input observations, sealed to one complete command.

use std::{fmt, time::Duration};

use crate::{
    command::{
        input_prefix_options, physical_input_base_options, physical_input_options,
        selected_bitmap_burn_input,
    },
    physical_inputs::{physical_input_index, physical_inputs},
    FfmpegCommand, FfmpegState, InputFile, InputKind, PipelineStep,
};

/// Typed generated clock settings, distinct from native decoding observations.
///
/// Input seek applies only to the caption's physical input. Output seek,
/// duration and timestamp offset remain separate output operations. Opaque
/// supplied input options may override those settings; they are not parsed or
/// treated as a qualified native policy. No value proves actual seek landing,
/// bitmap onset, video-frame alignment or source-caption eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextInputTiming {
    input_seek: Option<Duration>,
    output_seek: Option<Duration>,
    duration: Option<Duration>,
    output_ts_offset: Option<Duration>,
    retained_stream_timestamps: bool,
    opaque_input_options: bool,
}

impl SourceTeletextInputTiming {
    pub(crate) fn from_state(state: &FfmpegState) -> Self {
        Self {
            input_seek: None,
            output_seek: state.output_start,
            duration: state.duration,
            output_ts_offset: state.output_ts_offset,
            retained_stream_timestamps: state.retained_stream_timestamps,
            opaque_input_options: false,
        }
    }

    /// Exact typed pre-input seek; absent for a separately opened caption input.
    /// Opaque supplied options can still contain an unqualified seek.
    #[must_use]
    pub const fn input_seek(self) -> Option<Duration> {
        self.input_seek
    }

    /// Parsed generated pre-input seek, or zero when none was generated.
    /// Original owner options make this unknown rather than assuming zero.
    /// Retained-clock mode still needs separate native timestamp semantics.
    #[must_use]
    pub fn effective_input_seek_microseconds(self) -> Option<i64> {
        if self.opaque_input_options {
            return None;
        }
        crate::parsed_time_limit_microseconds(self.input_seek.unwrap_or_default())
    }

    /// Exact output-side seek, independent of pre-input seeking.
    #[must_use]
    pub const fn output_seek(self) -> Option<Duration> {
        self.output_seek
    }

    /// Exact typed output duration, before native argument parsing.
    #[must_use]
    pub const fn duration(self) -> Option<Duration> {
        self.duration
    }

    /// Output transport timestamp offset, not a source seek.
    #[must_use]
    pub const fn output_ts_offset(self) -> Option<Duration> {
        self.output_ts_offset
    }

    /// Whether the generated command retains source timestamps with `copyts`.
    #[must_use]
    pub const fn retained_stream_timestamps(self) -> bool {
        self.retained_stream_timestamps
    }

    /// Original options actually emitted for this owner may alter native timing.
    /// Options on a deduplicated caption input are not emitted and do not count.
    #[must_use]
    pub const fn has_opaque_input_options(self) -> bool {
        self.opaque_input_options
    }
}

/// Final generated physical inputs for the first selected source bitmap Burn.
///
/// The plan is built after pipeline normalization and sealed to the complete
/// executable, environment, arguments and generated output hints. It records
/// input ownership/options, without interpreting arbitrary custom options as
/// a supported consumer policy. Source freshness, enabled native inventory,
/// seek/decoder timing and actual-child lifetime remain the caller's duties.
#[derive(Clone, PartialEq, Eq)]
pub struct SourceTeletextInputPlan {
    command: FfmpegCommand,
    caption: InputFile,
    caption_index: usize,
    video: Option<(usize, usize)>,
    audio: Option<(usize, usize)>,
    physical_options: Vec<String>,
    timing: SourceTeletextInputTiming,
}

impl fmt::Debug for SourceTeletextInputPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceTeletextInputPlan")
            .field("caption_physical_input_index", &self.caption_index)
            .field("video_stream", &self.video)
            .field("audio_stream", &self.audio)
            .finish_non_exhaustive()
    }
}

impl SourceTeletextInputPlan {
    pub(crate) fn from_generated(
        command: &FfmpegCommand,
        inputs: &[InputFile],
        original_inputs: &[InputFile],
        steps: &[PipelineStep],
        intel: bool,
        mut timing: SourceTeletextInputTiming,
    ) -> Option<Self> {
        let caption = selected_bitmap_burn_input(inputs)?;
        caption.streams.first()?.source_teletext_page?;
        let owners = physical_inputs(inputs, intel);
        let caption_index = physical_input_index(&owners, caption, false)?;
        let selected_stream = |kind, last| {
            let input = inputs.iter().find(|input| input.kind == kind)?;
            Some((
                physical_input_index(&owners, input, last)?,
                input.streams.first()?.index,
            ))
        };
        let mut physical_options = if caption_index == 0 {
            input_prefix_options(steps)
        } else {
            Vec::new()
        };
        physical_options.extend(physical_input_options(
            inputs,
            &owners,
            caption_index,
            intel,
        ));
        // The generator emits StreamSeek before the first physical -i only.
        // A later caption owner must not borrow the video's pre-input seek.
        timing.input_seek = (caption_index == 0)
            .then(|| {
                steps.iter().find_map(|step| match step {
                    PipelineStep::StreamSeek { position } => Some(*position),
                    _ => None,
                })
            })
            .flatten();
        // Some output modes deliberately suppress the requested TS offset.
        if !steps.iter().any(|step| {
            matches!(
                step,
                PipelineStep::Generic {
                    name: "output_ts_offset",
                    ..
                }
            )
        }) {
            timing.output_ts_offset = None;
        }
        let owner = owners[caption_index];
        timing.opaque_input_options = original_inputs
            .iter()
            .find(|input| input.kind == owner.kind && input.source_key() == owner.source_key())
            .is_none_or(|original| {
                !physical_input_base_options(original_inputs, original, intel).is_empty()
            });
        Some(Self {
            command: command.clone(),
            caption: caption.clone(),
            caption_index,
            video: selected_stream(InputKind::Video, false),
            audio: selected_stream(InputKind::Audio, true),
            physical_options,
            timing,
        })
    }

    /// Require the entire generated command to remain unchanged.
    #[must_use]
    pub fn matches_command(&self, command: &FfmpegCommand) -> bool {
        &self.command == command
    }

    /// The final logical caption input, including its explicit page and stream.
    #[must_use]
    pub const fn caption_input(&self) -> &InputFile {
        &self.caption
    }

    #[must_use]
    pub const fn caption_physical_input_index(&self) -> usize {
        self.caption_index
    }

    /// Selected video's physical input and final global stream index.
    #[must_use]
    pub const fn video_stream(&self) -> Option<(usize, usize)> {
        self.video
    }

    /// Selected audio uses the last physical owner, including Intel reopens.
    #[must_use]
    pub const fn audio_stream(&self) -> Option<(usize, usize)> {
        self.audio
    }

    /// Distinct selected video/audio indices sharing the caption's physical
    /// input, followed by its caption index. Other logical mappings are outside
    /// this observation; no stream is enabled from default source metadata.
    #[must_use]
    pub fn enabled_caption_input_streams(&self) -> Vec<usize> {
        let mut enabled = Vec::with_capacity(3);
        for (physical, stream) in self.video.into_iter().chain(self.audio) {
            if physical == self.caption_index && !enabled.contains(&stream) {
                enabled.push(stream);
            }
        }
        if let Some(stream) = self.caption.streams.first() {
            if !enabled.contains(&stream.index) {
                enabled.push(stream.index);
            }
        }
        enabled
    }

    /// Actual generated options immediately before this physical `-i`, plus
    /// the emitted command prefix when this is the first physical input.
    /// This is a typed generator observation, not inferred argv semantics.
    #[must_use]
    pub fn physical_input_options(&self) -> &[String] {
        &self.physical_options
    }

    /// Clock settings from this sealed generation, not inferred from argv.
    #[must_use]
    pub const fn timing(&self) -> SourceTeletextInputTiming {
        self.timing
    }
}

#[cfg(test)]
mod tests;

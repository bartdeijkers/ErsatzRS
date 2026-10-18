//! Generated native caption input observations, sealed to one complete command.

use std::{fmt, time::Duration};

use crate::{
    command::{
        input_prefix_options, physical_input_base_options, physical_input_options,
        selected_bitmap_burn_input,
    },
    physical_inputs::{physical_input_index, physical_inputs},
    FfmpegCommand, FfmpegState, InputFile, InputKind, PipelineStep, StreamKind, SubtitleMethod,
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

/// Final generated physical inputs for an explicit source caption consumer.
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
    first_input_options: Vec<String>,
    first_input_timing: SourceTeletextInputTiming,
    bitmap_carriage: bool,
    mpegts_segments: Option<crate::HlsMpegTsSegments>,
}

/// The same normalized inputs and steps used to generate the sealed command.
pub(crate) struct GeneratedSourceTeletextPlan<'a> {
    pub command: &'a FfmpegCommand,
    pub inputs: &'a [InputFile],
    pub original_inputs: &'a [InputFile],
    pub steps: &'a [PipelineStep],
    pub intel: bool,
    pub timing: SourceTeletextInputTiming,
    pub mpegts_segments: Option<&'a crate::HlsMpegTsSegments>,
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
        generated: &GeneratedSourceTeletextPlan<'_>,
        expected_bitmap_copy: Option<&InputFile>,
    ) -> Option<Self> {
        let inputs = generated.inputs;
        let caption = if let Some(expected) = expected_bitmap_copy {
            let caption = selected_bitmap_copy(inputs, expected)?;
            selected_bitmap_copy(generated.original_inputs, expected)?;
            let mut encoders = generated.steps.iter().filter_map(|step| match step {
                PipelineStep::Encoder {
                    name,
                    kind: StreamKind::Subtitle,
                    ..
                } => Some(name),
                _ => None,
            });
            if encoders.next().is_none_or(|name| name != "copy") || encoders.next().is_some() {
                return None;
            }
            caption
        } else {
            let caption = selected_bitmap_burn_input(inputs)?;
            caption.streams.first()?.source_teletext_page?;
            caption
        };
        let owners = physical_inputs(inputs, generated.intel);
        let caption_index = physical_input_index(&owners, caption, false)?;
        let selected_stream = |kind, last| {
            let input = inputs.iter().find(|input| input.kind == kind)?;
            Some((
                physical_input_index(&owners, input, last)?,
                input.streams.first()?.index,
            ))
        };
        let video = selected_stream(InputKind::Video, false);
        let audio = selected_stream(InputKind::Audio, true);
        if expected_bitmap_copy.is_some()
            && (video.is_none_or(|(physical, _)| physical != 0)
                || caption_index == 0
                || audio.is_some_and(|(physical, _)| physical == caption_index)
                || mapped_caption_count(inputs, &owners, caption_index) != 1)
        {
            return None;
        }
        Some(Self {
            command: generated.command.clone(),
            caption: caption.clone(),
            caption_index,
            video,
            audio,
            physical_options: generated.options(&owners, caption_index),
            timing: generated.timing(&owners, caption_index),
            first_input_options: generated.options(&owners, 0),
            first_input_timing: generated.timing(&owners, 0),
            bitmap_carriage: expected_bitmap_copy.is_some(),
            mpegts_segments: generated.mpegts_segments.cloned(),
        })
    }

    /// Private segment-muxer intent sealed to this explicit bitmap command.
    /// This does not certify publication, segment ownership or source readiness.
    #[must_use]
    pub const fn mpegts_segments(&self) -> Option<&crate::HlsMpegTsSegments> {
        self.mpegts_segments.as_ref()
    }

    /// Require the entire generated command to remain unchanged.
    #[must_use]
    pub fn matches_command(&self, command: &FfmpegCommand) -> bool {
        &self.command == command
    }

    /// The final logical caption input with its exact stream/page metadata.
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

    /// Actual generated options for physical input zero, including its prefix.
    /// These remain distinct from an independent caption input's options.
    #[must_use]
    pub fn first_input_options(&self) -> &[String] {
        &self.first_input_options
    }

    /// Actual first-input seek/options policy, not the independent caption's seek.
    #[must_use]
    pub const fn first_input_timing(&self) -> SourceTeletextInputTiming {
        self.first_input_timing
    }

    /// Whether this plan used the explicit expected DVB-bitmap Copy builder.
    /// This generated observation grants no source or carriage authorization.
    #[must_use]
    pub const fn is_bitmap_carriage(&self) -> bool {
        self.bitmap_carriage
    }
}

impl GeneratedSourceTeletextPlan<'_> {
    fn options(&self, owners: &[&InputFile], index: usize) -> Vec<String> {
        let mut options = if index == 0 {
            input_prefix_options(self.steps)
        } else {
            Vec::new()
        };
        options.extend(physical_input_options(
            self.inputs,
            owners,
            index,
            self.intel,
        ));
        options
    }

    fn timing(&self, owners: &[&InputFile], index: usize) -> SourceTeletextInputTiming {
        let mut timing = self.timing;
        timing.input_seek = (index == 0)
            .then(|| {
                self.steps.iter().find_map(|step| match step {
                    PipelineStep::StreamSeek { position } => Some(*position),
                    _ => None,
                })
            })
            .flatten();
        if !self.steps.iter().any(|step| {
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
        let owner = owners[index];
        timing.opaque_input_options = self
            .original_inputs
            .iter()
            .find(|input| input.kind == owner.kind && input.source_key() == owner.source_key())
            .is_none_or(|original| {
                !physical_input_base_options(self.original_inputs, original, self.intel).is_empty()
            });
        timing
    }
}

fn selected_bitmap_copy<'a>(
    inputs: &'a [InputFile],
    expected: &InputFile,
) -> Option<&'a InputFile> {
    if expected.kind != InputKind::Subtitle
        || expected.subtitle_method != Some(SubtitleMethod::Copy)
        || !expected.is_image_based_subtitle
        || expected.subtitle_text_format.is_some()
        || !expected.filter_steps.is_empty()
        || expected.streams.len() != 1
        || expected.streams[0].index != 0
        || expected.streams[0].source_teletext_page.is_some()
    {
        return None;
    }
    let mut matches = inputs.iter().filter(|input| *input == expected);
    let selected = matches.next()?;
    matches.next().is_none().then_some(selected)
}

fn mapped_caption_count(inputs: &[InputFile], owners: &[&InputFile], index: usize) -> usize {
    inputs
        .iter()
        .filter(|input| {
            input.kind == InputKind::Subtitle
                && matches!(
                    input.subtitle_method,
                    Some(SubtitleMethod::Copy | SubtitleMethod::Convert)
                )
                && physical_input_index(owners, input, false) == Some(index)
        })
        .flat_map(|input| &input.streams)
        .filter(|stream| stream.index == 0)
        .count()
}

#[cfg(test)]
mod tests;

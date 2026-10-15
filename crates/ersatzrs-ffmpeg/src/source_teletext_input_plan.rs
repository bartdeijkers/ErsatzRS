//! Generated native caption input observations, sealed to one complete command.

use std::fmt;

use crate::{
    command::{input_prefix_options, physical_input_options, selected_bitmap_burn_input},
    physical_inputs::{physical_input_index, physical_inputs},
    FfmpegCommand, InputFile, InputKind, PipelineStep,
};

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
        steps: &[PipelineStep],
        intel: bool,
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
        Some(Self {
            command: command.clone(),
            caption: caption.clone(),
            caption_index,
            video: selected_stream(InputKind::Video, false),
            audio: selected_stream(InputKind::Audio, true),
            physical_options,
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
}

#[cfg(test)]
mod tests;

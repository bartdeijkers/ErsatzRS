//! Shared physical input ownership for argv and filter-label generation.

use crate::{InputFile, InputKind};

pub(crate) fn physical_inputs(inputs: &[InputFile], intel: bool) -> Vec<&InputFile> {
    let mut owners: Vec<&InputFile> = Vec::new();
    for kind in [
        InputKind::Video,
        InputKind::Audio,
        InputKind::Watermark,
        InputKind::GraphicsImage,
        InputKind::Concat,
        InputKind::Graphics,
        InputKind::Subtitle,
    ] {
        for input in inputs.iter().filter(|input| input.kind == kind) {
            if kind == InputKind::Subtitle && !input.is_image_based_subtitle {
                continue;
            }
            let existing = owners
                .iter()
                .any(|owner| owner.source_key() == input.source_key());
            let include = match kind {
                InputKind::Video | InputKind::Concat | InputKind::Graphics => true,
                InputKind::Audio => !existing || intel,
                _ => !existing,
            };
            if include {
                owners.push(input);
            }
        }
    }
    owners
}

pub(crate) fn physical_input_index(
    owners: &[&InputFile],
    input: &InputFile,
    last: bool,
) -> Option<usize> {
    if last {
        owners
            .iter()
            .rposition(|owner| owner.source_key() == input.source_key())
    } else {
        owners
            .iter()
            .position(|owner| owner.source_key() == input.source_key())
    }
}

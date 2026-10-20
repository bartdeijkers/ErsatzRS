//! Authenticate a complete Base-only prefix; never infer intent from empty ordinals.
use super::{operands, Page};
use ersatzrs_domain::TeletextPageAddress;
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextMainCompositionSource};

// A complete exact Base prefix is a bounded correspondence, not a general
// reverse compiler. Definitions, defaults, carriers, unknown and interleaved
// sources keep their existing refusal paths. Appended Main is rebuilt afresh.
fn prefix(service: &EncodedTeletextAuthoredService, owner: TeletextPageAddress) -> Option<usize> {
    let program = operands::compiled_program(service, owner)?;
    if !program.definition_positions().is_empty() || program.main_triplet_count() > 208 {
        return None;
    }
    let mut cursor = 0;
    let mut prefix = 0;
    let mut authored = false;
    for position in program.command_positions() {
        if position.source().object.is_some()
            || position.first_word() != cursor
            || position.semantic_word() < cursor
            || position.semantic_word() >= program.main_triplet_count()
        {
            return None;
        }
        cursor = position.semantic_word().checked_add(1)?;
        match position.original_main_source()? {
            TeletextMainCompositionSource::BaseCell { .. } if !authored => prefix = cursor,
            TeletextMainCompositionSource::OriginalPage { .. } => authored = true,
            _ => return None,
        }
    }
    (prefix > 0 && cursor == program.main_triplet_count()).then_some(prefix)
}

fn word(page: &Page<'_>, index: usize) -> Option<u32> {
    page.packets[0]
        .get(index / 13)?
        .map(|words| words[index % 13])
}

fn unchanged_prefix(baseline: &Page<'_>, edited: &Page<'_>, count: usize) -> bool {
    (0..count)
        .all(|index| word(baseline, index).is_some_and(|value| word(edited, index) == Some(value)))
}

pub(super) fn creation(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    baseline: &Page<'_>,
    edited: &Page<'_>,
) -> Option<usize> {
    let count = prefix(service, owner)?;
    let program = operands::compiled_program(service, owner)?;
    (count == program.main_triplet_count()
        && unchanged_prefix(baseline, edited, count)
        && word(edited, count).is_some_and(|value| value != super::FINAL))
    .then_some(count)
}

pub(super) fn removal(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    baseline: &Page<'_>,
    edited: &Page<'_>,
) -> bool {
    let Some(count) = prefix(service, owner) else {
        return false;
    };
    // Complete canonical termination/padding and exact fresh semantic-None
    // encoding are checked by the existing reconciliation and native gates.
    unchanged_prefix(baseline, edited, count)
        && word(baseline, count).is_some_and(|value| value != super::FINAL)
        && word(edited, count) == Some(super::FINAL)
}

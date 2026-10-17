//! Bounded Local glyph-program edits over authenticated native X26 spans.
use super::{Location, NativeTeletextEditorialTtiFailure, Page, Result, TeletextEditorialChanges};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextPageAddress,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use std::time::Instant;
mod alignment;
mod intents;
mod program;
mod qualify;
mod raw;
mod stream;
mod style;
pub(super) use qualify::check as qualify_recompiled;
fn fail(location: Location) -> super::NativeTeletextEditorialTtiError {
    super::NativeTeletextEditorialTtiError {
        row: Some(26),
        ..location.error(NativeTeletextEditorialTtiFailure::Structure)
    }
}
fn glyph_family(
    command: &TeletextEnhancementCommand,
) -> Option<ersatzrs_domain::TeletextColumnGlyphFamily> {
    match command {
        TeletextEnhancementCommand::AtColumn { operation, .. } => operation.glyph_family(),
        _ => None,
    }
}
pub(super) fn reconcile(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    baseline: &Page<'_>,
    edited: &Page<'_>,
    location: Location,
    deadline: Instant,
) -> Result<TeletextEditorialChanges> {
    location.check_deadline(deadline)?;
    super::operands::check_packets(service, owner, baseline, location, deadline)?;
    let native = super::operands::compiled_program(service, owner).ok_or_else(|| fail(location))?;
    if native.command_positions().len() > 208 || native.definition_positions().len() > 208 {
        return Err(fail(location));
    }
    let old = stream::Image::read(baseline, location, deadline)?;
    let new = stream::Image::read(edited, location, deadline)?;
    if old.markers.len() != new.markers.len()
        || old.markers.len() != native.definition_positions().len()
        || old.markers.first() != new.markers.first()
    {
        return Err(fail(location));
    }
    let mut aligned = old.words.clone();
    main_words(
        service,
        owner,
        native,
        (&old, &new),
        &mut aligned,
        location,
        deadline,
    )?;
    let mut candidates = Vec::new();
    for (index, position) in native.definition_positions().iter().enumerate() {
        location.check_deadline(deadline)?;
        if old.markers[index] != usize::from(position.absolute_index())
            || stream::marker(old.words[old.markers[index]], new.markers[index], location)?
                != new.words[new.markers[index]]
        {
            return Err(fail(location));
        }
        if let Some(definition) = service
            .declared()
            .semantic()
            .objects()
            .definition(position.target())
        {
            if definition.identity.owner() != Some(owner)
                || definition.object_type != position.object_type()
            {
                return Err(fail(location));
            }
            candidates.push(program::parse(
                definition,
                native,
                (&old, &new),
                index,
                &mut aligned,
                location,
                deadline,
            )?);
        } else {
            let (a, b) = (old.body(index), new.body(index));
            if a.len() != b.len() {
                return Err(fail(location));
            }
            for (a, b) in a.iter().zip(b) {
                if stream::relocate(*a, &old, &new, location)? != *b {
                    return Err(fail(location));
                }
            }
        }
    }
    let packets = packets(&aligned);
    let mut aligned_page = Page::default();
    aligned_page.packets[0] = packets.each_ref().map(Option::as_ref);
    let mut result =
        super::operands::reconcile(service, owner, baseline, &aligned_page, location, deadline)?;
    for mut candidate in candidates {
        patch_anchors(&mut candidate, &result, location)?;
        intents::append(candidate, &mut result, location)?;
    }
    if result.object_programs.is_empty() {
        return Err(fail(location));
    }
    Ok(result)
}
fn main_words(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    native: &ersatzrs_ffmpeg::teletext::EncodedTeletextLocalObjectProgram,
    images: (&stream::Image, &stream::Image),
    aligned: &mut [u32],
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let (old, new) = images;
    for (word, aligned_word) in aligned.iter_mut().enumerate().take(old.markers[0]) {
        location.check_deadline(deadline)?;
        if stream::relocate(old.words[word], old, new, location)? == new.words[word] {
            continue;
        }
        let mut positions = native
            .command_positions()
            .iter()
            .filter(|position| position.first_word() <= word && position.semantic_word() >= word);
        let position = positions.next().ok_or_else(|| fail(location))?;
        if positions.next().is_some() || position.source().object.is_some() {
            return Err(fail(location));
        }
        let command = position
            .original_local_command()
            .and_then(|command| {
                service
                    .declared()
                    .semantic()
                    .page_enhancements()
                    .get(&owner)
                    .and_then(|page| page.local_program.as_ref())
                    .and_then(|program| program.commands().get(command))
            })
            .ok_or_else(|| fail(location))?;
        let TeletextEnhancementCommand::AtColumn { column, operation } = command else {
            return Err(fail(location));
        };
        let permitted = if matches!(operation, TeletextColumnOperation::Drcs { .. }) {
            let actual = new.words[word];
            actual <= 0x3ffff
                && if word == position.semantic_word() {
                    actual & 0x7ff == u32::from(*column) | (13 << 6) && (actual >> 11) & 63 < 48
                } else {
                    actual & 0x7ff == 0x28 | (0x18 << 6) && (actual >> 11) & 0x30 == 0x10
                }
        } else {
            super::operands::operand(*column, *operation, old.words[word], new.words[word])
                .is_some()
        };
        if !permitted {
            return Err(fail(location));
        }
        *aligned_word = new.words[word];
    }
    Ok(())
}
fn patch_anchors(
    candidate: &mut program::Candidate,
    changes: &TeletextEditorialChanges,
    location: Location,
) -> Result<()> {
    let mut expected = candidate.expected.commands().to_vec();
    for operand in changes
        .object_operands
        .iter()
        .filter(|operand| operand.identity() == candidate.identity)
    {
        let ordinal = operand.command();
        if glyph_family(expected.get(ordinal).ok_or_else(|| fail(location))?).is_some() {
            return Err(fail(location));
        }
        let new = candidate
            .retained
            .iter()
            .position(|source| *source == Some(ordinal))
            .ok_or_else(|| fail(location))?;
        let command = TeletextEnhancementCommand::AtColumn {
            column: operand.column(),
            operation: operand.replacement(),
        };
        expected[ordinal] = command;
        candidate.replacement[new] = command;
    }
    candidate.expected = TeletextEnhancementProgram::new(expected).map_err(|_| fail(location))?;
    Ok(())
}
fn packets(words: &[u32]) -> super::operands::PacketWords {
    let mut result = [None; 16];
    for (target, chunk) in result.iter_mut().zip(words.chunks_exact(13)) {
        let mut value = [0; 13];
        value.copy_from_slice(chunk);
        *target = Some(value);
    }
    result
}

//! Final emitted-word correspondence, never reverse inference from pixels.

use super::{
    Location, NativeTeletextEditorialTtiError, NativeTeletextEditorialTtiFailure, Page, Result,
};
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextLocalProgramOperandOverlay, TeletextPageAddress,
};
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_display, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextCompiledCommandPosition, TeletextEncodedDisplayRecord,
};
use std::time::Instant;

pub(super) fn reconcile(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    baseline: &Page<'_>,
    edited: &Page<'_>,
    location: Location,
    deadline: Instant,
) -> Result<Vec<TeletextLocalProgramOperandOverlay>> {
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(value)
                if value.page().address() == owner =>
            {
                Some((value.page(), Some(value.normalized().encoded())))
            }
            EncodedTeletextAuthoredPage::NativeDisplay(value)
                if value.encoded().page().address() == owner =>
            {
                Some((value.encoded().page(), None))
            }
            _ => None,
        })
        .ok_or_else(|| location.error(NativeTeletextEditorialTtiFailure::Mapping))?;
    // An uploaded packet never supplies original intent.
    check_packets(service, owner, baseline, location, deadline)?;
    let program = service
        .declared()
        .semantic()
        .page_enhancements()
        .get(&owner)
        .and_then(|page| page.local_program.as_ref());
    let mut result = Vec::new();
    for designation in 0_u8..16 {
        let index = usize::from(designation);
        location.check_deadline(deadline)?;
        let (Some(before), Some(after)) = (baseline.packets[0][index], edited.packets[0][index])
        else {
            continue; // Packet presence has already been checked by protected().
        };
        for slot in 0..13 {
            if before[slot] == after[slot] {
                continue;
            }
            location.check_deadline(deadline)?;
            let error = || NativeTeletextEditorialTtiError {
                row: Some(26),
                column: Some(slot),
                designation: Some(designation),
                ..location.error(NativeTeletextEditorialTtiFailure::Packet)
            };
            let command = page
                .1
                .and_then(|encoded| {
                    encoded
                        .command_positions()
                        .iter()
                        .find(|position| position.semantic_word() == index * 13 + slot)
                })
                .and_then(TeletextCompiledCommandPosition::original_local_command)
                .ok_or_else(error)?;
            let Some(TeletextEnhancementCommand::AtColumn { column, operation }) =
                program.and_then(|program| program.commands().get(command))
            else {
                return Err(error());
            };
            let replacement =
                operand(*column, *operation, before[slot], after[slot]).ok_or_else(error)?;
            result.push(
                TeletextLocalProgramOperandOverlay::new(
                    owner,
                    command,
                    *column,
                    *operation,
                    replacement,
                )
                .map_err(|_| error())?,
            );
        }
    }
    Ok(result)
}

pub(super) fn check_packets(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    expected: &Page<'_>,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let actual = packet_words(service, owner, location, deadline)?;
    for designation in 0_u8..16 {
        let index = usize::from(designation);
        let before = expected.packets[0][index];
        let after = actual[index].as_ref();
        location.check_deadline(deadline)?;
        if before != after {
            return Err(NativeTeletextEditorialTtiError {
                row: Some(26),
                designation: Some(designation),
                column: before
                    .zip(after)
                    .and_then(|(old, new)| old.iter().zip(new).position(|(a, b)| a != b)),
                ..location.error(NativeTeletextEditorialTtiFailure::Packet)
            });
        }
    }
    Ok(())
}

fn located_word(location: Location, word: usize) -> NativeTeletextEditorialTtiError {
    NativeTeletextEditorialTtiError {
        row: Some(26),
        designation: u8::try_from(word / 13).ok(),
        column: Some(word % 13),
        ..location.error(NativeTeletextEditorialTtiFailure::Packet)
    }
}

fn packet_words(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    location: Location,
    deadline: Instant,
) -> Result<[Option<[u32; 13]>; 16]> {
    location.check_deadline(deadline)?;
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == owner =>
            {
                Some(page.page())
            }
            EncodedTeletextAuthoredPage::NativeDisplay(page)
                if page.encoded().page().address() == owner =>
            {
                Some(page.encoded().page())
            }
            _ => None,
        })
        .ok_or_else(|| location.error(NativeTeletextEditorialTtiFailure::Mapping))?;
    let projection = project_encoded_teletext_display(page, service.unit_count())
        .map_err(|_| location.error(NativeTeletextEditorialTtiFailure::Mapping))?;
    let mut result = [None; 16];
    for record in projection.records {
        location.check_deadline(deadline)?;
        if let TeletextEncodedDisplayRecord::Triplets {
            row: 26,
            designation,
            words,
        } = record
        {
            if result[usize::from(designation)].replace(words).is_some() {
                return Err(located_word(location, usize::from(designation) * 13));
            }
        }
    }
    Ok(result)
}

fn command_word(
    service: &EncodedTeletextAuthoredService,
    change: &TeletextLocalProgramOperandOverlay,
    expected: TeletextColumnOperation,
    location: Location,
    deadline: Instant,
) -> Result<usize> {
    let owner = change.owner();
    let command = service
        .declared()
        .semantic()
        .page_enhancements()
        .get(&owner)
        .and_then(|page| page.local_program.as_ref())
        .and_then(|program| program.commands().get(change.command()));
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == owner =>
            {
                Some(page)
            }
            _ => None,
        })
        .ok_or_else(|| location.error(NativeTeletextEditorialTtiFailure::Mapping))?;
    let mut found = None;
    for position in page.normalized().encoded().command_positions() {
        location.check_deadline(deadline)?;
        if position.original_local_command() == Some(change.command()) {
            let word = position.semantic_word();
            if word >= 16 * 13 || found.replace(word).is_some() {
                return Err(located_word(location, word));
            }
        }
    }
    let word = found.ok_or_else(|| location.error(NativeTeletextEditorialTtiFailure::Packet))?;
    if command
        != Some(&TeletextEnhancementCommand::AtColumn {
            column: change.column(),
            operation: expected,
        })
    {
        return Err(located_word(location, word));
    }
    Ok(word)
}

/// The complete uploaded X26 is checked against its private original. Only
/// reconciled existing operands may differ; their final words are found again
/// through unique retained-command provenance, never their old packet offsets.
pub(super) fn check_recompiled_packets(
    baseline: &EncodedTeletextAuthoredService,
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    uploaded: &Page<'_>,
    changes: &[TeletextLocalProgramOperandOverlay],
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let before = packet_words(baseline, owner, location, deadline)?;
    let after = packet_words(service, owner, location, deadline)?;
    // There are at most sixteen packets of thirteen words on one display.
    if changes
        .iter()
        .filter(|change| change.owner() == owner)
        .count()
        > 16 * 13
    {
        return Err(location.error(NativeTeletextEditorialTtiFailure::Packet));
    }
    let mut allowed = [None; 16 * 13];
    for change in changes.iter().filter(|change| change.owner() == owner) {
        location.check_deadline(deadline)?;
        let old_word = command_word(baseline, change, change.expected(), location, deadline)?;
        let original = before[old_word / 13]
            .map(|words| words[old_word % 13])
            .ok_or_else(|| located_word(location, old_word))?;
        let edited = uploaded.packets[0][old_word / 13]
            .map(|words| words[old_word % 13])
            .ok_or_else(|| located_word(location, old_word))?;
        if original == edited
            || operand(change.column(), change.expected(), original, edited)
                != Some(change.replacement())
            || allowed[old_word].replace(edited).is_some()
        {
            return Err(located_word(location, old_word));
        }
        let final_word = command_word(service, change, change.replacement(), location, deadline)?;
        if after[final_word / 13].map(|words| words[final_word % 13]) != Some(edited) {
            return Err(located_word(location, final_word));
        }
    }
    for designation in 0..16 {
        location.check_deadline(deadline)?;
        match (before[designation], uploaded.packets[0][designation]) {
            (None, None) => {}
            (Some(original), Some(edited)) => {
                for slot in 0..13 {
                    if original[slot] != edited[slot]
                        && allowed[designation * 13 + slot] != Some(edited[slot])
                    {
                        return Err(located_word(location, designation * 13 + slot));
                    }
                }
            }
            _ => return Err(located_word(location, designation * 13)),
        }
    }
    Ok(())
}

/// Table 28's exact address/mode/data fields; reserved bits stay refused. Only
/// existing same-family operands are reconstructed, never generated carriers,
/// object bodies, terminators, compatibility selectors or new commands.
fn operand(
    column: u8,
    expected: TeletextColumnOperation,
    before: u32,
    after: u32,
) -> Option<TeletextColumnOperation> {
    use TeletextColumnOperation::{Attributes, Background, Foreground};
    let (mode, data) = match expected {
        Foreground { colour } => (0, u32::from(colour.get())),
        Background { colour } => (3, u32::from(colour.get())),
        Attributes {
            double_height,
            double_width,
            boxed,
            conceal,
            invert,
            underline_or_separated_mosaics,
        } => (
            12,
            u32::from(double_height)
                | (u32::from(boxed) << 1)
                | (u32::from(conceal) << 2)
                | (u32::from(invert) << 4)
                | (u32::from(underline_or_separated_mosaics) << 5)
                | (u32::from(double_width) << 6),
        ),
        _ => return None,
    };
    if before != u32::from(column) | (mode << 6) | (data << 11)
        || after & 0x7ff != before & 0x7ff
        || after > 0x3ffff
    {
        return None;
    }
    let data = u8::try_from(after >> 11).ok()?;
    match expected {
        Foreground { .. } => Some(Foreground {
            colour: TeletextColourReference::new(data).ok()?,
        }),
        Background { .. } => Some(Background {
            colour: TeletextColourReference::new(data).ok()?,
        }),
        Attributes { .. } if data & 8 == 0 => Some(Attributes {
            double_height: data & 1 != 0,
            boxed: data & 2 != 0,
            conceal: data & 4 != 0,
            invert: data & 16 != 0,
            underline_or_separated_mosaics: data & 32 != 0,
            double_width: data & 64 != 0,
        }),
        _ => None,
    }
}

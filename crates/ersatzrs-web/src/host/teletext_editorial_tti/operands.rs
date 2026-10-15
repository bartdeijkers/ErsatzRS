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
    let projected = project_encoded_teletext_display(page, service.unit_count())
        .map_err(|_| location.error(NativeTeletextEditorialTtiFailure::Mapping))?;
    let mut actual = [None; 16];
    for record in &projected.records {
        if let TeletextEncodedDisplayRecord::Triplets {
            row: 26,
            designation,
            words,
        } = record
        {
            actual[usize::from(*designation)] = Some(words);
        }
    }
    for designation in 0_u8..16 {
        let index = usize::from(designation);
        let before = expected.packets[0][index];
        let after = actual[index];
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

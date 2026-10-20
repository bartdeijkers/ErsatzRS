//! Source-bound Main scalars with compiler-owned Base charset correspondence.
//!
//! Generated words never become authored intent. The caller must reconstruct
//! once and require exact complete uploaded packets before accepting this parse.
use super::{
    check_packets, compiled_program, located_word, operand, EncodedTeletextAuthoredService,
    Location, Page, Result, TeletextColumnOperation, TeletextEditorialChanges,
    TeletextLocalProgramOperandOverlay, TeletextPageAddress,
};
use std::time::Instant;

mod alignment;
mod model;
#[cfg(test)]
mod tests;

const CAPACITY: usize = 16 * 13;
const FINAL: u32 = 63 | (31 << 6) | (7 << 11);

enum Atom<'a> {
    Fixed(&'a [u32]),
    Scalar {
        command: usize,
        column: u8,
        expected: TeletextColumnOperation,
        before: u32,
    },
    Glyph {
        words: &'a [u32],
        original: &'a [u32],
        column: u8,
    },
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
    check_packets(service, owner, baseline, location, deadline)?;
    let old = words(baseline, location, deadline)?;
    let uploaded = words(edited, location, deadline)?;
    let atoms = model::build(service, owner, &old, location, deadline)?;
    let assigned = alignment::assign(&atoms, &uploaded, location, deadline)?;
    changes(owner, &atoms, &assigned, location, deadline)
}

/// Classify from the immutable source, never from possibly invalid uploaded
/// context. Any supported interleaved Base glyph can gain or lose its wrappers.
pub(super) fn requires_exact(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    baseline: &Page<'_>,
    location: Location,
    deadline: Instant,
) -> Result<bool> {
    let old = words(baseline, location, deadline)?;
    let atoms = model::build(service, owner, &old, location, deadline)?;
    Ok(atoms.iter().any(|atom| matches!(atom, Atom::Glyph { .. })))
}

/// Read complete canonical X26, with no requirement for Local definitions.
fn words(page: &Page<'_>, location: Location, deadline: Instant) -> Result<Vec<u32>> {
    location.check_deadline(deadline)?;
    let mut result = Vec::new();
    let mut absent = false;
    for packet in page.packets[0] {
        location.check_deadline(deadline)?;
        match packet {
            Some(packet) if !absent => {
                for word in packet {
                    if *word > 0x3ffff {
                        return Err(located_word(location, result.len()));
                    }
                    result.push(*word);
                }
            }
            None => absent = true,
            _ => return Err(located_word(location, result.len())),
        }
    }
    let end = result
        .iter()
        .position(|word| *word == FINAL)
        .ok_or_else(|| located_word(location, 0))?;
    if result.len() > CAPACITY
        || result.len() != (end + 1).div_ceil(13) * 13
        || result[end..].iter().any(|word| *word != FINAL)
    {
        return Err(located_word(location, end));
    }
    result.truncate(end);
    Ok(result)
}

fn changes(
    owner: TeletextPageAddress,
    atoms: &[Atom<'_>],
    assigned: &[u32],
    location: Location,
    deadline: Instant,
) -> Result<TeletextEditorialChanges> {
    let mut result = TeletextEditorialChanges::default();
    let mut values = assigned.iter();
    let mut charset_changed = false;
    for atom in atoms {
        location.check_deadline(deadline)?;
        let Atom::Scalar {
            command,
            column,
            expected,
            before,
        } = *atom
        else {
            continue;
        };
        let after = *values.next().ok_or_else(|| located_word(location, 0))?;
        if before == after {
            continue;
        }
        let replacement =
            operand(column, expected, before, after).ok_or_else(|| located_word(location, 0))?;
        result.operands.push(
            TeletextLocalProgramOperandOverlay::new(owner, command, column, expected, replacement)
                .map_err(|_| located_word(location, 0))?,
        );
        charset_changed |= matches!(expected, TeletextColumnOperation::DesignateCharset { .. });
    }
    if !charset_changed || values.next().is_some() {
        return Err(located_word(location, 0));
    }
    Ok(result)
}

//! Stateful DRCS edits of original references; packet topology stays immutable.
use super::{located_word, Location, PacketWords, Result, TeletextEditorialChanges};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextDrcsGlyphReference, TeletextDrcsGlyphSlot,
    TeletextDrcsSubtableIndex, TeletextDrcsTableScope, TeletextLocalObjectOperandOverlay,
    TeletextLocalProgramOperandOverlay, TeletextObjectIdentity, TeletextPageAddress,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use std::time::Instant;

#[path = "drcs_selection/model.rs"]
mod model;
#[path = "drcs_selection/replay.rs"]
mod replay;
const CAPACITY: usize = 16 * 13;
type Ledger = [Option<u32>; CAPACITY];
type Changes<'a> = (
    &'a [TeletextLocalProgramOperandOverlay],
    &'a [TeletextLocalObjectOperandOverlay],
);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Key {
    object: Option<TeletextObjectIdentity>,
    command: usize,
}
#[derive(Clone, Copy)]
struct OriginalGlyph {
    key: Key,
    column: u8,
    reference: TeletextDrcsGlyphReference,
    word: usize,
}
struct Model {
    references: [Option<OriginalGlyph>; CAPACITY],
    selectors: [Option<usize>; CAPACITY],
    boundaries: [bool; CAPACITY],
}
impl Default for Model {
    fn default() -> Self {
        Self {
            references: [None; CAPACITY],
            selectors: [None; CAPACITY],
            boundaries: [false; CAPACITY],
        }
    }
}
pub(super) struct SelectionChanges {
    pub(super) changes: TeletextEditorialChanges,
    pub(super) allowed: Ledger,
}
impl Default for SelectionChanges {
    fn default() -> Self {
        Self {
            changes: TeletextEditorialChanges::default(),
            allowed: [None; CAPACITY],
        }
    }
}
struct Replay {
    references: [Option<TeletextDrcsGlyphReference>; CAPACITY],
    allowed: Ledger,
}

pub(super) fn reconcile(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    before: &PacketWords,
    after: &PacketWords,
    location: Location,
    deadline: Instant,
) -> Result<SelectionChanges> {
    let Some(model) = model::build(service, owner, before, location, deadline)? else {
        return Ok(SelectionChanges::default());
    };
    let replay = replay::compare(&model, before, after, location, deadline)?;
    let mut result = SelectionChanges {
        allowed: replay.allowed,
        ..SelectionChanges::default()
    };
    for original in model.references.iter().flatten() {
        location.check_deadline(deadline)?;
        let replacement = replay.references[original.word]
            .ok_or_else(|| located_word(location, original.word))?;
        if replacement == original.reference {
            continue;
        }
        let expected = TeletextColumnOperation::Drcs {
            reference: original.reference,
        };
        let replacement = TeletextColumnOperation::Drcs {
            reference: replacement,
        };
        if let Some(identity) = original.key.object {
            result.changes.object_operands.push(
                TeletextLocalObjectOperandOverlay::new(
                    identity,
                    original.key.command,
                    original.column,
                    expected,
                    replacement,
                )
                .map_err(|_| located_word(location, original.word))?,
            );
        } else {
            result.changes.operands.push(
                TeletextLocalProgramOperandOverlay::new(
                    owner,
                    original.key.command,
                    original.column,
                    expected,
                    replacement,
                )
                .map_err(|_| located_word(location, original.word))?,
            );
        }
    }
    Ok(result)
}

pub(super) fn qualify_recompiled(
    baseline: &EncodedTeletextAuthoredService,
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    uploaded: &PacketWords,
    changes: Changes<'_>,
    location: Location,
    deadline: Instant,
) -> Result<Ledger> {
    let before = super::packet_words(baseline, owner, location, deadline)?;
    let derived = reconcile(baseline, owner, &before, uploaded, location, deadline)?;
    exact_changes(&derived.changes, changes, owner, location, deadline)?;
    let Some(old_model) = model::build(baseline, owner, &before, location, deadline)? else {
        return Ok(derived.allowed);
    };
    let input = replay::compare(&old_model, &before, uploaded, location, deadline)?;
    let after = super::packet_words(service, owner, location, deadline)?;
    let new_model = model::build(service, owner, &after, location, deadline)?
        .ok_or_else(|| located_word(location, 0))?;
    verify_final_references(&old_model, &input, &new_model, &after, location, deadline)?;
    Ok(derived.allowed)
}

fn verify_final_references(
    old_model: &Model,
    input: &Replay,
    new_model: &Model,
    after: &PacketWords,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    // Authenticate complete final references, not just scope/slot mode13 words.
    // Canonical selectors may move, disappear or appear during recompilation.
    replay::compare(new_model, after, after, location, deadline)?;
    for original in old_model.references.iter().flatten() {
        location.check_deadline(deadline)?;
        let mut matches = new_model
            .references
            .iter()
            .flatten()
            .filter(|value| value.key == original.key);
        let final_reference = matches
            .next()
            .ok_or_else(|| located_word(location, original.word))?;
        if matches.next().is_some()
            || final_reference.column != original.column
            || Some(final_reference.reference) != input.references[original.word]
        {
            return Err(located_word(location, original.word));
        }
    }
    Ok(())
}

fn exact_changes(
    expected: &TeletextEditorialChanges,
    supplied: Changes<'_>,
    owner: TeletextPageAddress,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let mut pages = 0;
    for value in supplied
        .0
        .iter()
        .filter(|value| value.owner() == owner && is_drcs(value.expected()))
    {
        location.check_deadline(deadline)?;
        pages += 1;
        if expected
            .operands
            .iter()
            .filter(|candidate| *candidate == value)
            .count()
            != 1
            || supplied
                .0
                .iter()
                .filter(|candidate| *candidate == value)
                .count()
                != 1
        {
            return Err(located_word(location, 0));
        }
    }
    let mut objects = 0;
    for value in supplied
        .1
        .iter()
        .filter(|value| value.owner() == owner && is_drcs(value.expected()))
    {
        location.check_deadline(deadline)?;
        objects += 1;
        if expected
            .object_operands
            .iter()
            .filter(|candidate| *candidate == value)
            .count()
            != 1
            || supplied
                .1
                .iter()
                .filter(|candidate| *candidate == value)
                .count()
                != 1
        {
            return Err(located_word(location, 0));
        }
    }
    if pages != expected.operands.len() || objects != expected.object_operands.len() {
        return Err(located_word(location, 0));
    }
    Ok(())
}

pub(super) const fn is_drcs(operation: TeletextColumnOperation) -> bool {
    matches!(operation, TeletextColumnOperation::Drcs { .. })
}
fn at(packets: &PacketWords, word: usize) -> Option<u32> {
    packets
        .get(word / 13)?
        .as_ref()
        .map(|packet| packet[word % 13])
}
pub(in super::super) fn selector(word: u32) -> Option<(usize, u8)> {
    if word > 0x3ffff || word & 0x7ff != 0x28 | (24 << 6) {
        return None;
    }
    let data = u8::try_from(word >> 11).ok()?;
    (data & 0x30 == 0x10).then_some((usize::from(data >> 6), data & 15))
}
pub(in super::super) fn glyph(word: u32, column: u8) -> Option<(usize, TeletextDrcsGlyphSlot)> {
    if word > 0x3ffff || column >= 40 || word & 0x7ff != u32::from(column) | (13 << 6) {
        return None;
    }
    let data = u8::try_from(word >> 11).ok()?;
    Some((
        usize::from(data >> 6),
        TeletextDrcsGlyphSlot::new(data & 63).ok()?,
    ))
}
pub(in super::super) fn reference(
    scope: usize,
    subtable: u8,
    slot: TeletextDrcsGlyphSlot,
) -> Option<TeletextDrcsGlyphReference> {
    Some(TeletextDrcsGlyphReference {
        table: if scope == 0 {
            TeletextDrcsTableScope::Global
        } else {
            TeletextDrcsTableScope::Normal
        },
        subtable: TeletextDrcsSubtableIndex::new(subtable).ok()?,
        slot,
    })
}

#[cfg(test)]
#[path = "drcs_selection/tests.rs"]
mod tests;

//! Fixed packet shape, scoped latches, and complete reference-effect coverage.
use super::super::{located_word, Location, Result};
use super::{at, glyph, reference, selector, Ledger, Model, PacketWords, Replay, CAPACITY};
use std::time::Instant;

#[derive(Clone, Copy)]
struct Latch {
    subtable: u8,
    changed_selector: Option<usize>,
}

pub(super) fn compare(
    model: &Model,
    before: &PacketWords,
    after: &PacketWords,
    location: Location,
    deadline: Instant,
) -> Result<Replay> {
    let mut old = [None; 2];
    let mut new = [None; 2];
    let mut used = [false; CAPACITY];
    let mut result = Replay {
        references: [None; CAPACITY],
        allowed: [None; CAPACITY],
    };
    for word in 0..CAPACITY {
        location.check_deadline(deadline)?;
        if model.boundaries[word] {
            old = [None; 2];
            new = [None; 2];
        }
        let (a, b) = match (at(before, word), at(after, word)) {
            (None, None) => continue,
            (Some(a), Some(b)) => (a, b),
            _ => return Err(located_word(location, word)),
        };
        let mode = (a >> 6) & 31;
        if mode == 24 && a & 63 >= 40 {
            let (a_scope, a_s1) = selector(a).ok_or_else(|| located_word(location, word))?;
            let (b_scope, b_s1) = selector(b).ok_or_else(|| located_word(location, word))?;
            let changed = if a == b {
                None
            } else {
                if model.selectors[word].is_none() {
                    return Err(located_word(location, word));
                }
                result.allowed[word] = Some(b);
                Some(word)
            };
            old[a_scope] = Some(Latch {
                subtable: a_s1,
                changed_selector: changed,
            });
            new[b_scope] = Some(Latch {
                subtable: b_s1,
                changed_selector: changed,
            });
        } else if mode == 13 && a & 63 < 40 {
            let column = u8::try_from(a & 63).map_err(|_| located_word(location, word))?;
            let a_ref = selected(a, column, &old, location, word)?;
            let b_ref = selected(b, column, &new, location, word)?;
            let original = model.references[word];
            if original.is_some_and(|value| value.column != column || value.reference != a_ref) {
                return Err(located_word(location, word));
            }
            if a_ref != b_ref {
                if original.is_none() {
                    return Err(located_word(location, word));
                }
                mark_used(b, &new, &mut used);
                if a != b {
                    result.allowed[word] = Some(b);
                }
            }
            if original.is_some() {
                result.references[word] = Some(b_ref);
            }
        }
    }
    reject_dangling(&result.allowed, model, &used, location)?;
    Ok(result)
}

fn selected(
    word: u32,
    column: u8,
    state: &[Option<Latch>; 2],
    location: Location,
    position: usize,
) -> Result<ersatzrs_domain::TeletextDrcsGlyphReference> {
    let (scope, slot) = glyph(word, column).ok_or_else(|| located_word(location, position))?;
    let latch = state[scope].ok_or_else(|| located_word(location, position))?;
    reference(scope, latch.subtable, slot).ok_or_else(|| located_word(location, position))
}
fn mark_used(word: u32, state: &[Option<Latch>; 2], used: &mut [bool; CAPACITY]) {
    let scope = usize::from(((word >> 11) & 64) != 0);
    if let Some(selector) = state[scope].and_then(|latch| latch.changed_selector) {
        used[selector] = true;
    }
}
fn reject_dangling(
    allowed: &Ledger,
    model: &Model,
    used: &[bool; CAPACITY],
    location: Location,
) -> Result<()> {
    for word in 0..CAPACITY {
        if allowed[word].is_some() && model.selectors[word].is_some() && !used[word] {
            return Err(located_word(location, word));
        }
    }
    Ok(())
}

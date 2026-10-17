//! Definition-local DRCS latches and exact ownership of derived selector words.
use super::super::operands::{drcs_selection, glyph_operation};
use super::{fail, style, Location, Result};
use ersatzrs_domain::{TeletextColumnOperation, TeletextEnhancementCommand};
use std::time::Instant;

#[derive(Clone, Copy)]
pub(super) struct Entry {
    pub index: usize,
    pub word: u32,
    pub command: Option<TeletextEnhancementCommand>,
    pub style: Option<TeletextEnhancementCommand>,
    pub selector: Option<usize>,
}
impl Entry {
    pub fn editable(self, styles: bool) -> Option<TeletextEnhancementCommand> {
        self.command.or(if styles { self.style } else { None })
    }
}
pub(super) struct Selector {
    pub index: usize,
    pub word: u32,
    pub used: bool,
}
pub(super) struct OriginalSpan {
    pub first: usize,
    pub word: usize,
    pub ordinal: usize,
}
pub(super) struct Body {
    pub entries: Vec<Entry>,
    pub selectors: Vec<Selector>,
}
impl Body {
    pub fn read(words: &[u32], location: Location, deadline: Instant) -> Result<Self> {
        location.check_deadline(deadline)?;
        if words.len() > 208 {
            return Err(fail(location));
        }
        let mut result = Self {
            entries: Vec::new(),
            selectors: Vec::new(),
        };
        let mut latches = [None; 2];
        for (index, &word) in words.iter().enumerate() {
            location.check_deadline(deadline)?;
            if word > 0x3ffff {
                return Err(fail(location));
            }
            if (word >> 6) & 31 == 24 && word & 63 >= 40 {
                let (scope, subtable) =
                    drcs_selection::selector(word).ok_or_else(|| fail(location))?;
                latches[scope] = Some((subtable, result.selectors.len()));
                result.selectors.push(Selector {
                    index,
                    word,
                    used: false,
                });
                continue;
            }
            let (command, selector) = decode(word, &latches, location)?;
            if let Some(selector) = selector {
                result.selectors[selector].used = true;
            }
            result.entries.push(Entry {
                index,
                word,
                command,
                style: style::decode(word),
                selector,
            });
        }
        Ok(result)
    }
}

/// Scalar-shaped edits retain the old scalar/anchor reconciliation. Style
/// insertion/deletion must change both semantic-entry count and style skeleton.
pub(super) fn style_topology_changed(before: &Body, after: &Body) -> bool {
    fn layout(body: &Body) -> impl Iterator<Item = (u8, u8)> + '_ {
        body.entries.iter().filter_map(|entry| {
            let command = entry.style?;
            let TeletextEnhancementCommand::AtColumn { column, .. } = command else {
                return None;
            };
            Some((style::kind(&command)?, column))
        })
    }
    before.entries.len() != after.entries.len() && !layout(before).eq(layout(after))
}
fn decode(
    word: u32,
    latches: &[Option<(u8, usize)>; 2],
    location: Location,
) -> Result<(Option<TeletextEnhancementCommand>, Option<usize>)> {
    let column = u8::try_from(word & 63).map_err(|_| fail(location))?;
    let mut selector = None;
    let operation = if column < 40 && (word >> 6) & 31 == 13 {
        let (scope, slot) = drcs_selection::glyph(word, column).ok_or_else(|| fail(location))?;
        let (subtable, index) = latches[scope].ok_or_else(|| fail(location))?;
        selector = Some(index);
        Some(TeletextColumnOperation::Drcs {
            reference: drcs_selection::reference(scope, subtable, slot)
                .ok_or_else(|| fail(location))?,
        })
    } else {
        glyph_operation(word)
    };
    Ok((
        operation.map(|operation| TeletextEnhancementCommand::AtColumn { column, operation }),
        selector,
    ))
}

/// Selectors without consumers are allowed only as exact ordered original
/// prefixes whose complete original consumer set was deleted by the typed map.
pub(super) fn qualify_selectors(
    before: &Body,
    uploaded: &Body,
    original_commands: &[OriginalSpan],
    retained: &[Option<usize>],
    location: Location,
    deadline: Instant,
) -> Result<()> {
    for selector in &before.selectors {
        location.check_deadline(deadline)?;
        if original_commands
            .iter()
            .filter(|span| span.first == selector.index && span.word == selector.index + 1)
            .count()
            != 1
        {
            return Err(fail(location));
        }
    }
    let mut end = before.selectors.len();
    for selector in uploaded.selectors.iter().rev() {
        location.check_deadline(deadline)?;
        let original = before.selectors[..end]
            .iter()
            .rposition(|old| old.word == selector.word);
        if let Some(index) = original {
            end = index;
        }
        if selector.used {
            continue;
        }
        let index = original.ok_or_else(|| fail(location))?;
        let consumers = before
            .entries
            .iter()
            .filter(|entry| entry.selector == Some(index));
        let mut found = false;
        for entry in consumers {
            found = true;
            let mut matches = original_commands
                .iter()
                .filter(|span| span.word == entry.index);
            let original = matches.next().ok_or_else(|| fail(location))?;
            if matches.next().is_some() || retained.contains(&Some(original.ordinal)) {
                return Err(fail(location));
            }
        }
        if !found {
            return Err(fail(location));
        }
    }
    Ok(())
}

/// Fixed semantic topology remains the original scalar-family contract. Selector
/// prefixes are derived and do not turn a pure family flip into structural intent.
pub(super) fn protect_fixed_families(
    before: &Body,
    after: &Body,
    location: Location,
) -> Result<()> {
    let same_layout =
        before.entries.len() == after.entries.len()
            && before.entries.iter().zip(&after.entries).all(|(a, b)| {
                match (a.command, b.command) {
                    (
                        Some(TeletextEnhancementCommand::AtColumn { column: a, .. }),
                        Some(TeletextEnhancementCommand::AtColumn { column: b, .. }),
                    ) => a == b,
                    (None, None) => true,
                    _ => false,
                }
            });
    if same_layout
        && before
            .entries
            .iter()
            .zip(&after.entries)
            .any(|(a, b)| match (a.command, b.command) {
                (
                    Some(TeletextEnhancementCommand::AtColumn { operation: a, .. }),
                    Some(TeletextEnhancementCommand::AtColumn { operation: b, .. }),
                ) => a.glyph_family() != b.glyph_family(),
                _ => false,
            })
    {
        return Err(fail(location));
    }
    Ok(())
}

pub(super) fn is_glyph(word: u32) -> bool {
    glyph_operation(word).is_some() || (word <= 0x3ffff && word & 63 < 40 && (word >> 6) & 31 == 13)
}

#[cfg(test)]
mod tests;

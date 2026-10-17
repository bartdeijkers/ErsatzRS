//! Sealed resource calls remain indivisible beside the existing raw glyph parser.
use super::super::super::operands;
use super::{alignment, fail, raw, style, Location, Result, Span};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementObjectDefinition,
    TeletextObjectScope, TeletextObjectType,
};
use std::time::Instant;

pub(in super::super::super) struct CallAtom {
    pub first: usize,
    pub word: usize,
    pub command: Command,
}

fn originals(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: &[u32],
    location: Location,
    deadline: Instant,
) -> Result<Vec<CallAtom>> {
    let mut calls = Vec::new();
    let mut next = 0;
    let mut seen = 0;
    for (ordinal, command) in definition.program.commands().iter().enumerate() {
        location.check_deadline(deadline)?;
        let mut matching = spans.iter().filter(|span| span.ordinal == ordinal);
        let span = match (matching.next(), matching.next()) {
            (None, None) if alignment::hidden_position(command).is_some() => continue,
            (Some(span), None) => span,
            _ => return Err(fail(location)),
        };
        seen += 1;
        if span.first != next || span.first > span.word || span.word >= words.len() {
            return Err(fail(location));
        }
        if matches!(command, Command::InvokeObject { .. }) {
            calls.push(CallAtom {
                first: span.first,
                word: span.word,
                command: *command,
            });
        }
        next = span.word + 1;
    }
    if next != words.len() || seen != spans.len() {
        return Err(fail(location));
    }
    Ok(calls)
}

fn check_atom(words: &[u32], atom: &CallAtom, location: Location) -> Result<()> {
    let Command::InvokeObject {
        target,
        object_type,
        offset,
    } = atom.command
    else {
        return Err(fail(location));
    };
    let scope = match target.scope() {
        TeletextObjectScope::Public => 0x30,
        TeletextObjectScope::Global => 0x38,
        TeletextObjectScope::Local => return Err(fail(location)),
    };
    let mode = match object_type {
        TeletextObjectType::Active => 17,
        TeletextObjectType::Adaptive => 18,
        TeletextObjectType::Passive => 19,
    };
    let word = *words.get(atom.word).ok_or_else(|| fail(location))?;
    if atom.first > atom.word || word > 0x3ffff || word & 0x38 != scope || (word >> 6) & 31 != mode
    {
        return Err(fail(location));
    }
    match offset {
        Some(offset) if atom.first.checked_add(1) == Some(atom.word) => {
            let prefix =
                u32::from(40 + offset.row()) | (16 << 6) | (u32::from(offset.column()) << 11);
            if words.get(atom.first) != Some(&prefix) {
                return Err(fail(location));
            }
        }
        None if atom.first == atom.word => {}
        _ => return Err(fail(location)),
    }
    Ok(())
}

fn without_calls(
    words: &[u32],
    calls: &[CallAtom],
    location: Location,
    deadline: Instant,
) -> Result<Vec<u32>> {
    let mut output = Vec::with_capacity(words.len());
    let mut cursor = 0;
    for atom in calls {
        location.check_deadline(deadline)?;
        check_atom(words, atom, location)?;
        if atom.first < cursor {
            return Err(fail(location));
        }
        output.extend_from_slice(&words[cursor..atom.first]);
        cursor = atom.word + 1;
    }
    output.extend_from_slice(&words[cursor..]);
    Ok(output)
}

fn style_layout(body: &raw::Body) -> impl Iterator<Item = (Option<u8>, u8)> + '_ {
    body.entries.iter().filter_map(|entry| {
        let command = entry.style?;
        let Command::AtColumn { column, .. } = command else {
            return None;
        };
        Some((style::kind(&command), column))
    })
}

fn protect_noncalls(
    words: (&[u32], &[u32]),
    calls: (&[CallAtom], &[CallAtom]),
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let old = without_calls(words.0, calls.0, location, deadline)?;
    let new = without_calls(words.1, calls.1, location, deadline)?;
    let before = raw::Body::read_bounded(&old, maximum, location, deadline)?;
    let after = raw::Body::read_bounded(&new, maximum, location, deadline)?;
    raw::protect_fixed_families(&before, &after, location)?;
    // Same-position style values follow the existing checked scalar decoder.
    // Alignment then omits the old writer and inserts a new one; it never grants
    // the changed style the original command's inherited display authority.
    if style_layout(&before).eq(style_layout(&after)) {
        for (old, new) in before
            .entries
            .iter()
            .filter(|entry| entry.style.is_some())
            .zip(after.entries.iter().filter(|entry| entry.style.is_some()))
        {
            location.check_deadline(deadline)?;
            let Some(Command::AtColumn { column, operation }) = old.style else {
                return Err(fail(location));
            };
            let Some(Command::AtColumn {
                column: new_column,
                operation: replacement,
            }) = new.style
            else {
                return Err(fail(location));
            };
            if column != new_column
                || operands::operand(column, operation, old.word, new.word) != Some(replacement)
            {
                return Err(fail(location));
            }
        }
    }
    Ok(())
}

fn collapse(
    mut body: raw::Body,
    calls: &[CallAtom],
    location: Location,
    deadline: Instant,
) -> Result<raw::Body> {
    for atom in calls {
        location.check_deadline(deadline)?;
        if body
            .selectors
            .iter()
            .any(|selector| (atom.first..=atom.word).contains(&selector.index))
        {
            return Err(fail(location));
        }
    }
    let mut entries = Vec::with_capacity(body.entries.len());
    let mut cursor = 0;
    let mut first_seen = false;
    for entry in body.entries {
        location.check_deadline(deadline)?;
        match calls.get(cursor) {
            Some(atom) if entry.index >= atom.first => {
                first_seen |= entry.index == atom.first;
                if !first_seen
                    || entry.index > atom.word
                    || entry.selector.is_some()
                    || entry.command.is_some()
                    || entry.style.is_some()
                {
                    return Err(fail(location));
                }
                if entry.index == atom.word {
                    entries.push(raw::Entry {
                        command: Some(atom.command),
                        ..entry
                    });
                    cursor += 1;
                    first_seen = false;
                }
            }
            _ => entries.push(entry),
        }
    }
    if cursor != calls.len() {
        return Err(fail(location));
    }
    body.entries = entries;
    Ok(body)
}

pub(super) fn read(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: (&[u32], &[u32]),
    after_calls: &[CallAtom],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<(raw::Body, raw::Body)> {
    location.check_deadline(deadline)?;
    if words.0.len() > maximum || words.1.len() > maximum || after_calls.len() > words.1.len() {
        return Err(fail(location));
    }
    let before_calls = originals(definition, spans, words.0, location, deadline)?;
    protect_noncalls(
        words,
        (&before_calls, after_calls),
        maximum,
        location,
        deadline,
    )?;
    let before = raw::Body::read_bounded(words.0, maximum, location, deadline)?;
    let after = raw::Body::read_bounded(words.1, maximum, location, deadline)?;
    Ok((
        collapse(before, &before_calls, location, deadline)?,
        collapse(after, after_calls, location, deadline)?,
    ))
}

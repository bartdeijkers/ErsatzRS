//! Whole resource-object alignment, retaining true original semantic ordinals.
use super::{alignment, fail, raw, stream, style, Location, Page, Result};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementObjectDefinition,
    TeletextEnhancementProgram,
};
use std::time::Instant;

pub(in super::super) struct Span {
    pub ordinal: usize,
    pub first: usize,
    pub word: usize,
}
pub(in super::super) struct Program {
    pub replacement: TeletextEnhancementProgram,
    pub retained: Vec<Option<usize>>,
}

pub(in super::super) fn image(
    page: &Page<'_>,
    location: Location,
    deadline: Instant,
) -> Result<(Vec<u32>, Vec<u32>)> {
    let image = stream::Image::read(page, location, deadline)?;
    if image.markers.len() != 1 {
        return Err(fail(location));
    }
    Ok((
        image.words[..=image.markers[0]].to_vec(),
        image.body(0).to_vec(),
    ))
}
fn editable(command: &TeletextEnhancementCommand) -> bool {
    super::glyph_family(command).is_some() || style::kind(command).is_some()
}
fn derived_prefix(entries: &[raw::Entry], cursor: usize) -> bool {
    let Some(entry) = entries.get(cursor) else {
        return false;
    };
    let address = entry.word & 63;
    let mode = (entry.word >> 6) & 31;
    ((mode == 4 && address >= 40) || (mode == 7 && address == 63 && entry.word >> 11 < 32))
        && entries
            .get(cursor + 1)
            .is_some_and(|next| next.editable(true).is_some())
}

pub(in super::super) fn parse(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: (&[u32], &[u32]),
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Program> {
    location.check_deadline(deadline)?;
    if definition.program.commands().len() > maximum || spans.len() > maximum {
        return Err(fail(location));
    }
    let raw_maximum = maximum
        .checked_mul(2)
        .ok_or_else(|| fail(location))?
        .min(507);
    let before = raw::Body::read_bounded(words.0, raw_maximum, location, deadline)?;
    let after = raw::Body::read_bounded(words.1, raw_maximum, location, deadline)?;
    raw::protect_fixed_families(&before, &after, location)?;
    validate_original(definition, spans, words.0, &before, location, deadline)?;
    let mut commands = Vec::new();
    let mut retained = Vec::new();
    let (mut ordinal, mut cursor) = (0, 0);
    while ordinal < definition.program.commands().len() {
        location.check_deadline(deadline)?;
        let command = definition.program.commands()[ordinal];
        if editable(&command)
            || alignment::hidden_position(&command).is_some()
            || after
                .entries
                .get(cursor)
                .is_some_and(|entry| entry.editable(true).is_some())
            || (derived_prefix(&after.entries, cursor)
                && !anchor_matches(ordinal, spans, words.0, &after.entries, cursor))
        {
            let start = ordinal;
            while ordinal < definition.program.commands().len()
                && (editable(&definition.program.commands()[ordinal])
                    || alignment::hidden_position(&definition.program.commands()[ordinal])
                        .is_some())
            {
                ordinal += 1;
            }
            let original = (start..ordinal)
                .map(|index| (index, definition.program.commands()[index]))
                .collect::<Vec<_>>();
            let cluster = cluster(
                &original,
                &after.entries,
                &mut cursor,
                maximum,
                location,
                deadline,
                (ordinal, spans, words.0),
            )?;
            commands.extend(cluster.commands);
            retained.extend(cluster.retained);
        } else {
            let consumed = consume_anchor(
                (ordinal, spans, words.0),
                (&after.entries, cursor),
                location,
            )?;
            commands.push(command);
            retained.push(Some(ordinal));
            cursor += consumed;
            ordinal += 1;
        }
        if commands.len() > maximum {
            return Err(fail(location));
        }
    }
    append_trailing(
        &after.entries,
        cursor,
        &mut (&mut commands, &mut retained),
        maximum,
        location,
    )?;
    qualify_selectors(
        definition,
        spans,
        (&before, &after),
        &retained,
        location,
        deadline,
    )?;
    Ok(Program {
        replacement: TeletextEnhancementProgram::new(commands).map_err(|_| fail(location))?,
        retained,
    })
}

fn append_trailing(
    entries: &[raw::Entry],
    mut cursor: usize,
    output: &mut (
        &mut Vec<TeletextEnhancementCommand>,
        &mut Vec<Option<usize>>,
    ),
    maximum: usize,
    location: Location,
) -> Result<()> {
    while cursor < entries.len() {
        if derived_prefix(entries, cursor) {
            cursor += 1;
        }
        output.0.push(
            entries
                .get(cursor)
                .and_then(|entry| entry.editable(true))
                .ok_or_else(|| fail(location))?,
        );
        output.1.push(None);
        cursor += 1;
        if output.0.len() > maximum {
            return Err(fail(location));
        }
    }
    Ok(())
}

fn cluster(
    original: &[(usize, TeletextEnhancementCommand)],
    entries: &[raw::Entry],
    cursor: &mut usize,
    maximum: usize,
    location: Location,
    deadline: Instant,
    next_anchor: (usize, &[Span], &[u32]),
) -> Result<alignment::Aligned> {
    let mut inserted = Vec::new();
    loop {
        // A real following AP can look like a derived compatibility prefix.
        // Its authenticated original span ends the cluster before prefix handling.
        if anchor_matches(
            next_anchor.0,
            next_anchor.1,
            next_anchor.2,
            entries,
            *cursor,
        ) {
            break;
        }
        if derived_prefix(entries, *cursor) {
            *cursor += 1;
        }
        let Some(command) = entries.get(*cursor).and_then(|entry| entry.editable(true)) else {
            break;
        };
        inserted.push(command);
        *cursor += 1;
    }
    alignment::bounded(original, &inserted, maximum, location, deadline)
}
fn consume_anchor(
    source: (usize, &[Span], &[u32]),
    actual: (&[raw::Entry], usize),
    location: Location,
) -> Result<usize> {
    if !anchor_matches(source.0, source.1, source.2, actual.0, actual.1) {
        return Err(fail(location));
    }
    let span = source
        .1
        .iter()
        .find(|span| span.ordinal == source.0)
        .ok_or_else(|| fail(location))?;
    Ok(span.word - span.first + 1)
}
fn qualify_selectors(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    bodies: (&raw::Body, &raw::Body),
    retained: &[Option<usize>],
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let selectors = spans
        .iter()
        .filter(|span| {
            matches!(
                definition.program.commands()[span.ordinal],
                TeletextEnhancementCommand::AtColumn {
                    operation: TeletextColumnOperation::Drcs { .. },
                    ..
                }
            )
        })
        .map(|span| raw::OriginalSpan {
            first: span.first,
            word: span.word,
            ordinal: span.ordinal,
        })
        .collect::<Vec<_>>();
    raw::qualify_selectors(bodies.0, bodies.1, &selectors, retained, location, deadline)
}

fn anchor_matches(
    ordinal: usize,
    spans: &[Span],
    before: &[u32],
    actual: &[raw::Entry],
    cursor: usize,
) -> bool {
    let Some(span) = spans.iter().find(|span| span.ordinal == ordinal) else {
        return false;
    };
    let Some(expected) = before.get(span.first..=span.word) else {
        return false;
    };
    let Some(end) = cursor.checked_add(expected.len()) else {
        return false;
    };
    actual.get(cursor..end).is_some_and(|entries| {
        entries
            .iter()
            .map(|entry| entry.word)
            .eq(expected.iter().copied())
    })
}

fn validate_original(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: &[u32],
    body: &raw::Body,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let mut next = 0;
    for (ordinal, command) in definition.program.commands().iter().enumerate() {
        location.check_deadline(deadline)?;
        let matches = spans
            .iter()
            .filter(|span| span.ordinal == ordinal)
            .collect::<Vec<_>>();
        if alignment::hidden_position(command).is_some() && matches.is_empty() {
            continue;
        }
        let [span] = matches.as_slice() else {
            return Err(fail(location));
        };
        if span.first != next || span.word < span.first || span.word >= words.len() {
            return Err(fail(location));
        }
        if editable(command)
            && body
                .entries
                .iter()
                .find(|entry| entry.index == span.word)
                .and_then(|entry| entry.editable(true))
                != Some(*command)
        {
            return Err(fail(location));
        }
        next = span.word + 1;
    }
    if next != words.len() {
        return Err(fail(location));
    }
    Ok(())
}

pub(in super::super) fn equivalent(
    left: &[u32],
    right: &[u32],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<bool> {
    let left = raw::Body::read_bounded(left, maximum, location, deadline)?;
    let right = raw::Body::read_bounded(right, maximum, location, deadline)?;
    Ok(left.entries.len() == right.entries.len()
        && left
            .entries
            .iter()
            .zip(&right.entries)
            .all(|(left, right)| match (left.command, right.command) {
                (
                    Some(TeletextEnhancementCommand::AtColumn {
                        operation: TeletextColumnOperation::Drcs { .. },
                        ..
                    }),
                    Some(TeletextEnhancementCommand::AtColumn {
                        operation: TeletextColumnOperation::Drcs { .. },
                        ..
                    }),
                ) => left.command == right.command,
                _ => left.word == right.word,
            }))
}

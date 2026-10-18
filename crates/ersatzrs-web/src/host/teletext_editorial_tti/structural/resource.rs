//! Whole resource-object alignment, retaining true original semantic ordinals.
use super::{alignment, fail, raw, stream, style, Location, Page, Result};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementObjectDefinition,
    TeletextEnhancementPosition, TeletextEnhancementProgram,
};
use std::time::Instant;

mod call_atoms;

pub(in super::super) use call_atoms::CallAtom;

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
fn editable(command: &TeletextEnhancementCommand, calls: bool) -> bool {
    super::glyph_family(command).is_some()
        || style::kind(command).is_some()
        || (calls && matches!(command, TeletextEnhancementCommand::InvokeObject { .. }))
}
fn derived_prefix(entries: &[raw::Entry], cursor: usize) -> bool {
    let Some(entry) = entries.get(cursor) else {
        return false;
    };
    let address = entry.word & 63;
    let mode = (entry.word >> 6) & 31;
    ((mode == 4 && address >= 40) || (mode == 7 && address == 63 && entry.word >> 11 < 32))
        && entries.get(cursor + 1).is_some_and(|next| {
            next.editable(true).is_some_and(|command| {
                !matches!(command, TeletextEnhancementCommand::InvokeObject { .. })
            })
        })
}

#[derive(Clone, Copy)]
struct Input<'a> {
    words: (&'a [u32], &'a [u32]),
    calls: Option<&'a [CallAtom]>,
}

impl Input<'_> {
    fn read(
        self,
        definition: &TeletextEnhancementObjectDefinition,
        spans: &[Span],
        maximum: usize,
        location: Location,
        deadline: Instant,
    ) -> Result<(raw::Body, raw::Body)> {
        if let Some(atoms) = self.calls {
            call_atoms::read(
                definition, spans, self.words, atoms, maximum, location, deadline,
            )
        } else {
            Ok((
                raw::Body::read_bounded(self.words.0, maximum, location, deadline)?,
                raw::Body::read_bounded(self.words.1, maximum, location, deadline)?,
            ))
        }
    }
}

pub(in super::super) fn parse(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: (&[u32], &[u32]),
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Program> {
    parse_inner(
        definition,
        spans,
        Input { words, calls: None },
        maximum,
        location,
        deadline,
    )
}

/// Only a server-authenticated, still-empty added definition may gain its first
/// explicit row positions. Existing programs retain the unchanged parser policy.
pub(in super::super) fn parse_first_body(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: (&[u32], &[u32]),
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Program> {
    location.check_deadline(deadline)?;
    if !definition.program.commands().is_empty() || !spans.is_empty() || !words.0.is_empty() {
        return Err(fail(location));
    }
    inserted_body(words.1, maximum, location, deadline)
}

/// Append only after the exact complete native body of an authenticated saved Add.
/// A missing prefix or absent new AP leaves the unchanged legacy route in charge.
pub(in super::super) fn parse_appended_body(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: (&[u32], &[u32]),
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Option<Program>> {
    location.check_deadline(deadline)?;
    let original_count = definition.program.commands().len();
    let raw_maximum = maximum
        .checked_mul(2)
        .ok_or_else(|| fail(location))?
        .min(507);
    if original_count > maximum
        || spans.len() > maximum
        || words.0.len() > raw_maximum
        || words.1.len() > raw_maximum
        || spans.iter().any(|span| span.ordinal >= original_count)
    {
        return Err(fail(location));
    }
    let Some(suffix) = words.1.strip_prefix(words.0) else {
        return Ok(None);
    };
    if !suffix
        .iter()
        .any(|word| (word >> 6) & 31 == 4 && word & 63 >= 40 && word >> 11 < 40)
    {
        return Ok(None);
    }
    let before = raw::Body::read_bounded(words.0, raw_maximum, location, deadline)?;
    validate_original(
        definition, spans, words.0, &before, false, location, deadline,
    )?;
    let appended = inserted_body(suffix, maximum - original_count, location, deadline)?;
    let mut commands = definition.program.commands().to_vec();
    commands.extend_from_slice(appended.replacement.commands());
    let mut retained = (0..original_count).map(Some).collect::<Vec<_>>();
    retained.extend(appended.retained);
    location.check_deadline(deadline)?;
    Ok(Some(Program {
        replacement: TeletextEnhancementProgram::new(commands).map_err(|_| fail(location))?,
        retained,
    }))
}

fn inserted_body(
    words: &[u32],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Program> {
    let raw_maximum = maximum
        .checked_mul(2)
        .ok_or_else(|| fail(location))?
        .min(507);
    let after = raw::Body::read_bounded(words, raw_maximum, location, deadline)?;
    let mut commands = Vec::new();
    let mut retained = Vec::new();
    for entry in &after.entries {
        location.check_deadline(deadline)?;
        if commands.len() >= maximum {
            return Err(fail(location));
        }
        let address = entry.word & 63;
        let mode = (entry.word >> 6) & 31;
        let column = entry.word >> 11;
        let command = if mode == 4 && address >= 40 && column < 40 {
            let row = if address == 40 { 24 } else { address - 40 };
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(
                    u8::try_from(row).map_err(|_| fail(location))?,
                    u8::try_from(column).map_err(|_| fail(location))?,
                )
                .map_err(|_| fail(location))?,
            }
        } else {
            entry.editable(true).ok_or_else(|| fail(location))?
        };
        commands.push(command);
        retained.push(None);
    }
    Ok(Program {
        replacement: TeletextEnhancementProgram::new(commands).map_err(|_| fail(location))?,
        retained,
    })
}

/// The caller authenticates full invocation identities before native selectors.
/// Every other raw command and selector retains the existing parser's ownership.
pub(in super::super) fn parse_with_calls(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: (&[u32], &[u32]),
    calls: &[CallAtom],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Program> {
    parse_inner(
        definition,
        spans,
        Input {
            words,
            calls: Some(calls),
        },
        maximum,
        location,
        deadline,
    )
}

fn parse_inner(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    input: Input<'_>,
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Program> {
    let words = input.words;
    let calls = input.calls.is_some();
    location.check_deadline(deadline)?;
    if definition.program.commands().len() > maximum || spans.len() > maximum {
        return Err(fail(location));
    }
    let raw_maximum = maximum
        .checked_mul(2)
        .ok_or_else(|| fail(location))?
        .min(507);
    let (before, after) = input.read(definition, spans, raw_maximum, location, deadline)?;
    raw::protect_fixed_families(&before, &after, location)?;
    validate_original(
        definition, spans, words.0, &before, calls, location, deadline,
    )?;
    let mut commands = Vec::new();
    let mut retained = Vec::new();
    let (mut ordinal, mut cursor) = (0, 0);
    while ordinal < definition.program.commands().len() {
        location.check_deadline(deadline)?;
        let command = definition.program.commands()[ordinal];
        if editable(&command, calls)
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
                && (editable(&definition.program.commands()[ordinal], calls)
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
                (maximum, calls),
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
    policy: (usize, bool),
    location: Location,
    deadline: Instant,
    next_anchor: (usize, &[Span], &[u32]),
) -> Result<alignment::Aligned> {
    let (maximum, calls) = policy;
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
    if calls {
        alignment::resource_calls(original, &inserted, maximum, location, deadline)
    } else {
        alignment::bounded(original, &inserted, maximum, location, deadline)
    }
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
    calls: bool,
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
        if editable(command, calls)
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

/// Align only authenticated resource invocation atoms; no new raw-word parser.
pub(in super::super) fn align_calls(
    old: &[(usize, TeletextEnhancementCommand)],
    new: &[TeletextEnhancementCommand],
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Program> {
    let aligned = alignment::calls(old, new, maximum, location, deadline)?;
    Ok(Program {
        replacement: TeletextEnhancementProgram::new(aligned.commands)
            .map_err(|_| fail(location))?,
        retained: aligned.retained,
    })
}

#[cfg(test)]
mod first_body_tests {
    use super::*;
    use ersatzrs_domain::{
        TeletextObjectId, TeletextObjectIdentity, TeletextObjectType, TeletextPageAddress,
        TeletextPageNumber, TeletextSubcode,
    };
    use std::time::Duration;

    pub(super) fn empty() -> TeletextEnhancementObjectDefinition {
        TeletextEnhancementObjectDefinition {
            identity: TeletextObjectIdentity::local(
                TeletextPageAddress {
                    page: TeletextPageNumber::new(0x100).unwrap(),
                    subcode: TeletextSubcode::new(0).unwrap(),
                },
                TeletextObjectId::new(0),
            ),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![]).unwrap(),
        }
    }
    pub(super) fn word(address: u32, mode: u32, data: u32) -> u32 {
        address | (mode << 6) | (data << 11)
    }
    pub(super) fn location() -> Location {
        Location {
            page: 0,
            subcode: 0,
            resource: None,
        }
    }

    #[test]
    fn first_body_preserves_real_row_positions_and_bounded_command_correspondence() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let words = [
            word(41, 4, 0),
            word(8, 9, 80),
            word(42, 4, 0),
            word(4, 9, 81),
            word(40, 4, 39),
            word(39, 9, 82),
        ];
        let parsed =
            parse_first_body(&empty(), &[], (&[], &words), 6, location(), deadline).unwrap();
        assert_eq!(parsed.retained, [None; 6]);
        for (ordinal, row, column) in [(0, 1, 0), (2, 2, 0), (4, 24, 39)] {
            assert_eq!(
                parsed.replacement.commands()[ordinal],
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(row, column).unwrap(),
                }
            );
        }
        assert!(parse_first_body(&empty(), &[], (&[], &words), 5, location(), deadline).is_err());
        assert!(
            parse_first_body(&empty(), &[], (&[], &words), 6, location(), Instant::now()).is_err()
        );
        let mut populated = empty();
        populated.program = parsed.replacement;
        assert!(parse_first_body(&populated, &[], (&[], &words), 6, location(), deadline).is_err());
    }

    #[test]
    fn first_body_rejects_unrepresented_positions_reserved_columns_and_context_words() {
        let deadline = Instant::now() + Duration::from_secs(5);
        for prefix in [
            word(39, 4, 0),
            word(41, 4, 40),
            word(63, 7, 0),
            word(40, 16, 0),
        ] {
            let words = [prefix, word(8, 9, 80)];
            assert!(
                parse_first_body(&empty(), &[], (&[], &words), 6, location(), deadline).is_err()
            );
        }
    }
}

#[cfg(test)]
#[path = "resource/appended_body_tests.rs"]
mod appended_body_tests;

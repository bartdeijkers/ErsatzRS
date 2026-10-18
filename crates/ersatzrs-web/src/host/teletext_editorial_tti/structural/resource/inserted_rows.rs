//! Exact surviving native spans delimit additional rows in authenticated Adds.
use super::{fail, inserted_body, raw, validate_original, Location, Program, Result, Span};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectDefinition, TeletextEnhancementProgram,
};
use std::time::Instant;

/// This reconstruction grants no edit authority; its caller authenticates the
/// saved Add and folds the resulting complete body into that same definition.
pub(in super::super::super) fn parse_inserted_rows(
    definition: &TeletextEnhancementObjectDefinition,
    spans: &[Span],
    words: (&[u32], &[u32]),
    maximum: usize,
    location: Location,
    deadline: Instant,
) -> Result<Option<Program>> {
    location.check_deadline(deadline)?;
    let original = definition.program.commands();
    let raw_maximum = maximum
        .checked_mul(2)
        .ok_or_else(|| fail(location))?
        .min(507);
    if original.len() > maximum
        || spans.len() > maximum
        || spans.iter().any(|span| span.ordinal >= original.len())
    {
        return Err(fail(location));
    }
    let before = raw::Body::read_bounded(words.0, raw_maximum, location, deadline)?;
    let after = raw::Body::read_bounded(words.1, raw_maximum, location, deadline)?;
    validate_original(
        definition, spans, words.0, &before, false, location, deadline,
    )?;
    let mut result = Reconstruction {
        commands: Vec::new(),
        retained: Vec::new(),
        cursor: 0,
        row: 0,
        inserted: false,
    };
    for (ordinal, command) in original.iter().enumerate() {
        location.check_deadline(deadline)?;
        if let Some(span) = spans.iter().find(|span| span.ordinal == ordinal) {
            let old = &words.0[span.first..=span.word];
            if !words.1[result.cursor..].starts_with(old) {
                let Some(row) = positive_row(command) else {
                    return result.inapplicable(location);
                };
                if span.first == 0
                    || row <= result.row
                    || !is_row_instruction(words.1.get(result.cursor))
                {
                    return result.inapplicable(location);
                }
                let end = boundary(words.1, result.cursor, row, location, deadline)?;
                result.insert(
                    &words.1[result.cursor..end],
                    row,
                    maximum,
                    location,
                    deadline,
                )?;
                if !words.1[result.cursor..].starts_with(old) {
                    return Err(fail(location));
                }
            }
            let semantic = after
                .entries
                .iter()
                .find(|entry| entry.index == result.cursor + span.word - span.first);
            if super::editable(command, false)
                && semantic.and_then(|entry| entry.editable(true)) != Some(*command)
            {
                return Err(fail(location));
            }
            result.cursor += old.len();
        }
        if let Some(row) = positive_row(command) {
            result.row = row;
        }
        if result.commands.len() >= maximum {
            return Err(fail(location));
        }
        result.commands.push(*command);
        result.retained.push(Some(ordinal));
    }
    if !result.inserted {
        return Ok(None);
    }
    if result.cursor != words.1.len() {
        return Err(fail(location));
    }
    location.check_deadline(deadline)?;
    Ok(Some(Program {
        replacement: TeletextEnhancementProgram::new(result.commands)
            .map_err(|_| fail(location))?,
        retained: result.retained,
    }))
}

struct Reconstruction {
    commands: Vec<TeletextEnhancementCommand>,
    retained: Vec<Option<usize>>,
    cursor: usize,
    row: u8,
    inserted: bool,
}
impl Reconstruction {
    fn inapplicable(&self, location: Location) -> Result<Option<Program>> {
        if self.inserted {
            Err(fail(location))
        } else {
            Ok(None)
        }
    }
    fn insert(
        &mut self,
        words: &[u32],
        next_row: u8,
        maximum: usize,
        location: Location,
        deadline: Instant,
    ) -> Result<()> {
        let remaining = maximum
            .checked_sub(self.commands.len())
            .ok_or_else(|| fail(location))?;
        let parsed = inserted_body(words, remaining, location, deadline)?;
        let body = raw::Body::read_bounded(words, 507, location, deadline)?;
        if body.selectors.iter().enumerate().any(|(index, selector)| {
            !selector.used
                || !body
                    .entries
                    .iter()
                    .any(|entry| entry.index == selector.index + 1 && entry.selector == Some(index))
        }) {
            return Err(fail(location));
        }
        let mut populated = false;
        let mut positioned = false;
        for command in parsed.replacement.commands() {
            location.check_deadline(deadline)?;
            if let Some(row) = positive_row(command) {
                if row <= self.row || row >= next_row || (positioned && !populated) {
                    return Err(fail(location));
                }
                self.row = row;
                populated = false;
                positioned = true;
            } else {
                populated = true;
            }
            self.commands.push(*command);
            self.retained.push(None);
        }
        if !positioned || !populated {
            return Err(fail(location));
        }
        self.inserted = true;
        self.cursor += words.len();
        Ok(())
    }
}

fn positive_row(command: &TeletextEnhancementCommand) -> Option<u8> {
    match command {
        TeletextEnhancementCommand::SetPosition { position } if position.row() > 0 => {
            Some(position.row())
        }
        _ => None,
    }
}
fn is_position(word: Option<&u32>) -> bool {
    word.is_some_and(|word| (word >> 6) & 31 == 4 && word & 63 >= 40 && word >> 11 < 40)
}
fn is_row_instruction(word: Option<&u32>) -> bool {
    word.is_some_and(|word| (word >> 6) & 31 == 4 && word & 63 >= 40)
}
fn boundary(
    words: &[u32],
    start: usize,
    row: u8,
    location: Location,
    deadline: Instant,
) -> Result<usize> {
    for (index, word) in words.iter().enumerate().skip(start) {
        location.check_deadline(deadline)?;
        if is_position(Some(word)) {
            let address = word & 63;
            let candidate = if address == 40 { 24 } else { address - 40 };
            if candidate >= u32::from(row) {
                return Ok(index);
            }
        }
    }
    Err(fail(location))
}

#[cfg(test)]
mod tests;

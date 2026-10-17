//! Glyph segments retain exact nonglyph anchors and original command ordinals.
use super::{alignment, fail, glyph_family, stream, Location, Result};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextGlyphCode, TeletextMosaic, TeletextObjectIdentity,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextLocalObjectProgram;
use std::time::Instant;
pub(super) struct Candidate {
    pub identity: TeletextObjectIdentity,
    pub expected: TeletextEnhancementProgram,
    pub replacement: Vec<TeletextEnhancementCommand>,
    pub retained: Vec<Option<usize>>,
    pub structural: bool,
}
pub(super) fn parse(
    definition: &ersatzrs_domain::TeletextEnhancementObjectDefinition,
    program: &EncodedTeletextLocalObjectProgram,
    images: (&stream::Image, &stream::Image),
    index: usize,
    aligned: &mut [u32],
    location: Location,
    deadline: Instant,
) -> Result<Candidate> {
    let (old, new) = images;
    let actual = new.body(index);
    location.check_deadline(deadline)?;
    if definition.program.commands().len() > 208 || actual.len() > 208 {
        return Err(fail(location));
    }
    check_rebuilt_count(definition.program.commands(), actual, location)?;
    let mut cursor = 0;
    let mut output = Candidate {
        identity: definition.identity,
        expected: definition.program.clone(),
        replacement: Vec::new(),
        retained: Vec::new(),
        structural: false,
    };
    let mut ordinal = 0;
    while ordinal < definition.program.commands().len() {
        location.check_deadline(deadline)?;
        let command = definition.program.commands()[ordinal];
        if cluster_command(definition, program, ordinal, location)?
            || actual
                .get(cursor)
                .is_some_and(|word| decode(*word).is_some())
        {
            let start = ordinal;
            while ordinal < definition.program.commands().len()
                && cluster_command(definition, program, ordinal, location)?
            {
                ordinal += 1;
            }
            let before = (start..ordinal)
                .map(|i| (i, definition.program.commands()[i]))
                .collect::<Vec<_>>();
            let mut inserted = Vec::new();
            while let Some(command) = actual.get(cursor).and_then(|word| decode(*word)) {
                inserted.push(command);
                cursor += 1;
            }
            let cluster = alignment::with_anchors(&before, &inserted, location, deadline)?;
            output.retained.extend(cluster.retained);
            output.replacement.extend(cluster.commands);
        } else {
            let positions = program
                .command_positions()
                .iter()
                .filter(|position| {
                    position.source().object == Some(definition.identity)
                        && position.source().command == ordinal
                })
                .collect::<Vec<_>>();
            if positions.len() > 1 {
                return Err(fail(location));
            }
            if let Some(position) = positions.first() {
                let count = position.semantic_word() - position.first_word() + 1;
                let words = actual
                    .get(cursor..cursor + count)
                    .ok_or_else(|| fail(location))?;
                let expected = &old.words[position.first_word()..=position.semantic_word()];
                anchor(command, expected, words, images, location)?;
                for (offset, word) in words.iter().enumerate() {
                    aligned[position.first_word() + offset] =
                        if stream::relocate(expected[offset], old, new, location)?
                            == expected[offset]
                        {
                            *word
                        } else {
                            expected[offset]
                        };
                }
                cursor += count;
            }
            output.replacement.push(command);
            output.retained.push(Some(ordinal));
            ordinal += 1;
        }
    }
    for word in &actual[cursor..] {
        output
            .replacement
            .push(decode(*word).ok_or_else(|| fail(location))?);
        output.retained.push(None);
    }
    finish(output, definition, program, aligned, location)
}
fn check_rebuilt_count(
    original: &[TeletextEnhancementCommand],
    actual: &[u32],
    location: Location,
) -> Result<()> {
    let glyphs = actual
        .iter()
        .filter(|word| decode(**word).is_some())
        .count();
    let mandatory = original
        .iter()
        .filter(|command| glyph_family(command).is_none())
        .count();
    if glyphs
        .checked_add(mandatory)
        .is_none_or(|count| count > 208)
    {
        return Err(fail(location));
    }
    Ok(())
}
fn cluster_command(
    definition: &ersatzrs_domain::TeletextEnhancementObjectDefinition,
    program: &EncodedTeletextLocalObjectProgram,
    ordinal: usize,
    location: Location,
) -> Result<bool> {
    let command = &definition.program.commands()[ordinal];
    let count = program
        .command_positions()
        .iter()
        .filter(|position| {
            position.source().object == Some(definition.identity)
                && position.source().command == ordinal
        })
        .count();
    if count > 1 || (count == 0 && !alignment::hidden_identity(command)) {
        return Err(fail(location));
    }
    // Only a sealed absent span authenticates an invisible original identity.
    // Every other nonglyph retains its exact physical anchor/span handling.
    Ok(glyph_family(command).is_some() || (count == 0 && alignment::hidden_identity(command)))
}
fn finish(
    mut output: Candidate,
    definition: &ersatzrs_domain::TeletextEnhancementObjectDefinition,
    program: &EncodedTeletextLocalObjectProgram,
    aligned: &mut [u32],
    location: Location,
) -> Result<Candidate> {
    output.structural = output.replacement.len() != definition.program.commands().len()
        || output
            .retained
            .iter()
            .enumerate()
            .any(|(i, value)| *value != Some(i))
        || output
            .replacement
            .iter()
            .zip(definition.program.commands())
            .any(|(a, b)| match (a, b) {
                (
                    TeletextEnhancementCommand::AtColumn { column: a, .. },
                    TeletextEnhancementCommand::AtColumn { column: b, .. },
                ) => a != b,
                _ => false,
            });
    if !output.structural {
        for (ordinal, command) in output
            .replacement
            .iter()
            .enumerate()
            .filter(|(_, command)| glyph_family(command).is_some())
        {
            let position = program
                .command_positions()
                .iter()
                .find(|position| {
                    position.source().object == Some(definition.identity)
                        && position.source().command == ordinal
                })
                .ok_or_else(|| fail(location))?;
            aligned[position.semantic_word()] = encode(*command).ok_or_else(|| fail(location))?;
        }
    }
    Ok(output)
}
pub(super) fn decode(word: u32) -> Option<TeletextEnhancementCommand> {
    if word > 0x3ffff || word & 63 >= 40 {
        return None;
    }
    let column = u8::try_from(word & 63).ok()?;
    let data = u8::try_from(word >> 11).ok()?;
    let operation = match (word >> 6) & 31 {
        9 => TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(data).ok()?,
        },
        1 if data & 32 != 0 => TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new((data & 31) | ((data & 64) >> 1)).ok()?,
        },
        _ => return None,
    };
    Some(TeletextEnhancementCommand::AtColumn { column, operation })
}
fn encode(command: TeletextEnhancementCommand) -> Option<u32> {
    let TeletextEnhancementCommand::AtColumn { column, operation } = command else {
        return None;
    };
    let (mode, data) = match operation {
        TeletextColumnOperation::G0 { code } => (9, u32::from(code.get())),
        TeletextColumnOperation::G1 { mosaic } => {
            let bits = u32::from(mosaic.blocks());
            (1, 32 | (bits & 31) | ((bits & 32) << 1))
        }
        _ => return None,
    };
    Some(u32::from(column) | (mode << 6) | (data << 11))
}
fn anchor(
    command: TeletextEnhancementCommand,
    before: &[u32],
    after: &[u32],
    images: (&stream::Image, &stream::Image),
    location: Location,
) -> Result<()> {
    if before == after {
        return Ok(());
    }
    if let TeletextEnhancementCommand::AtColumn { column, operation } = command {
        if before.len() == 1
            && super::super::operands::operand(column, operation, before[0], after[0]).is_some()
        {
            return Ok(());
        }
        if matches!(operation, TeletextColumnOperation::Drcs { .. }) && before.len() <= 2 {
            for (index, word) in after.iter().enumerate() {
                if index + 1 == after.len() {
                    if word & 0x7ff != u32::from(column) | (13 << 6)
                        || *word > 0x3ffff
                        || (word >> 11) & 63 >= 48
                    {
                        return Err(fail(location));
                    }
                } else if word & 0x7ff != 0x28 | (0x18 << 6)
                    || *word > 0x3ffff
                    || (word >> 11) & 0x30 != 0x10
                {
                    return Err(fail(location));
                }
            }
            return Ok(());
        }
    }
    for (old, new) in before.iter().zip(after) {
        if stream::relocate(*old, images.0, images.1, location)? != *new {
            return Err(fail(location));
        }
    }
    Ok(())
}

#[cfg(test)]
mod bounds_tests {
    use super::{check_rebuilt_count, Location};
    use ersatzrs_domain::{
        TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
        TeletextEnhancementPosition,
    };

    #[test]
    fn whole_definition_limit_combines_separate_clusters_and_mandatory_anchors() {
        let zero = TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 0).unwrap(),
        };
        let foreground = TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(7).unwrap(),
            },
        };
        let background = TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation: TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(0).unwrap(),
            },
        };
        let original = [zero, foreground, zero, background, zero];
        let glyph = (9 << 6) | (0x41 << 11);
        let mut actual = vec![glyph; 68];
        actual.push(7 << 11);
        actual.extend([glyph; 68]);
        actual.push(3 << 6);
        actual.extend([glyph; 68]);
        let location = Location {
            page: 124,
            subcode: 32,
            resource: None,
        };
        assert_eq!(actual.len(), 206);
        // Each cluster fits alone; 204 glyphs plus five original anchors do not.
        assert!(check_rebuilt_count(&original, &actual, location).is_err());
        actual.pop();
        assert!(check_rebuilt_count(&original, &actual, location).is_ok());
    }
}

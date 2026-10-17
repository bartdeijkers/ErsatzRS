//! Editable segments retain exact context anchors and original command ordinals.
use super::{alignment, fail, glyph_family, raw, stream, style, Location, Result};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextObjectIdentity,
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
    let before = raw::Body::read(old.body(index), location, deadline)?;
    let uploaded = raw::Body::read(actual, location, deadline)?;
    let styles = raw::style_topology_changed(&before, &uploaded);
    if styles {
        check_style_count(definition.program.commands(), &uploaded, location)?;
    } else {
        check_rebuilt_count(definition.program.commands(), actual, location)?;
    }
    raw::protect_fixed_families(&before, &uploaded, location)?;
    let actual = uploaded.entries.as_slice();
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
        if cluster_command(definition, program, ordinal, styles, location)?
            || actual
                .get(cursor)
                .is_some_and(|entry| entry.editable(styles).is_some())
        {
            let start = ordinal;
            while ordinal < definition.program.commands().len()
                && cluster_command(definition, program, ordinal, styles, location)?
            {
                ordinal += 1;
            }
            let original = (start..ordinal)
                .map(|i| (i, definition.program.commands()[i]))
                .collect::<Vec<_>>();
            let mut inserted = Vec::new();
            while let Some(command) = actual.get(cursor).and_then(|entry| entry.editable(styles)) {
                inserted.push(command);
                cursor += 1;
            }
            let cluster = if styles {
                alignment::with_styles(&original, &inserted, location, deadline)?
            } else {
                alignment::with_anchors(&original, &inserted, location, deadline)?
            };
            output.retained.extend(cluster.retained);
            output.replacement.extend(cluster.commands);
        } else {
            cursor += consume_anchor(
                (definition, ordinal),
                program,
                images,
                (actual, cursor),
                aligned,
                location,
            )?;
            output.replacement.push(command);
            output.retained.push(Some(ordinal));
            ordinal += 1;
        }
    }
    for entry in &actual[cursor..] {
        output
            .replacement
            .push(entry.editable(styles).ok_or_else(|| fail(location))?);
        output.retained.push(None);
    }
    let original = original_spans(definition, program, old.markers[index] + 1, location)?;
    raw::qualify_selectors(
        &before,
        &uploaded,
        &original,
        &output.retained,
        location,
        deadline,
    )?;
    Ok(finish(output, definition))
}
fn consume_anchor(
    source: (&ersatzrs_domain::TeletextEnhancementObjectDefinition, usize),
    program: &EncodedTeletextLocalObjectProgram,
    images: (&stream::Image, &stream::Image),
    input: (&[raw::Entry], usize),
    aligned: &mut [u32],
    location: Location,
) -> Result<usize> {
    let (definition, ordinal) = source;
    let mut positions = program.command_positions().iter().filter(|position| {
        position.source().object == Some(definition.identity)
            && position.source().command == ordinal
    });
    let position = positions.next().ok_or_else(|| fail(location))?;
    if positions.next().is_some() {
        return Err(fail(location));
    }
    let count = position.semantic_word() - position.first_word() + 1;
    let entries = input
        .0
        .get(input.1..input.1 + count)
        .ok_or_else(|| fail(location))?;
    if entries
        .windows(2)
        .any(|pair| pair[0].index + 1 != pair[1].index)
    {
        return Err(fail(location));
    }
    let words = entries.iter().map(|entry| entry.word).collect::<Vec<_>>();
    let expected = &images.0.words[position.first_word()..=position.semantic_word()];
    anchor(
        definition.program.commands()[ordinal],
        expected,
        &words,
        images,
        location,
    )?;
    for (offset, word) in words.iter().enumerate() {
        aligned[position.first_word() + offset] = if stream::relocate(
            expected[offset],
            images.0,
            images.1,
            location,
        )? == expected[offset]
        {
            *word
        } else {
            expected[offset]
        };
    }
    Ok(count)
}
fn check_rebuilt_count(
    original: &[TeletextEnhancementCommand],
    actual: &[u32],
    location: Location,
) -> Result<()> {
    let glyphs = actual.iter().filter(|word| raw::is_glyph(**word)).count();
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
    styles: bool,
    location: Location,
) -> Result<bool> {
    let command = definition
        .program
        .commands()
        .get(ordinal)
        .ok_or_else(|| fail(location))?;
    let count = program
        .command_positions()
        .iter()
        .filter(|position| {
            position.source().object == Some(definition.identity)
                && position.source().command == ordinal
        })
        .count();
    let hidden = alignment::hidden_position(command).is_some();
    if (hidden && count != 0) || (!hidden && count != 1) {
        return Err(fail(location));
    }
    // Only a sealed absent span authenticates an invisible original position.
    // Other context commands retain exact physical anchor/span handling;
    // editable styles still require a unique authenticated original span.
    Ok(glyph_family(command).is_some() || (styles && style::kind(command).is_some()) || hidden)
}

fn check_style_count(
    original: &[TeletextEnhancementCommand],
    actual: &raw::Body,
    location: Location,
) -> Result<()> {
    let mandatory = original
        .iter()
        .filter(|command| glyph_family(command).is_none() && style::kind(command).is_none())
        .count();
    if actual
        .entries
        .iter()
        .filter(|entry| entry.editable(true).is_some())
        .count()
        .checked_add(mandatory)
        .is_none_or(|count| count > 208)
    {
        return Err(fail(location));
    }
    Ok(())
}
fn original_spans(
    definition: &ersatzrs_domain::TeletextEnhancementObjectDefinition,
    program: &EncodedTeletextLocalObjectProgram,
    start: usize,
    location: Location,
) -> Result<Vec<raw::OriginalSpan>> {
    program
        .command_positions()
        .iter()
        .filter(|position| {
            position.source().object == Some(definition.identity)
                && matches!(
                    definition.program.commands().get(position.source().command),
                    Some(TeletextEnhancementCommand::AtColumn {
                        operation: TeletextColumnOperation::Drcs { .. },
                        ..
                    })
                )
        })
        .map(|position| {
            Ok(raw::OriginalSpan {
                first: position
                    .first_word()
                    .checked_sub(start)
                    .ok_or_else(|| fail(location))?,
                word: position
                    .semantic_word()
                    .checked_sub(start)
                    .ok_or_else(|| fail(location))?,
                ordinal: position.source().command,
            })
        })
        .collect()
}
fn finish(
    mut output: Candidate,
    definition: &ersatzrs_domain::TeletextEnhancementObjectDefinition,
) -> Candidate {
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
    output
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
    use super::{check_rebuilt_count, check_style_count, cluster_command, raw, Location};
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

    fn compiled_position(
        row: u8,
    ) -> (
        ersatzrs_domain::TeletextEnhancementObjectDefinition,
        ersatzrs_ffmpeg::teletext::EncodedTeletextLocalObjectProgram,
    ) {
        use ersatzrs_domain::{
            TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
            TeletextEnhancementProgram, TeletextGlyphCode, TeletextObjectCatalogBudget,
            TeletextObjectId, TeletextObjectIdentity, TeletextObjectType, TeletextPageAddress,
            TeletextPageNumber, TeletextResolvedObjectPageLinks, TeletextSubcode,
        };
        use ersatzrs_ffmpeg::teletext::{
            allocate_teletext_resource_objects, compile_teletext_local_program_with_objects,
        };
        let owner = TeletextPageAddress {
            page: TeletextPageNumber::new(124).unwrap(),
            subcode: TeletextSubcode::new(32).unwrap(),
        };
        let budget = TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 208,
        };
        let definition = TeletextEnhancementObjectDefinition {
            identity: TeletextObjectIdentity::local(owner, TeletextObjectId::new(65520)),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(row, 8).unwrap(),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'A').unwrap(),
                    },
                },
            ])
            .unwrap(),
        };
        let catalog =
            TeletextEnhancementObjectCatalog::new(vec![definition.clone()], budget).unwrap();
        let allocations = allocate_teletext_resource_objects(&catalog, &[], 4).unwrap();
        let encoded = compile_teletext_local_program_with_objects(
            owner,
            &TeletextEnhancementProgram::new(vec![]).unwrap(),
            &catalog,
            &allocations,
            TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            },
            budget,
        )
        .unwrap();
        (definition, encoded)
    }

    #[test]
    fn hidden_position_requires_exact_absent_original_span_and_bounded_source() {
        let (hidden, zero_span) = compiled_position(0);
        let (physical, real_span) = compiled_position(1);
        let location = Location {
            page: 124,
            subcode: 32,
            resource: None,
        };
        assert!(cluster_command(&hidden, &zero_span, 0, false, location).unwrap());
        assert!(cluster_command(&hidden, &zero_span, 1, false, location).unwrap());
        // Both encodings are genuine and share identity/ordinal. A different
        // original's physical position must not authenticate an invisible one.
        assert!(cluster_command(&hidden, &real_span, 0, false, location).is_err());
        assert!(cluster_command(&physical, &zero_span, 0, false, location).is_err());
        assert!(cluster_command(&hidden, &zero_span, usize::MAX, false, location).is_err());
    }

    #[test]
    fn style_rebuild_limit_counts_insertions_and_every_remaining_context_anchor() {
        let location = Location {
            page: 124,
            subcode: 32,
            resource: None,
        };
        let position = TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 8).unwrap(),
        };
        let foreground = TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(1).unwrap(),
            },
        };
        let mut original = vec![position; 2];
        original.extend([foreground; 206]);
        let body = |count| {
            raw::Body::read(
                &vec![8 | (1 << 11); count],
                location,
                std::time::Instant::now() + std::time::Duration::from_secs(30),
            )
            .unwrap()
        };
        assert!(check_style_count(&original, &body(207), location).is_err());
        assert!(check_style_count(&original, &body(206), location).is_ok());
        assert!(check_style_count(&original, &body(0), location).is_ok());
    }
}

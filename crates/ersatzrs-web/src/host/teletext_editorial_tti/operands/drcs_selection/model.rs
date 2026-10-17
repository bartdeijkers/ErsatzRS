//! Sealed final command spans authenticate original references, not uploaded IDs.
use super::super::{compiled_program, located_word, objects, Location, PacketWords, Result};
use super::{at, glyph, Key, Model, OriginalGlyph, CAPACITY};
use ersatzrs_domain::{TeletextColumnOperation, TeletextEnhancementCommand, TeletextPageAddress};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextCompiledCommandPosition};
use std::{collections::BTreeSet, time::Instant};

pub(super) fn build(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    packets: &PacketWords,
    location: Location,
    deadline: Instant,
) -> Result<Option<Model>> {
    let Some(program) = compiled_program(service, owner) else {
        return Ok(None);
    };
    if program.command_positions().len() > CAPACITY
        || program.definition_positions().len() > CAPACITY
    {
        return Err(located_word(location, 0));
    }
    let mut model = Model::default();
    let mut keys = BTreeSet::new();
    let mut occupied = [false; CAPACITY];
    for marker in program.definition_positions() {
        location.check_deadline(deadline)?;
        let word = usize::from(marker.absolute_index());
        if word >= CAPACITY || model.boundaries[word] {
            return Err(located_word(location, word));
        }
        model.boundaries[word] = true;
    }
    for position in program.command_positions() {
        location.check_deadline(deadline)?;
        let word = position.semantic_word();
        if word >= CAPACITY || position.first_word() > word {
            return Err(located_word(location, word));
        }
        for (offset, occupied) in occupied
            .iter_mut()
            .enumerate()
            .take(word + 1)
            .skip(position.first_word())
        {
            if std::mem::replace(occupied, true) {
                return Err(located_word(location, offset));
            }
        }
        let raw = at(packets, word).ok_or_else(|| located_word(location, word))?;
        let original = original(service, owner, position, packets, location, deadline)?;
        if let Some((key, column, reference)) = original {
            if !keys.insert((key.object, key.command))
                || model.references[word].is_some()
                || glyph(raw, column).is_none()
            {
                return Err(located_word(location, word));
            }
            model.references[word] = Some(OriginalGlyph {
                key,
                column,
                reference,
                word,
            });
            if position.first_word() != word {
                let first = position.first_word();
                if first + 1 != word
                    || model.selectors[first].replace(word).is_some()
                    || super::selector(
                        at(packets, first).ok_or_else(|| located_word(location, first))?,
                    )
                    .is_none()
                {
                    return Err(located_word(location, first));
                }
            }
        }
    }
    Ok(Some(model))
}

fn original(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    position: &TeletextCompiledCommandPosition,
    packets: &PacketWords,
    location: Location,
    deadline: Instant,
) -> Result<Option<(Key, u8, ersatzrs_domain::TeletextDrcsGlyphReference)>> {
    let source = position.source();
    let catalog = service.declared().semantic().objects();
    let (key, command) = if let Some(identity) = source.object {
        let Some(definition) = catalog.definition(identity) else {
            return Ok(None);
        };
        (
            Key {
                object: Some(identity),
                command: source.command,
            },
            definition.program.commands().get(source.command),
        )
    } else {
        let Some(command) = position.original_local_command() else {
            return Ok(None);
        };
        (
            Key {
                object: None,
                command,
            },
            service
                .declared()
                .semantic()
                .page_enhancements()
                .get(&owner)
                .and_then(|page| page.local_program.as_ref())
                .and_then(|program| program.commands().get(command)),
        )
    };
    let Some(TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Drcs { reference },
    }) = command
    else {
        return Ok(None);
    };
    if key.object.is_some() {
        let (definition, _, _) = objects::authored_operation(catalog, owner, source, location)?;
        if objects::retained_word(
            compiled_program(service, owner)
                .ok_or_else(|| located_word(location, position.semantic_word()))?,
            definition,
            source,
            packets,
            location,
            deadline,
        )? != position.semantic_word()
        {
            return Err(located_word(location, position.semantic_word()));
        }
    }
    Ok(Some((key, *column, *reference)))
}

//! Retained Local calls resolve only to sealed original same-owner definitions.
use super::{fail, raw, stream, Location, Result};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementObjectCatalog,
    TeletextEnhancementObjectDefinition, TeletextObjectOffset, TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextLocalObjectProgram;

type Definitions<'a> = (
    &'a TeletextEnhancementObjectDefinition,
    &'a TeletextEnhancementObjectCatalog,
);

pub(super) fn retained(
    definitions: Definitions<'_>,
    original: Command,
    expected: &[u32],
    input: (&[raw::Entry], usize),
    compiled: (
        &EncodedTeletextLocalObjectProgram,
        (&stream::Image, &stream::Image),
    ),
    location: Location,
) -> Result<(usize, Command)> {
    let (program, (old, new)) = compiled;
    // Retain the old exact/relocated-anchor contract first. A derived physical
    // target is not newly editable merely because its authored call is Local.
    if let Some(entries) = input.0.get(input.1..input.1 + expected.len()) {
        let mut exact = entries
            .windows(2)
            .all(|pair| pair[0].index + 1 == pair[1].index);
        for (entry, word) in entries.iter().zip(expected) {
            exact &= stream::relocate(*word, old, new, location)? == entry.word;
        }
        if exact {
            return Ok((expected.len(), original));
        }
    }
    if decode(expected, definitions, program, old, location)? != original {
        return Err(fail(location));
    }
    let first = input.0.get(input.1).ok_or_else(|| fail(location))?;
    let count = if first.word & 63 >= 40 && (first.word >> 6) & 31 == 16 {
        2
    } else {
        1
    };
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
    Ok((count, decode(&words, definitions, program, new, location)?))
}

pub(super) fn decode(
    words: &[u32],
    (definition, catalog): Definitions<'_>,
    program: &EncodedTeletextLocalObjectProgram,
    image: &stream::Image,
    location: Location,
) -> Result<Command> {
    let invalid = || fail(location);
    if words.iter().any(|word| *word > 0x3ffff) {
        return Err(invalid());
    }
    let (offset, call) = match words {
        [call] => (None, *call),
        [prefix, call] if prefix & 63 >= 40 && (prefix >> 6) & 31 == 16 => (
            Some(
                TeletextObjectOffset::new(
                    u8::try_from((prefix & 63) - 40).map_err(|_| invalid())?,
                    u8::try_from(prefix >> 11).map_err(|_| invalid())?,
                )
                .map_err(|_| invalid())?,
            ),
            *call,
        ),
        _ => return Err(invalid()),
    };
    let object_type = match (call >> 6) & 31 {
        17 => TeletextObjectType::Active,
        18 => TeletextObjectType::Adaptive,
        19 => TeletextObjectType::Passive,
        _ => return Err(invalid()),
    };
    let address = call & 63;
    if !matches!(address, 40 | 41) {
        return Err(invalid());
    }
    let data = call >> 11;
    let index = usize::try_from(((address & 1) * 8 + (data >> 4)) * 13 + (data & 15))
        .map_err(|_| invalid())?;
    let marker = image
        .markers
        .iter()
        .position(|value| *value == index)
        .ok_or_else(invalid)?;
    let position = program
        .definition_positions()
        .get(marker)
        .ok_or_else(invalid)?;
    let target = position.target();
    let original = catalog.definition(target).ok_or_else(invalid)?;
    let canonical = 0x28
        | u32::from(index >= 104)
        | (call & (31 << 6))
        | (u32::try_from(((index / 13) & 7) * 16 + index % 13).map_err(|_| invalid())? << 11);
    if call != canonical
        || target.owner() != definition.identity.owner()
        || target.owner().is_none()
        || original.object_type != object_type
        || position.object_type() != object_type
        || !definition.object_type.may_invoke(object_type)
    {
        return Err(invalid());
    }
    Ok(Command::InvokeObject {
        target,
        object_type,
        offset,
    })
}

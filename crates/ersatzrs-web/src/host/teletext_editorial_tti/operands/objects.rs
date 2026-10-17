//! Original local definitions, never an inferred expanded invocation or carrier.

use super::{
    compiled_program, located_word, operand, Location, PacketWords, Result,
    TeletextColumnOperation, TeletextCompiledCommandPosition, TeletextEnhancementCommand,
    TeletextLocalObjectOperandOverlay, TeletextPageAddress,
};
use ersatzrs_domain::{
    TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
    TeletextEnhancementSource, TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, EncodedTeletextLocalObjectProgram,
};
use std::time::Instant;

pub(super) fn reconcile(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    position: &TeletextCompiledCommandPosition,
    edited: (&PacketWords, u32),
    location: Location,
    deadline: Instant,
) -> Result<TeletextLocalObjectOperandOverlay> {
    let word = position.semantic_word();
    let (_, column, expected) = authored_operation(
        service.declared().semantic().objects(),
        owner,
        position.source(),
        location,
    )?;
    let before = word_at(edited.0, word).ok_or_else(|| located_word(location, word))?;
    let replacement =
        operand(column, expected, before, edited.1).ok_or_else(|| located_word(location, word))?;
    let identity = position
        .source()
        .object
        .ok_or_else(|| located_word(location, word))?;
    let change = TeletextLocalObjectOperandOverlay::new(
        identity,
        position.source().command,
        column,
        expected,
        replacement,
    )
    .map_err(|_| located_word(location, word))?;
    if command_word(service, &change, expected, edited.0, location, deadline)? != word {
        return Err(located_word(location, word));
    }
    Ok(change)
}

fn authored_operation(
    catalog: &TeletextEnhancementObjectCatalog,
    owner: TeletextPageAddress,
    source: TeletextEnhancementSource,
    location: Location,
) -> Result<(
    &TeletextEnhancementObjectDefinition,
    u8,
    TeletextColumnOperation,
)> {
    let fail = || location.error(super::NativeTeletextEditorialTtiFailure::Packet);
    let identity = source
        .object
        .filter(|identity| identity.owner() == Some(owner))
        .ok_or_else(fail)?;
    let definition = catalog.definition(identity).ok_or_else(fail)?;
    let Some(TeletextEnhancementCommand::AtColumn { column, operation }) =
        definition.program.commands().get(source.command)
    else {
        return Err(fail());
    };
    if !matches!(
        operation,
        TeletextColumnOperation::G0 { .. }
            | TeletextColumnOperation::G1 { .. }
            | TeletextColumnOperation::Foreground { .. }
            | TeletextColumnOperation::Background { .. }
            | TeletextColumnOperation::Attributes { .. }
            | TeletextColumnOperation::G2 { .. }
            | TeletextColumnOperation::G0Diacritic { .. }
    ) {
        return Err(fail());
    }
    Ok((definition, *column, *operation))
}

pub(super) fn command_word(
    service: &EncodedTeletextAuthoredService,
    change: &TeletextLocalObjectOperandOverlay,
    expected: TeletextColumnOperation,
    packets: &PacketWords,
    location: Location,
    deadline: Instant,
) -> Result<usize> {
    let source = TeletextEnhancementSource {
        object: Some(change.identity()),
        command: change.command(),
    };
    let (definition, column, operation) = authored_operation(
        service.declared().semantic().objects(),
        change.owner(),
        source,
        location,
    )?;
    if (column, operation) != (change.column(), expected) {
        return Err(location.error(super::NativeTeletextEditorialTtiFailure::Packet));
    }
    let program = compiled_program(service, change.owner())
        .ok_or_else(|| location.error(super::NativeTeletextEditorialTtiFailure::Mapping))?;
    retained_word(program, definition, source, packets, location, deadline)
}

fn retained_word(
    program: &EncodedTeletextLocalObjectProgram,
    definition: &TeletextEnhancementObjectDefinition,
    source: TeletextEnhancementSource,
    packets: &PacketWords,
    location: Location,
    deadline: Instant,
) -> Result<usize> {
    let fail = || location.error(super::NativeTeletextEditorialTtiFailure::Packet);
    if definition.identity.owner() != Some(program.owner())
        || source.object != Some(definition.identity)
    {
        return Err(fail());
    }
    let mut position = None;
    for candidate in program.command_positions() {
        location.check_deadline(deadline)?;
        if candidate.source() == source && position.replace(candidate).is_some() {
            return Err(fail());
        }
    }
    let position = position.ok_or_else(fail)?;
    let word = position.semantic_word();
    if word >= 16 * 13
        || position.first_word() > word
        || position.original_local_command().is_some()
    {
        return Err(located_word(location, word));
    }
    let mut marker = None;
    for candidate in program.definition_positions() {
        location.check_deadline(deadline)?;
        if candidate.target() == definition.identity && marker.replace(candidate).is_some() {
            return Err(fail());
        }
    }
    let marker = marker.ok_or_else(fail)?;
    let index = usize::from(marker.absolute_index());
    let number = usize::from(marker.object_number());
    if marker.object_type() != definition.object_type
        || number != ((index / 13) << 4) | (index % 13)
        || index >= position.first_word()
        || word_at(packets, index)
            != Some(definition_word(
                marker.object_number(),
                definition.object_type,
            ))
    {
        return Err(located_word(location, index));
    }
    for other in program.definition_positions() {
        location.check_deadline(deadline)?;
        let other_index = usize::from(other.absolute_index());
        if other_index > index && other_index <= word {
            return Err(located_word(location, word));
        }
    }
    Ok(word)
}

fn definition_word(number: u8, object_type: TeletextObjectType) -> u32 {
    let kind = match object_type {
        TeletextObjectType::Active => 1,
        TeletextObjectType::Adaptive => 2,
        TeletextObjectType::Passive => 3,
    };
    let number = u32::from(number);
    (0x28 | (number >> 7)) | ((0x14 | kind) << 6) | ((number & 127) << 11)
}

fn word_at(packets: &PacketWords, word: usize) -> Option<u32> {
    packets
        .get(word / 13)?
        .as_ref()
        .map(|packet| packet[word % 13])
}

#[cfg(test)]
#[path = "objects_tests.rs"]
mod tests;

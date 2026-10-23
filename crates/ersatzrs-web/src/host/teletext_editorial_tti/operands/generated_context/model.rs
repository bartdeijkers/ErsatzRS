//! Authenticate every original word and typed Base-cell wrapper before alignment.
use super::{
    compiled_program, located_word, operand, Atom, EncodedTeletextAuthoredService, Location,
    Result, TeletextPageAddress, CAPACITY,
};
use ersatzrs_domain::{
    TeletextEnhancementCharsetDesignation, TeletextEnhancementCommand, TeletextEnhancementPosition,
    TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectScope, TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextLocalObjectProgram, TeletextBaseCellCommandKind as Kind,
    TeletextCompiledCommandPosition, TeletextMainCompositionSource as Source,
};
use std::time::Instant;

pub(super) fn build<'a>(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    words: &'a [u32],
    location: Location,
    deadline: Instant,
) -> Result<Vec<Atom<'a>>> {
    location.check_deadline(deadline)?;
    let native = compiled_program(service, owner).ok_or_else(|| located_word(location, 0))?;
    let original = service
        .declared()
        .semantic()
        .page_enhancements()
        .get(&owner)
        .and_then(|page| page.local_program.as_ref())
        .ok_or_else(|| located_word(location, 0))?;
    if native.owner() != owner
        || words.len() > CAPACITY
        || native.main_triplet_count() != words.len()
        || native.command_positions().len() > CAPACITY
        || original.commands().len() > CAPACITY
    {
        return Err(located_word(location, 0));
    }
    let all_positions = native.command_positions();
    let main_end = all_positions.partition_point(|position| position.semantic_word() < words.len());
    if all_positions[main_end..]
        .iter()
        .any(|position| position.first_word() < words.len())
    {
        return Err(located_word(location, 0));
    }
    let positions = &all_positions[..main_end];
    coverage(positions, words.len(), location, deadline)?;
    let mut result = Vec::new();
    let mut seen = vec![false; original.commands().len()];
    let mut cursor = 0;
    while let Some(position) = positions.get(cursor) {
        location.check_deadline(deadline)?;
        let body = span(position, words, location)?;
        match position.original_main_source() {
            Some(Source::OriginalPage { command }) => {
                let expected = original
                    .commands()
                    .get(command)
                    .ok_or_else(|| located_word(location, position.first_word()))?;
                if position.original_local_command() != Some(command)
                    || position.original_base_glyph().is_some()
                    || std::mem::replace(&mut seen[command], true)
                {
                    return Err(located_word(location, position.first_word()));
                }
                let atom = match *expected {
                    TeletextEnhancementCommand::AtColumn { column, operation }
                        if body.len() == 1
                            && operand(column, operation, body[0], body[0]).is_some() =>
                    {
                        Atom::Scalar {
                            command,
                            column,
                            expected: operation,
                            before: body[0],
                        }
                    }
                    TeletextEnhancementCommand::InvokeObject {
                        target,
                        object_type,
                        offset,
                    } if target.scope() == TeletextObjectScope::Local => {
                        local_call(native, body, target, object_type, offset, location)?
                    }
                    _ => Atom::Fixed(body),
                };
                result.push(atom);
                cursor += 1;
            }
            Some(Source::BaseCell {
                kind: Kind::Position,
                ..
            }) => {
                if position.original_local_command().is_some()
                    || position.original_base_glyph().is_some()
                {
                    return Err(located_word(location, position.first_word()));
                }
                result.push(Atom::Fixed(body));
                cursor += 1;
            }
            Some(Source::BaseCell { .. }) => {
                result.push(glyph(positions, &mut cursor, words, location)?);
            }
            _ => return Err(located_word(location, position.first_word())),
        }
    }
    for (command, present) in original.commands().iter().zip(seen) {
        location.check_deadline(deadline)?;
        if !present
            && !matches!(command, TeletextEnhancementCommand::SetPosition { position } if position.row() == 0)
        {
            return Err(located_word(location, 0));
        }
    }
    Ok(result)
}

fn local_call<'a>(
    native: &EncodedTeletextLocalObjectProgram,
    body: &'a [u32],
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    offset: Option<TeletextObjectOffset>,
    location: Location,
) -> Result<Atom<'a>> {
    let fail = || located_word(location, 0);
    let definition = native
        .definition_positions()
        .iter()
        .find(|definition| definition.target() == target && definition.object_type() == object_type)
        .ok_or_else(fail)?;
    let number = u32::from(definition.object_number());
    let kind = match object_type {
        TeletextObjectType::Active => 1,
        TeletextObjectType::Adaptive => 2,
        TeletextObjectType::Passive => 3,
    };
    let invocation = (0x28 | (number >> 7)) | ((0x10 | kind) << 6) | ((number & 127) << 11);
    if body.len() != 1 + usize::from(offset.is_some())
        || body.last() != Some(&invocation)
        || usize::from(definition.absolute_index()) <= native.main_triplet_count()
    {
        return Err(fail());
    }
    if let Some(offset) = offset {
        let origin =
            (40 + u32::from(offset.row())) | (16 << 6) | (u32::from(offset.column()) << 11);
        if body[0] != origin {
            return Err(fail());
        }
    }
    Ok(Atom::LocalCall {
        words: body,
        definition: definition.absolute_index(),
        main_words: native.main_triplet_count(),
    })
}

/// Main repacking moves every immutable Local marker by the same amount.
/// Only its native pointer bits may differ; complete re-encoding remains final.
pub(super) fn relocated_call(
    words: &[u32],
    definition: u8,
    old_main: usize,
    new_main: usize,
    uploaded: &[u32],
) -> bool {
    let Some((&before, prefix)) = words.split_last() else {
        return false;
    };
    let Some((&after, uploaded_prefix)) = uploaded.split_last() else {
        return false;
    };
    let Some(index) = usize::from(definition)
        .checked_add(new_main)
        .and_then(|index| index.checked_sub(old_main))
        .filter(|index| *index < CAPACITY && *index > new_main)
    else {
        return false;
    };
    let number = (index / 13) * 16 + index % 13;
    let Ok(number) = u32::try_from(number) else {
        return false;
    };
    let expected = (before & !(1 | (127 << 11))) | (number >> 7) | ((number & 127) << 11);
    prefix == uploaded_prefix && after == expected
}

fn coverage(
    positions: &[TeletextCompiledCommandPosition],
    count: usize,
    location: Location,
    deadline: Instant,
) -> Result<()> {
    let mut cursor = 0;
    for position in positions {
        location.check_deadline(deadline)?;
        if position.source().object.is_some()
            || position.first_word() != cursor
            || position.semantic_word() < cursor
            || position.semantic_word() >= count
        {
            return Err(located_word(location, cursor));
        }
        cursor = position.semantic_word() + 1;
    }
    if cursor != count {
        return Err(located_word(location, cursor));
    }
    Ok(())
}

fn span<'a>(
    position: &TeletextCompiledCommandPosition,
    words: &'a [u32],
    location: Location,
) -> Result<&'a [u32]> {
    words
        .get(position.first_word()..=position.semantic_word())
        .ok_or_else(|| located_word(location, position.first_word()))
}

fn glyph<'a>(
    positions: &[TeletextCompiledCommandPosition],
    cursor: &mut usize,
    words: &'a [u32],
    location: Location,
) -> Result<Atom<'a>> {
    let fail = || located_word(location, *cursor);
    let first = positions.get(*cursor).ok_or_else(fail)?;
    let Some(Source::BaseCell {
        position: cell,
        kind,
    }) = first.original_main_source()
    else {
        return Err(fail());
    };
    if matches!(kind, Kind::LatinCharset | Kind::GlyphCharset) {
        wrapper(first, words, cell, location)?;
        *cursor += 1;
    } else if kind != Kind::Glyph {
        return Err(located_word(location, first.first_word()));
    }
    let position = positions
        .get(*cursor)
        .ok_or_else(|| located_word(location, first.first_word()))?;
    if position.original_main_source()
        != Some(Source::BaseCell {
            position: cell,
            kind: Kind::Glyph,
        })
        || position.original_base_glyph() != Some(cell)
        || position.original_local_command().is_some()
    {
        return Err(located_word(location, position.first_word()));
    }
    let body = span(position, words, location)?;
    *cursor += 1;
    if let Some(next) = positions.get(*cursor) {
        if next.original_main_source()
            == Some(Source::BaseCell {
                position: cell,
                kind: Kind::CharsetRestore,
            })
        {
            wrapper(next, words, cell, location)?;
            *cursor += 1;
        }
    }
    let end = positions[*cursor - 1].semantic_word();
    let original = words
        .get(first.first_word()..=end)
        .ok_or_else(|| located_word(location, first.first_word()))?;
    Ok(Atom::Glyph {
        words: body,
        original,
        column: cell.column(),
    })
}

fn wrapper(
    position: &TeletextCompiledCommandPosition,
    words: &[u32],
    cell: TeletextEnhancementPosition,
    location: Location,
) -> Result<()> {
    let body = span(position, words, location)?;
    if body.len() != 1
        || !charset_word(body[0], cell.column())
        || position.original_local_command().is_some()
        || position.original_base_glyph().is_some()
    {
        return Err(located_word(location, position.first_word()));
    }
    Ok(())
}

pub(super) fn charset_word(word: u32, column: u8) -> bool {
    word <= 0x3ffff
        && word & 0x7ff == u32::from(column) | (8 << 6)
        && u8::try_from(word >> 11)
            .ok()
            .is_some_and(|value| TeletextEnhancementCharsetDesignation::new(value).is_ok())
}

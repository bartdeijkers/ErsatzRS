//! Authenticate every original word and typed Base-cell wrapper before alignment.
use super::{
    compiled_program, located_word, operand, Atom, EncodedTeletextAuthoredService, Location,
    Result, TeletextPageAddress, CAPACITY,
};
use ersatzrs_domain::{
    TeletextEnhancementCharsetDesignation, TeletextEnhancementCommand, TeletextEnhancementPosition,
};
use ersatzrs_ffmpeg::teletext::{
    TeletextBaseCellCommandKind as Kind, TeletextCompiledCommandPosition,
    TeletextMainCompositionSource as Source,
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
        || !native.definition_positions().is_empty()
        || words.len() > CAPACITY
        || native.main_triplet_count() != words.len()
        || native.command_positions().len() > CAPACITY
        || original.commands().len() > CAPACITY
    {
        return Err(located_word(location, 0));
    }
    let positions = native.command_positions();
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
                let atom = if let TeletextEnhancementCommand::AtColumn { column, operation } =
                    *expected
                {
                    if body.len() == 1 && operand(column, operation, body[0], body[0]).is_some() {
                        Atom::Scalar {
                            command,
                            column,
                            expected: operation,
                            before: body[0],
                        }
                    } else {
                        Atom::Fixed(body)
                    }
                } else {
                    Atom::Fixed(body)
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

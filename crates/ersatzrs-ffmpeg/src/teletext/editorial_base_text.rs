//! Checked original Base-text correspondence, never reverse inference from pixels.

use super::{
    charset, layout, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextCompiledCommandPosition,
};
use ersatzrs_domain::{
    TeletextBaseAuthoredContent, TeletextBaseTextEdit, TeletextBaseTextSpelling,
    TeletextColumnOperation, TeletextDiacriticIndex, TeletextGlyphCode, TeletextPageAddress,
};

/// Recognize an exact generated Text glyph's same-family scalar edit.
///
/// Only private factory-bound Base glyph provenance is used. The original
/// scalar and charset come from the same checked service; the existing encoder
/// must reproduce the replacement operation exactly. Unqualified aliases and
/// noncanonical coded alternatives are refused, never silently normalized.
/// Receipts, protected effects, complete uploaded packet correspondence and
/// concept/publication authority remain the host's responsibility.
#[must_use]
pub fn resolve_teletext_base_text_edit(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    position: &TeletextCompiledCommandPosition,
    before: u32,
    after: u32,
) -> Option<TeletextBaseTextEdit> {
    resolve(service, owner, position, before, after, false)
}

/// Recognize the same checked Base edit while retaining a noncanonical native
/// spelling explicitly. This does not relax the canonical-only API or grant
/// receipt, protected-effect, concept or publication authority.
#[must_use]
pub fn resolve_teletext_lossless_base_text_edit(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    position: &TeletextCompiledCommandPosition,
    before: u32,
    after: u32,
) -> Option<TeletextBaseTextEdit> {
    resolve(service, owner, position, before, after, true)
}

fn resolve(
    service: &EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    position: &TeletextCompiledCommandPosition,
    before: u32,
    after: u32,
    allow_native: bool,
) -> Option<TeletextBaseTextEdit> {
    let page = service.pages().iter().find_map(|page| match page {
        EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address() == owner => {
            Some(page)
        }
        _ => None,
    })?;
    if !page
        .normalized()
        .encoded()
        .command_positions()
        .contains(position)
    {
        return None;
    }
    let cell = position.original_base_glyph()?;
    let original = page.normalized().original();
    let old_operation = original.base_replacement(cell.row(), cell.column())?;
    let (mode, code) = match old_operation {
        TeletextColumnOperation::G2 { code } => (0x0f, code.get()),
        TeletextColumnOperation::G0Diacritic { code, diacritic } => {
            (0x10 | diacritic.get(), code.get())
        }
        _ => return None,
    };
    if before != u32::from(cell.column()) | (u32::from(mode) << 6) | (u32::from(code) << 11)
        || after & 0x3f != u32::from(cell.column())
    {
        return None;
    }
    let new_mode = u8::try_from((after >> 6) & 31).ok()?;
    if !matches!((mode, new_mode), (0x0f, 0x0f) | (0x10..=0x1f, 0x10..=0x1f)) {
        return None;
    }
    let replacement_character_code = u8::try_from(after >> 11).ok()?;
    let designation = original.base_replacement_charset(cell.row(), cell.column())?;
    let character = if allow_native {
        charset::decode_base_text_spelling(
            new_mode,
            replacement_character_code,
            designation.pair(),
        )?
    } else {
        charset::decode_generated_replacement(
            new_mode,
            replacement_character_code,
            designation.pair(),
        )?
    };
    let draft = service
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)?;
    let layout =
        layout::compile_for_service_composition(service.declared().semantic(), draft).ok()?;
    let canonical = layout.encode_character(character);
    let exact_word = canonical
        .is_some_and(|value| value.enhancement == Some((new_mode, replacement_character_code)));
    if !exact_word && !allow_native {
        return None;
    }
    let TeletextBaseAuthoredContent::Text {
        character: expected,
    } = original.base_display().cells()[usize::from(cell.row())][usize::from(cell.column())]
        .authored()?
        .content()
    else {
        return None;
    };
    let edit = TeletextBaseTextEdit::new(owner, cell, expected, character).ok()?;
    let fallback =
        original.base_display().cells()[usize::from(cell.row())][usize::from(cell.column())].code();
    if !allow_native || exact_word && canonical.is_some_and(|value| value.fallback == fallback) {
        return Some(edit);
    }
    let code = TeletextGlyphCode::new(replacement_character_code).ok()?;
    let operation = if new_mode == 0x0f {
        TeletextColumnOperation::G2 { code }
    } else {
        TeletextColumnOperation::G0Diacritic {
            code,
            diacritic: TeletextDiacriticIndex::new(new_mode & 15).ok()?,
        }
    };
    edit.with_native_spelling(
        TeletextBaseTextSpelling::new(
            owner,
            cell,
            character,
            TeletextGlyphCode::new(fallback).ok()?,
            designation,
            operation,
        )
        .ok()?,
    )
    .ok()
}

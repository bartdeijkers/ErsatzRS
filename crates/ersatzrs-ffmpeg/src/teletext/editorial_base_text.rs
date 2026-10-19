//! Checked original Base-text correspondence, never reverse inference from pixels.

use super::{
    charset, layout, EncodedTeletextAuthoredPage, EncodedTeletextAuthoredService,
    TeletextCompiledCommandPosition,
};
use ersatzrs_domain::{
    TeletextBaseAuthoredContent, TeletextBaseTextEdit, TeletextColumnOperation, TeletextPageAddress,
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
    let pair = original
        .base_replacement_charset(cell.row(), cell.column())?
        .pair();
    let character =
        charset::decode_generated_replacement(new_mode, replacement_character_code, pair)?;
    let draft = service
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)?;
    let layout =
        layout::compile_for_service_composition(service.declared().semantic(), draft).ok()?;
    if layout.encode_character(character)?.enhancement
        != Some((new_mode, replacement_character_code))
    {
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
    TeletextBaseTextEdit::new(owner, cell, expected, character).ok()
}

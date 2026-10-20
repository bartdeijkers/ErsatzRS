//! One checked native spelling remains Base intent, never an authored Local command.
use super::{
    charset, Layout, PositionedBaseGlyph, TeletextColumnOperation, TeletextEncodingError,
    TeletextEnhancementPosition, TeletextPageDraft, TeletextSemanticService,
};

pub(super) fn initialize(
    layout: &mut Layout,
    service: &TeletextSemanticService,
    page: &TeletextPageDraft,
) -> Result<(), TeletextEncodingError> {
    for spelling in service.base_text_spellings_for(page.address) {
        let position = spelling.position();
        let failure = || TeletextEncodingError::BaseTextSpelling {
            address: page.address,
            row: position.row(),
            column: position.column(),
        };
        let (mode, data) = match spelling.operation() {
            TeletextColumnOperation::G2 { code } => (0x0f, code.get()),
            TeletextColumnOperation::G0Diacritic { code, diacritic } => {
                (0x10 | diacritic.get(), code.get())
            }
            _ => return Err(failure()),
        };
        let glyph = PositionedBaseGlyph::new(
            position.row(),
            position.column(),
            mode,
            data,
            layout.text_charset,
        )
        .ok_or_else(failure)?;
        if glyph.charset() != spelling.charset()
            || charset::decode_base_text_spelling(mode, data, glyph.charset().pair())
                != Some(spelling.character())
        {
            return Err(failure());
        }
        // The service has already bound the exact scalar to an existing Text run.
        // Independent inverse meaning above prevents a malformed native hint
        // from painting a different glyph than the declared logical character.
        let encoding = charset::Character {
            fallback: spelling.fallback().get(),
            enhancement: Some((mode, data)),
        };
        layout
            .native_text_encodings
            .insert(position, (spelling.character(), encoding));
    }
    Ok(())
}

impl Layout {
    pub(in super::super) fn encode_character_at(
        &self,
        row: u8,
        column: u8,
        character: char,
    ) -> Option<charset::Character> {
        let position = TeletextEnhancementPosition::new(row, column).ok()?;
        if let Some((expected, encoding)) = self.native_text_encodings.get(&position) {
            return (*expected == character).then_some(*encoding);
        }
        self.encode_character(row, character)
    }
}

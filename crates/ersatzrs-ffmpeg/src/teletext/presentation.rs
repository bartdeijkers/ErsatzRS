//! Explicit presentation declarations and X28/0 or M29/0 body coding.
//!
//! EN 300 706 tables 4 and 11 share these 234 data bits. This does not
//! validate a complete service, compile enhancement programs or publish pages.

use ersatzrs_domain::{
    TeletextCharsetValueError, TeletextColourReference, TeletextColourRemapping,
    TeletextLevel25Palette, TeletextLevel25PresentationDefaults, TeletextNationalOption,
    TeletextPageCharsetDesignation, TeletextPanelStatus, TeletextPanelWidth, TeletextSidePanels,
};

use super::packets::{hamming_24_18, hamming_8_4};

/// Preserve the legacy encoder's explicit Western Latin page declaration.
/// Reserved national option seven is rejected rather than lifted differently.
pub(super) fn western_latin_v1_presentation(
    national: TeletextNationalOption,
) -> Result<TeletextLevel25PresentationDefaults, TeletextCharsetValueError> {
    let primary = TeletextPageCharsetDesignation::new(national.get())?;
    Ok(TeletextLevel25PresentationDefaults {
        primary,
        secondary: None,
        palette: TeletextLevel25Palette::standard(),
        screen_colour: TeletextColourReference::BLACK,
        row_colour: TeletextColourReference::BLACK,
        black_background_substitution: false,
        remapping: TeletextColourRemapping::FgClut0BgClut0,
        panels: TeletextSidePanels {
            left: false,
            right: false,
            status: TeletextPanelStatus::Level35Only,
            width: TeletextPanelWidth::ZERO,
        },
    })
}

/// Encode thirteen 18-bit X28/0 or M29/0 presentation words.
///
/// Encodes only the explicit supplied values, without Hamming coding. This does
/// not create or authenticate an authored declaration, establish provenance,
/// validate a service or grant publication authority. Callers must resolve the
/// applicable declaration policy, especially `DecoderDependent` secondary
/// selection, before constructing the supplied values.
#[must_use]
pub fn encode_teletext_presentation_words(
    defaults: &TeletextLevel25PresentationDefaults,
) -> [u32; 13] {
    let mut words = [0_u32; 13];
    let mut offset = 0;
    write_bits(&mut words, &mut offset, 0, 7);
    write_bits(
        &mut words,
        &mut offset,
        u32::from(defaults.primary.get()),
        7,
    );
    write_bits(
        &mut words,
        &mut offset,
        u32::from(
            defaults
                .secondary
                .map_or(0x7f, TeletextPageCharsetDesignation::get),
        ),
        7,
    );
    write_bits(&mut words, &mut offset, u32::from(defaults.panels.left), 1);
    write_bits(&mut words, &mut offset, u32::from(defaults.panels.right), 1);
    write_bits(
        &mut words,
        &mut offset,
        u32::from(defaults.panels.status == TeletextPanelStatus::Level25And35),
        1,
    );
    write_bits(
        &mut words,
        &mut offset,
        u32::from(defaults.panels.width.get()),
        4,
    );
    for colour in defaults.palette.clut2.iter().chain(&defaults.palette.clut3) {
        for component in colour.get() {
            write_bits(&mut words, &mut offset, u32::from(component), 4);
        }
    }
    write_bits(
        &mut words,
        &mut offset,
        u32::from(defaults.screen_colour.get()),
        5,
    );
    write_bits(
        &mut words,
        &mut offset,
        u32::from(defaults.row_colour.get()),
        5,
    );
    write_bits(
        &mut words,
        &mut offset,
        u32::from(defaults.black_background_substitution),
        1,
    );
    let remapping = match defaults.remapping {
        TeletextColourRemapping::FgClut0BgClut0 => 0,
        TeletextColourRemapping::FgClut0BgClut1 => 1,
        TeletextColourRemapping::FgClut0BgClut2 => 2,
        TeletextColourRemapping::FgClut1BgClut1 => 3,
        TeletextColourRemapping::FgClut1BgClut2 => 4,
        TeletextColourRemapping::FgClut2BgClut1 => 5,
        TeletextColourRemapping::FgClut2BgClut2 => 6,
        TeletextColourRemapping::FgClut2BgClut3 => 7,
    };
    write_bits(&mut words, &mut offset, remapping, 3);
    words
}

/// Encode a checked display declaration as a pre-bit-reversal packet body.
///
/// Designation is zero. The first seven bits select basic display/odd parity
/// for X28/0, or packet function zero for M29/0; the remaining coding is identical.
pub(super) fn encode_presentation(defaults: &TeletextLevel25PresentationDefaults) -> [u8; 40] {
    let words = encode_teletext_presentation_words(defaults);
    let mut body = [0_u8; 40];
    body[0] = hamming_8_4(0);
    for (target, word) in body[1..].chunks_exact_mut(3).zip(words) {
        target.copy_from_slice(&hamming_24_18(word));
    }
    body
}

fn write_bits(words: &mut [u32; 13], offset: &mut usize, value: u32, width: u8) {
    for bit in 0..width {
        words[*offset / 18] |= ((value >> bit) & 1) << (*offset % 18);
        *offset += 1;
    }
}

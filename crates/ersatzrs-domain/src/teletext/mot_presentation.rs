//! Receiver presentation fallback from original POP flags, not an X28 declaration.

use super::{
    TeletextCharsetValueError, TeletextColourReference, TeletextColourRemapping,
    TeletextLevel25Palette, TeletextLevel25PresentationDefaults, TeletextMotFallback,
    TeletextNationalOption, TeletextPageCharsetDesignation, TeletextPanelStatus,
    TeletextPanelWidth, TeletextSidePanels,
};

/// Resolve Table 21 and Annex D.2.1 without changing header charset or colours.
/// The caller must establish the original associated POP and absence of both
/// appropriate page X28/0 and own-magazine M29/0. GPOP flags are reserved.
/// No declaration, resolved owner or publication authority is manufactured.
/// # Errors
/// Rejects reserved header national option seven rather than guessing a charset.
pub fn resolve_teletext_mot_presentation_fallback(
    national: TeletextNationalOption,
    flags: TeletextMotFallback,
) -> Result<TeletextLevel25PresentationDefaults, TeletextCharsetValueError> {
    let panel = if flags.protocol_no_panels() {
        0
    } else {
        flags.panel_code()
    };
    Ok(TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(national.get())?,
        secondary: None,
        palette: TeletextLevel25Palette::standard(),
        screen_colour: TeletextColourReference::BLACK,
        row_colour: TeletextColourReference::BLACK,
        black_background_substitution: !flags.protocol_no_panels()
            && flags.black_background_substitution(),
        remapping: TeletextColourRemapping::FgClut0BgClut0,
        panels: TeletextSidePanels {
            left: matches!(panel, 1 | 3),
            right: matches!(panel, 2 | 3),
            status: TeletextPanelStatus::Level25And35,
            width: if panel == 3 {
                TeletextPanelWidth::EIGHT
            } else {
                TeletextPanelWidth::ZERO
            },
        },
    })
}

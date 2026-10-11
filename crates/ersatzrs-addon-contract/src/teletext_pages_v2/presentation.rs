//! Raw authored X28/M29 fields; range and precedence checks belong to the host.

use super::required_nullable;

wire_object! {
    /// Complete editable CLUT2/3; Level 2.5 fixed CLUT0/1 are not producer fields.
    pub struct TeletextLevel25PaletteWire {
        pub clut2: [[u8; 3]; 8],
        pub clut3: [[u8; 3]; 8],
    }
}

wire_string_enum! {
/// Assigned spacing-colour remapping names, independent of domain enums.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextColourRemappingWire {
    FgClut0BgClut0,
    FgClut0BgClut1,
    FgClut0BgClut2,
    FgClut1BgClut1,
    FgClut1BgClut2,
    FgClut2BgClut1,
    FgClut2BgClut2,
    FgClut2BgClut3,
}
}

wire_string_enum! {
/// Preserved panel applicability; metadata is not Level 3.5 engine support.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextPanelStatusWire {
    Level35Only,
    Level25And35,
}
}

wire_object! {
    /// Raw panel flags and width; no automatic clipping or canonicalization.
    pub struct TeletextSidePanelsWire {
        pub left: bool,
        pub right: bool,
        pub status: TeletextPanelStatusWire,
        pub width: u8,
    }
}

wire_object! {
    /// Present X28/M29 declaration, retaining original charset aliases.
    pub struct TeletextLevel25PresentationWire {
        pub primary: u8,
        #[serde(deserialize_with = "required_nullable")]
        pub secondary: Option<u8>,
        pub palette: TeletextLevel25PaletteWire,
        pub screen_colour: u8,
        pub row_colour: u8,
        pub black_background_substitution: bool,
        pub remapping: TeletextColourRemappingWire,
        pub panels: TeletextSidePanelsWire,
    }
}

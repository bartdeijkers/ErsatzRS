//! Ordered raw enhancement intent, without raw pointers or transport headers.

use super::{
    required_nullable, TeletextDrcsGlyphReferenceWire, TeletextObjectIdentityWire,
    TeletextObjectTypeWire,
};

wire_object! {
    /// Relative position or invocation offset; host checks its context-specific range.
    pub struct TeletextEnhancementCoordinatesWire {
        pub row: u8,
        pub column: u8,
    }
}

wire_string_enum! {
/// Whether the G3 command is also intended for Level 1.5 clients.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextG3CompatibilityWire {
    Level15,
    Level25,
}
}

wire_string_enum! {
/// Assigned nonspacing flash modes.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextFlashModeWire {
    Steady,
    Normal,
    Inverted,
    AdjacentClut,
}
}

wire_string_enum! {
/// Assigned nonspacing flash rate/phase names.
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextFlashRateWire {
    Slow1Hz,
    Fast2HzPhase1,
    Fast2HzPhase2,
    Fast2HzPhase3,
    Fast2HzIncremental,
    Fast2HzDecremental,
}
}

wire_tagged_enum! {
/// Raw glyph/attribute intent; Unicode text does not replace repertoire codes.
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextColumnOperationWire {
    Foreground {
        colour: u8,
    },
    Background {
        colour: u8,
    },
    DesignateCharset {
        designation: u8,
    },
    G0 {
        code: u8,
    },
    G1 {
        mosaic: u8,
    },
    G1Alpha {
        code: u8,
    },
    G2 {
        code: u8,
    },
    G3 {
        code: u8,
        compatibility: TeletextG3CompatibilityWire,
    },
    G0Diacritic {
        code: u8,
        diacritic: u8,
    },
    Drcs {
        reference: TeletextDrcsGlyphReferenceWire,
    },
    Attributes {
        double_height: bool,
        double_width: bool,
        boxed: bool,
        conceal: bool,
        invert: bool,
        underline_or_separated_mosaics: bool,
    },
    Flash {
        mode: TeletextFlashModeWire,
        rate: TeletextFlashRateWire,
    },
}
}

wire_tagged_enum! {
/// Ordered semantic operations; physical pointer allocation is host-owned.
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextEnhancementCommandWire {
    SetPosition {
        position: TeletextEnhancementCoordinatesWire,
    },
    SetScreenColour {
        colour: u8,
    },
    SetRowColour {
        row: u8,
        colour: u8,
        following_rows: bool,
    },
    AtColumn {
        column: u8,
        operation: TeletextColumnOperationWire,
    },
    InvokeObject {
        target: TeletextObjectIdentityWire,
        object_type: TeletextObjectTypeWire,
        #[serde(deserialize_with = "required_nullable")]
        offset: Option<TeletextEnhancementCoordinatesWire>,
    },
}
}

wire_object! {
    /// Authored order is retained exactly; expansion requires independent host budgets.
    pub struct TeletextEnhancementProgramWire {
        pub commands: Vec<TeletextEnhancementCommandWire>,
    }
}

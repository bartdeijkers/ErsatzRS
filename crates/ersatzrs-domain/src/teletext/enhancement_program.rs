//! Ordered semantic enhancements, EN 300 706 clause 12.3 (ADR 0123).
//!
//! This preserves authored commands without compiling triplets or resolving
//! object origins, repertoire assignments, attribute inheritance or resources.
//! Payload/expansion budgets and final display validity belong to their callers.

use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use super::{
    TeletextColourReference, TeletextDiacriticIndex, TeletextDrcsGlyphReference,
    TeletextEnhancementCharsetDesignation, TeletextEnhancementPosition, TeletextG1AlphaCode,
    TeletextGlyphCode, TeletextMosaic, TeletextObjectIdentity, TeletextObjectOffset,
    TeletextObjectType,
};

/// A command has an out-of-range row or column; ordinal is zero-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextEnhancementProgramError {
    #[error("teletext enhancement command {ordinal} has row {row}; maximum is 24")]
    Row { ordinal: usize, row: u8 },
    #[error("teletext enhancement command {ordinal} has column {column}; maximum is 39")]
    Column { ordinal: usize, column: u8 },
}

/// Whether a G3 character also uses the compatibility command understood at 1.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextG3Compatibility {
    /// Older command; actual Level 1.5 repertoire support remains decoder-dependent.
    Level15,
    /// Enhanced command intended to be ignored by Level 1.5 decoders.
    Level25,
}

/// Additional flash mode from EN 300 706 table 29, distinct from rate/phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextFlashMode {
    /// Foreground is steady.
    Steady,
    /// Flash foreground to background colour.
    Normal,
    /// Flash to background with the normal phase inverted.
    Inverted,
    /// Flash to the corresponding entry in the adjacent paired CLUT.
    AdjacentClut,
}

/// The six assigned flash rate/phase choices of table 29; reserved codes are absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextFlashRate {
    /// Slow one-Hertz rate.
    Slow1Hz,
    /// Fast two-Hertz rate, phase one.
    Fast2HzPhase1,
    /// Fast two-Hertz rate, phase two.
    Fast2HzPhase2,
    /// Fast two-Hertz rate, phase three.
    Fast2HzPhase3,
    /// Fast incremental flashing, initially phase one, apparent rightward movement.
    Fast2HzIncremental,
    /// Fast decremental flashing, initially phase one, apparent leftward movement.
    Fast2HzDecremental,
}

/// Operation at one relative column, retaining explicit glyph/repertoire intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextColumnOperation {
    Foreground {
        colour: TeletextColourReference,
    },
    Background {
        colour: TeletextColourReference,
    },
    DesignateCharset {
        designation: TeletextEnhancementCharsetDesignation,
    },
    G0 {
        code: TeletextGlyphCode,
    },
    G1 {
        mosaic: TeletextMosaic,
    },
    /// Level 2.5 G1 alphabetic position, using current G0 and nonspacing D5.
    G1Alpha {
        code: TeletextG1AlphaCode,
    },
    G2 {
        code: TeletextGlyphCode,
    },
    G3 {
        code: TeletextGlyphCode,
        compatibility: TeletextG3Compatibility,
    },
    G0Diacritic {
        code: TeletextGlyphCode,
        diacritic: TeletextDiacriticIndex,
    },
    /// Explicit normal/global subtable reference; no mutable selection command.
    Drcs {
        reference: TeletextDrcsGlyphReference,
    },
    /// Non-spacing attributes; boxing/window interpretation needs page-header context.
    Attributes {
        double_height: bool,
        double_width: bool,
        boxed: bool,
        conceal: bool,
        invert: bool,
        underline_or_separated_mosaics: bool,
    },
    /// Flashing semantics are retained without promising renderer timing.
    Flash {
        mode: TeletextFlashMode,
        rate: TeletextFlashRate,
    },
}

/// Logical command order is meaningful; raw terminators/pointer definitions are absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextEnhancementCommand {
    SetPosition {
        position: TeletextEnhancementPosition,
    },
    SetScreenColour {
        colour: TeletextColourReference,
    },
    SetRowColour {
        row: u8,
        colour: TeletextColourReference,
        following_rows: bool,
    },
    AtColumn {
        column: u8,
        operation: TeletextColumnOperation,
    },
    /// Origin modifier belongs to this invocation alone, never to a later command.
    InvokeObject {
        target: TeletextObjectIdentity,
        object_type: TeletextObjectType,
        #[serde(deserialize_with = "required_offset")]
        offset: Option<TeletextObjectOffset>,
    },
}

/// Unpublished ordered program with checked local row/column values.
///
/// An empty program is valid. No arbitrary command count is imposed here;
/// bounded input and transitive expansion require explicit caller budgets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TeletextEnhancementProgram {
    commands: Vec<TeletextEnhancementCommand>,
}

impl TeletextEnhancementProgram {
    /// Check local ranges without sorting, merging or executing commands.
    /// # Errors
    /// Returns the first invalid row or column with its command ordinal.
    pub fn new(
        commands: Vec<TeletextEnhancementCommand>,
    ) -> Result<Self, TeletextEnhancementProgramError> {
        for (ordinal, command) in commands.iter().enumerate() {
            match command {
                TeletextEnhancementCommand::SetRowColour { row, .. } if *row > 24 => {
                    return Err(TeletextEnhancementProgramError::Row { ordinal, row: *row });
                }
                TeletextEnhancementCommand::AtColumn { column, .. } if *column > 39 => {
                    return Err(TeletextEnhancementProgramError::Column {
                        ordinal,
                        column: *column,
                    });
                }
                _ => {}
            }
        }
        Ok(Self { commands })
    }

    /// Exact authored order, with no mutable bypass of the constructor checks.
    #[must_use]
    pub fn commands(&self) -> &[TeletextEnhancementCommand] {
        &self.commands
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProgramFields {
    commands: Vec<TeletextEnhancementCommand>,
}

impl<'de> Deserialize<'de> for TeletextEnhancementProgram {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ProgramVisitor;

        impl<'de> Visitor<'de> for ProgramVisitor {
            type Value = ProgramFields;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a teletext enhancement program object")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                ProgramFields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }

        let fields = deserializer.deserialize_map(ProgramVisitor)?;
        Self::new(fields.commands).map_err(serde::de::Error::custom)
    }
}

/// Require the nullable field to be present rather than defaulting an omitted modifier.
fn required_offset<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<TeletextObjectOffset>, D::Error> {
    Option::deserialize(deserializer)
}

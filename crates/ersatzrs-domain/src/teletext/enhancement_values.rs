//! Relative enhancement values, EN 300 706 clauses 12.3/13.7 (ADR 0123).
//!
//! These ranges do not establish a resolved display position, a character-set
//! mapping, object expansion validity, encoding feasibility or publication.

use std::fmt;

use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Serialize};

/// Failed enhancement-value construction, retaining the offending location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextEnhancementGeometryError {
    /// Relative positions may include row zero and column zero.
    #[error(
        "teletext enhancement position row {row} column {column} requires row 0–24 and column 0–39"
    )]
    Position { row: u8, column: u8 },
    /// An object origin modifier is not an already resolved screen position.
    #[error("teletext object offset row {row} column {column} requires row 0–23 and column 0–71")]
    ObjectOffset { row: u8, column: u8 },
    /// A glyph code is not a Unicode scalar or a character-set designation.
    #[error("teletext glyph code {code:02X} must be between 20 and 7F hexadecimal")]
    GlyphCode { code: u8 },
    /// Alphabetic G1 positions use current G0 without national substitution.
    #[error("teletext G1 alphabetic code {code:02X} must be between 40 and 5F hexadecimal")]
    G1AlphaCode { code: u8 },
}

/// Relative main-area enhancement position; object origin is resolved later.
///
/// Header restrictions cannot be applied here: relative row zero/column zero is
/// also the valid initial position of an invoked object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "Coordinates")]
pub struct TeletextEnhancementPosition {
    row: u8,
    column: u8,
}

impl TeletextEnhancementPosition {
    pub(crate) const FIRST: Self = Self { row: 0, column: 0 };
    pub(crate) const LAST: Self = Self {
        row: 24,
        column: 39,
    };

    /// Preserve checked relative coordinates without origin arithmetic.
    /// # Errors
    /// Returns `Position` outside rows 0–24 or columns 0–39.
    pub fn new(row: u8, column: u8) -> Result<Self, TeletextEnhancementGeometryError> {
        if row <= 24 && column <= 39 {
            Ok(Self { row, column })
        } else {
            Err(TeletextEnhancementGeometryError::Position { row, column })
        }
    }

    /// Original relative row.
    #[must_use]
    pub const fn row(self) -> u8 {
        self.row
    }

    /// Original relative column.
    #[must_use]
    pub const fn column(self) -> u8 {
        self.column
    }
}

impl<'de> Deserialize<'de> for TeletextEnhancementPosition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let coordinates = deserialize_coordinates(deserializer)?;
        Self::new(coordinates.row, coordinates.column).map_err(serde::de::Error::custom)
    }
}

impl From<TeletextEnhancementPosition> for Coordinates {
    fn from(value: TeletextEnhancementPosition) -> Self {
        Self {
            row: value.row,
            column: value.column,
        }
    }
}

/// Nonnegative object-origin modifier; includes offsets into either side-panel area.
///
/// Validity after adding the active position, and which display area is addressed,
/// belong to the later object/display resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(into = "Coordinates")]
pub struct TeletextObjectOffset {
    row: u8,
    column: u8,
}

impl TeletextObjectOffset {
    /// Preserve an offset without clipping or applying an object origin.
    /// # Errors
    /// Returns `ObjectOffset` outside rows 0–23 or columns 0–71.
    pub fn new(row: u8, column: u8) -> Result<Self, TeletextEnhancementGeometryError> {
        if row <= 23 && column <= 71 {
            Ok(Self { row, column })
        } else {
            Err(TeletextEnhancementGeometryError::ObjectOffset { row, column })
        }
    }

    /// Original vertical origin modifier.
    #[must_use]
    pub const fn row(self) -> u8 {
        self.row
    }

    /// Original horizontal origin modifier.
    #[must_use]
    pub const fn column(self) -> u8 {
        self.column
    }
}

impl<'de> Deserialize<'de> for TeletextObjectOffset {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let coordinates = deserialize_coordinates(deserializer)?;
        Self::new(coordinates.row, coordinates.column).map_err(serde::de::Error::custom)
    }
}

impl From<TeletextObjectOffset> for Coordinates {
    fn from(value: TeletextObjectOffset) -> Self {
        Self {
            row: value.row,
            column: value.column,
        }
    }
}

/// Seven-bit glyph position 0x20–0x7f; its meaning requires an explicit repertoire.
///
/// This checks the position range only. A later charset-aware validator must
/// reject unassigned or prohibited entries of the selected character set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextGlyphCode(u8);

impl TeletextGlyphCode {
    /// Keep a glyph position without dropping parity or high bits.
    /// # Errors
    /// Returns `GlyphCode` outside 0x20–0x7f.
    pub fn new(code: u8) -> Result<Self, TeletextEnhancementGeometryError> {
        if (0x20..=0x7f).contains(&code) {
            Ok(Self(code))
        } else {
            Err(TeletextEnhancementGeometryError::GlyphCode { code })
        }
    }

    /// Original glyph position, not a decoded character.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextGlyphCode {
    type Error = TeletextEnhancementGeometryError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextGlyphCode> for u8 {
    fn from(value: TeletextGlyphCode) -> Self {
        value.get()
    }
}

/// Alphabetic positions 0x40–0x5f within the Level 2.5 G1 mode (table 47).
///
/// These select the current G0 without national substitution; D5 supplies
/// underline. This scalar does not resolve a repertoire, glyph or raster, and
/// must not be normalized to a Level 1.5 G0/diacritic operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextG1AlphaCode(u8);

impl TeletextG1AlphaCode {
    /// Preserve a checked alphabetic G1 position without masking high bits.
    /// # Errors
    /// Returns `G1AlphaCode` outside 0x40–0x5f.
    pub fn new(code: u8) -> Result<Self, TeletextEnhancementGeometryError> {
        if (0x40..=0x5f).contains(&code) {
            Ok(Self(code))
        } else {
            Err(TeletextEnhancementGeometryError::G1AlphaCode { code })
        }
    }

    /// Exact transmitted alphabetic position, not a Unicode character.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextG1AlphaCode {
    type Error = TeletextEnhancementGeometryError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}
impl From<TeletextG1AlphaCode> for u8 {
    fn from(value: TeletextG1AlphaCode) -> Self {
        value.get()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Coordinates {
    row: u8,
    column: u8,
}

/// Require an object even though derived struct deserializers also accept tuples.
fn deserialize_coordinates<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Coordinates, D::Error> {
    struct CoordinatesVisitor;

    impl<'de> Visitor<'de> for CoordinatesVisitor {
        type Value = Coordinates;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a teletext coordinate object")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            Coordinates::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(CoordinatesVisitor)
}

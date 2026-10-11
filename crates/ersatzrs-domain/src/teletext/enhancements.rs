//! Checked Level 2.5 values, EN 300 706 clauses 12.4/14 and ADR 0123.
//!
//! These primitives do not establish resource dependencies, per-display DRCS
//! limits, encoding feasibility or publication readiness. The closed v1 page
//! model retains its existing representation.

use serde::{Deserialize, Serialize};

/// Enhancement construction failure, with an index for array-valued inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextEnhancementValueError {
    /// Colour-map references address one of four eight-entry CLUTs.
    #[error("teletext colour reference must be between zero and 31")]
    ColourReference,
    /// Component index is zero-based red, green, then blue.
    #[error("teletext RGB component {component} must be between zero and 15")]
    RgbComponent { component: usize },
    /// Each DRCS subtable contains at most 48 glyph slots.
    #[error("teletext DRCS glyph slot must be between zero and 47")]
    DrcsGlyphSlot,
    /// Logical subtable selection is independent of physical update bits.
    #[error("teletext DRCS subtable index must be between zero and 15")]
    DrcsSubtableIndex,
    /// Row index is zero-based from the top of the glyph.
    #[error("teletext DRCS bitmap row {row} must contain at most twelve pixel bits")]
    DrcsBitmapRow { row: usize },
}

/// Five-bit colour-map reference, not an editable colour or an RGB value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextColourReference(u8);

impl TeletextColourReference {
    /// Fixed CLUT0 black, independent of editable palette declarations.
    pub const BLACK: Self = Self(0);

    /// Validate a reference without masking unknown high bits.
    /// # Errors
    /// Returns `ColourReference` for values above 31.
    pub fn new(value: u8) -> Result<Self, TeletextEnhancementValueError> {
        if value <= 31 {
            Ok(Self(value))
        } else {
            Err(TeletextEnhancementValueError::ColourReference)
        }
    }

    /// Original colour-map reference.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextColourReference {
    type Error = TeletextEnhancementValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextColourReference> for u8 {
    fn from(value: TeletextColourReference) -> Self {
        value.get()
    }
}

/// Four-bit red, green and blue components, with no alpha or implicit scaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "[u8; 3]", into = "[u8; 3]")]
pub struct TeletextRgb([u8; 3]);

impl TeletextRgb {
    /// Preserve three checked components in red/green/blue order.
    /// # Errors
    /// Returns `RgbComponent` identifying the first component above 15.
    pub fn new(components: [u8; 3]) -> Result<Self, TeletextEnhancementValueError> {
        for (component, value) in components.iter().enumerate() {
            if *value > 15 {
                return Err(TeletextEnhancementValueError::RgbComponent { component });
            }
        }
        Ok(Self(components))
    }

    /// Original red/green/blue components.
    #[must_use]
    pub const fn get(self) -> [u8; 3] {
        self.0
    }
}

impl TryFrom<[u8; 3]> for TeletextRgb {
    type Error = TeletextEnhancementValueError;

    fn try_from(value: [u8; 3]) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextRgb> for [u8; 3] {
    fn from(value: TeletextRgb) -> Self {
        value.get()
    }
}

/// Editable Level 2.5 colour tables; fixed CLUT0/1 are deliberately absent.
///
/// This is an explicit authored declaration, not an implicit receiver default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextLevel25Palette {
    /// Eight independently checked colours for CLUT2.
    pub clut2: [TeletextRgb; 8],
    /// Eight independently checked colours for CLUT3.
    pub clut3: [TeletextRgb; 8],
}

impl TeletextLevel25Palette {
    /// EN 300 706 table 30 CLUT2/3 colours, explicitly requested by the caller.
    /// A missing presentation declaration never invokes this constructor implicitly.
    #[must_use]
    pub const fn standard() -> Self {
        Self {
            clut2: [
                TeletextRgb([15, 0, 5]),
                TeletextRgb([15, 7, 0]),
                TeletextRgb([0, 15, 7]),
                TeletextRgb([15, 15, 11]),
                TeletextRgb([0, 12, 10]),
                TeletextRgb([5, 0, 0]),
                TeletextRgb([6, 5, 2]),
                TeletextRgb([12, 7, 7]),
            ],
            clut3: [
                TeletextRgb([3, 3, 3]),
                TeletextRgb([15, 7, 7]),
                TeletextRgb([7, 15, 7]),
                TeletextRgb([15, 15, 7]),
                TeletextRgb([7, 7, 15]),
                TeletextRgb([15, 7, 15]),
                TeletextRgb([7, 15, 15]),
                TeletextRgb([13, 13, 13]),
            ],
        }
    }
}

/// Checked glyph slot within a DRCS subtable; not a Unicode code point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextDrcsGlyphSlot(u8);

impl TeletextDrcsGlyphSlot {
    /// Validate a slot without wrapping or truncation.
    /// # Errors
    /// Returns `DrcsGlyphSlot` for values above 47.
    pub fn new(value: u8) -> Result<Self, TeletextEnhancementValueError> {
        if value <= 47 {
            Ok(Self(value))
        } else {
            Err(TeletextEnhancementValueError::DrcsGlyphSlot)
        }
    }

    /// Original zero-based slot.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextDrcsGlyphSlot {
    type Error = TeletextEnhancementValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextDrcsGlyphSlot> for u8 {
    fn from(value: TeletextDrcsGlyphSlot) -> Self {
        value.get()
    }
}

/// Four-bit logical DRCS subtable index, separate from resource header metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextDrcsSubtableIndex(u8);

impl TeletextDrcsSubtableIndex {
    /// Validate a logical subtable selection.
    /// # Errors
    /// Returns `DrcsSubtableIndex` for values above 15.
    pub fn new(value: u8) -> Result<Self, TeletextEnhancementValueError> {
        if value <= 15 {
            Ok(Self(value))
        } else {
            Err(TeletextEnhancementValueError::DrcsSubtableIndex)
        }
    }

    /// Original zero-based subtable index.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextDrcsSubtableIndex {
    type Error = TeletextEnhancementValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextDrcsSubtableIndex> for u8 {
    fn from(value: TeletextDrcsSubtableIndex) -> Self {
        value.get()
    }
}

/// Ten top-to-bottom rows of twelve monochrome DRCS pixels.
///
/// Bit 11 is the leftmost pixel and bit 0 the rightmost. One selects foreground,
/// zero background. This is authored geometry, not parity-protected PTU bytes;
/// multicolour Level 3.5 modes are not represented by this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "[u16; 10]", into = "[u16; 10]")]
pub struct TeletextDrcsBitmap([u16; 10]);

impl TeletextDrcsBitmap {
    /// Preserve all pixels, rejecting rows wider than twelve bits.
    /// # Errors
    /// Returns `DrcsBitmapRow` identifying the first row above `0x0fff`.
    pub fn new(rows: [u16; 10]) -> Result<Self, TeletextEnhancementValueError> {
        for (row, value) in rows.iter().enumerate() {
            if *value > 0x0fff {
                return Err(TeletextEnhancementValueError::DrcsBitmapRow { row });
            }
        }
        Ok(Self(rows))
    }

    /// Original top-to-bottom rows with bit 11 on the left.
    #[must_use]
    pub const fn get(self) -> [u16; 10] {
        self.0
    }
}

impl TryFrom<[u16; 10]> for TeletextDrcsBitmap {
    type Error = TeletextEnhancementValueError;

    fn try_from(value: [u16; 10]) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextDrcsBitmap> for [u16; 10] {
    fn from(value: TeletextDrcsBitmap) -> Self {
        value.get()
    }
}

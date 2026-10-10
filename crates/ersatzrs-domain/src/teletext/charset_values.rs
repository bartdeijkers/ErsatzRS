//! Checked charset designations, EN 300 706 clauses 12.3.4/15.2–15.4 (ADR 0123).
//!
//! Raw aliases are preserved independently of repertoire-pair classification.
//! These values do not validate glyph assignments, composed characters, or the
//! transitive Level 2.5 limit of two enhancement repertoire pairs per page.

use serde::{Deserialize, Serialize};

/// Failed charset-value construction, retaining the offending input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextCharsetValueError {
    /// A primary page designation must name an assigned entry of table 32.
    #[error("teletext page charset designation {code:02X} hexadecimal is reserved")]
    PageDesignation { code: u8 },
    /// An enhancement designation must name an assigned entry of table 34.
    #[error("teletext enhancement charset designation {code:02X} hexadecimal is reserved")]
    EnhancementDesignation { code: u8 },
    /// A diacritic selector is a four-bit index, not a glyph code.
    #[error("teletext diacritic index {index} must be between zero and 15")]
    DiacriticIndex { index: u8 },
}

/// Repertoire identity for primary and X26 G0/G2 designations.
///
/// National-option aliases do not create additional repertoire pairs. This is
/// not a secondary-language designation: a second G0 does not designate G2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TeletextCharsetPair {
    /// Latin G0 and Latin G2.
    LatinLatin,
    /// Serbian/Croatian Cyrillic G0 and Cyrillic G2.
    Cyrillic1Cyrillic,
    /// Russian/Bulgarian Cyrillic G0 and Cyrillic G2.
    Cyrillic2Cyrillic,
    /// Ukrainian Cyrillic G0 and Cyrillic G2.
    Cyrillic3Cyrillic,
    /// Greek G0 and Greek G2.
    GreekGreek,
    /// Latin G0 and Arabic G2.
    LatinArabic,
    /// Arabic G0 and Arabic G2.
    ArabicArabic,
    /// Hebrew G0 and Arabic G2; there is no Hebrew G2 designation.
    HebrewArabic,
}

/// Assigned primary page charset and national-option designation from table 32.
///
/// The same numeric codes name second G0/national selections in table 33, but
/// that use does not designate G2. Preserve aliases for interchange rather than
/// replacing them with a canonical language code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextPageCharsetDesignation(u8);

impl TeletextPageCharsetDesignation {
    /// Preserve an assigned page code without masking or alias normalization.
    /// # Errors
    /// Returns `PageDesignation` for reserved codes, including `0x7f`.
    pub fn new(code: u8) -> Result<Self, TeletextCharsetValueError> {
        if matches!(
            code,
            0x00..=0x06 | 0x08..=0x0c | 0x0e | 0x10..=0x16 | 0x1d | 0x1f
                | 0x20..=0x26 | 0x36 | 0x37 | 0x40 | 0x44 | 0x47 | 0x55 | 0x57
        ) {
            Ok(Self(code))
        } else {
            Err(TeletextCharsetValueError::PageDesignation { code })
        }
    }

    /// Read table 33's absent-second-G0 sentinel separately from primary codes.
    ///
    /// A returned code selects only G0 and its national subset. It does not
    /// replace the primary G2 or establish an X26 enhancement repertoire pair.
    /// # Errors
    /// Returns `PageDesignation` for reserved codes other than `0x7f`.
    pub fn secondary_from_code(code: u8) -> Result<Option<Self>, TeletextCharsetValueError> {
        if code == 0x7f {
            Ok(None)
        } else {
            Self::new(code).map(Some)
        }
    }

    /// Original seven-bit designation, including its national-option alias.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Repertoire pair when this code is used as a primary page designation.
    ///
    /// Do not interpret a second G0 selection through this method: its G2
    /// remains the primary G2 independently of the second G0 and ESC state.
    #[must_use]
    pub const fn pair(self) -> TeletextCharsetPair {
        pair_for_assigned_code(self.0)
    }
}

impl TryFrom<u8> for TeletextPageCharsetDesignation {
    type Error = TeletextCharsetValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextPageCharsetDesignation> for u8 {
    fn from(value: TeletextPageCharsetDesignation) -> Self {
        value.get()
    }
}

/// Assigned X26 G0/G2 pair designation from table 34, without national subsets.
///
/// The valid set differs from page designation codes. Numeric Latin aliases
/// remain distinct values even when their repertoire pairs are equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextEnhancementCharsetDesignation(u8);

impl TeletextEnhancementCharsetDesignation {
    /// Preserve an assigned X26 code without dropping high bits.
    /// # Errors
    /// Returns `EnhancementDesignation` for all reserved codes.
    pub fn new(code: u8) -> Result<Self, TeletextCharsetValueError> {
        if matches!(
            code,
            0x00..=0x26 | 0x36 | 0x37 | 0x40 | 0x44 | 0x47 | 0x55 | 0x57
        ) {
            Ok(Self(code))
        } else {
            Err(TeletextCharsetValueError::EnhancementDesignation { code })
        }
    }

    /// Original seven-bit designation, without alias normalization.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Repertoire classification for the later transitive two-pair validation.
    #[must_use]
    pub const fn pair(self) -> TeletextCharsetPair {
        pair_for_assigned_code(self.0)
    }
}

impl TryFrom<u8> for TeletextEnhancementCharsetDesignation {
    type Error = TeletextCharsetValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextEnhancementCharsetDesignation> for u8 {
    fn from(value: TeletextEnhancementCharsetDesignation) -> Self {
        value.get()
    }
}

/// Classify codes that have already passed one of the designation constructors.
const fn pair_for_assigned_code(code: u8) -> TeletextCharsetPair {
    match code {
        0x20 => TeletextCharsetPair::Cyrillic1Cyrillic,
        0x24 => TeletextCharsetPair::Cyrillic2Cyrillic,
        0x25 => TeletextCharsetPair::Cyrillic3Cyrillic,
        0x37 => TeletextCharsetPair::GreekGreek,
        0x40 | 0x44 => TeletextCharsetPair::LatinArabic,
        0x47 | 0x57 => TeletextCharsetPair::ArabicArabic,
        0x55 => TeletextCharsetPair::HebrewArabic,
        _ => TeletextCharsetPair::LatinLatin,
    }
}

/// Four-bit diacritic selector from X26 modes `0x10..=0x1f`.
///
/// Zero selects no mark. Every other index, including 9 and 12, is preserved;
/// actual composition requires the current G0/G2 context. The mode `0x10`,
/// glyph `0x2a` special case displaying `@` belongs to the compiler/renderer,
/// not this scalar. This type does not promise that a composed glyph exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextDiacriticIndex(u8);

impl TeletextDiacriticIndex {
    /// Preserve a four-bit selector without normalizing it to a Latin accent.
    /// # Errors
    /// Returns `DiacriticIndex` for values above 15.
    pub fn new(index: u8) -> Result<Self, TeletextCharsetValueError> {
        if index <= 15 {
            Ok(Self(index))
        } else {
            Err(TeletextCharsetValueError::DiacriticIndex { index })
        }
    }

    /// Original diacritic selector, with zero meaning no mark.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextDiacriticIndex {
    type Error = TeletextCharsetValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextDiacriticIndex> for u8 {
    fn from(value: TeletextDiacriticIndex) -> Self {
        value.get()
    }
}

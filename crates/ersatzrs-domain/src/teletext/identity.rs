//! Validated display identities; EN 300 706 9.3.1 and Annex A, ADR 0121.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Failure to construct an authored teletext value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextValueError {
    /// Display addresses use decimal digits and magazines one through eight.
    #[error("teletext display page must be between 100 and 899")]
    PageNumber,
    /// The high bit of S2 and the high two bits of S4 are header controls.
    #[error("teletext subcode contains bits reserved for header controls")]
    Subcode,
    /// Each mosaic cell contains six independently addressable blocks.
    #[error("teletext mosaic must contain at most six block bits")]
    Mosaic,
    /// Language identifiers do not contain display names or locale tags.
    #[error("teletext language must contain three lowercase ASCII letters")]
    Language,
    /// Page headers carry three national-option bits.
    #[error("teletext national option must be between zero and seven")]
    NationalOption,
}

/// An ordinary decimal display address, excluding reserved non-display pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct TeletextPageNumber(u16);

impl TeletextPageNumber {
    /// Construct a display page; hexadecimal/internal addresses are not accepted.
    /// # Errors
    /// Returns `PageNumber` outside the decimal display range 100 through 899.
    pub fn new(number: u16) -> Result<Self, TeletextValueError> {
        if (100..=899).contains(&number) {
            Ok(Self(number))
        } else {
            Err(TeletextValueError::PageNumber)
        }
    }

    /// Decimal address shown to readers and used in the management viewer.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Magazine number; magazine eight is represented by zero on the wire.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "checked page numbers 100..899 yield magazines 1..8"
    )]
    pub const fn magazine(self) -> u8 {
        (self.0 / 100) as u8
    }

    /// Packet-address magazine bits, independent of the page's decimal digits.
    #[must_use]
    pub const fn wire_magazine(self) -> u8 {
        self.magazine() & 7
    }

    /// BCD tens/units within the magazine; excludes hexadecimal A through F.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "each decimal digit is at most nine, so the BCD result is at most 0x99"
    )]
    pub const fn wire_page(self) -> u8 {
        (((self.0 / 10) % 10) * 16 + self.0 % 10) as u8
    }
}

impl TryFrom<u16> for TeletextPageNumber {
    type Error = TeletextValueError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextPageNumber> for u16 {
    fn from(value: TeletextPageNumber) -> Self {
        value.get()
    }
}

impl fmt::Display for TeletextPageNumber {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Raw standards-defined S4/S3/S2/S1 identity, not a dense ordinal or decimal ID.
///
/// Valid bits are `0x3f7f`; holes hold independent page-header flags. Subcode zero
/// denotes a single page. A rotating service retains each actual wire subcode.
#[derive(
    Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "u16", into = "u16")]
pub struct TeletextSubcode(u16);

impl TeletextSubcode {
    /// Keep a subcode's identity without masking away malformed header bits.
    /// # Errors
    /// Returns `Subcode` if any bit reserved for page-header controls is set.
    pub fn new(value: u16) -> Result<Self, TeletextValueError> {
        if value & !0x3f7f == 0 {
            Ok(Self(value))
        } else {
            Err(TeletextValueError::Subcode)
        }
    }

    /// Wire identity, retaining hexadecimal nibbles and their gaps.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for TeletextSubcode {
    type Error = TeletextValueError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextSubcode> for u16 {
    fn from(value: TeletextSubcode) -> Self {
        value.get()
    }
}

/// A language code in the three-letter DVB form, e.g. `eng`, `nld` or `und`.
///
/// Construction checks syntax; it does not claim registry membership or decoder
/// character-set capability. Locale and language display names remain separate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TeletextLanguage(String);

impl TeletextLanguage {
    /// Validate the code without case folding, truncation or locale substitution.
    /// # Errors
    /// Returns `Language` unless the code contains exactly three lowercase ASCII letters.
    pub fn new(code: String) -> Result<Self, TeletextValueError> {
        if code.len() == 3 && code.bytes().all(|byte| byte.is_ascii_lowercase()) {
            Ok(Self(code))
        } else {
            Err(TeletextValueError::Language)
        }
    }

    /// Original three-letter code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for TeletextLanguage {
    type Error = TeletextValueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextLanguage> for String {
    fn from(value: TeletextLanguage) -> Self {
        value.0
    }
}

/// Three-bit national selection, in C12/C13/C14 order with C12 most significant.
/// This value stays separate from language/region identity; encoders place the
/// bits into their standards-defined header positions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextNationalOption(u8);

impl TeletextNationalOption {
    /// Validate a header national option without choosing a language or region.
    /// # Errors
    /// Returns `NationalOption` if the value needs more than three bits.
    pub fn new(value: u8) -> Result<Self, TeletextValueError> {
        if value < 8 {
            Ok(Self(value))
        } else {
            Err(TeletextValueError::NationalOption)
        }
    }

    /// Original three-bit selection value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextNationalOption {
    type Error = TeletextValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextNationalOption> for u8 {
    fn from(value: TeletextNationalOption) -> Self {
        value.get()
    }
}

/// Six-block mosaic in display order: top/middle/bottom, left then right.
///
/// Bits zero through five identify blocks; these are not raw teletext code bytes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct TeletextMosaic(u8);

impl TeletextMosaic {
    /// Construct a cell without discarding unknown high bits.
    /// # Errors
    /// Returns `Mosaic` if the pattern needs more than six bits.
    pub fn new(blocks: u8) -> Result<Self, TeletextValueError> {
        if blocks < 64 {
            Ok(Self(blocks))
        } else {
            Err(TeletextValueError::Mosaic)
        }
    }

    /// Six-block display pattern, not an encoded G1 code.
    #[must_use]
    pub const fn blocks(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for TeletextMosaic {
    type Error = TeletextValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<TeletextMosaic> for u8 {
    fn from(value: TeletextMosaic) -> Self {
        value.blocks()
    }
}

/// Exact page/subpage key used by generation readers and navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextPageAddress {
    /// Decimal display page.
    pub page: TeletextPageNumber,
    /// Actual standards-defined subcode, independent of page number.
    pub subcode: TeletextSubcode,
}

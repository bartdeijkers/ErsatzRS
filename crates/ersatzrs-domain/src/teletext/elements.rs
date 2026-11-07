//! Library of user-supplied teletext pages ("Teletext elements").
//!
//! An element is one teletext page, optionally with subpages, uploaded as a
//! TTI, T42, EP1 or HTT file. The library stores canonical TTI bytes plus
//! field bindings that fill marked page areas with static or dynamic data.
//! Storing or binding an element grants no publication or broadcast
//! authority; inclusion in a channel's teletext service is a separate step.

use serde::{Deserialize, Serialize};

use crate::EntityId;

/// Maximum stored element name length in bytes.
pub const MAX_TELETEXT_ELEMENT_NAME_BYTES: usize = 100;
/// Maximum accepted upload size in bytes for any supported source format.
pub const MAX_TELETEXT_ELEMENT_UPLOAD_BYTES: usize = 256 * 1024;
/// Maximum number of subpages kept from one upload.
pub const MAX_TELETEXT_ELEMENT_SUBPAGES: usize = 64;
/// Maximum number of field bindings per element.
pub const MAX_TELETEXT_ELEMENT_BINDINGS: usize = 64;
/// Maximum static text length in characters for one binding.
pub const MAX_TELETEXT_ELEMENT_STATIC_TEXT_CHARS: usize = 40;
/// Supported subpage cycle range in seconds.
pub const TELETEXT_ELEMENT_CYCLE_SECONDS: std::ops::RangeInclusive<u16> = 1..=600;

/// The file format an element was uploaded from. Storage is always TTI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextElementSourceFormat {
    Tti,
    T42,
    Ep1,
    Htt,
}

impl TeletextElementSourceFormat {
    /// Stable storage value.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        match self {
            Self::Tti => 1,
            Self::T42 => 2,
            Self::Ep1 => 3,
            Self::Htt => 4,
        }
    }

    /// Parse a stable storage value.
    #[must_use]
    pub const fn from_i32(value: i32) -> Option<Self> {
        Some(match value {
            1 => Self::Tti,
            2 => Self::T42,
            3 => Self::Ep1,
            4 => Self::Htt,
            _ => return None,
        })
    }

    /// Short upper-case label, identical in every culture.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Tti => "TTI",
            Self::T42 => "T42",
            Self::Ep1 => "EP1",
            Self::Htt => "HTT",
        }
    }

    /// Detect the format from a file name extension (case-insensitive).
    #[must_use]
    pub fn from_file_name(name: &str) -> Option<Self> {
        let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
        Some(match extension.as_str() {
            "tti" | "ttix" => Self::Tti,
            "t42" => Self::T42,
            "ep1" | "epx" => Self::Ep1,
            "htt" => Self::Htt,
            _ => return None,
        })
    }
}

/// Which data a bound field shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextElementBindingSource {
    /// Field not yet configured; the original page content stays visible.
    Unset,
    /// A fixed text entered by the user.
    Static,
    ChannelName,
    ChannelNumber,
    /// Local date, e.g. "Sat 07 Nov".
    Date,
    /// Local time, "HH:MM".
    Time,
    NowTitle,
    NowStart,
    NextTitle,
    NextStart,
}

impl TeletextElementBindingSource {
    /// Every selectable source in display order.
    pub const ALL: [Self; 10] = [
        Self::Unset,
        Self::Static,
        Self::ChannelName,
        Self::ChannelNumber,
        Self::Date,
        Self::Time,
        Self::NowTitle,
        Self::NowStart,
        Self::NextTitle,
        Self::NextStart,
    ];

    /// Stable form value.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Unset => "unset",
            Self::Static => "static",
            Self::ChannelName => "channel_name",
            Self::ChannelNumber => "channel_number",
            Self::Date => "date",
            Self::Time => "time",
            Self::NowTitle => "now_title",
            Self::NowStart => "now_start",
            Self::NextTitle => "next_title",
            Self::NextStart => "next_start",
        }
    }

    /// Parse a stable form value.
    #[must_use]
    pub fn from_key(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.key() == value)
    }

    /// Suggest a source from a marker name typed in the page, e.g. `%NOW%%%`.
    #[must_use]
    pub fn suggest(marker: &str) -> Self {
        match marker.to_ascii_uppercase().as_str() {
            "CHANNEL" | "KANAAL" | "CHANNELNAME" | "NAME" => Self::ChannelName,
            "NUMBER" | "NUMMER" | "CHANNELNUMBER" => Self::ChannelNumber,
            "DATE" | "DATUM" => Self::Date,
            "TIME" | "TIJD" => Self::Time,
            "NOW" | "NU" | "NOWTITLE" => Self::NowTitle,
            "NOWSTART" | "NUTIJD" | "NOWTIME" => Self::NowStart,
            "NEXT" | "STRAKS" | "NEXTTITLE" => Self::NextTitle,
            "NEXTSTART" | "STRAKSTIJD" | "NEXTTIME" => Self::NextStart,
            _ => Self::Unset,
        }
    }
}

/// One fillable field at a fixed location on one subpage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextElementBinding {
    /// Marker name found in the page, or a user label for a manual field.
    pub field: String,
    /// Zero-based subpage index.
    pub subpage: u16,
    /// Display row 1..=24 (the header row belongs to the channel).
    pub row: u8,
    /// First column 0..=39.
    pub column: u8,
    /// Field width in cells; `column + length <= 40`.
    pub length: u8,
    pub source: TeletextElementBindingSource,
    /// Text for [`TeletextElementBindingSource::Static`]; empty otherwise.
    #[serde(default)]
    pub static_text: String,
}

/// Binding validation failure without user content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextElementBindingError {
    #[error("too many teletext element bindings")]
    TooMany,
    #[error("teletext element binding is outside the page")]
    OutOfPage,
    #[error("teletext element binding refers to a missing subpage")]
    MissingSubpage,
    #[error("teletext element bindings overlap")]
    Overlap,
    #[error("teletext element binding text is too long")]
    TextTooLong,
}

/// Validate bindings against a page with `subpages` subpages.
/// # Errors
/// Rejects out-of-range, overlapping or oversized bindings.
pub fn validate_teletext_element_bindings(
    bindings: &[TeletextElementBinding],
    subpages: usize,
) -> Result<(), TeletextElementBindingError> {
    if bindings.len() > MAX_TELETEXT_ELEMENT_BINDINGS {
        return Err(TeletextElementBindingError::TooMany);
    }
    for (index, binding) in bindings.iter().enumerate() {
        if usize::from(binding.subpage) >= subpages {
            return Err(TeletextElementBindingError::MissingSubpage);
        }
        if !(1..=24).contains(&binding.row)
            || binding.length == 0
            || usize::from(binding.column) + usize::from(binding.length) > 40
        {
            return Err(TeletextElementBindingError::OutOfPage);
        }
        if binding.static_text.chars().count() > MAX_TELETEXT_ELEMENT_STATIC_TEXT_CHARS
            || binding.field.chars().count() > MAX_TELETEXT_ELEMENT_STATIC_TEXT_CHARS
        {
            return Err(TeletextElementBindingError::TextTooLong);
        }
        let end = binding.column + binding.length;
        if bindings[..index].iter().any(|other| {
            other.subpage == binding.subpage
                && other.row == binding.row
                && other.column < end
                && binding.column < other.column + other.length
        }) {
            return Err(TeletextElementBindingError::Overlap);
        }
    }
    Ok(())
}

/// A stored element.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextElement {
    pub id: EntityId,
    pub name: String,
    pub source_format: TeletextElementSourceFormat,
    pub source_file_name: String,
    /// Canonical TTI bytes (CRLF), downloadable for `QTeletextMaker`.
    pub document: Vec<u8>,
    /// Suggested page number as hexadecimal magazine/page, 0x100..=0x8FF.
    pub page_number: u16,
    pub subpage_count: u16,
    pub cycle_seconds: u16,
    pub bindings: Vec<TeletextElementBinding>,
    /// Incremented by every successful save; used for optimistic concurrency.
    pub revision: i64,
    pub updated_at: String,
}

impl std::fmt::Debug for TeletextElement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeletextElement")
            .field("id", &self.id)
            .field("source_format", &self.source_format)
            .field("page_number", &self.page_number)
            .field("subpage_count", &self.subpage_count)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

/// List row with usage information.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextElementSummary {
    pub id: EntityId,
    pub name: String,
    pub source_format: TeletextElementSourceFormat,
    pub page_number: u16,
    pub subpage_count: u16,
    pub binding_count: u16,
    pub unset_binding_count: u16,
    pub channel_count: u16,
}

/// A channel that shows an element, for "Used on".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeletextElementUsage {
    pub channel_id: EntityId,
    pub channel_number: String,
    pub channel_name: String,
    /// Effective page number (channel override or element default).
    pub page_number: u16,
    pub enabled: bool,
}

/// A channel's link to an element.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelTeletextElement {
    pub id: EntityId,
    pub channel_id: EntityId,
    pub element_id: EntityId,
    pub element_name: String,
    /// Channel-specific page number; `None` uses the element's default.
    pub page_number: Option<u16>,
    /// Effective page number.
    pub effective_page_number: u16,
    pub enabled: bool,
}

/// Format a hexadecimal magazine/page number as the teletext page users see.
#[must_use]
pub fn teletext_element_page_label(page: u16) -> String {
    let magazine = (page >> 8) & 0x7;
    let magazine = if magazine == 0 { 8 } else { magazine };
    format!("{magazine}{:02X}", page & 0xff)
}

/// Parse a decimal-looking page number "100".."899" into its stored form.
#[must_use]
pub fn parse_teletext_element_page_number(value: &str) -> Option<u16> {
    let value = value.trim();
    if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = u16::from_str_radix(value, 16).ok()?;
    (0x100..=0x899).contains(&parsed).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(row: u8, column: u8, length: u8) -> TeletextElementBinding {
        TeletextElementBinding {
            field: "NOW".to_owned(),
            subpage: 0,
            row,
            column,
            length,
            source: TeletextElementBindingSource::NowTitle,
            static_text: String::new(),
        }
    }

    #[test]
    fn page_numbers_round_trip_as_users_see_them() {
        assert_eq!(parse_teletext_element_page_number("100"), Some(0x100));
        assert_eq!(parse_teletext_element_page_number("899"), Some(0x899));
        assert_eq!(parse_teletext_element_page_number("099"), None);
        assert_eq!(parse_teletext_element_page_number("1A0"), None);
        assert_eq!(teletext_element_page_label(0x123), "123");
        assert_eq!(teletext_element_page_label(0x0ff), "8FF");
    }

    #[test]
    fn bindings_reject_overlap_and_out_of_page() {
        assert!(validate_teletext_element_bindings(&[binding(4, 0, 40)], 1).is_ok());
        assert_eq!(
            validate_teletext_element_bindings(&[binding(4, 1, 40)], 1),
            Err(TeletextElementBindingError::OutOfPage)
        );
        assert_eq!(
            validate_teletext_element_bindings(&[binding(0, 1, 4)], 1),
            Err(TeletextElementBindingError::OutOfPage)
        );
        assert_eq!(
            validate_teletext_element_bindings(&[binding(4, 0, 10), binding(4, 9, 3)], 1),
            Err(TeletextElementBindingError::Overlap)
        );
        let mut other = binding(4, 0, 3);
        other.subpage = 1;
        assert_eq!(
            validate_teletext_element_bindings(&[other], 1),
            Err(TeletextElementBindingError::MissingSubpage)
        );
    }

    #[test]
    fn formats_follow_file_extensions_and_markers_suggest_sources() {
        assert_eq!(
            TeletextElementSourceFormat::from_file_name("Welkom.TTI"),
            Some(TeletextElementSourceFormat::Tti)
        );
        assert_eq!(
            TeletextElementSourceFormat::from_file_name("p100.t42"),
            Some(TeletextElementSourceFormat::T42)
        );
        assert_eq!(TeletextElementSourceFormat::from_file_name("x.png"), None);
        assert_eq!(
            TeletextElementBindingSource::suggest("nu"),
            TeletextElementBindingSource::NowTitle
        );
        assert_eq!(
            TeletextElementBindingSource::suggest("ANY"),
            TeletextElementBindingSource::Unset
        );
    }
}

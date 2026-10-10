//! Preparatory response-only `teletext.pages.v1` wire types (ADR 0121).
//!
//! This is an unvalidated producer draft, not a complete frozen capability:
//! request context, production invocation and host integration remain separate.
//! Independent wire types deliberately mirror the domain authored page shape
//! without depending on domain, application, storage or `FFmpeg` crates. Parsing
//! never authorizes publication or establishes geometry, references, character
//! representability, spacing controls or usable page content.

use serde::{Deserialize, Deserializer, Serialize};

#[path = "teletext_pages_json.rs"]
mod json;

/// Explicit response schema version; not a manifest or capability activation.
pub const TELETEXT_PAGES_SCHEMA_VERSION: u32 = 1;
/// Maximum raw response bytes, including JSON syntax and trailing whitespace.
pub const MAX_TELETEXT_PAGES_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Resource policy for a byte-exact opaque host correlation value, not a DVB limit.
pub const MAX_TELETEXT_PAGES_GENERATION_BYTES: usize = 1024;
/// Maximum page/subpage records in one unvalidated producer response.
pub const MAX_TELETEXT_PAGES_RECORDS: usize = 1024;

/// Static rejection reasons without raw payload, keys, values or correlation data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPagesError {
    #[error("teletext pages response exceeds its raw byte limit")]
    OutputTooLarge,
    #[error("teletext pages response is not valid UTF-8")]
    InvalidUtf8,
    #[error("teletext pages response does not match its JSON wire contract")]
    InvalidJson,
    #[error("teletext pages response contains a duplicate JSON object key")]
    DuplicateKey,
    #[error("teletext pages response schema version is unsupported")]
    UnsupportedSchema,
    #[error("teletext pages generation must be nonempty and within its byte limit")]
    InvalidGeneration,
    #[error("teletext pages response does not echo the expected generation")]
    GenerationMismatch,
    #[error("teletext pages response must contain between one and 1024 records")]
    PageCount,
}

/// Required service context and complete producer draft, still unvalidated by the host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextPagesResponse {
    pub schema_version: u32,
    /// Opaque correlation echoed exactly; it is not a credential or source revision.
    pub generation: String,
    pub language: String,
    pub initial_page: u16,
    pub pages: Vec<TeletextPageDraftWire>,
}

impl TeletextPagesResponse {
    /// Parse one bounded response with byte-exact generation correlation.
    ///
    /// A first streaming traversal rejects duplicate keys in every object,
    /// including tagged content. A second pass constructs the typed wire draft;
    /// no intermediate JSON value can overwrite duplicate keys. Both use the
    /// ordinary `serde_json` recursion limit. The raw 8 MiB cap is enforced before
    /// parsing; it is not a bound on total allocations or process RSS. No fields
    /// receive implicit defaults, including a navigation target's nullable subcode.
    /// The returned wire draft still needs host value/layout/reference/encoding
    /// validation. No language, page number or national-option policy is applied.
    /// # Errors
    /// Rejects oversized, non-UTF-8, malformed, duplicate-key, unknown/missing-field
    /// responses, unsupported schemas, invalid correlation or record counts.
    pub fn from_json(bytes: &[u8], expected_generation: &str) -> Result<Self, TeletextPagesError> {
        if bytes.len() > MAX_TELETEXT_PAGES_OUTPUT_BYTES {
            return Err(TeletextPagesError::OutputTooLarge);
        }
        std::str::from_utf8(bytes).map_err(|_| TeletextPagesError::InvalidUtf8)?;
        json::reject_duplicate_keys(bytes)?;
        let response: Self =
            serde_json::from_slice(bytes).map_err(|_| TeletextPagesError::InvalidJson)?;
        if response.schema_version != TELETEXT_PAGES_SCHEMA_VERSION {
            return Err(TeletextPagesError::UnsupportedSchema);
        }
        if !valid_generation(expected_generation) || !valid_generation(&response.generation) {
            return Err(TeletextPagesError::InvalidGeneration);
        }
        if response.generation != expected_generation {
            return Err(TeletextPagesError::GenerationMismatch);
        }
        if !(1..=MAX_TELETEXT_PAGES_RECORDS).contains(&response.pages.len()) {
            return Err(TeletextPagesError::PageCount);
        }
        Ok(response)
    }
}

fn valid_generation(generation: &str) -> bool {
    !generation.is_empty() && generation.len() <= MAX_TELETEXT_PAGES_GENERATION_BYTES
}

/// Raw authored address; display range and subcode bit validity are host checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextPageAddressWire {
    pub page: u16,
    pub subcode: u16,
}

/// Page intent, independently from source descriptors or playback selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextPageKindWire {
    FullPage,
    Subtitle,
    Newsflash,
}

/// Declarative basic colour; transport representation remains host work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextColourWire {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

/// Declarative normal or two-row height, without promising valid row occupancy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextHeightWire {
    Normal,
    Double,
}

/// Explicit run attributes; no implicit browser or transport defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextStyleWire {
    pub foreground: TeletextColourWire,
    pub background: TeletextColourWire,
    pub height: TeletextHeightWire,
    pub boxed: bool,
    pub flash: bool,
    pub conceal: bool,
}

/// Literal Unicode text or raw mosaic values, never HTML or renderer arguments.
/// Mosaic block-width and character representability are host validation duties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextRunContentWire {
    Text { text: String },
    Mosaics { cells: Vec<u8>, separated: bool },
}

/// A run at its requested position; bounds, overlaps and controls remain unchecked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextRunWire {
    pub column: u8,
    pub style: TeletextStyleWire,
    pub content: TeletextRunContentWire,
}

/// Sparse requested display row, without inventing missing editorial content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextRowWire {
    pub row: u8,
    pub runs: Vec<TeletextRunWire>,
}

/// Link role independently from colour-only display cues or transport packet slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextNavigationRoleWire {
    Index,
    Red,
    Green,
    Yellow,
    Cyan,
}

/// Requested exact subpage or page, without claiming that the target exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextNavigationTargetWire {
    pub page: u16,
    /// Required JSON field: null targets a page rather than an invented subpage.
    #[serde(deserialize_with = "required_nullable_subcode")]
    pub subcode: Option<u16>,
}

fn required_nullable_subcode<'de, D>(deserializer: D) -> Result<Option<u16>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<u16>::deserialize(deserializer)
}

/// Editorial navigation suggestion; role uniqueness and references are unchecked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextNavigationLinkWire {
    pub role: TeletextNavigationRoleWire,
    pub target: TeletextNavigationTargetWire,
}

/// Independent unvalidated wire mirror of an authored domain page draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextPageDraftWire {
    pub address: TeletextPageAddressWire,
    pub kind: TeletextPageKindWire,
    pub language: String,
    pub national_option: u8,
    pub rows: Vec<TeletextRowWire>,
    pub navigation: Vec<TeletextNavigationLinkWire>,
}

#[cfg(test)]
#[path = "teletext_pages_tests.rs"]
mod tests;

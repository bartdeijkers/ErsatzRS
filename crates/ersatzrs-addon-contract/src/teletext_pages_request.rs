//! Preparatory read-only teletext request wire boundary (ADRs 0121/0123).
//!
//! These types do not activate a capability or acquire a guide. The host must
//! validate civil-day windows, IANA timezone and locale, identities, references,
//! programme semantics and generation ownership. Actual segments and guide
//! intervals preserve excluded filler gaps without duplicating raw filler rows.
//! Successful wire parsing does not prove guide completeness or usable pages.

use std::io::{self, Write};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use super::MAX_TELETEXT_PAGES_GENERATION_BYTES;

/// Explicit v1 request version; its guide shape and bounds remain unchanged.
pub const TELETEXT_PAGES_V1_REQUEST_SCHEMA_VERSION: u32 = 1;
/// Explicit v2 request version; the unchanged guide fields are selected by the host.
pub const TELETEXT_PAGES_V2_REQUEST_SCHEMA_VERSION: u32 = 2;
/// Explicit v3 request version; authoring metadata accompanies the same guide shape.
pub const TELETEXT_PAGES_V3_REQUEST_SCHEMA_VERSION: u32 = 3;

/// Maximum complete request bytes, including JSON syntax and whitespace.
pub const MAX_TELETEXT_PAGES_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum channel records supplied in one request.
pub const MAX_TELETEXT_PAGES_CHANNELS: usize = 256;
/// Maximum programme occurrences across all channels, without truncation.
pub const MAX_TELETEXT_PAGES_OCCURRENCES: usize = 10_000;

/// Static wire rejection reasons; no guide content or correlation value is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPagesRequestError {
    #[error("teletext pages request exceeds its raw byte limit")]
    InputTooLarge,
    #[error("teletext pages request is not valid UTF-8")]
    InvalidUtf8,
    #[error("teletext pages request does not match its JSON wire contract")]
    InvalidJson,
    #[error("teletext pages request contains a duplicate JSON object key")]
    DuplicateKey,
    #[error("teletext pages request schema version is unsupported")]
    UnsupportedSchema,
    #[error("teletext pages generation must be nonempty and within its byte limit")]
    InvalidGeneration,
    #[error("teletext pages request exceeds its channel record limit")]
    ChannelCount,
    #[error("teletext pages request exceeds its total programme occurrence limit")]
    OccurrenceCount,
}

/// Supplied guide context, without semantic validation or invocation authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextPagesRequest {
    pub schema_version: u32,
    /// Opaque host correlation, not a credential or source revision.
    pub generation: String,
    pub locale: String,
    pub timezone: String,
    #[serde(with = "utc_instant")]
    pub captured_at: DateTime<Utc>,
    pub window: TeletextGuideWindowWire,
    pub channels: Vec<TeletextGuideChannelWire>,
}

impl TeletextPagesRequest {
    /// Parse one bounded JSON request with no defaults or duplicate keys.
    ///
    /// Two passes reuse the response's streaming duplicate-key checker and then
    /// construct the typed request, both with the default JSON recursion limit.
    /// The raw cap is checked before UTF-8 decoding or either parse pass; it is
    /// not a bound on total allocations or process RSS. Missing guides may use
    /// zero channels or occurrences. Semantic checks remain the host's duty.
    ///
    /// # Errors
    /// Rejects oversize, invalid UTF-8/JSON, duplicate keys, unknown/missing fields,
    /// unsupported schemas, invalid generation and channel/occurrence overflow.
    pub fn from_json(bytes: &[u8]) -> Result<Self, TeletextPagesRequestError> {
        Self::from_json_for_schema(bytes, TELETEXT_PAGES_V1_REQUEST_SCHEMA_VERSION)
    }

    /// Parse only the schema explicitly selected by the trusted host.
    ///
    /// Schemas 1 through 3 share the same guide shape and bounds. The payload cannot
    /// negotiate a version or trigger a retry under another schema.
    /// # Errors
    /// Returns the same strict wire errors as `from_json`, including an unsupported
    /// expected schema or a payload version differing from that selection.
    pub fn from_json_for_schema(
        bytes: &[u8],
        expected_schema: u32,
    ) -> Result<Self, TeletextPagesRequestError> {
        validate_expected_schema(expected_schema)?;
        if bytes.len() > MAX_TELETEXT_PAGES_INPUT_BYTES {
            return Err(TeletextPagesRequestError::InputTooLarge);
        }
        std::str::from_utf8(bytes).map_err(|_| TeletextPagesRequestError::InvalidUtf8)?;
        super::json::reject_duplicate_keys(bytes).map_err(|error| match error {
            super::TeletextPagesError::DuplicateKey => TeletextPagesRequestError::DuplicateKey,
            _ => TeletextPagesRequestError::InvalidJson,
        })?;
        let request: Self =
            serde_json::from_slice(bytes).map_err(|_| TeletextPagesRequestError::InvalidJson)?;
        request.validate_wire_bounds(expected_schema)?;
        Ok(request)
    }

    /// Encode a complete bounded request, returning no partial output on failure.
    ///
    /// The writer retains at most 4 MiB of serialized bytes, including expansion
    /// from JSON escaping. This does not bound the supplied DTO's allocations or
    /// serializer temporary storage, and does not validate guide semantics.
    ///
    /// # Errors
    /// Rejects unsupported schemas, invalid generation, record-count overflow,
    /// oversized serialized requests or serialization failures.
    pub fn to_json(&self) -> Result<Vec<u8>, TeletextPagesRequestError> {
        self.to_json_for_schema(TELETEXT_PAGES_V1_REQUEST_SCHEMA_VERSION)
    }

    /// Encode only the schema explicitly selected by the trusted host.
    ///
    /// This preserves all guide fields, instant precision and byte/count fences;
    /// it never replaces the supplied DTO's schema version.
    /// # Errors
    /// Returns the same wire errors as `to_json`, including an unsupported expected
    /// schema or a DTO version differing from that selection.
    pub fn to_json_for_schema(
        &self,
        expected_schema: u32,
    ) -> Result<Vec<u8>, TeletextPagesRequestError> {
        self.validate_wire_bounds(expected_schema)?;
        let mut writer = BoundedRequestWriter {
            bytes: Vec::new(),
            overflow: false,
        };
        if serde_json::to_writer(&mut writer, self).is_err() {
            return Err(if writer.overflow {
                TeletextPagesRequestError::InputTooLarge
            } else {
                TeletextPagesRequestError::InvalidJson
            });
        }
        Ok(writer.bytes)
    }

    fn validate_wire_bounds(&self, expected_schema: u32) -> Result<(), TeletextPagesRequestError> {
        validate_expected_schema(expected_schema)?;
        if self.schema_version != expected_schema {
            return Err(TeletextPagesRequestError::UnsupportedSchema);
        }
        if self.generation.is_empty() || self.generation.len() > MAX_TELETEXT_PAGES_GENERATION_BYTES
        {
            return Err(TeletextPagesRequestError::InvalidGeneration);
        }
        if self.channels.len() > MAX_TELETEXT_PAGES_CHANNELS {
            return Err(TeletextPagesRequestError::ChannelCount);
        }
        self.channels.iter().try_fold(0usize, |count, channel| {
            count
                .checked_add(channel.programmes.len())
                .filter(|count| *count <= MAX_TELETEXT_PAGES_OCCURRENCES)
                .ok_or(TeletextPagesRequestError::OccurrenceCount)
        })?;
        Ok(())
    }
}

fn validate_expected_schema(expected_schema: u32) -> Result<(), TeletextPagesRequestError> {
    if matches!(
        expected_schema,
        TELETEXT_PAGES_V1_REQUEST_SCHEMA_VERSION
            | TELETEXT_PAGES_V2_REQUEST_SCHEMA_VERSION
            | TELETEXT_PAGES_V3_REQUEST_SCHEMA_VERSION
    ) {
        Ok(())
    } else {
        Err(TeletextPagesRequestError::UnsupportedSchema)
    }
}

/// Explicit UTC guide bounds; ordering and three civil days are host checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextGuideWindowWire {
    #[serde(with = "utc_instant")]
    pub start: DateTime<Utc>,
    #[serde(with = "utc_instant")]
    pub finish: DateTime<Utc>,
}

/// Channel identity and read-only guide, without engine-owned editorial ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextGuideChannelWire {
    pub id: i64,
    pub number: String,
    pub name: String,
    pub programmes: Vec<TeletextProgrammeWire>,
    pub context: TeletextProgrammeContextWire,
}

/// Occurrence reference within its channel, preserving supplied source identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextProgrammeReferenceWire {
    pub playout_id: i64,
    /// Zero is a valid first occurrence index for an `ExternalJson` projection.
    pub first_item_id: i64,
}

/// Minimal metadata and actual segments of one grouped guide occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextProgrammeWire {
    pub identity: TeletextProgrammeReferenceWire,
    #[serde(with = "utc_instant")]
    pub start: DateTime<Utc>,
    #[serde(with = "utc_instant")]
    pub stop: DateTime<Utc>,
    pub title: String,
    #[serde(deserialize_with = "required_nullable")]
    pub custom_title: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub sub_title: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub description: Option<String>,
    pub segments: Vec<TeletextProgrammeSegmentWire>,
}

/// Actual source segment; source offsets are distinct from broadcast instants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextProgrammeSegmentWire {
    pub item_id: i64,
    #[serde(deserialize_with = "required_nullable")]
    pub media_item_id: Option<i64>,
    #[serde(with = "utc_instant")]
    pub start: DateTime<Utc>,
    #[serde(with = "utc_instant")]
    pub finish: DateTime<Utc>,
    pub in_point_milliseconds: i64,
    pub out_point_milliseconds: i64,
}

/// Supplied roles at capture time, independently from the complete guide list.
/// Missing roles must be explicit null; reference validity is a host check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextProgrammeContextWire {
    #[serde(deserialize_with = "required_nullable")]
    pub current: Option<TeletextProgrammeReferenceWire>,
    #[serde(deserialize_with = "required_nullable")]
    pub next: Option<TeletextProgrammeReferenceWire>,
    #[serde(deserialize_with = "required_nullable")]
    pub continuing: Option<TeletextProgrammeReferenceWire>,
    pub later_today: Vec<TeletextProgrammeReferenceWire>,
    pub ambiguous: bool,
}

fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

// Chrono's optional serde feature is not needed: use explicit RFC3339 instants,
// normalize supplied offsets to UTC, and preserve subsecond precision. Both
// directions require a four-digit UTC year so successful output parses back.
mod utc_instant {
    use chrono::{DateTime, Datelike, SecondsFormat, Utc};
    use serde::{de::Error as _, ser::Error as _, Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(instant: &DateTime<Utc>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if !(0..=9999).contains(&instant.year()) {
            return Err(S::Error::custom("expected a four-digit UTC year"));
        }
        serializer.serialize_str(&instant.to_rfc3339_opts(SecondsFormat::AutoSi, true))
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        let instant = DateTime::parse_from_rfc3339(&text)
            .map_err(|_| D::Error::custom("expected an RFC3339 instant"))?
            .with_timezone(&Utc);
        if !(0..=9999).contains(&instant.year()) {
            return Err(D::Error::custom("expected a four-digit UTC year"));
        }
        Ok(instant)
    }
}

struct BoundedRequestWriter {
    bytes: Vec<u8>,
    overflow: bool,
}

impl Write for BoundedRequestWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_TELETEXT_PAGES_INPUT_BYTES - self.bytes.len() {
            self.overflow = true;
            return Err(io::Error::other("teletext request byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "teletext_pages_request_tests.rs"]
mod tests;

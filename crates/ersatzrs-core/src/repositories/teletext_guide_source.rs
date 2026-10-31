//! Historical semantic guide association, without freshness or output authority.

use std::{fmt, num::NonZeroU8};

use chrono::{DateTime, Utc};
use ersatzrs_domain::EntityId;
use serde::{
    de::{MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};

use crate::programme::ProgrammeClock;

#[path = "teletext_guide_source_current.rs"]
mod current;
pub use current::{TeletextGuideGenerationQualification, TeletextGuideQualificationError};

/// Private guide-source receipt version, independent of page contracts.
pub const TELETEXT_GUIDE_SOURCE_FENCE_VERSION: u16 = 1;
/// Latest context fingerprint: v2 also retains exact accepted document bytes.
/// V1 remains readable as historical semantic-only data, without activation authority.
pub const TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION: u16 = 2;
/// Same selected-channel bound as the existing teletext request contract.
pub const MAX_TELETEXT_GUIDE_SOURCE_CHANNELS: usize = 256;
/// No individual text field can exceed the existing complete request byte cap.
pub const MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// Versioned preparatory data tied to a saved guide input by native validation.
///
/// Field validation does not establish catalog membership, generation ownership,
/// unchanged raw sources, current Now/Next accuracy or publication authority.
/// Selected channel order and exact civil bounds belong to the original input.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct TeletextGuideSourceFence {
    pub version: u16,
    pub context_digest_version: u16,
    pub context_digest: [u8; 32],
    pub channel_ids: Vec<EntityId>,
    pub captured_at: DateTime<Utc>,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub locale: String,
    pub timezone: String,
}

impl fmt::Debug for TeletextGuideSourceFence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextGuideSourceFence")
            .field("version", &self.version)
            .field("context_digest_version", &self.context_digest_version)
            .field("channels", &self.channel_ids.len())
            .finish_non_exhaustive()
    }
}

/// Static receipt validation failure, without stored strings or fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextGuideSourceFenceError {
    #[error("teletext guide-source receipt version is unsupported")]
    Version,
    #[error("teletext guide-source selected channels are invalid")]
    Channels,
    #[error("teletext guide-source locale or named clock is invalid")]
    Metadata,
    #[error("teletext guide-source civil window is invalid")]
    CalendarWindow,
}

impl TeletextGuideSourceFence {
    /// Validate shape, canonical metadata and the exact three-civil-date window.
    /// Empty selected channels remain legal, as in the request contract.
    /// Locale catalog membership remains the native host's responsibility.
    ///
    /// # Errors
    /// Rejects unsupported versions, channel counts/identities, metadata or
    /// an unrepresentable/mismatched calendar window; never normalizes input.
    pub fn validate(&self) -> Result<(), TeletextGuideSourceFenceError> {
        if self.version != TELETEXT_GUIDE_SOURCE_FENCE_VERSION
            || !matches!(
                self.context_digest_version,
                1 | TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION
            )
        {
            return Err(TeletextGuideSourceFenceError::Version);
        }
        if self.channel_ids.len() > MAX_TELETEXT_GUIDE_SOURCE_CHANNELS
            || self
                .channel_ids
                .iter()
                .enumerate()
                .any(|(index, id)| *id <= 0 || self.channel_ids[..index].contains(id))
        {
            return Err(TeletextGuideSourceFenceError::Channels);
        }
        if self.locale.is_empty()
            || self.locale.len() > MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES
            || !self
                .locale
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || self.timezone.is_empty()
            || self.timezone.len() > MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES
        {
            return Err(TeletextGuideSourceFenceError::Metadata);
        }
        let clock = ProgrammeClock::named(&self.timezone)
            .map_err(|_| TeletextGuideSourceFenceError::Metadata)?;
        if clock.iana_name() != Some(self.timezone.as_str()) {
            return Err(TeletextGuideSourceFenceError::Metadata);
        }
        let days = NonZeroU8::new(3).ok_or(TeletextGuideSourceFenceError::CalendarWindow)?;
        let bounds = clock
            .calendar_day_bounds(self.captured_at, days)
            .map_err(|_| TeletextGuideSourceFenceError::CalendarWindow)?;
        if bounds != (self.start, self.finish) {
            return Err(TeletextGuideSourceFenceError::CalendarWindow);
        }
        Ok(())
    }
}

// This internal mirror intentionally closes the serialized shape. The outer
// visitor accepts only a map; channel slots are capped before each Vec push.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    version: u16,
    context_digest_version: u16,
    context_digest: [u8; 32],
    #[serde(deserialize_with = "channel_ids")]
    channel_ids: Vec<EntityId>,
    captured_at: DateTime<Utc>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    #[serde(deserialize_with = "bounded_text")]
    locale: String,
    #[serde(deserialize_with = "bounded_text")]
    timezone: String,
}

impl<'de> Deserialize<'de> for TeletextGuideSourceFence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Object;
        impl<'de> Visitor<'de> for Object {
            type Value = TeletextGuideSourceFence;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a guide-source receipt object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let Fields {
                    version,
                    context_digest_version,
                    context_digest,
                    channel_ids,
                    captured_at,
                    start,
                    finish,
                    locale,
                    timezone,
                } = Fields::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map_err(|_| serde::de::Error::custom("invalid guide-source receipt"))?;
                let value = TeletextGuideSourceFence {
                    version,
                    context_digest_version,
                    context_digest,
                    channel_ids,
                    captured_at,
                    start,
                    finish,
                    locale,
                    timezone,
                };
                value
                    .validate()
                    .map_err(|_| serde::de::Error::custom("invalid guide-source receipt"))?;
                Ok(value)
            }
        }
        deserializer
            .deserialize_map(Object)
            .map_err(|_| serde::de::Error::custom("invalid guide-source receipt"))
    }
}

fn channel_ids<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<EntityId>, D::Error> {
    struct Channels;
    impl<'de> Visitor<'de> for Channels {
        type Value = Vec<EntityId>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded selected-channel list")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
            let mut ids = Vec::new();
            while let Some(id) = sequence.next_element::<EntityId>()? {
                if ids.len() == MAX_TELETEXT_GUIDE_SOURCE_CHANNELS || id <= 0 || ids.contains(&id) {
                    return Err(serde::de::Error::custom("invalid selected-channel list"));
                }
                ids.push(id);
            }
            Ok(ids)
        }
    }
    deserializer.deserialize_seq(Channels)
}

fn bounded_text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct Text;
    impl Visitor<'_> for Text {
        type Value = String;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded guide-source string")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if value.len() > MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES {
                return Err(E::custom("guide-source string exceeds request bound"));
            }
            Ok(value.to_owned())
        }
    }
    // Escaped JSON parser scratch remains covered by the caller's raw envelope
    // bound; no owned model string is copied before this decoded-length check.
    deserializer.deserialize_str(Text)
}

/// Fresh, caller-owned SQL/model/fingerprint limits; never persisted in a receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextGuideSourceCheckLimits {
    pub max_records: usize,
    pub max_source_bytes: usize,
    pub max_model_nodes: usize,
    pub max_model_bytes: usize,
    pub max_digest_fields: usize,
    pub max_digest_bytes: usize,
}

#[cfg(test)]
#[path = "teletext_guide_source_tests.rs"]
mod tests;

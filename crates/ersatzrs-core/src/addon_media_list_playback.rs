//! Immutable, persisted interval playback plans for add-on media lists (ADR 0087).

use std::ops::Range;

use ersatzrs_domain::media_playback_timeline::{MediaPlaybackTimeline, MediaPlaybackTimelineError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Failure to validate an interval plan, without source URLs or secrets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum AddonMediaListPlaybackTimelinePlanError {
    /// The wire version is not supported by this server.
    #[error("unsupported add-on playback timeline plan version")]
    UnsupportedVersion,
    /// The fingerprint is not a lowercase hexadecimal SHA-256 digest.
    #[error("playback context fingerprint must contain 64 lowercase hexadecimal characters")]
    InvalidContextFingerprint,
    /// The source timing or exclusions do not describe playable content.
    #[error(transparent)]
    InvalidTimeline(#[from] MediaPlaybackTimelineError),
}

/// Validated source timing and canonical exclusions bound to a publication context.
///
/// Ranges are half-open whole-source millisecond coordinates. Construction and
/// deserialization validate through the domain timeline and canonicalize exclusions
/// to their union inside the source window. No independently writable programme
/// duration or transient provider URL is persisted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TimelinePlanWire")]
pub struct AddonMediaListPlaybackTimelinePlan {
    version: u32,
    context_fingerprint: String,
    source_duration_milliseconds: u64,
    source_window_milliseconds: Range<u64>,
    excluded_intervals_milliseconds: Vec<Range<u64>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimelinePlanWire {
    version: u32,
    context_fingerprint: String,
    source_duration_milliseconds: u64,
    source_window_milliseconds: Range<u64>,
    excluded_intervals_milliseconds: Vec<Range<u64>>,
}

impl TryFrom<TimelinePlanWire> for AddonMediaListPlaybackTimelinePlan {
    type Error = AddonMediaListPlaybackTimelinePlanError;

    fn try_from(wire: TimelinePlanWire) -> Result<Self, Self::Error> {
        if wire.version != 1 {
            return Err(Self::Error::UnsupportedVersion);
        }
        Self::new(
            wire.source_duration_milliseconds,
            wire.source_window_milliseconds,
            &wire.excluded_intervals_milliseconds,
            wire.context_fingerprint,
        )
    }
}

impl AddonMediaListPlaybackTimelinePlan {
    /// Validate a version-one plan and canonicalize exclusions within its window.
    ///
    /// # Errors
    /// Rejects an invalid context fingerprint or any domain timeline validation
    /// failure, including removal of all playable content.
    pub fn new(
        source_duration_milliseconds: u64,
        source_window_milliseconds: Range<u64>,
        exclusions: &[Range<u64>],
        context_fingerprint: String,
    ) -> Result<Self, AddonMediaListPlaybackTimelinePlanError> {
        if context_fingerprint.len() != 64
            || !context_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(AddonMediaListPlaybackTimelinePlanError::InvalidContextFingerprint);
        }
        let timeline = MediaPlaybackTimeline::from_exclusions(
            source_duration_milliseconds,
            source_window_milliseconds.clone(),
            exclusions,
        )?;
        let mut excluded_intervals_milliseconds = Vec::new();
        let mut cursor = source_window_milliseconds.start;
        for retained in timeline.retained_ranges() {
            if cursor < retained.start {
                excluded_intervals_milliseconds.push(cursor..retained.start);
            }
            cursor = retained.end;
        }
        if cursor < source_window_milliseconds.end {
            excluded_intervals_milliseconds.push(cursor..source_window_milliseconds.end);
        }
        Ok(Self {
            version: 1,
            context_fingerprint,
            source_duration_milliseconds,
            source_window_milliseconds,
            excluded_intervals_milliseconds,
        })
    }

    /// Persisted schema version, currently one.
    #[must_use]
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Fingerprint of the publication context that owns this plan.
    #[must_use]
    pub fn context_fingerprint(&self) -> &str {
        &self.context_fingerprint
    }

    /// Duration of the complete unedited source in milliseconds.
    #[must_use]
    pub fn source_duration_milliseconds(&self) -> u64 {
        self.source_duration_milliseconds
    }

    /// Whole-source window selected for playback, such as an existing chapter.
    #[must_use]
    pub fn source_window_milliseconds(&self) -> Range<u64> {
        self.source_window_milliseconds.clone()
    }

    /// Sorted, disjoint exclusions clipped to the selected source window.
    #[must_use]
    pub fn excluded_intervals_milliseconds(&self) -> &[Range<u64>] {
        &self.excluded_intervals_milliseconds
    }

    /// Reconstruct the domain timeline used for playback and programme joins.
    ///
    /// # Errors
    /// Returns domain validation errors if the domain timing contract changes.
    pub fn timeline(&self) -> Result<MediaPlaybackTimeline, MediaPlaybackTimelineError> {
        MediaPlaybackTimeline::from_exclusions(
            self.source_duration_milliseconds,
            self.source_window_milliseconds.clone(),
            &self.excluded_intervals_milliseconds,
        )
    }

    /// Exact programme duration derived from the validated canonical exclusions.
    #[must_use]
    pub fn duration_milliseconds(&self) -> u64 {
        let excluded: u64 = self
            .excluded_intervals_milliseconds
            .iter()
            .map(|range| range.end - range.start)
            .sum();
        // Construction guarantees disjoint exclusions inside this nonempty window.
        self.source_window_milliseconds.end - self.source_window_milliseconds.start - excluded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fingerprint() -> String {
        "0123456789abcdef".repeat(4)
    }

    fn valid_wire() -> serde_json::Value {
        json!({
            "version": 1,
            "context_fingerprint": fingerprint(),
            "source_duration_milliseconds": 10_000,
            "source_window_milliseconds": {"start": 0, "end": 10_000},
            "excluded_intervals_milliseconds": []
        })
    }

    #[test]
    fn canonical_serialization_roundtrip() {
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            10_000,
            0..10_000,
            &[4_000..5_000, 1_000..3_000, 2_000..4_000, 1_000..3_000],
            fingerprint(),
        )
        .unwrap();
        let mut expected = valid_wire();
        expected["excluded_intervals_milliseconds"] = json!([{"start": 1_000, "end": 5_000}]);
        assert_eq!(serde_json::to_value(&plan).unwrap(), expected);
        assert_eq!(
            serde_json::from_value::<AddonMediaListPlaybackTimelinePlan>(expected).unwrap(),
            plan
        );
        assert_eq!(plan.duration_milliseconds(), 6_000);
        assert_eq!(plan.timeline().unwrap().duration_milliseconds(), 6_000);
        assert_eq!(plan.version(), 1);
        assert_eq!(plan.context_fingerprint(), fingerprint());
        assert_eq!(plan.source_duration_milliseconds(), 10_000);
        assert_eq!(plan.source_window_milliseconds(), 0..10_000);
    }

    #[test]
    fn malformed_decode_is_rejected() {
        for (field, value) in [
            ("source_duration_milliseconds", json!(0)),
            ("source_duration_milliseconds", json!(u64::MAX)),
            (
                "source_window_milliseconds",
                json!({"start": 20, "end": 10}),
            ),
            (
                "excluded_intervals_milliseconds",
                json!([{"start": 4, "end": 4}]),
            ),
            (
                "excluded_intervals_milliseconds",
                json!([{"start": 0, "end": 10_001}]),
            ),
            ("duration_milliseconds", json!(1)),
        ] {
            let mut wire = valid_wire();
            wire[field] = value;
            assert!(serde_json::from_value::<AddonMediaListPlaybackTimelinePlan>(wire).is_err());
        }
    }

    #[test]
    fn empty_exclusions_succeed_but_all_removed_fails() {
        let plan =
            serde_json::from_value::<AddonMediaListPlaybackTimelinePlan>(valid_wire()).unwrap();
        assert!(plan.excluded_intervals_milliseconds().is_empty());
        assert_eq!(plan.duration_milliseconds(), 10_000);
        assert_eq!(
            AddonMediaListPlaybackTimelinePlan::new(
                10_000,
                0..10_000,
                &[0..5_000, 5_000..10_000],
                fingerprint(),
            ),
            Err(AddonMediaListPlaybackTimelinePlanError::InvalidTimeline(
                MediaPlaybackTimelineError::NoPlayableContent
            ))
        );
    }

    #[test]
    fn unsupported_version_and_bad_fingerprints_are_rejected() {
        let mut wire = valid_wire();
        wire["version"] = json!(2);
        assert!(serde_json::from_value::<AddonMediaListPlaybackTimelinePlan>(wire).is_err());
        for invalid in [
            String::new(),
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
        ] {
            let mut wire = valid_wire();
            wire["context_fingerprint"] = json!(invalid);
            assert!(serde_json::from_value::<AddonMediaListPlaybackTimelinePlan>(wire).is_err());
            assert_eq!(
                AddonMediaListPlaybackTimelinePlan::new(10_000, 0..10_000, &[], invalid),
                Err(AddonMediaListPlaybackTimelinePlanError::InvalidContextFingerprint)
            );
        }
    }

    #[test]
    fn chapter_window_clips_and_canonicalizes_exclusions() {
        let plan = AddonMediaListPlaybackTimelinePlan::new(
            20_000,
            5_000..15_000,
            &[
                0..1_000,
                4_000..6_000,
                10_000..11_000,
                14_000..16_000,
                18_000..20_000,
            ],
            fingerprint(),
        )
        .unwrap();
        assert_eq!(
            plan.excluded_intervals_milliseconds(),
            &[5_000..6_000, 10_000..11_000, 14_000..15_000]
        );
        assert_eq!(
            plan.timeline().unwrap().retained_ranges(),
            &[6_000..10_000, 11_000..14_000]
        );
        assert_eq!(plan.duration_milliseconds(), 7_000);
    }
}

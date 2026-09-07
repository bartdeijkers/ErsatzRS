//! Host-selected retained ranges streamed by an add-on without exposing transport URLs.

use std::{fmt, ops::Range};

use serde::{Deserialize, Serialize};

use crate::{
    invalid, media_list_operations::validate_context, validate_import_request_size,
    validate_media_list_playback_options, AddonCapability, AddonMediaListOptionValues,
    ContractError, MAX_MEDIA_LIST_INTERVALS, MAX_MEDIA_LIST_PREPARATION_VIDEO_HEIGHT,
};

/// Produce retained media through `play-intervals` with one bounded JSON stdin request.
/// Stdout contains a continuous NUT stream with raw video and PCM audio, never JSON
/// metadata, transport URLs or headers. Earlier playback versions are unchanged.
pub const CAPABILITY_REMOTE_STREAM_PLAY_V4: &str = "remote-stream.play.v4";

/// Frozen publication identity and host-selected source ranges for one playback attempt.
///
/// Ranges are ordered, half-open whole-source millisecond coordinates, already
/// clipped to the host's programme seek and remaining duration. The add-on must
/// execute this selection without rediscovering exclusions or changing the frozen
/// duration. Output uses a continuous zero-based edited clock; scheduling and the
/// final output clock remain host-owned. Source credentials must never be logged.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteStreamRetainedPlayRequest {
    pub source_url: String,
    pub provider_id: String,
    pub context_fingerprint: String,
    pub source_duration_milliseconds: u64,
    pub options: AddonMediaListOptionValues,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_video_height: Option<u32>,
    #[serde(deserialize_with = "deserialize_ranges")]
    pub retained_intervals_milliseconds: Vec<Range<u64>>,
}

impl fmt::Debug for RemoteStreamRetainedPlayRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RemoteStreamRetainedPlayRequest")
            .finish_non_exhaustive()
    }
}

impl RemoteStreamRetainedPlayRequest {
    /// Validate identity, effective playback options and the bounded source selection.
    ///
    /// # Errors
    /// Rejects invalid identity, finite timing, height, ranges, options or request size.
    /// Callers must bound input bytes before deserializing and validate before dispatch.
    pub fn validate(&self, schema: &AddonCapability) -> Result<(), ContractError> {
        validate_context(
            &self.source_url,
            &self.provider_id,
            &self.context_fingerprint,
        )?;
        if self.source_duration_milliseconds == 0
            || self.source_duration_milliseconds > i64::MAX as u64
            || self.max_video_height.is_some_and(|height| {
                height == 0 || height > MAX_MEDIA_LIST_PREPARATION_VIDEO_HEIGHT
            })
        {
            return invalid(
                "remote stream retained play request",
                "requires bounded finite timing and height",
            );
        }
        if self.retained_intervals_milliseconds.is_empty()
            || self.retained_intervals_milliseconds.len() > MAX_MEDIA_LIST_INTERVALS + 1
        {
            return invalid(
                "remote stream retained intervals",
                "requires a nonempty bounded selection",
            );
        }
        let mut previous_end = 0;
        for range in &self.retained_intervals_milliseconds {
            if range.start < previous_end
                || range.start >= range.end
                || range.end > self.source_duration_milliseconds
            {
                return invalid(
                    "remote stream retained intervals",
                    "requires ordered nonoverlapping positive ranges within the source",
                );
            }
            previous_end = range.end;
        }
        validate_media_list_playback_options(&self.options, schema, true)?;
        validate_import_request_size(self)
    }
}

// Keep each range object closed as well as the request envelope.
fn deserialize_ranges<'de, D>(deserializer: D) -> Result<Vec<Range<u64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct WireRange {
        start: u64,
        end: u64,
    }
    Vec::<WireRange>::deserialize(deserializer).map(|ranges| {
        ranges
            .into_iter()
            .map(|range| range.start..range.end)
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> RemoteStreamRetainedPlayRequest {
        serde_json::from_value(serde_json::json!({
            "source_url":"https://media.example.test/item", "provider_id":"item",
            "context_fingerprint":"a".repeat(64), "source_duration_milliseconds":20000,
            "options":{"schema":"media-list.options.v1","values":{"enabled":true}},
            "max_video_height":1080,
            "retained_intervals_milliseconds":[{"start":0,"end":5000},{"start":10000,"end":20000}]
        }))
        .unwrap()
    }

    fn schema() -> AddonCapability {
        serde_json::from_value(serde_json::json!({
            "id":"media-list.options.v1", "options":[{
                "key":"enabled", "label":{"en-US":"Enabled"}, "description":{"en-US":"Fixture"},
                "scope":"both", "operation_targets":["playback"], "applicability":"finite_only",
                "kind":"boolean", "default":true
            }]
        }))
        .unwrap()
    }

    #[test]
    fn retained_play_request_roundtrips_and_accepts_adjacent_ranges() {
        let mut value = request();
        value.validate(&schema()).unwrap();
        let json = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            serde_json::from_slice::<RemoteStreamRetainedPlayRequest>(&json).unwrap(),
            value
        );
        value.max_video_height = None;
        value.retained_intervals_milliseconds = vec![0..5000, 5000..20000];
        value.validate(&schema()).unwrap();
        assert!(serde_json::to_value(&value)
            .unwrap()
            .get("max_video_height")
            .is_none());
    }

    #[test]
    fn retained_play_request_rejects_empty_reversed_overlapping_and_outside_ranges() {
        for ranges in [
            vec![],
            vec![Range { start: 0, end: 0 }],
            vec![Range { start: 10, end: 9 }],
            vec![Range {
                start: 0,
                end: 20001,
            }],
            vec![10..20, 0..5],
            vec![0..10, 9..20],
            vec![0..10, 0..10],
        ] {
            let mut value = request();
            value.retained_intervals_milliseconds = ranges;
            assert!(value.validate(&schema()).is_err());
        }
    }

    #[test]
    fn retained_play_request_bounds_interval_count() {
        let mut value = request();
        value.retained_intervals_milliseconds = (0..=MAX_MEDIA_LIST_INTERVALS)
            .map(|index| index as u64 * 2..index as u64 * 2 + 1)
            .collect();
        value.validate(&schema()).unwrap();
        value.retained_intervals_milliseconds.push(10000..10001);
        assert!(value.validate(&schema()).is_err());
    }

    #[test]
    fn retained_play_request_rejects_unknown_missing_and_malformed_fields() {
        let original = serde_json::to_value(request()).unwrap();
        for (key, replacement) in [
            ("headers", serde_json::json!({})),
            (
                "retained_intervals_milliseconds",
                serde_json::json!([{"start":0,"end":1,"url":"https://media.example.test/item"}]),
            ),
            (
                "retained_intervals_milliseconds",
                serde_json::json!([{"start":-1,"end":1}]),
            ),
            (
                "retained_intervals_milliseconds",
                serde_json::json!([{"start":0.5,"end":1}]),
            ),
            ("retained_intervals_milliseconds", serde_json::Value::Null),
        ] {
            let mut value = original.clone();
            value[key] = replacement;
            assert!(serde_json::from_value::<RemoteStreamRetainedPlayRequest>(value).is_err());
        }
        let mut value = original;
        value
            .as_object_mut()
            .unwrap()
            .remove("retained_intervals_milliseconds");
        assert!(serde_json::from_value::<RemoteStreamRetainedPlayRequest>(value).is_err());
    }

    #[test]
    fn retained_play_request_validates_identity_timing_height_and_options() {
        for (key, replacement) in [
            ("source_url", serde_json::json!("")),
            ("provider_id", serde_json::json!("")),
            ("context_fingerprint", serde_json::json!("A".repeat(64))),
            ("source_duration_milliseconds", serde_json::json!(0)),
            ("source_duration_milliseconds", serde_json::json!(u64::MAX)),
            ("max_video_height", serde_json::json!(0)),
            ("max_video_height", serde_json::json!(4321)),
            (
                "options",
                serde_json::json!({"schema":"unknown","values":{}}),
            ),
            (
                "options",
                serde_json::json!({"schema":"media-list.options.v1","values":{"enabled":"true"}}),
            ),
        ] {
            let mut value = serde_json::to_value(request()).unwrap();
            value[key] = replacement;
            let value: RemoteStreamRetainedPlayRequest = serde_json::from_value(value).unwrap();
            assert!(value.validate(&schema()).is_err());
        }
    }

    #[test]
    fn retained_play_request_debug_is_redacted_before_validation() {
        let mut value = request();
        value.provider_id = "private-invalid-identity".into();
        value.context_fingerprint = "private-invalid-fingerprint".into();
        assert_eq!(
            format!("{value:?}"),
            "RemoteStreamRetainedPlayRequest { .. }"
        );
    }
}

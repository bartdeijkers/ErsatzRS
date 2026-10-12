use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use async_trait::async_trait;
use chrono::{Duration, TimeZone as _, Utc};
use ersatzrs_domain::{EntityId, TeletextChannelSelectionSnapshot, TeletextPublicationRequest};
use serde_json::{json, Value};

use super::{
    TeletextGuideSourceCheckLimits, TeletextGuideSourceFence, TeletextGuideSourceFenceError,
    MAX_TELETEXT_GUIDE_SOURCE_CHANNELS, MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES,
    TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION, TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
};
use crate::{
    programme::ProgrammeClock,
    repositories::{
        Result, TeletextPublicationMutation, TeletextPublicationRejection,
        TeletextPublicationRepository, TeletextPublicationRequestSave,
    },
};

fn fence() -> TeletextGuideSourceFence {
    let captured_at = Utc.with_ymd_and_hms(2026, 3, 28, 12, 0, 0).unwrap();
    TeletextGuideSourceFence {
        version: TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
        context_digest_version: TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
        context_digest: [7; 32],
        channel_ids: vec![2, 1],
        captured_at,
        start: Utc.with_ymd_and_hms(2026, 3, 27, 23, 0, 0).unwrap(),
        finish: Utc.with_ymd_and_hms(2026, 3, 30, 22, 0, 0).unwrap(),
        locale: "nl".into(),
        timezone: "Europe/Amsterdam".into(),
    }
}

#[test]
fn receipt_roundtrip_preserves_order_digest_and_three_civil_dates_across_dst() {
    let original = fence();
    original.validate().unwrap();
    assert_eq!((original.finish - original.start).num_hours(), 71);
    let bytes = serde_json::to_vec(&original).unwrap();
    let restored: TeletextGuideSourceFence = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(original, restored);
    assert_eq!(restored.channel_ids, vec![2, 1]);
    let mut empty = original;
    empty.channel_ids.clear();
    empty.validate().unwrap();
}

#[test]
fn serialized_receipt_is_map_only_closed_and_requires_every_field() {
    let original = serde_json::to_value(fence()).unwrap();
    for key in [
        "version",
        "context_digest_version",
        "context_digest",
        "channel_ids",
        "captured_at",
        "start",
        "finish",
        "locale",
        "timezone",
    ] {
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(
            serde_json::from_value::<TeletextGuideSourceFence>(missing).is_err(),
            "{key}"
        );
    }
    let mut extra = original.clone();
    extra["unexpected"] = json!("private-marker");
    let error = serde_json::from_value::<TeletextGuideSourceFence>(extra)
        .unwrap_err()
        .to_string();
    assert!(!error.contains("private-marker"));
    assert!(serde_json::from_value::<TeletextGuideSourceFence>(json!([])).is_err());
    assert!(serde_json::from_value::<TeletextGuideSourceFence>(Value::Null).is_err());
    let mut short = original.clone();
    short["context_digest"] = json!(vec![7; 31]);
    assert!(serde_json::from_value::<TeletextGuideSourceFence>(short).is_err());
    let mut version = original;
    version["version"] = json!(2);
    assert!(serde_json::from_value::<TeletextGuideSourceFence>(version).is_err());
    let json = String::from_utf8(serde_json::to_vec(&fence()).unwrap()).unwrap();
    let duplicate = json.replacen('{', "{\"version\":1,", 1);
    assert!(serde_json::from_str::<TeletextGuideSourceFence>(&duplicate).is_err());
}

#[test]
fn selected_channels_are_bounded_positive_unique_and_never_sorted() {
    let mut original = fence();
    original.channel_ids = (1..=MAX_TELETEXT_GUIDE_SOURCE_CHANNELS)
        .map(|id| i64::try_from(id).unwrap())
        .collect();
    let restored: TeletextGuideSourceFence =
        serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(restored.channel_ids, original.channel_ids);
    for ids in [
        vec![0],
        vec![-1],
        vec![1, 1],
        (1..=MAX_TELETEXT_GUIDE_SOURCE_CHANNELS + 1)
            .map(|id| i64::try_from(id).unwrap())
            .collect(),
    ] {
        let mut invalid = fence();
        invalid.channel_ids = ids;
        assert_eq!(
            invalid.validate(),
            Err(TeletextGuideSourceFenceError::Channels)
        );
        assert!(serde_json::from_slice::<TeletextGuideSourceFence>(
            &serde_json::to_vec(&invalid).unwrap()
        )
        .is_err());
    }
}

#[test]
fn invalid_versions_metadata_and_window_fail_without_normalization() {
    let mut invalid = fence();
    invalid.context_digest_version = 2;
    assert_eq!(
        invalid.validate(),
        Err(TeletextGuideSourceFenceError::Version)
    );
    for locale in ["", " NL", "NL", "nl ", "nl_nl"] {
        let mut invalid = fence();
        invalid.locale = locale.into();
        assert_eq!(
            invalid.validate(),
            Err(TeletextGuideSourceFenceError::Metadata)
        );
    }
    for timezone in ["", "not-a-clock", "europe/amsterdam", "Europe/Amsterdam "] {
        let mut invalid = fence();
        invalid.timezone = timezone.into();
        assert_eq!(
            invalid.validate(),
            Err(TeletextGuideSourceFenceError::Metadata)
        );
    }
    let mut invalid = fence();
    invalid.finish += Duration::nanoseconds(1);
    assert_eq!(
        invalid.validate(),
        Err(TeletextGuideSourceFenceError::CalendarWindow)
    );
    let mut invalid = fence();
    invalid.captured_at = chrono::DateTime::from_timestamp(59, 1_000_000_000).unwrap();
    assert_eq!(
        invalid.validate(),
        Err(TeletextGuideSourceFenceError::CalendarWindow)
    );
}

#[test]
fn receipt_keeps_capture_nanoseconds_and_does_not_invent_host_catalog_membership() {
    let mut original = fence();
    original.captured_at += Duration::nanoseconds(7);
    original.locale = "synthetic-catalog-code".into();
    original.validate().unwrap();
    let restored: TeletextGuideSourceFence =
        serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
    assert_eq!(restored.captured_at.timestamp_subsec_nanos(), 7);
    assert_eq!(restored.locale, original.locale);
    // Core validates named calendar semantics, not which names the host supports.
    let clock = ProgrammeClock::named(&restored.timezone).unwrap();
    assert_eq!(clock.iana_name(), Some(restored.timezone.as_str()));
}

#[test]
fn oversized_decoded_text_and_debug_do_not_expose_stored_context() {
    let mut original = fence();
    original.locale = "private-locale-marker".into();
    original.timezone = "private-zone-marker".into();
    let debug = format!("{original:?}");
    assert!(!debug.contains("private"));
    assert!(!debug.contains("[7"));
    let mut value = serde_json::to_value(fence()).unwrap();
    value["locale"] = json!("x".repeat(MAX_TELETEXT_GUIDE_SOURCE_TEXT_BYTES + 1));
    let error = serde_json::from_value::<TeletextGuideSourceFence>(value)
        .unwrap_err()
        .to_string();
    assert!(!error.contains("xxxxxxxx"));
}

struct StaticOnly {
    calls: AtomicUsize,
}
#[async_trait]
impl TeletextPublicationRepository for StaticOnly {
    async fn request(
        &self,
        _save: &TeletextPublicationRequestSave,
        _maximum: usize,
    ) -> Result<TeletextPublicationMutation<TeletextPublicationRequest>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(TeletextPublicationMutation::Rejected(
            TeletextPublicationRejection::InvalidValues,
        ))
    }
    async fn current(&self, _id: EntityId) -> Result<Option<TeletextPublicationRequest>> {
        Ok(None)
    }
}
#[tokio::test]
async fn source_aware_default_is_unavailable_and_never_downgrades_to_static() {
    let repository = StaticOnly {
        calls: AtomicUsize::new(0),
    };
    let save = TeletextPublicationRequestSave {
        design_id: 1,
        revision: 1,
        digest: "0".repeat(64),
        expected_design_head: 1,
        expected_request_id: None,
        selection: TeletextChannelSelectionSnapshot {
            channel_id: 1,
            bindings: Vec::new(),
        },
    };
    let limits = TeletextGuideSourceCheckLimits {
        max_records: 1,
        max_source_bytes: 1,
        max_model_nodes: 1,
        max_model_bytes: 1,
        max_digest_fields: 1,
        max_digest_bytes: 1,
    };
    assert_eq!(
        repository
            .request_with_source(&save, &fence(), limits, 1, Instant::now())
            .await
            .unwrap(),
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::GuideUnavailable)
    );
    assert_eq!(repository.calls.load(Ordering::SeqCst), 0);
    repository.request(&save, 1).await.unwrap();
    assert_eq!(repository.calls.load(Ordering::SeqCst), 1);
}

//! Saved fence integrity is separate from transactional current-source admission.

use super::*;
use crate::{
    teletext_channel_documents::{preflight_requests, TeletextChannelRequestTemplates},
    teletext_guide::{SuppliedTeletextGuideContext, TeletextGuideChannel},
};
use ersatzrs_core::repositories::{
    TeletextGuideSourceFence, MAX_TELETEXT_GUIDE_SOURCE_CHANNELS,
    TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION, TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
};

fn context() -> SuppliedTeletextGuideContext<'static> {
    const CHANNELS: &[TeletextGuideChannel<'static>] = &[
        TeletextGuideChannel {
            id: 17,
            number: "17",
            name: "private-source-name-a",
            programmes: &[],
        },
        TeletextGuideChannel {
            id: 18,
            number: "18",
            name: "private-source-name-b",
            programmes: &[],
        },
    ];
    SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl"],
        timezone: "Europe/Amsterdam",
        captured_at: "2026-10-11T12:00:00.125Z".parse().unwrap(),
        channels: CHANNELS,
    }
}

fn templates(fixture: &Fixture) -> TeletextChannelRequestTemplates {
    preflight_requests(&fixture.selection, &context()).unwrap()
}

fn fence(requests: &TeletextChannelRequestTemplates) -> TeletextGuideSourceFence {
    let parsed = ersatzrs_addon_contract::TeletextPagesRequest::from_json(
        requests.request(TeletextPageContract::V1).unwrap(),
    )
    .unwrap();
    TeletextGuideSourceFence {
        version: TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
        context_digest_version: TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
        context_digest: [0x5a; 32],
        channel_ids: parsed.channels.iter().map(|channel| channel.id).collect(),
        captured_at: parsed.captured_at,
        start: parsed.window.start,
        finish: parsed.window.finish,
        locale: parsed.locale,
        timezone: parsed.timezone,
    }
}

fn encoded(fixture: &Fixture) -> EncodedTeletextDesignDocuments {
    let requests = templates(fixture);
    encode_teletext_design_documents_with_source(
        fixture.input(),
        &requests,
        &fence(&requests),
        budget(),
    )
    .unwrap()
}

fn value(fixture: &Fixture) -> Value {
    serde_json::from_slice(encoded(fixture).bytes()).unwrap()
}

#[test]
fn exact_fence_and_request_bytes_restore_without_current_source_authority() {
    let fixture = Fixture::mixed();
    let requests = templates(&fixture);
    let source = fence(&requests);
    let saved =
        encode_teletext_design_documents_with_source(fixture.input(), &requests, &source, budget())
            .unwrap();
    let restored = decode_teletext_design_documents(saved.bytes(), budget()).unwrap();
    assert_eq!(restored.guide_source(), Some(&source));
    for contract in [TeletextPageContract::V1, TeletextPageContract::V2] {
        assert_eq!(
            restored.guide_inputs().unwrap().request(contract),
            requests.request(contract)
        );
    }
    let parts = restored.into_parts();
    assert_eq!(parts.guide_source.as_ref(), Some(&source));
    for debug in [format!("{:?}", parts.guide_source), format!("{parts:?}")] {
        for private in [
            GENERATION,
            "private-source-name",
            "Europe/Amsterdam",
            "5a5a5a",
        ] {
            assert!(!debug.contains(private));
        }
    }
    let mut descriptive = source.clone();
    descriptive.context_digest[0] ^= 1;
    // This codec checks shape/association, not whether a database produced the
    // fingerprint. Native plus transactional source admission remain mandatory.
    assert!(encode_teletext_design_documents_with_source(
        fixture.input(),
        &requests,
        &descriptive,
        budget()
    )
    .is_ok());
    let old = encode_teletext_design_documents_with_requests(fixture.input(), &requests, budget())
        .unwrap();
    let marker = b",\"guide_source\":";
    let index = saved
        .bytes()
        .windows(marker.len())
        .position(|bytes| bytes == marker)
        .unwrap();
    assert_eq!(
        &saved.bytes()[..index],
        &old.bytes()[..old.bytes().len() - 1]
    );
    assert!(decode_teletext_design_documents(old.bytes(), budget())
        .unwrap()
        .guide_source()
        .is_none());
}

#[test]
fn source_presence_requires_real_guide_inputs_and_cannot_attach_to_requestless_off() {
    let fixture = Fixture::mixed();
    let baseline = value(&fixture);
    let mut absent = baseline.clone();
    absent.as_object_mut().unwrap().remove("guide_inputs");
    assert_eq!(
        decode(&absent).unwrap_err().reason,
        TeletextDesignDocumentFailure::GuideSource
    );
    let selection = TeletextChannelSelectionSnapshot {
        channel_id: 9,
        bindings: vec![],
    };
    let input = TeletextDesignDocumentInput {
        selection: &selection,
        generation: GENERATION,
        sources: &[],
        entry_addon_id: None,
        entry: None,
        v1_region: None,
        resource_transmissions: &[],
    };
    let requests = preflight_requests(&selection, &context()).unwrap();
    let source = fence(&templates(&fixture));
    assert_eq!(
        encode_teletext_design_documents_with_source(input, &requests, &source, budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::GuideSource
    );
    let saved = encode_teletext_design_documents_with_requests(input, &requests, budget()).unwrap();
    let mut off: Value = serde_json::from_slice(saved.bytes()).unwrap();
    off["guide_source"] = baseline["guide_source"].clone();
    assert_eq!(
        decode(&off).unwrap_err().reason,
        TeletextDesignDocumentFailure::GuideSource
    );
    assert!(decode_teletext_design_documents(saved.bytes(), budget())
        .unwrap()
        .guide_source()
        .is_none());
}

#[test]
fn guide_source_is_closed_nonnullable_and_checked_before_restoration() {
    let baseline = value(&Fixture::mixed());
    for field in [
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
        let mut changed = baseline.clone();
        changed["guide_source"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(decode(&changed).is_err());
    }
    for source in [Value::Null, json!([]), json!("source")] {
        let mut changed = baseline.clone();
        changed["guide_source"] = source;
        assert_eq!(
            decode(&changed).unwrap_err().reason,
            TeletextDesignDocumentFailure::Json
        );
    }
    for (field, replacement) in [
        ("unknown", json!(true)),
        ("version", json!(2)),
        (
            "context_digest_version",
            json!(TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION + 1),
        ),
        ("context_digest", json!(vec![0; 31])),
        ("context_digest", json!(vec![0; 33])),
        ("context_digest", json!(vec![256; 32])),
        ("channel_ids", json!([17, 17])),
        ("channel_ids", json!([0])),
    ] {
        let mut changed = baseline.clone();
        changed["guide_source"][field] = replacement;
        assert!(decode(&changed).is_err());
    }
    let text = serde_json::to_string(&baseline).unwrap();
    let duplicate_source = text.replace(
        "\"guide_source\":",
        &format!(
            "\"guide_source\":{},\"guide_source\":",
            baseline["guide_source"]
        ),
    );
    let duplicate_version = text.replace("\"version\":1", "\"version\":1,\"version\":1");
    for raw in [duplicate_source, duplicate_version] {
        assert_eq!(
            decode_teletext_design_documents(raw.as_bytes(), budget())
                .unwrap_err()
                .reason,
            TeletextDesignDocumentFailure::Json
        );
    }
}

#[test]
fn ordered_source_metadata_must_match_the_same_parsed_common_requests() {
    let fixture = Fixture::mixed();
    let requests = templates(&fixture);
    let original = fence(&requests);
    let mut cases = Vec::new();
    let mut changed = original.clone();
    changed.channel_ids.reverse();
    cases.push(changed);
    let mut changed = original.clone();
    changed.channel_ids[1] = 19;
    cases.push(changed);
    let mut changed = original.clone();
    changed.captured_at += chrono::Duration::seconds(1);
    cases.push(changed);
    let mut changed = original.clone();
    changed.locale = "en".into();
    cases.push(changed);
    let mut changed = original.clone();
    changed.timezone = "UTC".into();
    (changed.start, changed.finish) = ersatzrs_core::programme::ProgrammeClock::named("UTC")
        .unwrap()
        .calendar_day_bounds(changed.captured_at, std::num::NonZeroU8::new(3).unwrap())
        .unwrap();
    cases.push(changed);
    for source in cases {
        source.validate().unwrap();
        assert_eq!(
            encode_teletext_design_documents_with_source(
                fixture.input(),
                &requests,
                &source,
                budget()
            )
            .unwrap_err()
            .reason,
            TeletextDesignDocumentFailure::GuideSource
        );
        let mut stored = value(&fixture);
        stored["guide_source"] = serde_json::to_value(source).unwrap();
        assert_eq!(
            decode(&stored).unwrap_err().reason,
            TeletextDesignDocumentFailure::GuideSource
        );
    }
    let mut changed = value(&fixture);
    for field in ["v1", "v2"] {
        let mut request: Value =
            serde_json::from_str(changed["guide_inputs"][field].as_str().unwrap()).unwrap();
        request["window"]["start"] = json!("2026-10-10T00:00:00Z");
        changed["guide_inputs"][field] = json!(serde_json::to_string(&request).unwrap());
    }
    assert_eq!(
        decode(&changed).unwrap_err().reason,
        TeletextDesignDocumentFailure::GuideSource
    );
}

#[test]
fn source_costs_share_exact_context_and_serialized_limits_with_requests() {
    let fixture = Fixture::mixed();
    let requests = templates(&fixture);
    let source = fence(&requests);
    let saved = encoded(&fixture);
    let value: Value = serde_json::from_slice(saved.bytes()).unwrap();
    assert_eq!(
        value["guide_source"]["captured_at"],
        "2026-10-11T12:00:00.125Z"
    );
    let mut exact = budget();
    exact.max_context_bytes = super::guide_inputs_tests::context_bytes(&value);
    exact.max_serialized_bytes = saved.bytes().len();
    assert!(encode_teletext_design_documents_with_source(
        fixture.input(),
        &requests,
        &source,
        exact
    )
    .is_ok());
    assert!(decode_teletext_design_documents(saved.bytes(), exact).is_ok());
    let mut short = exact;
    short.max_context_bytes -= 1;
    assert!(matches!(
        encode_teletext_design_documents_with_source(fixture.input(), &requests, &source, short)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "context",
            ..
        }
    ));
    assert!(matches!(
        decode_teletext_design_documents(saved.bytes(), short)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "context",
            ..
        }
    ));
    let mut short = exact;
    short.max_serialized_bytes -= 1;
    assert!(encode_teletext_design_documents_with_source(
        fixture.input(),
        &requests,
        &source,
        short
    )
    .is_err());
    assert!(decode_teletext_design_documents(saved.bytes(), short).is_err());
    let mut too_many = value;
    too_many["guide_source"]["channel_ids"] = json!((1..=i64::try_from(
        MAX_TELETEXT_GUIDE_SOURCE_CHANNELS + 1
    )
    .unwrap())
        .collect::<Vec<_>>());
    assert!(matches!(
        decode(&too_many).unwrap_err().reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "guide channels",
            count: 257,
            maximum: 256
        }
    ));
}

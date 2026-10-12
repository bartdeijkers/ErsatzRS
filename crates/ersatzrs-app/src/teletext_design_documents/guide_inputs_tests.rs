//! Exact historical inputs are integrity facts, not authenticated source receipts.

use super::*;
use crate::{
    teletext_channel_documents::{preflight_requests, TeletextChannelRequestTemplates},
    teletext_guide::{SuppliedTeletextGuideContext, TeletextGuideChannel},
};
use chrono::TimeZone as _;
use ersatzrs_addon_contract::{TeletextPagesRequest, MAX_TELETEXT_PAGES_INPUT_BYTES};

fn request_context() -> SuppliedTeletextGuideContext<'static> {
    const CHANNELS: &[TeletextGuideChannel<'static>] = &[TeletextGuideChannel {
        id: 17,
        number: "17.1",
        name: "private-guide-é-\"sentinel\"",
        programmes: &[],
    }];
    SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl"],
        timezone: "Europe/Amsterdam",
        captured_at: chrono::Utc
            .with_ymd_and_hms(2026, 10, 11, 12, 0, 0)
            .unwrap(),
        channels: CHANNELS,
    }
}

fn requests(selection: &TeletextChannelSelectionSnapshot) -> TeletextChannelRequestTemplates {
    preflight_requests(selection, &request_context()).unwrap()
}

fn encoded_with_requests(fixture: &Fixture) -> EncodedTeletextDesignDocuments {
    encode_teletext_design_documents_with_requests(
        fixture.input(),
        &requests(&fixture.selection),
        budget(),
    )
    .unwrap()
}

fn value_with_requests(fixture: &Fixture) -> Value {
    serde_json::from_slice(encoded_with_requests(fixture).bytes()).unwrap()
}

#[test]
fn mixed_requests_preserve_exact_bytes_and_legacy_encoding_omits_the_new_field() {
    let fixture = Fixture::mixed();
    let templates = requests(&fixture.selection);
    let legacy = fixture.encoded();
    let encoded =
        encode_teletext_design_documents_with_requests(fixture.input(), &templates, budget())
            .unwrap();
    let marker = b",\"guide_inputs\":";
    let index = encoded
        .bytes()
        .windows(marker.len())
        .position(|bytes| bytes == marker)
        .unwrap();
    assert_eq!(
        &encoded.bytes()[..index],
        &legacy.bytes()[..legacy.bytes().len() - 1]
    );
    assert!(!fixture
        .value()
        .as_object()
        .unwrap()
        .contains_key("guide_inputs"));
    let legacy_restored = decode_teletext_design_documents(legacy.bytes(), budget()).unwrap();
    assert!(legacy_restored.guide_inputs().is_none());
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    for contract in [TeletextPageContract::V1, TeletextPageContract::V2] {
        assert_eq!(
            restored.guide_inputs().unwrap().request(contract),
            templates.request(contract)
        );
    }
    assert_eq!(restored.sources()[0].bytes(), fixture.sources[0].bytes());
    assert_eq!(restored.sources()[1].bytes(), fixture.sources[1].bytes());
    let parts = restored.into_parts();
    assert_eq!(
        parts
            .guide_inputs
            .as_ref()
            .unwrap()
            .request(TeletextPageContract::V1),
        templates.request(TeletextPageContract::V1)
    );
    for debug in [format!("{:?}", parts.guide_inputs), format!("{parts:?}")] {
        for sentinel in [
            GENERATION,
            "private-guide-é",
            "Europe/Amsterdam",
            "private-setting-sentinel",
        ] {
            assert!(!debug.contains(sentinel));
        }
        assert!(debug.contains("v1_bytes"));
    }
}

#[test]
fn request_formatting_and_utf8_round_trip_without_normalizing_original_strings() {
    let fixture = Fixture::mixed();
    let mut value = value_with_requests(&fixture);
    let original = value["guide_inputs"]["v1"].as_str().unwrap();
    let parsed = TeletextPagesRequest::from_json(original.as_bytes()).unwrap();
    let mut exact = serde_json::to_vec_pretty(&parsed).unwrap();
    exact.extend_from_slice(b"\r\n \t");
    value["guide_inputs"]["v1"] = json!(std::str::from_utf8(&exact).unwrap());
    let restored = decode(&value).unwrap();
    assert_eq!(
        restored
            .guide_inputs()
            .unwrap()
            .request(TeletextPageContract::V1)
            .unwrap(),
        exact
    );
    assert!(std::str::from_utf8(&exact).unwrap().contains("é"));
    assert!(exact.ends_with(b"\r\n \t"));
}

#[test]
fn single_contract_and_explicit_off_keep_presence_distinct_from_legacy_absence() {
    for contract in [TeletextPageContract::V1, TeletextPageContract::V2] {
        let mut fixture = Fixture::mixed();
        let remove = usize::from(contract == TeletextPageContract::V1);
        fixture.selection.bindings.remove(remove);
        fixture.sources.remove(remove);
        let mut input = fixture.input();
        input.entry_addon_id = Some(fixture.selection.bindings[0].addon_id.as_str());
        input.entry = Some(TeletextPageAddress {
            page: TeletextPageNumber::new(if contract == TeletextPageContract::V1 {
                100
            } else {
                200
            })
            .unwrap(),
            subcode: TeletextSubcode::new(16).unwrap(),
        });
        input.v1_region =
            (contract == TeletextPageContract::V1).then_some(TeletextSavedV1Region::WesternLatin);
        let encoded = encode_teletext_design_documents_with_requests(
            input,
            &requests(&fixture.selection),
            budget(),
        )
        .unwrap();
        let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
        for candidate in [TeletextPageContract::V1, TeletextPageContract::V2] {
            assert_eq!(
                restored
                    .guide_inputs()
                    .unwrap()
                    .request(candidate)
                    .is_some(),
                candidate == contract
            );
        }
    }
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
    let encoded =
        encode_teletext_design_documents_with_requests(input, &requests(&selection), budget())
            .unwrap();
    let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    assert_eq!(
        value["guide_inputs"],
        json!({"input_version":1,"v1":null,"v2":null})
    );
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert!(restored.guide_inputs().is_some());
    assert!(restored
        .guide_inputs()
        .unwrap()
        .request(TeletextPageContract::V1)
        .is_none());
    assert!(decode_teletext_design_documents(
        encode_teletext_design_documents(input, budget())
            .unwrap()
            .bytes(),
        budget()
    )
    .unwrap()
    .guide_inputs()
    .is_none());
    let mut invalid = value;
    invalid["guide_inputs"]["v1"] =
        value_with_requests(&Fixture::mixed())["guide_inputs"]["v1"].clone();
    assert_eq!(
        decode(&invalid).unwrap_err().reason,
        TeletextDesignDocumentFailure::GuideInputs
    );
}

#[test]
fn present_guide_inputs_are_closed_required_nullable_and_versioned() {
    let baseline = value_with_requests(&Fixture::mixed());
    for field in ["input_version", "v1", "v2"] {
        let mut changed = baseline.clone();
        changed["guide_inputs"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert_eq!(
            decode(&changed).unwrap_err().reason,
            TeletextDesignDocumentFailure::Json
        );
    }
    for input in [
        Value::Null,
        json!([]),
        json!("request"),
        json!({"input_version":1,"v1":null,"v2":null,"unknown":true}),
    ] {
        let mut changed = baseline.clone();
        changed["guide_inputs"] = input;
        assert!(decode(&changed).is_err());
    }
    for version in [0, 2] {
        let mut changed = baseline.clone();
        changed["guide_inputs"]["input_version"] = json!(version);
        assert_eq!(
            decode(&changed).unwrap_err().reason,
            TeletextDesignDocumentFailure::GuideInputs
        );
    }
    let bytes = serde_json::to_string(&baseline).unwrap();
    let duplicated_subversion = bytes.replace(
        "\"input_version\":1",
        "\"input_version\":1,\"input_version\":1",
    );
    let duplicated_root = bytes.replace(
        "\"guide_inputs\":",
        &format!(
            "\"guide_inputs\":{},\"guide_inputs\":",
            baseline["guide_inputs"]
        ),
    );
    let duplicated_variant = bytes.replace(
        "\"v1\":",
        &format!("\"v1\":{},\"v1\":", baseline["guide_inputs"]["v1"]),
    );
    for invalid in [duplicated_subversion, duplicated_root, duplicated_variant] {
        assert_eq!(
            decode_teletext_design_documents(invalid.as_bytes(), budget())
                .unwrap_err()
                .reason,
            TeletextDesignDocumentFailure::Json
        );
    }
}

#[test]
fn requests_must_match_selected_contract_generation_and_every_common_context_field() {
    let baseline = value_with_requests(&Fixture::mixed());
    for field in ["v1", "v2"] {
        let mut missing = baseline.clone();
        missing["guide_inputs"][field] = Value::Null;
        assert_eq!(
            decode(&missing).unwrap_err().reason,
            TeletextDesignDocumentFailure::GuideInputs
        );
    }
    for (field, replacement) in [
        ("schema_version", json!(1)),
        ("generation", json!("foreign-generation")),
        ("locale", json!("en")),
        ("timezone", json!("UTC")),
        ("captured_at", json!("2026-10-11T12:00:01Z")),
        (
            "window",
            json!({"start":"2026-10-10T00:00:00Z","finish":"2026-10-14T00:00:00Z"}),
        ),
        ("channels", json!([])),
    ] {
        let mut changed = baseline.clone();
        let mut request: Value =
            serde_json::from_str(changed["guide_inputs"]["v2"].as_str().unwrap()).unwrap();
        request[field] = replacement;
        changed["guide_inputs"]["v2"] = json!(serde_json::to_string(&request).unwrap());
        assert_eq!(
            decode(&changed).unwrap_err().reason,
            TeletextDesignDocumentFailure::GuideInputs
        );
    }
    let mut changed = baseline.clone();
    let request = changed["guide_inputs"]["v1"]
        .as_str()
        .unwrap()
        .replace("\"locale\":\"nl\"", "\"locale\":\"nl\",\"locale\":\"nl\"");
    changed["guide_inputs"]["v1"] = json!(request);
    assert_eq!(
        decode(&changed).unwrap_err().reason,
        TeletextDesignDocumentFailure::GuideInputs
    );
    for raw in ["{}", "null", "[]"] {
        let mut changed = baseline.clone();
        changed["guide_inputs"]["v1"] = json!(raw);
        assert_eq!(
            decode(&changed).unwrap_err().reason,
            TeletextDesignDocumentFailure::GuideInputs
        );
    }
    let mut changed = baseline;
    let mut request: Value =
        serde_json::from_str(changed["guide_inputs"]["v1"].as_str().unwrap()).unwrap();
    request["unknown"] = json!(true);
    changed["guide_inputs"]["v1"] = json!(serde_json::to_string(&request).unwrap());
    assert_eq!(
        decode(&changed).unwrap_err().reason,
        TeletextDesignDocumentFailure::GuideInputs
    );
}

pub(super) fn context_bytes(value: &Value) -> usize {
    match value {
        Value::String(text) => text.len(),
        Value::Array(values) => values.iter().map(context_bytes).sum(),
        Value::Object(fields) => fields
            .iter()
            .filter(|(key, _)| key.as_str() != "response")
            .map(|(_, value)| context_bytes(value))
            .sum(),
        _ => 0,
    }
}

#[test]
fn exact_shared_context_serialized_and_per_request_wire_limits_are_independent() {
    let fixture = Fixture::mixed();
    let templates = requests(&fixture.selection);
    let encoded = encoded_with_requests(&fixture);
    let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    let mut exact = budget();
    exact.max_context_bytes = context_bytes(&value);
    exact.max_serialized_bytes = encoded.bytes().len();
    exact.max_context_field_bytes = 64;
    assert!(
        encode_teletext_design_documents_with_requests(fixture.input(), &templates, exact).is_ok()
    );
    assert!(decode_teletext_design_documents(encoded.bytes(), exact).is_ok());
    let mut short_context = exact;
    short_context.max_context_bytes -= 1;
    for result in [
        encode_teletext_design_documents_with_requests(fixture.input(), &templates, short_context)
            .map(|_| ()),
        decode_teletext_design_documents(encoded.bytes(), short_context).map(|_| ()),
    ] {
        assert!(matches!(
            result.unwrap_err().reason,
            TeletextDesignDocumentFailure::Budget {
                dimension: "context",
                ..
            }
        ));
    }
    let mut short_storage = exact;
    short_storage.max_serialized_bytes -= 1;
    assert!(encode_teletext_design_documents_with_requests(
        fixture.input(),
        &templates,
        short_storage
    )
    .is_err());
    assert!(decode_teletext_design_documents(encoded.bytes(), short_storage).is_err());
    let mut large = budget();
    large.max_context_bytes = 2 * MAX_TELETEXT_PAGES_INPUT_BYTES;
    large.max_serialized_bytes = 3 * MAX_TELETEXT_PAGES_INPUT_BYTES;
    let mut padded = value;
    let mut request = padded["guide_inputs"]["v1"].as_str().unwrap().to_owned();
    request.extend(std::iter::repeat_n(
        ' ',
        MAX_TELETEXT_PAGES_INPUT_BYTES - request.len(),
    ));
    padded["guide_inputs"]["v1"] = json!(request);
    let bytes = serde_json::to_vec(&padded).unwrap();
    let restored = decode_teletext_design_documents(&bytes, large).unwrap();
    assert_eq!(
        restored
            .guide_inputs()
            .unwrap()
            .request(TeletextPageContract::V1)
            .unwrap()
            .len(),
        MAX_TELETEXT_PAGES_INPUT_BYTES
    );
    let mut oversized = padded["guide_inputs"]["v1"].as_str().unwrap().to_owned();
    oversized.push(' ');
    padded["guide_inputs"]["v1"] = json!(oversized);
    let error =
        decode_teletext_design_documents(&serde_json::to_vec(&padded).unwrap(), large).unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "guide request",
            ..
        }
    ));
}

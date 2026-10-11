use super::*;
use serde_json::{json, Value};

fn instant() -> DateTime<Utc> {
    "2026-10-25T00:30:00.123456789Z".parse().unwrap()
}

fn reference() -> TeletextProgrammeReferenceWire {
    TeletextProgrammeReferenceWire {
        playout_id: 7,
        first_item_id: 0,
    }
}

fn programme() -> TeletextProgrammeWire {
    TeletextProgrammeWire {
        identity: reference(),
        start: instant(),
        stop: instant(),
        title: "A literal <script> & %{pts} guide".to_owned(),
        custom_title: None,
        sub_title: Some("Episode".to_owned()),
        description: None,
        segments: vec![TeletextProgrammeSegmentWire {
            item_id: 0,
            media_item_id: None,
            start: instant(),
            finish: instant(),
            in_point_milliseconds: 1_234,
            out_point_milliseconds: 65_432,
        }],
    }
}

fn channel() -> TeletextGuideChannelWire {
    TeletextGuideChannelWire {
        id: 3,
        number: "3.1".to_owned(),
        name: "Channel".to_owned(),
        programmes: vec![programme()],
        context: TeletextProgrammeContextWire {
            current: Some(reference()),
            next: None,
            continuing: None,
            later_today: vec![reference()],
            ambiguous: false,
        },
    }
}

fn request() -> TeletextPagesRequest {
    TeletextPagesRequest {
        schema_version: TELETEXT_PAGES_V1_REQUEST_SCHEMA_VERSION,
        generation: "Opaque generation".to_owned(),
        locale: "nl-NL".to_owned(),
        timezone: "Europe/Amsterdam".to_owned(),
        captured_at: instant(),
        window: TeletextGuideWindowWire {
            start: instant(),
            finish: instant(),
        },
        channels: vec![channel()],
    }
}

fn parse_value(value: &Value) -> Result<TeletextPagesRequest, TeletextPagesRequestError> {
    TeletextPagesRequest::from_json(&serde_json::to_vec(value).unwrap())
}

#[test]
fn complete_request_roundtrips_literal_content_nanos_and_external_zero_identity() {
    let original = request();
    let encoded = original.to_json().unwrap();
    let decoded = TeletextPagesRequest::from_json(&encoded).unwrap();
    assert_eq!(decoded, original);
    let value: Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(value["captured_at"], "2026-10-25T00:30:00.123456789Z");
    assert_eq!(
        value["channels"][0]["programmes"][0]["identity"]["first_item_id"],
        0
    );
    assert_eq!(
        value["channels"][0]["programmes"][0]["custom_title"],
        Value::Null
    );
    assert_eq!(
        value["channels"][0]["programmes"][0]["title"],
        original.channels[0].programmes[0].title
    );
}

#[test]
fn every_object_is_closed_and_every_field_including_nullable_is_required() {
    let value = serde_json::to_value(request()).unwrap();
    for pointer in [
        "",
        "/window",
        "/channels/0",
        "/channels/0/programmes/0",
        "/channels/0/programmes/0/identity",
        "/channels/0/programmes/0/segments/0",
        "/channels/0/context",
        "/channels/0/context/current",
        "/channels/0/context/later_today/0",
    ] {
        let keys: Vec<String> = value
            .pointer(pointer)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        for key in keys {
            let mut missing = value.clone();
            missing
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(&key);
            assert_eq!(
                parse_value(&missing),
                Err(TeletextPagesRequestError::InvalidJson),
                "{pointer}/{key}"
            );
        }
        let mut unknown = value.clone();
        unknown
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".to_owned(), json!(null));
        assert_eq!(
            parse_value(&unknown),
            Err(TeletextPagesRequestError::InvalidJson),
            "{pointer}"
        );
    }
}

#[test]
fn explicit_nullable_metadata_and_roles_accept_null_and_reject_wrong_shapes() {
    let mut value = serde_json::to_value(request()).unwrap();
    for pointer in [
        "/channels/0/programmes/0/custom_title",
        "/channels/0/programmes/0/sub_title",
        "/channels/0/programmes/0/description",
        "/channels/0/programmes/0/segments/0/media_item_id",
        "/channels/0/context/current",
        "/channels/0/context/next",
        "/channels/0/context/continuing",
    ] {
        *value.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(parse_value(&value).is_ok());
        let mut invalid = value.clone();
        *invalid.pointer_mut(pointer).unwrap() = json!([1]);
        assert_eq!(
            parse_value(&invalid),
            Err(TeletextPagesRequestError::InvalidJson)
        );
    }
}

#[test]
fn duplicate_keys_are_rejected_even_nested_escaped_empty_or_unknown() {
    let encoded = String::from_utf8(request().to_json().unwrap()).unwrap();
    for (original, replacement) in [
        (
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
        ),
        (
            "\"generation\":",
            "\"generatio\\u006e\":\"other\",\"generation\":",
        ),
        ("\"playout_id\":7", "\"playout_id\":7,\"playout_id\":7"),
        (
            "\"in_point_milliseconds\":1234",
            "\"in_point_milliseconds\":1234,\"in_point_milliseconds\":1234",
        ),
        (
            "\"ambiguous\":false",
            "\"\":null,\"\":null,\"ambiguous\":false",
        ),
        (
            "\"ambiguous\":false",
            "\"extra\":{\"secret\":1,\"secret\":2},\"ambiguous\":false",
        ),
    ] {
        assert!(encoded.contains(original));
        let duplicate = encoded.replacen(original, replacement, 1);
        assert_eq!(
            TeletextPagesRequest::from_json(duplicate.as_bytes()),
            Err(TeletextPagesRequestError::DuplicateKey)
        );
    }
}

#[test]
fn generation_is_opaque_byte_bounded_and_schema_is_checked_on_both_paths() {
    for generation in [" ".to_owned(), "é".repeat(512)] {
        let mut valid = request();
        valid.generation = generation;
        assert_eq!(
            TeletextPagesRequest::from_json(&valid.to_json().unwrap()).unwrap(),
            valid
        );
    }
    for generation in [String::new(), "é".repeat(513)] {
        let mut invalid = request();
        invalid.generation = generation;
        assert_eq!(
            invalid.to_json(),
            Err(TeletextPagesRequestError::InvalidGeneration)
        );
        assert_eq!(
            parse_value(&serde_json::to_value(&invalid).unwrap()),
            Err(TeletextPagesRequestError::InvalidGeneration)
        );
    }
    let mut invalid = request();
    invalid.schema_version = 2;
    assert_eq!(
        invalid.to_json(),
        Err(TeletextPagesRequestError::UnsupportedSchema)
    );
    assert_eq!(
        parse_value(&serde_json::to_value(invalid).unwrap()),
        Err(TeletextPagesRequestError::UnsupportedSchema)
    );
}

#[test]
fn no_channels_or_programmes_are_permitted_without_invented_fallback() {
    let mut supplied = request();
    supplied.channels.clear();
    assert_eq!(
        TeletextPagesRequest::from_json(&supplied.to_json().unwrap()).unwrap(),
        supplied
    );
    let mut empty = channel();
    empty.programmes.clear();
    empty.context.current = None;
    empty.context.later_today.clear();
    supplied.channels.push(empty);
    assert_eq!(
        TeletextPagesRequest::from_json(&supplied.to_json().unwrap()).unwrap(),
        supplied
    );
}

#[test]
fn channel_count_is_inclusive_and_never_truncated() {
    let mut supplied = request();
    let mut empty = channel();
    empty.programmes.clear();
    supplied.channels = vec![empty.clone(); MAX_TELETEXT_PAGES_CHANNELS];
    assert_eq!(
        TeletextPagesRequest::from_json(&supplied.to_json().unwrap())
            .unwrap()
            .channels
            .len(),
        MAX_TELETEXT_PAGES_CHANNELS
    );
    supplied.channels.push(empty);
    assert_eq!(
        supplied.to_json(),
        Err(TeletextPagesRequestError::ChannelCount)
    );
    assert_eq!(
        parse_value(&serde_json::to_value(supplied).unwrap()),
        Err(TeletextPagesRequestError::ChannelCount)
    );
}

#[test]
fn occurrence_budget_counts_all_channels_without_counting_context_references() {
    let mut supplied = request();
    let mut minimal = programme();
    minimal.segments.clear();
    supplied.channels = vec![channel(), channel()];
    for channel in &mut supplied.channels {
        channel.programmes = vec![minimal.clone(); MAX_TELETEXT_PAGES_OCCURRENCES / 2];
    }
    let encoded = supplied.to_json().unwrap();
    let decoded = TeletextPagesRequest::from_json(&encoded).unwrap();
    assert_eq!(
        decoded
            .channels
            .iter()
            .map(|channel| channel.programmes.len())
            .sum::<usize>(),
        MAX_TELETEXT_PAGES_OCCURRENCES
    );
    supplied.channels[1].programmes.push(minimal);
    assert_eq!(
        supplied.to_json(),
        Err(TeletextPagesRequestError::OccurrenceCount)
    );
    assert_eq!(
        parse_value(&serde_json::to_value(supplied).unwrap()),
        Err(TeletextPagesRequestError::OccurrenceCount)
    );
}

#[test]
fn raw_input_limit_includes_whitespace_and_precedes_decoding() {
    let mut encoded = request().to_json().unwrap();
    encoded.resize(MAX_TELETEXT_PAGES_INPUT_BYTES, b' ');
    assert!(TeletextPagesRequest::from_json(&encoded).is_ok());
    encoded.push(0xff);
    assert_eq!(
        TeletextPagesRequest::from_json(&encoded),
        Err(TeletextPagesRequestError::InputTooLarge)
    );
    assert_eq!(
        TeletextPagesRequest::from_json(&[0xff]),
        Err(TeletextPagesRequestError::InvalidUtf8)
    );
}

#[test]
fn bounded_serialization_accepts_exact_limit_and_rejects_escaped_expansion() {
    let mut supplied = request();
    supplied.channels.clear();
    supplied.locale.clear();
    let overhead = supplied.to_json().unwrap().len();
    supplied.locale = "a".repeat(MAX_TELETEXT_PAGES_INPUT_BYTES - overhead);
    let exact = supplied.to_json().unwrap();
    assert_eq!(exact.len(), MAX_TELETEXT_PAGES_INPUT_BYTES);
    assert_eq!(TeletextPagesRequest::from_json(&exact).unwrap(), supplied);
    supplied.locale.push('a');
    assert_eq!(
        supplied.to_json(),
        Err(TeletextPagesRequestError::InputTooLarge)
    );
    supplied.locale = "\0".repeat(MAX_TELETEXT_PAGES_INPUT_BYTES / 2);
    assert!(supplied.locale.len() < MAX_TELETEXT_PAGES_INPUT_BYTES);
    assert_eq!(
        supplied.to_json(),
        Err(TeletextPagesRequestError::InputTooLarge)
    );
}

#[test]
fn bounded_writer_never_retains_more_than_limit_after_rejected_write() {
    let mut writer = BoundedRequestWriter {
        bytes: Vec::new(),
        overflow: false,
    };
    writer
        .write_all(&vec![b'x'; MAX_TELETEXT_PAGES_INPUT_BYTES])
        .unwrap();
    assert!(writer.write_all(b"x").is_err());
    assert!(writer.overflow);
    assert_eq!(writer.bytes.len(), MAX_TELETEXT_PAGES_INPUT_BYTES);
}

#[test]
fn instants_preserve_nanos_normalize_offsets_and_reject_non_instants() {
    let mut value = serde_json::to_value(request()).unwrap();
    value["captured_at"] = json!("2026-10-25T02:30:00.123456789+02:00");
    assert_eq!(parse_value(&value).unwrap().captured_at, instant());
    for malformed in [
        json!("2026-10-25"),
        json!("2026-10-25T00:30:00"),
        json!(7),
        json!(null),
    ] {
        value["captured_at"] = malformed;
        assert_eq!(
            parse_value(&value),
            Err(TeletextPagesRequestError::InvalidJson)
        );
    }
}

#[test]
fn trailing_documents_recursion_and_static_errors_do_not_leak_content() {
    let mut encoded = request().to_json().unwrap();
    encoded.extend_from_slice(b"\n{}");
    assert_eq!(
        TeletextPagesRequest::from_json(&encoded),
        Err(TeletextPagesRequestError::InvalidJson)
    );
    let deep = format!("{}0{}", "[".repeat(150), "]".repeat(150));
    assert_eq!(
        TeletextPagesRequest::from_json(deep.as_bytes()),
        Err(TeletextPagesRequestError::InvalidJson)
    );
    let error = TeletextPagesRequest::from_json(b"{\"private-content\":}").unwrap_err();
    assert!(!error.to_string().contains("private-content"));
    assert!(!format!("{error:?}").contains("private-content"));
}

#[test]
fn wire_boundary_does_not_claim_semantic_validation() {
    let mut supplied = request();
    supplied.locale.clear();
    supplied.timezone = "not-an-IANA-zone".to_owned();
    supplied.window.finish = supplied.window.start - chrono::Duration::hours(1);
    supplied.channels[0].id = -1;
    supplied.channels[0].context.current = Some(TeletextProgrammeReferenceWire {
        playout_id: -1,
        first_item_id: -1,
    });
    assert_eq!(
        TeletextPagesRequest::from_json(&supplied.to_json().unwrap()).unwrap(),
        supplied
    );
}

fn utc_fields(supplied: &mut TeletextPagesRequest) -> [&mut DateTime<Utc>; 7] {
    let programme = &mut supplied.channels[0].programmes[0];
    let segment = &mut programme.segments[0];
    [
        &mut supplied.captured_at,
        &mut supplied.window.start,
        &mut supplied.window.finish,
        &mut programme.start,
        &mut programme.stop,
        &mut segment.start,
        &mut segment.finish,
    ]
}

#[test]
fn all_instant_fields_reject_extended_utc_years_and_offset_overflow() {
    let pointers = [
        "/captured_at",
        "/window/start",
        "/window/finish",
        "/channels/0/programmes/0/start",
        "/channels/0/programmes/0/stop",
        "/channels/0/programmes/0/segments/0/start",
        "/channels/0/programmes/0/segments/0/finish",
    ];
    for (index, pointer) in pointers.into_iter().enumerate() {
        for invalid in [DateTime::<Utc>::MIN_UTC, DateTime::<Utc>::MAX_UTC] {
            let mut supplied = request();
            *utc_fields(&mut supplied)[index] = invalid;
            assert_eq!(
                supplied.to_json(),
                Err(TeletextPagesRequestError::InvalidJson),
                "{pointer}"
            );
        }
        for invalid in [
            "+10000-01-01T00:00:00Z",
            "-0001-12-31T23:59:59Z",
            "9999-12-31T23:59:59-01:00",
            "0000-01-01T00:00:00+01:00",
        ] {
            let mut value = serde_json::to_value(request()).unwrap();
            *value.pointer_mut(pointer).unwrap() = json!(invalid);
            assert_eq!(
                parse_value(&value),
                Err(TeletextPagesRequestError::InvalidJson),
                "{pointer}"
            );
        }
        for (wire, normalized) in [
            ("0000-01-01T00:00:00Z", "0000-01-01T00:00:00Z"),
            ("9999-12-31T23:59:59Z", "9999-12-31T23:59:59Z"),
            ("0000-01-01T01:00:00+01:00", "0000-01-01T00:00:00Z"),
            ("9999-12-31T22:59:59-01:00", "9999-12-31T23:59:59Z"),
        ] {
            let mut value = serde_json::to_value(request()).unwrap();
            *value.pointer_mut(pointer).unwrap() = json!(wire);
            let parsed = parse_value(&value).unwrap();
            let encoded = parsed.to_json().unwrap();
            assert_eq!(TeletextPagesRequest::from_json(&encoded).unwrap(), parsed);
            let encoded_value: Value = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(encoded_value.pointer(pointer).unwrap(), normalized);
        }
    }
}

#[test]
fn trusted_request_v2_roundtrip_changes_only_version_and_preserves_default_v1_bytes() {
    let mut original = request();
    let v1 = original.to_json().unwrap();
    assert_eq!(original.to_json_for_schema(1).unwrap(), v1);
    assert_eq!(
        TeletextPagesRequest::from_json_for_schema(&v1, 1).unwrap(),
        original
    );
    original.schema_version = TELETEXT_PAGES_V2_REQUEST_SCHEMA_VERSION;
    let v2 = original.to_json_for_schema(2).unwrap();
    assert_eq!(
        TeletextPagesRequest::from_json_for_schema(&v2, 2).unwrap(),
        original
    );
    let mut expected: Value = serde_json::from_slice(&v1).unwrap();
    expected["schema_version"] = json!(2);
    assert_eq!(serde_json::from_slice::<Value>(&v2).unwrap(), expected);
    assert_eq!(
        original.to_json(),
        Err(TeletextPagesRequestError::UnsupportedSchema)
    );
    assert_eq!(
        TeletextPagesRequest::from_json(&v2),
        Err(TeletextPagesRequestError::UnsupportedSchema)
    );
}

#[test]
fn trusted_request_selection_never_negotiates_payload_schema() {
    for actual in [1, 2] {
        let mut supplied = request();
        supplied.schema_version = actual;
        let bytes = supplied.to_json_for_schema(actual).unwrap();
        for expected in [0, 1, 2, 3, u32::MAX] {
            if expected == actual {
                continue;
            }
            assert_eq!(
                TeletextPagesRequest::from_json_for_schema(&bytes, expected),
                Err(TeletextPagesRequestError::UnsupportedSchema)
            );
            assert_eq!(
                supplied.to_json_for_schema(expected),
                Err(TeletextPagesRequestError::UnsupportedSchema)
            );
        }
    }
}

#[test]
fn trusted_request_v2_keeps_raw_count_and_generation_fences() {
    let mut supplied = request();
    supplied.schema_version = 2;
    let mut bytes = supplied.to_json_for_schema(2).unwrap();
    bytes.resize(MAX_TELETEXT_PAGES_INPUT_BYTES, b' ');
    assert!(TeletextPagesRequest::from_json_for_schema(&bytes, 2).is_ok());
    bytes.push(0xff);
    assert_eq!(
        TeletextPagesRequest::from_json_for_schema(&bytes, 2),
        Err(TeletextPagesRequestError::InputTooLarge)
    );
    supplied.channels = vec![channel(); MAX_TELETEXT_PAGES_CHANNELS + 1];
    assert_eq!(
        supplied.to_json_for_schema(2),
        Err(TeletextPagesRequestError::ChannelCount)
    );
    assert_eq!(
        TeletextPagesRequest::from_json_for_schema(&serde_json::to_vec(&supplied).unwrap(), 2),
        Err(TeletextPagesRequestError::ChannelCount)
    );
    let mut minimal = programme();
    minimal.segments.clear();
    supplied.channels = vec![channel()];
    supplied.channels[0].programmes = vec![minimal; MAX_TELETEXT_PAGES_OCCURRENCES + 1];
    assert_eq!(
        supplied.to_json_for_schema(2),
        Err(TeletextPagesRequestError::OccurrenceCount)
    );
    supplied.channels.clear();
    supplied.generation.clear();
    assert_eq!(
        supplied.to_json_for_schema(2),
        Err(TeletextPagesRequestError::InvalidGeneration)
    );
    supplied.generation = "synthetic-v2".into();
    supplied.locale = "\0".repeat(MAX_TELETEXT_PAGES_INPUT_BYTES / 2);
    assert_eq!(
        supplied.to_json_for_schema(2),
        Err(TeletextPagesRequestError::InputTooLarge)
    );
}

#[test]
fn trusted_request_v2_keeps_duplicate_nullable_and_instant_contracts() {
    let mut supplied = request();
    supplied.schema_version = 2;
    let bytes = supplied.to_json_for_schema(2).unwrap();
    let duplicate = String::from_utf8(bytes.clone()).unwrap().replacen(
        "\"schema_version\":2",
        "\"schema_version\":2,\"schema_version\":2",
        1,
    );
    assert_eq!(
        TeletextPagesRequest::from_json_for_schema(duplicate.as_bytes(), 2),
        Err(TeletextPagesRequestError::DuplicateKey)
    );
    let original: Value = serde_json::from_slice(&bytes).unwrap();
    for field in ["custom_title", "sub_title", "description"] {
        let mut missing = original.clone();
        missing["channels"][0]["programmes"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert_eq!(
            TeletextPagesRequest::from_json_for_schema(&serde_json::to_vec(&missing).unwrap(), 2),
            Err(TeletextPagesRequestError::InvalidJson)
        );
    }
    let mut offset = original.clone();
    offset["captured_at"] = json!("2026-10-25T02:30:00.123456789+02:00");
    assert_eq!(
        TeletextPagesRequest::from_json_for_schema(&serde_json::to_vec(&offset).unwrap(), 2)
            .unwrap()
            .captured_at,
        instant()
    );
    offset["captured_at"] = json!("2026-10-25T00:30:00");
    assert_eq!(
        TeletextPagesRequest::from_json_for_schema(&serde_json::to_vec(&offset).unwrap(), 2),
        Err(TeletextPagesRequestError::InvalidJson)
    );
}

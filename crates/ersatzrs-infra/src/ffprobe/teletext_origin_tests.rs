use serde_json::{json, Value};

use super::*;

fn fixture() -> Value {
    json!({
        "format": {"format_name": "mpegts", "duration": "12.0", "start_time": "1.418667"},
        "streams": [
            {"index": 0, "id": "0x100", "codec_type": "video", "codec_name": "mpeg2video",
             "width": 492, "height": 250, "start_pts": 129_600, "time_base": "1/90000"},
            {"index": 1, "id": "0x101", "codec_type": "audio", "codec_name": "aac",
             "channels": 1, "start_pts": 127_680, "time_base": "1/90000"},
            {"index": 2, "id": "0x102", "codec_type": "subtitle", "codec_name": "dvb_teletext",
             "start_pts": {"ignored_non_av": true}, "time_base": "not-an-av-timestamp"},
            {"index": 3, "id": "0x103", "codec_type": "audio", "codec_name": "aac",
             "channels": 1, "tags": {"language": "nld"}, "start_pts": -1, "time_base": "1/90000"}
        ]
    })
}

fn parse(value: &Value) -> Result<NativeSourceOrigin, NativeSourceOriginError> {
    parse_source_origin(&serde_json::to_vec(value).unwrap())
}

#[test]
fn native_origin_preserves_every_av_track_exact_identity_start_and_decimal_precision() {
    let origin = parse(&fixture()).unwrap();
    let format = origin.format_start().unwrap();
    assert_eq!(
        (format.units(), format.scale(), format.quantum()),
        (1_418_667, 1_000_000, (1, 1_000_000))
    );
    let streams = origin.streams();
    assert_eq!(streams.len(), 3);
    assert_eq!(
        streams
            .iter()
            .map(|stream| stream.identity())
            .collect::<Vec<_>>(),
        [
            SourceAvStreamIdentity {
                stream_index: 0,
                pid: 256,
                kind: SourceAvStreamKind::Video
            },
            SourceAvStreamIdentity {
                stream_index: 1,
                pid: 257,
                kind: SourceAvStreamKind::Audio
            },
            SourceAvStreamIdentity {
                stream_index: 3,
                pid: 259,
                kind: SourceAvStreamKind::Audio
            },
        ]
    );
    assert_eq!(
        streams
            .iter()
            .map(|stream| stream.start_pts())
            .collect::<Vec<_>>(),
        [Some(129_600), Some(127_680), Some(-1)]
    );
    assert!(streams.iter().all(|stream| {
        let base = stream.time_base().unwrap();
        (base.numerator(), base.denominator()) == (1, 90_000)
    }));
    assert_eq!(
        format.contains_rational_within_quantum(127_680, 90_000),
        Some(true)
    );
    assert_eq!(
        format.contains_rational_within_quantum(129_600, 90_000),
        Some(false)
    );
}

#[test]
fn native_unknown_and_independent_optional_timing_never_become_zero() {
    let empty = parse(&json!({})).unwrap();
    assert_eq!(empty.format_start(), None);
    assert!(empty.streams().is_empty());
    for unknown in [Value::Null, json!("N/A")] {
        let mut metadata = fixture();
        metadata["format"]["start_time"] = unknown.clone();
        metadata["streams"][0]["start_pts"] = unknown.clone();
        metadata["streams"][1]["time_base"] = unknown;
        metadata["streams"][3]
            .as_object_mut()
            .unwrap()
            .remove("start_pts");
        let origin = parse(&metadata).unwrap();
        assert_eq!(origin.format_start(), None);
        assert_eq!(origin.streams()[0].start_pts(), None);
        assert!(origin.streams()[0].time_base().is_some());
        assert!(origin.streams()[1].start_pts().is_some());
        assert_eq!(origin.streams()[1].time_base(), None);
        assert_eq!(origin.streams()[2].start_pts(), None);
    }
    let mut metadata = fixture();
    metadata["format"]["start_time"] = json!("-0.000000000000000001");
    metadata["streams"][0]["start_pts"] = json!(i64::MIN);
    let origin = parse(&metadata).unwrap();
    let start = origin.format_start().unwrap();
    assert_eq!(
        (start.units(), start.scale()),
        (-1, 1_000_000_000_000_000_000)
    );
    assert_eq!(origin.streams()[0].start_pts(), Some(i64::MIN));
}

#[test]
fn native_signed_decimals_preserve_trailing_precision_and_checked_integer_limits() {
    let precise = NativeSourceTime::parse("-12.3400").unwrap();
    assert_eq!((precise.units(), precise.scale()), (-123_400, 10_000));
    assert_ne!(precise, NativeSourceTime::parse("-12.34").unwrap());
    assert_eq!(
        NativeSourceTime::parse(&i128::MIN.to_string())
            .unwrap()
            .units(),
        i128::MIN
    );
    assert_eq!(
        NativeSourceTime::parse(&i128::MAX.to_string())
            .unwrap()
            .units(),
        i128::MAX
    );
    for value in [
        "",
        "-",
        "+1",
        ".1",
        "1.",
        "1.2.3",
        "1e2",
        "NaN",
        " 1",
        "1 ",
        "1.0000000000000000000",
        "170141183460469231731687303715884105728",
        "-170141183460469231731687303715884105729",
    ] {
        assert_eq!(
            NativeSourceTime::parse(value),
            Err(NativeSourceOriginError::Timing)
        );
    }
    assert!(NativeSourceTime::parse(&"0".repeat(MAX_DECIMAL_BYTES)).is_ok());
    assert_eq!(
        NativeSourceTime::parse(&"0".repeat(MAX_DECIMAL_BYTES + 1)),
        Err(NativeSourceOriginError::Timing)
    );
}

#[test]
fn native_decimal_quantum_enclosure_is_signed_inclusive_and_never_uses_float() {
    let value = NativeSourceTime::parse("-1.00").unwrap();
    for numerator in [-101, -100, -99] {
        assert_eq!(
            value.contains_rational_within_quantum(numerator, 100),
            Some(true)
        );
    }
    for numerator in [-102, -98] {
        assert_eq!(
            value.contains_rational_within_quantum(numerator, 100),
            Some(false)
        );
    }
    assert_eq!(value.contains_rational_within_quantum(0, 0), None);
    assert_eq!(value.contains_rational_within_quantum(i128::MAX, 1), None);
    assert_eq!(
        NativeSourceTime::parse(&i128::MAX.to_string())
            .unwrap()
            .contains_rational_within_quantum(i128::MAX, 1),
        None
    );
    let tiny = NativeSourceTime::parse("0.000000000000000001").unwrap();
    assert_eq!(
        tiny.contains_rational_within_quantum(2, 1_000_000_000_000_000_000),
        Some(true)
    );
    assert_eq!(
        tiny.contains_rational_within_quantum(3, 1_000_000_000_000_000_000),
        Some(false)
    );
}

#[test]
fn native_origin_rejects_invalid_av_timing_types_syntax_and_overflow() {
    for invalid in [json!(true), json!(0), json!([]), json!({}), json!("N/a")] {
        let mut metadata = fixture();
        metadata["format"]["start_time"] = invalid;
        assert_eq!(parse(&metadata), Err(NativeSourceOriginError::Timing));
    }
    for invalid in [
        json!(true),
        json!(1.5),
        json!("123"),
        json!("9223372036854775808"),
        json!(u64::MAX),
    ] {
        let mut metadata = fixture();
        metadata["streams"][0]["start_pts"] = invalid;
        assert_eq!(parse(&metadata), Err(NativeSourceOriginError::Timing));
    }
    for invalid in [
        "0/1",
        "1/0",
        "-1/90000",
        "1/-90000",
        "1.0/90000",
        "1/90000/1",
        "1/2147483648",
        "2147483648/1",
        " 1/90000",
        "1/90000 ",
    ] {
        let mut metadata = fixture();
        metadata["streams"][0]["time_base"] = json!(invalid);
        assert_eq!(parse(&metadata), Err(NativeSourceOriginError::Timing));
    }
    let maximum = NativeSourceTimeBase::parse("2147483647/2147483647").unwrap();
    assert_eq!(
        (maximum.numerator(), maximum.denominator()),
        (i32::MAX, i32::MAX)
    );
}

#[test]
fn native_origin_rejects_invalid_or_duplicate_av_identities_without_partial_results() {
    for invalid in [
        Value::Null,
        json!(-1),
        json!(2_147_483_648_u64),
        json!(1.5),
        json!("0"),
    ] {
        let mut metadata = fixture();
        metadata["streams"][3]["index"] = invalid;
        assert_eq!(parse(&metadata), Err(NativeSourceOriginError::Identity));
    }
    for invalid in [
        Value::Null,
        json!(256),
        json!("256"),
        json!("0x1f"),
        json!("0x1fff"),
        json!("0x2000"),
        json!("0x10000"),
        json!("0x"),
        json!("0xzz"),
    ] {
        let mut metadata = fixture();
        metadata["streams"][3]["id"] = invalid;
        assert_eq!(parse(&metadata), Err(NativeSourceOriginError::Identity));
    }
    for field in ["index", "id"] {
        let mut metadata = fixture();
        metadata["streams"][3][field] = metadata["streams"][0][field].clone();
        assert_eq!(parse(&metadata), Err(NativeSourceOriginError::Identity));
    }
    let mut metadata = fixture();
    metadata["streams"][0]["id"] = json!("0x20");
    metadata["streams"][3]["id"] = json!("0x1ffe");
    assert!(parse(&metadata).is_ok());
}

#[test]
fn native_origin_input_and_row_counts_are_bounded_before_projection() {
    let rows = vec![json!({"codec_type": "data"}); MAX_STREAMS];
    let mut metadata = json!({"streams": rows});
    assert!(parse(&metadata).unwrap().streams().is_empty());
    metadata["streams"]
        .as_array_mut()
        .unwrap()
        .push(json!({"codec_type": "data"}));
    assert_eq!(parse(&metadata), Err(NativeSourceOriginError::Metadata));
    assert_eq!(
        parse_source_origin(&vec![
            b' ';
            super::super::MAX_REMOTE_FFPROBE_OUTPUT_BYTES + 1
        ]),
        Err(NativeSourceOriginError::Metadata)
    );
}

#[test]
fn added_origin_metadata_does_not_change_the_ordinary_probe_projection() {
    let with = fixture();
    let mut without = with.clone();
    without["format"]
        .as_object_mut()
        .unwrap()
        .remove("start_time");
    for stream in without["streams"].as_array_mut().unwrap() {
        let stream = stream.as_object_mut().unwrap();
        stream.remove("start_pts");
        stream.remove("time_base");
    }
    let plain =
        super::super::parse_ffprobe_json("synthetic.ts", &serde_json::to_vec(&without).unwrap())
            .unwrap();
    let mut timed =
        super::super::parse_ffprobe_json("synthetic.ts", &serde_json::to_vec(&with).unwrap())
            .unwrap();
    // Ordinary extraction independently records wall time on every call.
    timed.version.date_added = plain.version.date_added;
    timed.version.date_updated = plain.version.date_updated;
    assert_eq!(timed, plain);
    let _ = parse(&with).unwrap();
}

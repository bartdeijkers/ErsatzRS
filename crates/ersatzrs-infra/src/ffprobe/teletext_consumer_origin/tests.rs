use serde_json::{json, Value};

use super::*;

fn fixture() -> Value {
    json!({
        "format":{"format_name":"mpegts","start_time":"1.000000"},
        "streams":[
            {"index":0,"id":"0x100","codec_type":"video","start_pts":270_000,"time_base":"1/90000"},
            {"index":1,"id":"0x101","codec_type":"audio","start_pts":90000,"time_base":"1/90000"},
            {"index":2,"id":"0x102","codec_type":"audio","start_pts":180_000,"time_base":"1/90000"},
            {"index":3,"id":"0x103","codec_type":"subtitle","start_pts":135_000,"time_base":"1/90000"}
        ]
    })
}

fn parse(value: &Value, enabled: &[i32]) -> NativeSourceConsumerOrigin {
    parse_consumer_origin(&serde_json::to_vec(value).unwrap(), enabled).unwrap()
}

#[test]
fn explicit_audio_and_subtitle_selection_changes_presentation_but_not_seek_origin() {
    let early = parse(&fixture(), &[0, 1]).timing(0).unwrap();
    let late = parse(&fixture(), &[0, 2]).timing(0).unwrap();
    let caption = parse(&fixture(), &[0, 2, 3]).timing(0).unwrap();
    assert_eq!(early.enabled_start_microseconds(), 1_000_000);
    assert_eq!(late.enabled_start_microseconds(), 2_000_000);
    assert_eq!(caption.enabled_start_microseconds(), 1_500_000);
    for timing in [early, late, caption] {
        assert_eq!(
            timing.format_start_bounds_microseconds(),
            (999_999, 1_000_001)
        );
        assert_eq!(
            timing.initial_seek_bounds_microseconds(),
            (999_999, 1_000_001)
        );
        assert_eq!(timing.effective_seek_microseconds(), 0);
    }
    assert_eq!(
        late.presentation_zero_bounds_microseconds(),
        (2_000_000, 2_000_000)
    );
    assert_eq!(
        caption.presentation_zero_bounds_microseconds(),
        (1_500_000, 1_500_000)
    );
    assert_eq!(
        early.presentation_zero_bounds_microseconds(),
        (1_000_000, 1_000_001)
    );
}

#[test]
fn nonzero_seek_dominates_enabled_origin_without_seeking_from_that_origin() {
    let origin = parse(&fixture(), &[0, 2]);
    let timing = origin.timing(4_000_000).unwrap();
    assert_eq!(timing.enabled_start_microseconds(), 2_000_000);
    assert_eq!(
        timing.initial_seek_bounds_microseconds(),
        (4_999_999, 5_000_001)
    );
    assert_eq!(
        timing.presentation_zero_bounds_microseconds(),
        (4_999_999, 5_000_001)
    );
    assert_ne!(
        timing.initial_seek_bounds_microseconds(),
        (6_000_000, 6_000_000)
    );
    assert!(origin.timing(-1).is_none());
}

#[test]
fn projection_retains_requested_order_and_exact_non_av_identity_and_timing() {
    let origin = parse(&fixture(), &[3, 2, 0]);
    assert_eq!(origin.format_name(), Some("mpegts"));
    assert_eq!(origin.format_start().unwrap().units(), 1_000_000);
    assert_eq!(
        origin
            .streams()
            .iter()
            .map(NativeSourceConsumerStream::index)
            .collect::<Vec<_>>(),
        [3, 2, 0]
    );
    let caption = &origin.streams()[0];
    assert_eq!(caption.pid(), 0x103);
    assert_eq!(caption.codec_type(), "subtitle");
    assert_eq!(caption.start_pts(), Some(135_000));
    let base = caption.time_base().unwrap();
    assert_eq!((base.numerator(), base.denominator()), (1, 90000));
}

#[test]
fn exact_rescaling_uses_nearest_rounding_with_signed_half_ties_away_from_zero() {
    for (start, base, expected) in [
        (1, "1/2000000", 1),
        (-1, "1/2000000", -1),
        (3, "1/2000000", 2),
        (-3, "1/2000000", -2),
        (1, "1/3000000", 0),
        (-1, "1/3000000", 0),
        (2, "1/3000000", 1),
        (-2, "1/3000000", -1),
    ] {
        let metadata = json!({
            "format":{"format_name":"mpegts","start_time":"0.000000"},
            "streams":[{"index":0,"id":"0x100","codec_type":"data","start_pts":start,"time_base":base}]
        });
        assert_eq!(
            parse(&metadata, &[0])
                .timing(0)
                .unwrap()
                .enabled_start_microseconds(),
            expected
        );
    }
}

#[test]
fn explicit_absent_starts_are_ignored_without_turning_unknown_into_zero() {
    for missing in [Value::Null, json!("N/A")] {
        let mut metadata = fixture();
        metadata["streams"][3]["start_pts"] = missing;
        let origin = parse(&metadata, &[0, 2, 3]);
        assert_eq!(origin.streams()[2].start_pts(), None);
        assert_eq!(
            origin.timing(0).unwrap().enabled_start_microseconds(),
            2_000_000
        );
        assert!(parse(&metadata, &[3]).timing(0).is_none());
    }
    let mut absent = fixture();
    absent["streams"][3]
        .as_object_mut()
        .unwrap()
        .remove("start_pts");
    assert!(parse(&absent, &[3]).timing(0).is_none());
    absent["streams"][3]["start_pts"] = json!(135_000);
    absent["streams"][3]
        .as_object_mut()
        .unwrap()
        .remove("time_base");
    assert!(parse(&absent, &[0, 3]).timing(0).is_none());
}

#[test]
fn unknown_format_precision_and_checked_arithmetic_cannot_grant_timing() {
    for (field, value) in [
        ("format_name", Value::Null),
        ("format_name", json!("matroska,webm")),
        ("start_time", Value::Null),
        ("start_time", json!("N/A")),
        ("start_time", json!("1.0")),
        ("start_time", json!("8796093.022209")),
    ] {
        let mut metadata = fixture();
        metadata["format"][field] = value;
        assert!(parse(&metadata, &[0, 1]).timing(0).is_none());
    }
    assert!(parse(&fixture(), &[0, 1]).timing(i64::MAX).is_none());
    let mut overflow = fixture();
    overflow["streams"][3]["start_pts"] = json!(i64::MAX);
    overflow["streams"][3]["time_base"] = json!("2147483647/1");
    assert!(parse(&overflow, &[3]).timing(0).is_none());
}

#[test]
fn missing_duplicate_or_invalid_enabled_inventory_is_rejected_atomically() {
    let bytes = serde_json::to_vec(&fixture()).unwrap();
    for enabled in [
        vec![],
        vec![-1],
        vec![0, 0],
        vec![0, 9],
        vec![0; MAX_STREAMS + 1],
    ] {
        assert!(parse_consumer_origin(&bytes, &enabled).is_err());
    }
    let mut duplicate = fixture();
    duplicate["streams"][3]["index"] = json!(0);
    assert!(parse_consumer_origin(&serde_json::to_vec(&duplicate).unwrap(), &[0]).is_err());
    let mut duplicate = fixture();
    duplicate["streams"][3]["id"] = json!("0x100");
    assert!(parse_consumer_origin(&serde_json::to_vec(&duplicate).unwrap(), &[0, 3]).is_err());
    for invalid in [Value::Null, json!("3"), json!(-1), json!(i64::MAX)] {
        let mut metadata = fixture();
        metadata["streams"][3]["index"] = invalid;
        assert!(parse_consumer_origin(&serde_json::to_vec(&metadata).unwrap(), &[0]).is_err());
    }
}

#[test]
fn selected_non_av_timing_and_identity_are_validated_while_unselected_values_stay_opaque() {
    for (field, invalid) in [
        ("id", json!(256)),
        ("id", json!("0x1fff")),
        ("id", json!("0x+100")),
        ("codec_type", json!("unfamiliar")),
        ("start_pts", json!(1.5)),
        ("start_pts", json!("1")),
        ("start_pts", json!(i64::MIN)),
        ("time_base", json!("0/1")),
        ("time_base", json!(true)),
    ] {
        let mut metadata = fixture();
        metadata["streams"][3][field] = invalid;
        assert!(parse_consumer_origin(&serde_json::to_vec(&metadata).unwrap(), &[3]).is_err());
    }
    let mut opaque = fixture();
    opaque["streams"][3]["id"] = json!(false);
    opaque["streams"][3]["start_pts"] = json!({"uninterpreted":true});
    opaque["streams"][3]["time_base"] = json!("not-timing");
    assert_eq!(parse(&opaque, &[0, 1]).streams().len(), 2);
}

#[test]
fn original_byte_and_stream_bounds_apply_before_selected_projection() {
    let metadata = json!({"streams":vec![json!({"index":0,"codec_type":"data"}); MAX_STREAMS + 1]});
    assert!(parse_consumer_origin(&serde_json::to_vec(&metadata).unwrap(), &[0]).is_err());
    let oversized = vec![b' '; super::super::MAX_REMOTE_FFPROBE_OUTPUT_BYTES + 1];
    assert!(parse_consumer_origin(&oversized, &[0]).is_err());
}

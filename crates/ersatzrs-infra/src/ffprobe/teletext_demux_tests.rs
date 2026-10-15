//! Native demux observations retain order and uncertainty without cue authority.

use serde_json::{json, Value};

use crate::ffprobe::teletext_demux::{
    parse_source_demux, NativeSourceDemuxError, NativeSourceDemuxTrace,
};

#[test]
fn native_packet_flags_keep_unknown_discard_and_corrupt_distinct() {
    for (flags, discard, corrupt) in [
        ("___", false, false),
        ("K__", false, false),
        ("_D_", true, false),
        ("KD_", true, false),
        ("__C", false, true),
        ("K_C", false, true),
        ("_DC", true, true),
        ("KDC", true, true),
    ] {
        let mut value = fixture();
        value["packets"][0]["flags"] = json!(flags);
        let trace = parse(&value).unwrap();
        let packet = trace.packets()[0];
        assert_eq!(packet.is_discard(), Some(discard));
        assert_eq!(packet.is_corrupt(), Some(corrupt));
    }
    for flags in [
        Value::Null,
        json!("N/A"),
        json!(""),
        json!("K_"),
        json!("_X_"),
        json!("___C"),
    ] {
        let mut value = fixture();
        value["packets"][0]["flags"] = flags;
        let packet = parse(&value).unwrap().packets()[0];
        assert_eq!(packet.is_discard(), None);
        assert_eq!(packet.is_corrupt(), None);
    }
    assert_eq!(parse(&fixture()).unwrap().packets()[0].is_corrupt(), None);
}

fn fixture() -> Value {
    json!({
        "format": {"format_name": "mpegts", "duration": "60.0"},
        "streams": [
            {"index": 4, "id": "0x100", "codec_type": "video", "codec_name": "mpeg2video", "time_base": "1/90000"},
            {"index": 0, "id": "0x101", "codec_type": "audio", "codec_name": "aac", "time_base": "1/90000"},
            {"index": 7, "id": "0x102", "codec_type": "subtitle", "codec_name": "dvb_teletext", "time_base": "1/90000"}
        ],
        "programs": [
            {"program_num": 13, "pmt_pid": 4096, "pcr_pid": 256, "streams": [{"index":4},{"index":0},{"index":7}]}
        ],
        "packets": [
            {"stream_index":7, "pts":8_589_934_590_i64, "pos":"376"},
            {"stream_index":4, "pts":-1, "dts":-3, "pos":"188"},
            {"stream_index":0, "pts":8_589_934_602_i64, "dts":8_589_934_600_i64, "pos":"377"},
            {"stream_index":4, "pts":0, "dts":-1, "pos":"0"}
        ]
    })
}

fn parse(metadata: &Value) -> Result<NativeSourceDemuxTrace, NativeSourceDemuxError> {
    parse_source_demux(&serde_json::to_vec(metadata).unwrap())
}

#[test]
fn demux_preserves_native_packet_order_signed_clocks_and_independent_identities() {
    let trace = parse(&fixture()).unwrap();
    assert_eq!(
        trace
            .streams()
            .iter()
            .map(|stream| (stream.index(), stream.pid(), stream.codec_type()))
            .collect::<Vec<_>>(),
        [(4, 256, "video"), (0, 257, "audio"), (7, 258, "subtitle")]
    );
    assert!(trace.streams().iter().all(|stream| {
        let base = stream.time_base().unwrap();
        (base.numerator(), base.denominator()) == (1, 90_000)
    }));
    let program = &trace.programs()[0];
    assert_eq!(
        (program.number(), program.pmt_pid(), program.pcr_pid()),
        (13, 4096, 256)
    );
    assert_eq!(program.stream_indices(), [4, 0, 7]);
    assert_eq!(
        trace
            .packets()
            .iter()
            .map(|packet| (
                packet.stream_index(),
                packet.pts(),
                packet.dts(),
                packet.position(),
                packet.source_packet()
            ))
            .collect::<Vec<_>>(),
        [
            (7, Some(8_589_934_590), None, Some(376), Some(2)),
            (4, Some(-1), Some(-3), Some(188), Some(1)),
            (0, Some(8_589_934_602), Some(8_589_934_600), Some(377), None),
            (4, Some(0), Some(-1), Some(0), Some(0))
        ]
    );
    assert!(!trace.hit_packet_limit());
}

#[test]
fn demux_missing_null_and_native_unknown_values_never_become_zero() {
    for unknown in [Value::Null, json!("N/A")] {
        let mut metadata = fixture();
        metadata["streams"][0]["time_base"] = unknown.clone();
        metadata["packets"][0]["pts"] = unknown.clone();
        metadata["packets"][0]["dts"] = unknown.clone();
        metadata["packets"][0]["pos"] = unknown;
        metadata["streams"][1]
            .as_object_mut()
            .unwrap()
            .remove("time_base");
        metadata["packets"][1]
            .as_object_mut()
            .unwrap()
            .remove("pts");
        metadata["packets"][1]
            .as_object_mut()
            .unwrap()
            .remove("dts");
        metadata["packets"][1]
            .as_object_mut()
            .unwrap()
            .remove("pos");
        metadata["packets"][2]["pos"] = json!("-1");
        let trace = parse(&metadata).unwrap();
        assert_eq!(trace.streams()[0].time_base(), None);
        assert_eq!(trace.streams()[1].time_base(), None);
        for packet in &trace.packets()[..2] {
            assert_eq!(packet.pts(), None);
            assert_eq!(packet.dts(), None);
            assert_eq!(packet.position(), None);
            assert_eq!(packet.source_packet(), None);
        }
        assert_eq!(trace.packets()[2].position(), None);
        assert_eq!(trace.packets()[2].source_packet(), None);
    }
    let mut metadata = fixture();
    metadata["packets"] = json!([]);
    let trace = parse(&metadata).unwrap();
    assert!(trace.packets().is_empty());
    assert!(!trace.hit_packet_limit());
}

#[test]
fn demux_retains_all_programs_and_shared_global_memberships_without_first_choice() {
    let mut metadata = fixture();
    metadata["programs"].as_array_mut().unwrap().push(json!({
        "program_num": 29, "pmt_pid": 4096, "pcr_pid": 256,
        "streams": [{"index":7},{"index":4},{"index":0}]
    }));
    let trace = parse(&metadata).unwrap();
    assert_eq!(trace.programs().len(), 2);
    assert_eq!(trace.programs()[0].stream_indices(), [4, 0, 7]);
    assert_eq!(trace.programs()[1].stream_indices(), [7, 4, 0]);
    assert_eq!(trace.streams().len(), 3);
}

#[test]
fn demux_invalid_or_duplicate_global_streams_reject_the_entire_trace() {
    for invalid in [
        Value::Null,
        json!(-1),
        json!(2_147_483_648_u64),
        json!("4"),
        json!(1.5),
    ] {
        let mut metadata = fixture();
        metadata["streams"][2]["index"] = invalid;
        assert!(parse(&metadata).is_err());
    }
    for invalid in [
        Value::Null,
        json!(258),
        json!("0x1f"),
        json!("0x1fff"),
        json!("0x2000"),
        json!("0xzz"),
        json!("0x"),
    ] {
        let mut metadata = fixture();
        metadata["streams"][2]["id"] = invalid;
        assert!(parse(&metadata).is_err());
    }
    for field in ["index", "id"] {
        let mut metadata = fixture();
        metadata["streams"][2][field] = metadata["streams"][0][field].clone();
        assert!(parse(&metadata).is_err());
    }
    for invalid in [
        json!("0/90000"),
        json!("1/0"),
        json!("-1/90000"),
        json!("1/2147483648"),
        json!(90_000),
    ] {
        let mut metadata = fixture();
        metadata["streams"][2]["time_base"] = invalid;
        assert!(parse(&metadata).is_err());
    }
}

#[test]
fn demux_invalid_duplicate_or_unresolved_program_memberships_are_rejected() {
    for (field, invalid) in [
        ("program_num", json!(0)),
        ("program_num", json!(65_536)),
        ("pmt_pid", json!(0x1f)),
        ("pmt_pid", json!(0x1fff)),
        ("pcr_pid", json!(0x1f)),
        ("pcr_pid", json!(0x2000)),
    ] {
        let mut metadata = fixture();
        metadata["programs"][0][field] = invalid;
        assert!(parse(&metadata).is_err());
    }
    let mut duplicate = fixture();
    let program = duplicate["programs"][0].clone();
    duplicate["programs"].as_array_mut().unwrap().push(program);
    assert!(parse(&duplicate).is_err());
    for members in [
        json!([{"index":4},{"index":4}]),
        json!([{"index":999}]),
        json!([{}]),
    ] {
        let mut metadata = fixture();
        metadata["programs"][0]["streams"] = members;
        assert!(parse(&metadata).is_err());
    }
    let mut missing = fixture();
    missing["packets"][3]["stream_index"] = json!(999);
    assert!(parse(&missing).is_err());
}

#[test]
fn demux_invalid_timestamps_and_positions_cannot_create_usable_source_offsets() {
    for field in ["pts", "dts"] {
        for invalid in [
            json!(i64::MIN),
            json!(u64::MAX),
            json!(1.5),
            json!("1"),
            json!(false),
        ] {
            let mut metadata = fixture();
            metadata["packets"][3][field] = invalid;
            assert!(parse(&metadata).is_err());
        }
    }
    for invalid in [
        json!("-2"),
        json!("18446744073709551616"),
        json!("1.0"),
        json!("abc"),
        json!(188),
        json!(false),
    ] {
        let mut metadata = fixture();
        metadata["packets"][3]["pos"] = invalid;
        assert!(parse(&metadata).is_err());
    }
    let mut metadata = fixture();
    metadata["packets"][3]["pos"] = json!(u64::MAX.to_string());
    metadata["packets"][3]["pts"] = json!(i64::MAX);
    let trace = parse(&metadata).unwrap();
    assert_eq!(trace.packets()[3].position(), Some(u64::MAX));
    assert_eq!(trace.packets()[3].source_packet(), None);
    assert_eq!(trace.packets()[3].pts(), Some(i64::MAX));
}

#[test]
fn demux_requires_explicit_arrays_and_distinguishes_a_full_bounded_packet_trace() {
    for field in ["streams", "programs", "packets"] {
        let mut missing = fixture();
        missing.as_object_mut().unwrap().remove(field);
        assert!(parse(&missing).is_err());
        let mut null = fixture();
        null[field] = Value::Null;
        assert!(parse(&null).is_err());
    }
    let mut metadata = fixture();
    metadata["packets"] = json!(vec![json!({"stream_index":4,"pts":0,"pos":"0"}); 4095]);
    assert!(!parse(&metadata).unwrap().hit_packet_limit());
    metadata["packets"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stream_index":4,"pts":0,"pos":"0"}));
    let full = parse(&metadata).unwrap();
    assert_eq!(full.packets().len(), 4096);
    assert!(full.hit_packet_limit());
    metadata["packets"]
        .as_array_mut()
        .unwrap()
        .push(json!({"stream_index":4,"pts":0,"pos":"0"}));
    assert!(parse(&metadata).is_err());
}

#[test]
fn demux_program_and_json_capacity_bound_the_supplied_trace() {
    let mut metadata = fixture();
    let programs: Vec<_> = (0..1024)
        .map(|ordinal| {
            json!({
                "program_num": ordinal + 1, "pmt_pid": ordinal + 4096, "pcr_pid":256,
                "streams":[{"index":4}]
            })
        })
        .collect();
    metadata["programs"] = json!(programs);
    assert_eq!(parse(&metadata).unwrap().programs().len(), 1024);
    metadata["programs"].as_array_mut().unwrap().push(json!({
        "program_num":1025,"pmt_pid":5120,"pcr_pid":256,"streams":[{"index":4}]
    }));
    assert!(parse(&metadata).is_err());
    let mut oversized = fixture();
    oversized["ignored_padding"] =
        json!("x".repeat(crate::ffprobe::MAX_REMOTE_FFPROBE_OUTPUT_BYTES));
    assert!(
        parse(&oversized).is_err(),
        "valid JSON exceeding the byte cap must be rejected too"
    );
}

#[test]
fn demux_packet_and_membership_fields_leave_ordinary_metadata_unchanged() {
    let with = fixture();
    let mut without = with.clone();
    without.as_object_mut().unwrap().remove("packets");
    without.as_object_mut().unwrap().remove("programs");
    for stream in without["streams"].as_array_mut().unwrap() {
        stream.as_object_mut().unwrap().remove("time_base");
    }
    let plain =
        crate::ffprobe::parse_ffprobe_json("synthetic.ts", &serde_json::to_vec(&without).unwrap())
            .unwrap();
    let mut extended =
        crate::ffprobe::parse_ffprobe_json("synthetic.ts", &serde_json::to_vec(&with).unwrap())
            .unwrap();
    extended.version.date_added = plain.version.date_added;
    extended.version.date_updated = plain.version.date_updated;
    assert_eq!(extended, plain);
}

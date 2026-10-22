use super::*;
use serde_json::{json, Value};

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(5)
}

fn fixture() -> Value {
    let stream = json!({"index":0,"codec_type":"video","id":"0x100",
        "time_base":"1/90000","nb_read_packets":"309"});
    json!({
        "streams":[stream.clone()],
        "programs":[{"program_num":1,"streams":[stream]}],
        "frames":[
            {"media_type":"video","stream_index":0,"pts":898_290,"duration":3600},
            {"media_type":"video","stream_index":0,"pts":901_890,"duration":3600},
            {"media_type":"video","stream_index":0,"pts":905_490,"duration":3600},
            {"media_type":"video","stream_index":0,"pts":909_090,"duration":3600}
        ]
    })
}

fn check(value: Value) -> Result<Option<FiniteVideoCut>, SourceTeletextPageDecodeError> {
    let output = serde_json::from_value(value).unwrap();
    qualify_output(&output, (0, 256, 1), 9200, (861_599, 861_601), deadline())
}

#[test]
fn finite_video_cut_uses_actual_complete_frame_ends_and_exact_source_clock() {
    let cut = check(fixture()).unwrap().unwrap();
    assert_eq!(cut.out_ms(), 9200);
    assert_eq!(cut.cut_pts_90k(), 827_946);
    // Variable durations are observed directly; no nominal rate is consulted.
    let mut value = fixture();
    value["frames"][0]["duration"] = json!(1800);
    value["frames"][1]["duration"] = json!(3601);
    assert_eq!(check(value).unwrap(), Some(cut));
}

#[test]
fn finite_video_cut_needs_complete_prefix_exclusion_and_later_outpoint_bookend() {
    for range in [0..2, 0..3, 2..4, 0..0] {
        let mut value = fixture();
        value["frames"] = json!(&value["frames"].as_array().unwrap()[range]);
        assert_eq!(check(value).unwrap(), None);
    }
    // A count below the packet cap and a successful process are not EOF proof.
    let mut value = fixture();
    value["streams"][0]["nb_read_packets"] = json!("3");
    value["programs"][0]["streams"][0]["nb_read_packets"] = json!("3");
    value["frames"].as_array_mut().unwrap().truncate(3);
    assert_eq!(check(value).unwrap(), None);
}

#[test]
fn finite_video_cut_refuses_incomplete_then_complete_frame_prefix() {
    let mut value = fixture();
    value["frames"][0]["duration"] = json!(20_000);
    assert_eq!(check(value).unwrap_err(), invalid());
}

#[test]
fn finite_video_cut_refuses_invalid_frame_timing_stream_and_decode_errors() {
    for scenario in 0..7 {
        let mut value = fixture();
        match scenario {
            0 => value["frames"][1]["pts"] = Value::Null,
            1 => value["frames"][1]["duration"] = Value::Null,
            2 => value["frames"][1]["duration"] = json!(0),
            3 => value["frames"][1]["pts"] = json!(898_290),
            4 => value["frames"][1]["decode_error_flags"] = json!(1),
            5 => value["frames"][1]["stream_index"] = json!(3),
            _ => value["frames"][1]["media_type"] = json!("audio"),
        }
        assert!(check(value).is_err(), "scenario {scenario}");
    }
    let mut value = fixture();
    value["frames"][3]["pts"] = json!(i64::MAX);
    assert!(check(value).is_err());
}

#[test]
fn finite_video_cut_checks_exact_selected_pid_program_index_and_native_time_base() {
    let output: Output = serde_json::from_value(fixture()).unwrap();
    for (index, pid, program) in [(1, 256, 1), (0, 257, 1), (0, 256, 2)] {
        assert_eq!(
            qualify_output(
                &output,
                (index, pid, program),
                9200,
                (861_599, 861_601),
                deadline()
            )
            .unwrap_err(),
            SourceTeletextPageDecodeError::StreamChanged
        );
    }
    for time_base in ["1/1000", "2/180000", "0/90000", "1/0"] {
        let mut value = fixture();
        value["streams"][0]["time_base"] = json!(time_base);
        value["programs"][0]["streams"][0]["time_base"] = json!(time_base);
        assert!(check(value).is_err());
    }
}

#[test]
fn finite_video_cut_requires_the_held_native_video_time_base_and_role() {
    for (time_base, kind, expected) in [
        (json!("1/90000"), "video", true),
        (Value::Null, "video", false),
        (json!("1/1000"), "video", false),
        (json!("1/90000"), "audio", false),
    ] {
        let bytes = serde_json::to_vec(&json!({
            "streams":[{"index":0,"id":"0x100","codec_type":kind,
                "time_base":time_base}]
        }))
        .unwrap();
        let origin = crate::ffprobe::teletext_origin::parse_source_origin(&bytes).unwrap();
        assert_eq!(native_video_pid(&origin, 0).is_ok(), expected);
        assert!(native_video_pid(&origin, 1).is_err());
    }
}

#[test]
fn finite_video_cut_rejects_bad_cardinality_and_packet_counts() {
    for scenario in 0..7 {
        let mut value = fixture();
        match scenario {
            0 => value["streams"].as_array_mut().unwrap().clear(),
            1 => {
                let stream = value["streams"][0].clone();
                value["streams"].as_array_mut().unwrap().push(stream);
            }
            2 => {
                let program = value["programs"][0].clone();
                value["programs"].as_array_mut().unwrap().push(program);
            }
            3 => value["programs"][0]["streams"]
                .as_array_mut()
                .unwrap()
                .clear(),
            _ => {
                let count = ["0", "4097", "N/A"][scenario - 4];
                value["streams"][0]["nb_read_packets"] = json!(count);
                value["programs"][0]["streams"][0]["nb_read_packets"] = json!(count);
            }
        }
        assert!(check(value).is_err(), "scenario {scenario}");
    }
    let mut value = fixture();
    let frame = value["frames"][0].clone();
    value["frames"] = json!(vec![frame; 4097]);
    assert!(check(value).is_err());
}

#[test]
fn finite_video_cut_refuses_origin_classification_and_tick_rounding_ambiguity() {
    let output: Output = serde_json::from_value(fixture()).unwrap();
    // The complete/excluded decision straddles the reported origin enclosure.
    assert!(qualify_output(&output, (0, 256, 1), 9200, (861_000, 862_000), deadline()).is_err());
    assert!(qualify_output(&output, (0, 256, 1), 9200, (861_594, 861_596), deadline()).is_err());
    assert!(qualify_output(&output, (0, 256, 1), 9200, (861_601, 861_599), deadline()).is_err());
    assert_eq!(normalize_cut(100, -50).unwrap(), 105);
    assert_eq!(normalize_cut(100, 50).unwrap(), 95);
}

#[test]
fn finite_video_cut_preserves_deadline_and_checked_outpoint_limits() {
    let output: Output = serde_json::from_value(fixture()).unwrap();
    assert_eq!(
        qualify_output(
            &output,
            (0, 256, 1),
            9200,
            (861_599, 861_601),
            Instant::now()
        )
        .unwrap_err(),
        SourceTeletextPageDecodeError::Deadline
    );
    for out_ms in [0, u64::MAX] {
        assert!(
            qualify_output(&output, (0, 256, 1), out_ms, (861_599, 861_601), deadline()).is_err()
        );
    }
}

#[test]
fn finite_video_cut_command_snapshots_original_prefix_and_selected_packet_bound() {
    let command = command("ffprobe", 0, Path::new("held.ts")).unwrap();
    insta::assert_snapshot!(command.args.join("\n"), @r"
-v
error
-select_streams
0
-read_intervals
%+#4096
-count_packets
-show_frames
-show_streams
-show_programs
-show_entries
stream=index,codec_type,id,time_base,nb_read_packets:program=program_num:program_stream=index,codec_type,id,time_base,nb_read_packets:frame=media_type,stream_index,pts,duration,decode_error_flags
-of
json
-f
mpegts
held.ts
");
}

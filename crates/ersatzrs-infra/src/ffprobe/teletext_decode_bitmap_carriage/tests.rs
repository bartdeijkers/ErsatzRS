use super::*;
use ersatzrs_domain::{
    MediaSourceKind, SourceTeletextBindingIdentity, SourceTeletextRevision, TeletextPageNumber,
};
fn request() -> SourceTeletextPageDecodeRequest {
    SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(
            11,
            MediaSourceKind::Local,
            12,
            13,
            14,
            SourceTeletextRevision::new("synthetic-bitmap-revision".into()).unwrap(),
        )
        .unwrap(),
        stream_index: 3,
        pid: 0x103,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    }
}
use crate::ffprobe::teletext_decode::SourceTeletextBitmapEvents;
use serde_json::{json, Value};

fn packet(pts: i64, duration: i64) -> Value {
    json!({"type":"packet","codec_type":"subtitle","stream_index":0,"pts":pts,"dts":pts,"duration":duration,"flags":"K__","data_hash":format!("SHA256:{}","ab".repeat(32))})
}
fn frame(pts: i64, rectangles: usize) -> Value {
    json!({"type":"subtitle","media_type":"subtitle","pts":pts,"format":0,"start_display_time":0,"end_display_time":30000,"num_rects":rectangles})
}
pub(super) fn output() -> Value {
    json!({
        "streams":[{"index":0,"codec_name":"dvb_subtitle","codec_type":"subtitle","time_base":"1/1000"}],
        "format":{"nb_streams":1,"format_name":"matroska,webm"},
        "packets_and_frames":[
            packet(560,24520),frame(560_000,1),
            packet(25080,24520),frame(25_080_000,0),
            packet(25080,15000),frame(25_080_000,1),
            packet(40080,15000),frame(40_080_000,0),
        ],
    })
}
fn events() -> SourceTeletextBitmapEvents {
    SourceTeletextBitmapEvents {
        count: 2,
        first_pts_microseconds: Some(2_040_600),
        last_pts_microseconds: Some(26_560_600),
    }
}
fn parse_value(
    value: &Value,
) -> Result<SourceTeletextBitmapArtifactObservation, SourceTeletextPageDecodeError> {
    parse(&serde_json::to_vec(value).unwrap(), events(), 1_480_600)
}
#[test]
fn bitmap_carriage_groups_preserve_same_pts_clear_replacement_and_distinct_native_origin() {
    let observation = parse_value(&output()).unwrap();
    assert_eq!(observation.nonempty_frames(), 2);
    assert_eq!(observation.clear_frames(), 2);
    assert!(observation
        .groups()
        .iter()
        .all(|group| group.all_packets_key()));
    assert_eq!(
        observation
            .groups()
            .iter()
            .map(|g| g.pts_microseconds())
            .collect::<Vec<_>>(),
        [560_000, 25_080_000, 40_080_000]
    );
    assert_eq!(observation.groups()[1].clear_frames(), 1);
    assert_eq!(observation.groups()[1].nonempty_frames(), 1);
    assert_eq!(observation.last_packet_end_microseconds(), 55_080_000);
    assert!(parse(&serde_json::to_vec(&output()).unwrap(), events(), 1_418_667).is_err());
}
#[test]
fn bitmap_carriage_rejects_lost_eof_bitmap_empty_tail_and_ambiguous_quantization() {
    let bytes = serde_json::to_vec(&output()).unwrap();
    for source in [
        SourceTeletextBitmapEvents {
            count: 3,
            ..events()
        },
        SourceTeletextBitmapEvents {
            last_pts_microseconds: Some(41_560_600),
            ..events()
        },
        SourceTeletextBitmapEvents {
            first_pts_microseconds: None,
            ..events()
        },
        SourceTeletextBitmapEvents {
            first_pts_microseconds: Some(2_040_601),
            ..events()
        },
    ] {
        assert!(parse(&bytes, source, 1_480_600).is_err());
    }
    let mut value = output();
    value["packets_and_frames"]
        .as_array_mut()
        .unwrap()
        .truncate(6);
    assert!(parse_value(&value).is_err());
}
#[test]
fn bitmap_carriage_rejects_wrong_stream_clock_order_pairing_and_missing_clear() {
    let cases = [
        ("/streams/0/codec_name", json!("dvb_teletext")),
        ("/streams/0/index", json!(1)),
        ("/streams/0/time_base", json!("1/90000")),
        ("/format/nb_streams", json!(2)),
        ("/packets_and_frames/0/pts", Value::Null),
        ("/packets_and_frames/0/dts", json!(559)),
        ("/packets_and_frames/0/flags", json!("___")),
        ("/packets_and_frames/0/flags", Value::Null),
        ("/packets_and_frames/0/duration", json!(0)),
        ("/packets_and_frames/1/pts", json!(560_001)),
        ("/packets_and_frames/1/format", json!(1)),
        ("/packets_and_frames/1/start_display_time", json!(1)),
        ("/packets_and_frames/1/num_rects", json!(4097)),
        ("/packets_and_frames/4/stream_index", json!(1)),
        ("/packets_and_frames/6/duration", json!(i64::MAX)),
        ("/packets_and_frames/7/num_rects", json!(1)),
    ];
    for (pointer, replacement) in cases {
        let mut value = output();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(parse_value(&value).is_err(), "{pointer}");
    }
    let mut value = output();
    value["packets_and_frames"]
        .as_array_mut()
        .unwrap()
        .swap(0, 1);
    assert!(parse_value(&value).is_err());
    let mut value = output();
    value["packets_and_frames"][6]["pts"] = json!(10);
    value["packets_and_frames"][6]["dts"] = json!(10);
    value["packets_and_frames"][7]["pts"] = json!(10000);
    assert!(parse_value(&value).is_err());
}
#[test]
fn bitmap_carriage_native_generation_and_probe_arguments_are_explicit() {
    let command = bitmap_command("ffmpeg", &request(), Path::new("synthetic-source.ts")).unwrap();
    let probe = artifact_probe_command("ffprobe", Path::new("synthetic-caption.mks")).unwrap();
    insta::assert_snapshot!(format!("{}\n{}\n\n{}\n{}", command.program,command.args.join("\n"),probe.program,probe.args.join("\n")), @r"
ffmpeg
-fflags
+genpts+discardcorrupt+igndts
-nostdin
-hide_banner
-loglevel
error
-threads
1
-txt_page:3
888
-txt_format:3
bitmap
-txt_default_region:3
-1
-fix_sub_duration:3
-i
synthetic-source.ts
-map
0:3
-c:s
dvbsub
-f
matroska
pipe:1

ffprobe
-v
error
-show_data_hash
sha256
-show_streams
-show_format
-show_packets
-show_frames
-show_entries
stream=index,codec_name,codec_type,time_base:format=format_name,nb_streams:packet=codec_type,stream_index,pts,dts,duration,flags,data_hash:frame=media_type,pts,format,start_display_time,end_display_time,num_rects
-of
json
synthetic-caption.mks
");
}

#[test]
fn bitmap_carriage_group_budget_matches_the_existing_source_and_concat_limit() {
    fn bounded(groups: usize) -> (Value, SourceTeletextBitmapEvents) {
        let mut value = output();
        let records = (0..groups)
            .flat_map(|index| {
                let milliseconds = i64::try_from(index).unwrap() * 10;
                [
                    packet(milliseconds, 10),
                    frame(milliseconds * 1000, usize::from(index + 1 < groups)),
                ]
            })
            .collect::<Vec<_>>();
        value["packets_and_frames"] = json!(records);
        let source = SourceTeletextBitmapEvents {
            count: groups - 1,
            first_pts_microseconds: Some(0),
            last_pts_microseconds: Some(i64::try_from(groups - 2).unwrap() * 10_000),
        };
        (value, source)
    }
    let (value, source) = bounded(4096);
    assert_eq!(
        parse(&serde_json::to_vec(&value).unwrap(), source, 0)
            .unwrap()
            .groups()
            .len(),
        4096
    );
    let (value, source) = bounded(4097);
    assert!(parse(&serde_json::to_vec(&value).unwrap(), source, 0).is_err());
}

#[test]
fn native_clock_rescaling_matches_fractional_millisecond_source_endpoints_exactly() {
    let mut value = output();
    for (index, pts, duration) in [
        (0, 600, 24540),
        (2, 25140, 24540),
        (4, 25140, 14960),
        (6, 40100, 14960),
    ] {
        value["packets_and_frames"][index]["pts"] = json!(pts);
        value["packets_and_frames"][index]["dts"] = json!(pts);
        value["packets_and_frames"][index]["duration"] = json!(duration);
        value["packets_and_frames"][index + 1]["pts"] = json!(pts * 1000);
    }
    let source = SourceTeletextBitmapEvents {
        count: 2,
        first_pts_microseconds: Some(2_100_622),
        last_pts_microseconds: Some(26_640_822),
    };
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(parse(&bytes, source, 1_500_678).is_ok());
    // Exact output coordinates remain mandatory: no plus/minus-one-ms window.
    for delta in [-1000, 1000] {
        let mut wrong = source;
        wrong.first_pts_microseconds = wrong.first_pts_microseconds.map(|pts| pts + delta);
        assert!(parse(&bytes, wrong, 1_500_678).is_err());
    }
    for invalid in [
        None,
        Some(2_100_623),
        Some(1_500_667),
        Some(i64::MIN),
        Some(i64::MAX),
    ] {
        assert!(parse(
            &bytes,
            SourceTeletextBitmapEvents {
                first_pts_microseconds: invalid,
                ..source
            },
            1_500_678
        )
        .is_err());
    }
}

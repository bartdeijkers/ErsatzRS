use super::super::tests::output;
use super::*;
use serde_json::{json, Value};

fn observe(value: &Value) -> SourceTeletextBitmapArtifactObservation {
    parser::parse_observation(&serde_json::to_vec(value).unwrap()).unwrap()
}
fn copied(value: &Value, start: i64, end: Option<i64>) -> Value {
    let mut result = value.clone();
    let records = value["packets_and_frames"].as_array().unwrap();
    let mut copied = Vec::new();
    for pair in records.chunks_exact(2) {
        let pts = pair[0]["pts"].as_i64().unwrap() * 1000;
        if pts < start || end.is_some_and(|end| pts >= end) {
            continue;
        }
        let mut packet = pair[0].clone();
        packet["pts"] = json!((pts - start) / 1000);
        packet["dts"] = packet["pts"].clone();
        let mut frame = pair[1].clone();
        frame["pts"] = json!(pts - start);
        copied.extend([packet, frame]);
    }
    result["packets_and_frames"] = json!(copied);
    result
}
#[test]
fn bitmap_isolator_selects_blank_active_same_pts_and_final_clear_without_av_authority() {
    let original = observe(&output());
    for (start, bootstrap, following, delay) in [
        (-61_933, 40_080_000, Some(560_000), Some(621_933)),
        (560_000, 560_000, Some(25_080_000), Some(24_520_000)),
        (17_520_000, 560_000, Some(25_080_000), Some(7_560_000)),
        (25_080_000, 25_080_000, Some(40_080_000), Some(15_000_000)),
        (50_000_000, 40_080_000, None, None),
    ] {
        let selected = select(&original, start).unwrap();
        assert_eq!(
            (selected.bootstrap, selected.following, selected.delay),
            (bootstrap, following, delay)
        );
    }
    assert!(select(&original, i64::MIN).is_err());
}
#[test]
fn bitmap_isolator_qualifies_complete_ordered_payloads_at_zero_and_exact_future_sequence() {
    let original = observe(&output());
    for (start, end) in [
        (560_000, Some(25_080_000)),
        (25_080_000, Some(40_080_000)),
        (40_080_000, None),
        (560_000, None),
        (25_080_000, None),
    ] {
        let fragment = observe(&copied(&output(), start, end));
        parser::qualify_fragment(&fragment, &original, start, end).unwrap();
        assert_eq!(fragment.groups()[0].pts_microseconds(), 0);
    }
    let group = copied(&output(), 25_080_000, Some(40_080_000));
    let mut changed = group.clone();
    changed["packets_and_frames"][0]["data_hash"] = json!(format!("SHA256:{}", "cd".repeat(32)));
    assert!(
        parser::qualify_fragment(&observe(&changed), &original, 25_080_000, Some(40_080_000))
            .is_err()
    );
    let mut changed = group.clone();
    changed["packets_and_frames"]
        .as_array_mut()
        .unwrap()
        .drain(..2);
    assert!(
        parser::qualify_fragment(&observe(&changed), &original, 25_080_000, Some(40_080_000))
            .is_err()
    );
    let mut changed = group.clone();
    changed["packets_and_frames"]
        .as_array_mut()
        .unwrap()
        .rotate_left(2);
    assert!(
        parser::qualify_fragment(&observe(&changed), &original, 25_080_000, Some(40_080_000))
            .is_err()
    );
    let mut changed = group;
    changed["packets_and_frames"][0]["duration"] = json!(1);
    assert!(
        parser::qualify_fragment(&observe(&changed), &original, 25_080_000, Some(40_080_000))
            .is_err()
    );
    let raw_preroll = observe(&output());
    assert!(parser::qualify_fragment(&raw_preroll, &original, 25_080_000, None).is_err());
}
#[test]
fn bitmap_isolator_requires_payload_hash_identity() {
    for hash in [
        Value::Null,
        json!("SHA256:00"),
        json!(format!("SHA256:{}", "zz".repeat(32))),
    ] {
        let mut value = output();
        value["packets_and_frames"][0]["data_hash"] = hash;
        assert!(parser::parse_observation(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}
#[test]
fn bitmap_isolator_commands_seek_after_open_and_isolate_whole_equal_pts_group() {
    let one =
        fragment_command("ffmpeg", Path::new("private-caption.mks"), 25_080_000, true).unwrap();
    let future = fragment_command(
        "ffmpeg",
        Path::new("private-caption.mks"),
        40_080_000,
        false,
    )
    .unwrap();
    insta::assert_snapshot!(format!("one complete group\n{}\n\nfuture sequence\n{}",one.args.join("\n"),future.args.join("\n")),@r"
one complete group
-nostdin
-hide_banner
-loglevel
error
-i
private-caption.mks
-ss
25.080000
-t
0.000001
-map
0:s:0
-c:s
copy
-f
matroska
pipe:1

future sequence
-nostdin
-hide_banner
-loglevel
error
-i
private-caption.mks
-ss
40.080000
-map
0:s:0
-c:s
copy
-f
matroska
pipe:1
");
    assert!(fragment_command("ffmpeg", Path::new("private-caption.mks"), -1, true).is_err());
}

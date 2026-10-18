//! Exact fixture packet association and source/converted-cue clock comparison.

use super::super::super::copy_runtime_tests::video_slices;
use super::*;
use crate::next_engine::protocol::{Source, Track};
use std::collections::BTreeSet;

pub(super) fn timing(track: &Track) -> (Option<u64>, Option<u64>) {
    let Source::Local {
        in_point_ms,
        out_point_ms,
        ..
    } = track.source.as_ref().unwrap()
    else {
        panic!("finite local track")
    };
    (*in_point_ms, *out_point_ms)
}

async fn packets(ffprobe: &Path, path: &Path, stream: &str, video: bool) -> serde_json::Value {
    let mut probe = Command::new(ffprobe);
    probe
        .args([
            "-v",
            "error",
            "-select_streams",
            stream,
            "-show_packets",
            "-show_streams",
            "-show_format",
            "-show_data_hash",
            "sha256",
            "-show_entries",
            if video {
                "packet=pts,dts,data_hash:packet_side_data=:stream=index,codec_name,time_base:format=start_time"
            } else {
                "packet=data_hash:packet_side_data=:stream=index,codec_name,time_base:format=start_time"
            },
            "-of",
            "compact=p=1:nk=0",
        ])
        .arg(path);
    let mut result = serde_json::json!({"packets":[],"streams":[],"format":{}});
    for line in std::str::from_utf8(&tool(probe).await)
        .unwrap()
        .lines()
        .filter(|line| !line.is_empty())
    {
        let mut fields = line.split('|');
        let kind = fields.next().unwrap();
        if kind == "program" {
            continue;
        }
        let mut row = serde_json::Map::new();
        for (name, value) in fields.filter_map(|field| field.split_once('=')) {
            let value = match name {
                "pts" | "dts" | "index" => serde_json::json!(value.parse::<i64>().unwrap()),
                "data_hash" | "codec_name" | "time_base" | "start_time" => {
                    serde_json::json!(value)
                }
                _ => continue,
            };
            assert!(row.insert(name.into(), value).is_none());
        }
        match kind {
            "packet" => result["packets"].as_array_mut().unwrap().push(row.into()),
            "stream" => result["streams"].as_array_mut().unwrap().push(row.into()),
            "format" => result["format"] = row.into(),
            _ => panic!("unexpected probe section {kind}"),
        }
    }
    result
}

fn hashes(value: &serde_json::Value) -> BTreeSet<&str> {
    value["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|packet| packet["data_hash"].as_str().unwrap())
        .collect()
}

async fn copied_audio(ffprobe: &Path, source: &Path, captured: &Path) {
    let dutch = packets(ffprobe, source, "a:1", false).await;
    let english = packets(ffprobe, source, "a:0", false).await;
    let actual = packets(ffprobe, captured, "a:0", false).await;
    assert_eq!(actual["streams"][0]["codec_name"], "aac");
    let expected = hashes(&dutch);
    let wrong = hashes(&english);
    let packets = actual["packets"].as_array().unwrap();
    assert!(packets.len() >= 40);
    assert!(packets
        .iter()
        .all(|packet| expected.contains(packet["data_hash"].as_str().unwrap())));
    assert!(
        packets
            .iter()
            .filter(|packet| !wrong.contains(packet["data_hash"].as_str().unwrap()))
            .count()
            >= 40,
        "actual AAC payloads must distinguish the selected alternate track"
    );
}

async fn copied_video(ffmpeg: &Path, source: &Path, actual: &Path) {
    let mut bodies = Vec::new();
    for path in [source, actual] {
        let output = tempfile::NamedTempFile::new().unwrap();
        let mut extract = Command::new(ffmpeg);
        extract
            .args(["-v", "error", "-nostdin", "-y", "-i"])
            .arg(path);
        extract
            .args([
                "-map",
                "0:v:0",
                "-t",
                "20",
                "-c",
                "copy",
                "-bsf:v",
                "h264_mp4toannexb",
                "-fs",
                "8388609",
                "-f",
                "h264",
            ])
            .arg(output.path());
        tool(extract).await;
        assert!(output.as_file().metadata().unwrap().len() <= 8 * 1024 * 1024);
        bodies.push(tokio::fs::read(output.path()).await.unwrap());
    }
    let original = video_slices(&bodies[0]);
    let actual = video_slices(&bodies[1]);
    assert!(actual.len() >= 20);
    assert!(actual.iter().all(|slice| original.contains(slice)));
}

fn video_shift(source: &serde_json::Value, actual: &serde_json::Value) -> i64 {
    for value in [source, actual] {
        assert_eq!(value["streams"][0]["time_base"], "1/90000");
    }
    let original = source["packets"].as_array().unwrap();
    let mut offsets = BTreeSet::new();
    let mut matched = 0;
    for packet in actual["packets"].as_array().unwrap() {
        let candidates = original
            .iter()
            .filter(|candidate| candidate["data_hash"] == packet["data_hash"])
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            continue;
        }
        assert_eq!(candidates.len(), 1, "copied packet identity must be unique");
        let before = candidates[0];
        let pts = packet["pts"].as_i64().unwrap() - before["pts"].as_i64().unwrap();
        let dts = packet["dts"].as_i64().unwrap() - before["dts"].as_i64().unwrap();
        assert_eq!(
            pts, dts,
            "copied packet ordering must keep the exact source PTS/DTS relation"
        );
        offsets.insert(pts);
        matched += 1;
    }
    assert!(
        matched >= 20,
        "fixture must provide independently unique coded video packets"
    );
    assert_eq!(
        offsets.len(),
        1,
        "all unique copied packets must share one timestamp shift"
    );
    *offsets.first().unwrap()
}

fn milliseconds(text: &str) -> i64 {
    let fields = text
        .split([':', ',', '.'])
        .map(|value| value.parse::<i64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(fields.len(), 4);
    fields[0] * 3_600_000 + fields[1] * 60_000 + fields[2] * 1000 + fields[3]
}

fn cue_blocks(body: &str) -> Vec<(i64, i64, String)> {
    body.split("\n\n")
        .filter_map(|block| {
            let mut lines = block.lines();
            let timing = lines.find(|line| line.contains(" --> "))?;
            let (start, end) = timing.split_once(" --> ").unwrap();
            Some((
                milliseconds(start),
                milliseconds(end),
                lines.collect::<Vec<_>>().join("\n"),
            ))
        })
        .collect()
}

fn vtt_cues(documents: &[String]) -> BTreeSet<(i64, i64, String)> {
    documents
        .iter()
        .flat_map(|document| {
            let map = document
                .lines()
                .find_map(|line| line.strip_prefix("X-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:"))
                .unwrap()
                .parse::<i64>()
                .unwrap();
            cue_blocks(document)
                .into_iter()
                .map(move |(start, end, text)| (map + start * 90, map + end * 90, text))
        })
        .collect()
}

fn format_ticks(text: &str) -> i64 {
    let (whole, fraction) = text.split_once('.').unwrap();
    assert_eq!(fraction.len(), 6, "pinned ffprobe plain microsecond writer");
    let scale = 1_000_000_i64;
    let numerator = whole.parse::<i64>().unwrap() * scale + fraction.parse::<i64>().unwrap();
    // ffprobe reports AV_TIME_BASE timestamps with six decimal places. The
    // pinned writer's closed one-microsecond enclosure must contain ONE exact
    // 90kHz tick, not a guessed/rounded tick or a widened cue comparison.
    let lower = ((numerator - 1) * 90_000 + scale - 1).div_euclid(scale);
    let upper = ((numerator + 1) * 90_000).div_euclid(scale);
    assert_eq!(
        lower, upper,
        "reported synthetic origin must identify one exact MPEG-TS tick"
    );
    lower
}

#[test]
fn pinned_decimal_writer_identifies_exact_source_ticks_without_cue_tolerance() {
    for (reported, ticks) in [
        ("1.400000", 126_000),
        ("0.861667", 77550),
        ("1.418667", 127_680),
    ] {
        assert_eq!(format_ticks(reported), ticks);
    }
    assert!(std::panic::catch_unwind(|| format_ticks("0.861671")).is_err());
    assert!(std::panic::catch_unwind(|| format_ticks("0.86")).is_err());
}

pub(super) async fn check(
    tools: (&Path, &Path),
    source: &Path,
    captured: &Path,
    cues: &[String],
    sidecar: &str,
    interval: (u64, u64, bool),
    proof: &Path,
) {
    let (ffmpeg, ffprobe) = tools;
    copied_video(ffmpeg, source, captured).await;
    copied_audio(ffprobe, source, captured).await;
    let original = packets(ffprobe, source, "v:0", true).await;
    let actual = packets(ffprobe, captured, "v:0", true).await;
    let shift = video_shift(&original, &actual);
    let origin = format_ticks(original["format"]["start_time"].as_str().unwrap());
    assert_ne!(
        origin, 0,
        "source must discriminate full-format origin normalization"
    );
    let (seek, end, captioned) = interval;
    let independent =
        tokio::fs::read_to_string(source.parent().unwrap().join("independent-889.srt"))
            .await
            .unwrap();
    assert_eq!(
        cue_blocks(sidecar),
        cue_blocks(&independent),
        "complete cached text must equal independently decoded page889 timing and payload"
    );
    let first_output_pts = actual["packets"][0]["pts"].as_i64().unwrap();
    let landed_source_ticks = first_output_pts - shift - origin;
    assert!(first_output_pts >= 0 && landed_source_ticks >= 0);
    // Pinned Next0.2.2 render_subtitle_segment fixes the offset using the two
    // independently truncated millisecond coordinates, clamps below zero and
    // nudges the MPEGTS timestamp map by one tick. No timing tolerance is used.
    let offset_ms = first_output_pts / 90 - landed_source_ticks / 90;
    let mapped = |value: i64| 1 + (value + offset_ms).max(0) * 90;
    let join = i64::try_from(seek).unwrap();
    let finish = i64::try_from(end).unwrap();
    let expected = cue_blocks(&independent)
        .into_iter()
        .filter_map(|(start, end, text)| {
            let start = mapped(start.max(join));
            let end = mapped(end.min(finish));
            (end > start).then_some((start, end, text))
        })
        .collect::<BTreeSet<_>>();
    let actual_cues = vtt_cues(cues);
    tokio::fs::write(proof.join("timing-and-payload.json"),serde_json::to_vec_pretty(&serde_json::json!({
        "original_format_start_ticks":origin,"video_copy_shift_ticks":shift,"source_seek_ms":seek,
        "source_out_ms":end,"expected":expected,"actual":actual_cues,
        "first_output_video_pts_ticks":first_output_pts,"landed_source_ticks":landed_source_ticks,
        "fixed_subtitle_offset_ms":offset_ms,"timestamp_map_nudge_ticks":1,
        "coded_video_pts_dts_association":true,"alternate_aac_payload_identity":true,
    })).unwrap()).await.unwrap();
    if captioned {
        for text in ["889 EARLY", "889 LATE", "889 FINAL"] {
            assert!(
                actual_cues.iter().any(|(_, _, body)| body.contains(text)),
                "missing actual {text}"
            );
        }
    } else {
        assert!(expected.is_empty());
    }
    assert_eq!(
        actual_cues, expected,
        "cue timeline must match copied source AV, exact seek and outpoint"
    );
}

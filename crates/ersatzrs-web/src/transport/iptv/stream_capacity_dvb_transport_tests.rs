//! The finite, SHA-pinned DVB fixture has intentionally sparse timestamps.
//! This verifier qualifies its complete cue sequence before interpreting `TSDuck`'s
//! per-PID absolute-gap counter; the ordinary all-PID verifier is unchanged.

use std::collections::BTreeMap;

use ersatzrs_ffmpeg::teletext::source_clock::{
    SourceTeletextClockError, SourceTeletextClockObserver,
};

use super::*;

const SOURCE_SHA: &str = "b6ffa783e1cc96926a846b73fa8182755a11555296294ef9a740c09d9245c560";
const SUBTITLE_PID: u16 = 258;

pub(super) async fn verify(tools: &[String; 3], output: &Path, evidence: &Path, bytes: &[u8]) {
    let source = evidence.join("library/Synthetic Caption.ts");
    let source_bytes = bounded_read(&source, 4 * 1024 * 1024).await;
    assert_eq!(capacity_digest(&source_bytes), SOURCE_SHA);
    let (seek, command_report) = actual_seek(evidence, &source).await;
    let source_packets = video_packets(&tools[1], &source_bytes).await;
    let output_packets = video_packets(&tools[1], bytes).await;
    let (shift, witnesses) = copied_video_shift(&source_packets, &output_packets);
    let (expected, projection) = expected_cues(seek, shift);
    let first_video = first_copied_video(&source_packets, &output_packets, shift);
    let cues = raw_cues(bytes);
    let actual: Vec<_> = cues.iter().map(|cue| cue.1).collect();
    let clock_evidence = json!({"source_sha256":SOURCE_SHA,"source_command_report":command_report,
        "executed_effective_seek_microseconds":seek,"copied_video_shift_ticks90k":shift,
        "copied_video_witnesses":witnesses,"first_copied_video":first_video,
        "projection":projection,"expected_sequences":expected,
        "actual_cues":cues.iter().map(|(packet,pts,pcr)| json!({"source_packet":packet,
            "raw_pts90k":pts,"preceding_pcr27m":pcr})).collect::<Vec<_>>(),
        "scope":"Exact retained fixture/source/command/video-copy association; not a general subtitle-gap exemption"});
    tokio::fs::write(
        output.join("capacity-dvb-clock.json"),
        serde_json::to_vec_pretty(&clock_evidence).unwrap(),
    )
    .await
    .unwrap();
    assert!(
        qualified_cues(
            &expected,
            &cues,
            first_video["capture_pts"].as_i64().unwrap(),
        ),
        "bootstrap must initialize A before copied AV; future events must retain their exact SS/AV clock"
    );
    assert!(
        witnesses.first().unwrap()["capture_position"]
            .as_u64()
            .unwrap()
            < cues[1].0 * 188
    );
    assert!(
        witnesses.last().unwrap()["capture_position"]
            .as_u64()
            .unwrap()
            > cues[3].0 * 188
    );
    for (_, pts, pcr) in cues.iter().skip(1) {
        // This floor is precisely the native MPEG-TS demuxer's PCR90k conversion,
        // not a presentation tolerance or permission for a late subtitle.
        assert!(
            *pts >= i64::try_from(pcr / 300).unwrap(),
            "DVB PES must not arrive behind its program PCR"
        );
    }
    let expected_leaps = actual
        .windows(2)
        .filter(|pair| pair[0].abs_diff(pair[1]) > 270_000)
        .count();
    let report: Value = serde_json::from_slice(
        &bounded_read(&output.join("capacity-tsanalyze.json"), 256 * 1024).await,
    )
    .unwrap();
    strict_report(&report, expected_leaps);
}

pub(super) async fn bounded_read(path: &Path, limit: usize) -> Vec<u8> {
    assert!(tokio::fs::metadata(path).await.unwrap().len() <= u64::try_from(limit).unwrap());
    let bytes = tokio::fs::read(path).await.unwrap();
    assert!(bytes.len() <= limit);
    bytes
}

async fn video_packets(tool: &str, bytes: &[u8]) -> Vec<Value> {
    let args = [
        "-v",
        "error",
        "-fflags",
        "+noparse+nofillin",
        "-fix_teletext_pts",
        "0",
        "-f",
        "mpegts",
        "-i",
        "pipe:0",
        "-select_streams",
        "v:0",
        "-show_packets",
        "-show_streams",
        "-show_entries",
        "packet=pts,dts,pos,data_hash:stream=time_base",
        "-show_data_hash",
        "sha256",
        "-of",
        "json",
    ];
    let report: Value =
        serde_json::from_slice(&probe(tool, &args, bytes, 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(report["streams"].as_array().unwrap().len(), 1);
    assert_eq!(report["streams"][0]["time_base"], "1/90000");
    report["packets"].as_array().unwrap().clone()
}

fn copied_video_shift(source: &[Value], capture: &[Value]) -> (i64, Vec<Value>) {
    let mut hashes: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for packet in source {
        hashes
            .entry(packet["data_hash"].as_str().unwrap())
            .or_default()
            .push(packet);
    }
    let mut witnesses = Vec::new();
    let mut common = None;
    for packet in capture {
        let Some(candidates) = hashes.get(packet["data_hash"].as_str().unwrap()) else {
            continue;
        };
        let [original] = candidates.as_slice() else {
            continue;
        };
        let (Some(pts), Some(dts), Some(source_presentation), Some(source_decode)) = (
            packet["pts"].as_i64(),
            packet["dts"].as_i64(),
            original["pts"].as_i64(),
            original["dts"].as_i64(),
        ) else {
            continue;
        };
        let shift = pts.checked_sub(source_presentation).unwrap();
        assert_eq!(dts.checked_sub(source_decode), Some(shift));
        assert_eq!(
            *common.get_or_insert(shift),
            shift,
            "every unique copied video witness must share one offset"
        );
        witnesses.push(json!({"sha256":packet["data_hash"],"source_pts":source_presentation,"source_dts":source_decode,
            "capture_pts":pts,"capture_dts":dts,"capture_position":packet["pos"].as_str().unwrap().parse::<u64>().unwrap()}));
    }
    assert!(
        witnesses.len() >= 3,
        "multiple independent copied-payload witnesses are required"
    );
    (common.unwrap(), witnesses)
}

fn first_copied_video(source: &[Value], capture: &[Value], shift: i64) -> Value {
    let packet = capture
        .iter()
        .find(|packet| {
            // Repeated slate/skip-frame payloads cannot identify source landing.
            // Include a genuine pre-seek keyframe: suppressing it in the oracle
            // would hide a late caption on the first retained picture.
            let mut originals = source
                .iter()
                .filter(|original| packet["data_hash"] == original["data_hash"]);
            let Some(original) = originals.next() else {
                return false;
            };
            originals.next().is_none()
                && ["pts", "dts"].iter().all(|field| {
                    packet[*field]
                        .as_i64()
                        .zip(original[*field].as_i64())
                        .is_some_and(|(actual, original)| {
                            original.checked_add(shift) == Some(actual)
                        })
                })
        })
        .expect("an actual unique copied source-video packet must be identified");
    json!({"capture_dts":packet["dts"],"capture_pts":packet["pts"],
        "capture_position":packet["pos"],"sha256":packet["data_hash"]})
}

fn qualified_cues(expected: &[[i64; 4]], cues: &[(u64, i64, u64)], first_picture_pts: i64) -> bool {
    let [bootstrap, clear, update, terminal] = cues else {
        return false;
    };
    expected.iter().all(|candidate| {
        // Startup initializes the retained state before actual copied AV. It is
        // not an attempt to replay the historical A cue at the consumed seek.
        let startup_pcr = i128::from(bootstrap.2);
        bootstrap.1 >= candidate[0]
            && bootstrap.1 <= first_picture_pts
            && startup_pcr <= i128::from(first_picture_pts) * 300
            && [clear.1, update.1, terminal.1] == candidate[1..]
    })
}

fn expected_cues(seek: i64, shift: i64) -> (Vec<[i64; 4]>, Value) {
    // The exact source SHA binds the independently measured n9 source metadata:
    // F=1.418667 (its three integer-microsecond bounds), C=1.480600, and
    // artifact groups .560/25.080/40.080. Both actual enabled AV tracks start
    // before this chosen post-wait SS, so Kv=F+SS for each correlated bound.
    let mut sequences = Vec::new();
    let mut projections = Vec::new();
    let mut stable_delay = None;
    for format in 1_418_666..=1_418_668 {
        let av_zero = seek.checked_add(format).unwrap();
        let instep = av_zero.checked_sub(1_480_600).unwrap();
        assert!(
            (560_000..25_080_000).contains(&instep),
            "this fixture must resume its active A before B"
        );
        let exact_delay = 25_080_000_i64.checked_sub(instep).unwrap();
        let delay = ((exact_delay + 500) / 1000) * 1000;
        assert_eq!(
            *stable_delay.get_or_insert(delay),
            delay,
            "origin bounds must select one rounded delay"
        );
        let zero = shift
            .checked_add(
                i64::try_from((i128::from(av_zero) * 90_000 + 500_000) / 1_000_000).unwrap(),
            )
            .unwrap();
        let next = zero.checked_add(delay * 90 / 1000).unwrap().max(0);
        // Native MPEG-TS stream-copy makes the same-PTS clear/B pair strictly
        // increasing by one 90k tick. This is the qualified fixture behavior.
        sequences.push([zero.max(0), next, next + 1, next + 15 * 90_000]);
        projections.push(json!({"format_bound_us":format,"artifact_instep_us":instep,
            "next_delay_us":delay,"delay_residual_us":delay-exact_delay,"wrapper_zero_ticks90k":zero}));
    }
    sequences.sort_unstable();
    sequences.dedup();
    (sequences, json!(projections))
}

async fn actual_seek(evidence: &Path, source: &Path) -> (i64, String) {
    let source = source.to_str().unwrap();
    assert!(!source
        .chars()
        .any(|ch| ch.is_control() || matches!(ch, '"' | '\\')));
    let marker = format!(" -i \"{source}\" ");
    let mut entries = tokio::fs::read_dir(evidence.join("native-command-reports"))
        .await
        .unwrap();
    let mut commands = Vec::new();
    let mut count = 0;
    while let Some(entry) = entries.next_entry().await.unwrap() {
        assert!(entry.file_type().await.unwrap().is_file());
        count += 1;
        assert!(count <= 64);
        let report = String::from_utf8(bounded_read(&entry.path(), 8 * 1024 * 1024).await).unwrap();
        let Some(line) = report
            .split_once("Command line:\n")
            .and_then(|(_, rest)| rest.lines().next())
        else {
            continue;
        };
        let Some((before, after)) = line.split_once(&marker) else {
            continue;
        };
        if !after.starts_with("-f concat -safe 0 -protocol_whitelist file -i ")
            || !line.contains("-filter_complex \"[0:3]aresample=async=1,apad[a]\"")
            || !line.contains("-map 0:0 -map \"[a]\" -map 1:0")
            || !line.contains("-codec:v copy -codec:a aac -codec:s copy")
        {
            continue;
        }
        let tokens: Vec<_> = before.split_whitespace().collect();
        let seeks: Vec<_> = tokens.windows(2).filter(|pair| pair[0] == "-ss").collect();
        assert_eq!(seeks.len(), 1);
        commands.push((
            parse_seek(seeks[0][1]),
            entry.file_name().into_string().unwrap(),
        ));
    }
    let [(seek, name)] = commands.as_slice() else {
        panic!("one actual executed generated Copy-caption consumer command is required");
    };
    (*seek, name.clone())
}

fn parse_seek(value: &str) -> i64 {
    let parts: Vec<_> = value.split(':').collect();
    assert_eq!(parts.len(), 3);
    let hours: i64 = parts[0].parse().unwrap();
    let minutes: i64 = parts[1].parse().unwrap();
    let (seconds, fraction) = parts[2].split_once('.').unwrap();
    let seconds: i64 = seconds.parse().unwrap();
    assert!(hours >= 0 && (0..60).contains(&minutes) && (0..60).contains(&seconds));
    assert!(
        !fraction.is_empty()
            && fraction.len() <= 7
            && fraction.bytes().all(|byte| byte.is_ascii_digit())
    );
    let fraction = format!("{fraction:0<6}");
    hours
        .checked_mul(3600)
        .unwrap()
        .checked_add(minutes.checked_mul(60).unwrap())
        .unwrap()
        .checked_add(seconds)
        .unwrap()
        .checked_mul(1_000_000)
        .unwrap()
        .checked_add(fraction[..6].parse::<i64>().unwrap())
        .unwrap()
}

fn raw_cues(bytes: &[u8]) -> Vec<(u64, i64, u64)> {
    assert!(bytes.len().is_multiple_of(188));
    let mut observer = SourceTeletextClockObserver::new_source_epoch(256, 0).unwrap();
    let mut cues = Vec::new();
    for (index, chunk) in bytes.chunks_exact(188).enumerate() {
        let raw: &[u8; 188] = chunk.try_into().unwrap();
        if let Err(error) = observer.push(raw) {
            assert!(
                cues.is_empty() && error == SourceTeletextClockError::Discontinuity,
                "no new PCR epoch may begin after the selected DVB programme starts"
            );
            observer =
                SourceTeletextClockObserver::new_source_epoch(256, u64::try_from(index).unwrap())
                    .unwrap();
            observer.push(raw).unwrap();
        }
        if let Some(pts) = subtitle_pts(raw) {
            cues.push((
                u64::try_from(index).unwrap(),
                pts,
                observer.report().unwrap().last.unwrap().raw_ticks,
            ));
        }
    }
    cues
}

fn subtitle_pts(raw: &[u8; 188]) -> Option<i64> {
    let pid = (u16::from(raw[1] & 0x1f) << 8) | u16::from(raw[2]);
    if pid != SUBTITLE_PID || raw[1] & 0x40 == 0 {
        return None;
    }
    let control = (raw[3] >> 4) & 3;
    assert!(control == 1 || control == 3);
    let offset = if control == 3 {
        5 + usize::from(raw[4])
    } else {
        4
    };
    assert!(
        offset <= 174,
        "the actual generated private-PES header must fit its start packet"
    );
    let pes = &raw[offset..];
    assert_eq!(&pes[..4], &[0, 0, 1, 0xbd]);
    assert_eq!(pes[6] & 0xc0, 0x80);
    let flags = pes[7] >> 6;
    assert!(flags == 2 || flags == 3);
    assert!(pes[8] >= 5 && 9 + usize::from(pes[8]) < pes.len());
    assert_eq!(pes[9 + usize::from(pes[8])], 0x20);
    let p = &pes[9..14];
    assert_eq!(p[0] >> 4, flags);
    assert_eq!((p[0] & 1, p[2] & 1, p[4] & 1), (1, 1, 1));
    Some(
        (i64::from((p[0] >> 1) & 7) << 30)
            | (i64::from(p[1]) << 22)
            | (i64::from(p[2] >> 1) << 15)
            | (i64::from(p[3]) << 7)
            | i64::from(p[4] >> 1),
    )
}

fn strict_report(report: &Value, subtitle_leaps: usize) {
    for field in ["invalid-syncs", "transport-errors"] {
        assert_eq!(report["ts"]["packets"][field], 0);
    }
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
    assert!(
        report["ts"]["pcr-bitrate"]
            .as_u64()
            .unwrap()
            .abs_diff(u64::try_from(muxrate()).unwrap())
            < 2000
    );
    let pids = report["pids"].as_array().unwrap();
    for expected in [256, 257, 258, 512, 4096] {
        assert!(pids.iter().any(|pid| pid["id"] == expected));
    }
    assert_eq!(
        pids.iter().find(|pid| pid["id"] == 257).unwrap()["language"],
        "nld"
    );
    let subtitle = pids.iter().find(|pid| pid["id"] == 258).unwrap();
    assert!(subtitle["description"]
        .as_str()
        .unwrap()
        .contains("DVB subtitles"));
    assert_eq!(subtitle["language"], "eng");
    for pid in pids {
        for field in [
            "discontinuities",
            "dts-leap",
            "pcr-leap",
            "duplicated",
            "invalid-scrambling",
            "scrambled",
        ] {
            assert_eq!(pid["packets"][field], 0);
        }
        let expected = if pid["id"] == 258 {
            u64::try_from(subtitle_leaps).unwrap()
        } else {
            0
        };
        assert_eq!(pid["packets"]["pts-leap"], expected);
    }
}

#[test]
fn dvb_fixture_actual_seek_preserves_native_microsecond_truncation() {
    assert_eq!(parse_seek("00:00:17.3683564"), 17_368_356);
    assert_eq!(parse_seek("01:02:03.4"), 3_723_400_000);
}

#[test]
fn dvb_fixture_projection_uses_the_independently_measured_video_shift() {
    let (expected, projection) = expected_cues(17_368_356, -237_204);
    assert_eq!(expected, [[1_453_628, 2_153_288, 2_153_289, 3_503_288]]);
    assert_eq!(projection[0]["next_delay_us"], 7_774_000);
    assert_ne!(expected[0].as_slice(), [1_453_628, 3_647_204]);
}

#[test]
fn dvb_fixture_bootstrap_is_bounded_by_copied_av_not_a_time_tolerance() {
    let expected = [[1_443_913, 2_135_833, 2_135_834, 3_485_833]];
    let mut cues = [
        (52_848, 1_443_996, 433_223_873),
        (78_163, 2_135_833, 640_559_454),
        (78_164, 2_135_834, 640_559_454),
        (127_567, 3_485_833, 1_045_565_559),
    ];
    assert!(qualified_cues(&expected, &cues, 1_469_762));
    cues[0].2 = 433_000_000; // Delivery before presentation is valid.
    assert!(qualified_cues(&expected, &cues, 1_469_762));
    assert!(!qualified_cues(&expected, &cues, 1_443_912));
    assert!(!qualified_cues(&expected, &cues[..2], 1_469_762));
    cues[1].1 += 1;
    assert!(!qualified_cues(&expected, &cues, 1_469_762));
    cues[1].1 -= 1;
    cues[0].1 = 1_469_763;
    assert!(!qualified_cues(&expected, &cues, 1_469_762));
    cues[0].1 = 1_443_996;
    cues[0].2 = 1_469_762 * 300 + 1;
    assert!(!qualified_cues(&expected, &cues, 1_469_762));
}

#[test]
fn dvb_fixture_unique_pre_seek_keyframe_is_not_hidden_by_waiting_payload_collision() {
    let source = [
        json!({"data_hash":"same-skip-frame","pts":1000,"dts":900}),
        json!({"data_hash":"same-skip-frame","pts":2000,"dts":1900}),
        json!({"data_hash":"unique-keyframe","pts":1400,"dts":1300}),
    ];
    let capture = [
        json!({"data_hash":"same-skip-frame","pts":800,"dts":700,"pos":"188"}),
        json!({"data_hash":"unique-keyframe","pts":1200,"dts":1100,"pos":"376"}),
        json!({"data_hash":"same-skip-frame","pts":1800,"dts":1700,"pos":"564"}),
    ];
    let first = first_copied_video(&source, &capture, -200);
    assert_eq!(first["capture_position"], "376");
    assert_eq!(first["capture_pts"], 1200);
}

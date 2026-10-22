//! Exact clock join for the qualified retained-source software H.264 Burn fixture.
//!
//! Logging derivatives are untrusted until their entire TS equals public HTTP.

use super::*;

const FRAME_TICKS: i64 = 3_600;
const MAX_FRAMES: usize = 256;
const CROP_BYTES: usize = 320 * 48 * 3;

fn expected_args(
    source: &Path,
    sidecar: &Path,
    fonts: &Path,
    output: &Path,
    interval: (u64, u64),
) -> Vec<String> {
    let (seek, end) = interval;
    assert!(end > seek);
    let seconds = format!("{}.{:03}", seek / 1000, seek % 1000);
    // ADR 0100: this is a fixed oracle for the qualified Next software fixture,
    // not another production command generator or a fallback playback policy.
    let graph = format!(
        "[0:0]setpts=PTS+round({seconds}/TB),subtitles={}:fontsdir={},setpts=PTS-round({seconds}/TB)[v]",
        sidecar.display(), fonts.display()
    );
    let mut args = [
        "-threads",
        "0",
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-fflags",
        "+genpts+discardcorrupt+igndts",
        "-ss",
        &format!("{seek}ms"),
        "-noautorotate",
        "-i",
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(source.to_str().unwrap().to_owned());
    args.extend(encoding_args(&graph, end - seek));
    args.push(output.join("live%06d.ts").to_str().unwrap().to_owned());
    args.extend(
        [
            "-hls_segment_type",
            "mpegts",
            "-hls_flags",
            "program_date_time+omit_endlist+append_list+independent_segments",
            "-hls_segment_options",
            "mpegts_flags=+initial_discontinuity",
        ]
        .map(str::to_owned),
    );
    args.push(output.join("ffmpeg.m3u8").to_str().unwrap().to_owned());
    args
}

fn encoding_args(graph: &str, duration_ms: u64) -> Vec<String> {
    [
        "-filter_complex",
        "[0:3]aresample=async=1,apad[a]",
        "-filter_complex",
        graph,
        "-map",
        "[v]",
        "-map",
        "[a]",
        "-muxdelay",
        "0",
        "-muxpreload",
        "0",
        "-movflags",
        "+faststart",
        "-noautoscale",
        "-acodec",
        "aac",
        "-b:a",
        "128k",
        "-maxrate:a",
        "128k",
        "-bufsize:a",
        "256k",
        "-ar",
        "48000",
        "-vcodec",
        "libx264",
        "-b:v",
        "4000k",
        "-maxrate:v",
        "4000k",
        "-bufsize:v",
        "2000k",
        "-map_metadata",
        "-1",
        "-t",
        &format!("{duration_ms}ms"),
        "-shortest",
        "-video_track_timescale",
        "90000",
        "-map_metadata:s:a:0",
        "0:s:3",
        "-g",
        "50",
        "-keyint_min",
        "50",
        "-force_key_frames",
        "expr:gte(t,n_forced*2)",
        "-f",
        "hls",
        "-hls_time",
        "4",
        "-hls_list_size",
        "0",
        "-segment_list_flags",
        "+live",
        "-hls_segment_filename",
    ]
    .map(str::to_owned)
    .to_vec()
}

fn logging_derivative(base: &[String], taps: bool, output: &Path) -> Vec<String> {
    let mut args = base.to_vec();
    let level = args.iter().position(|arg| arg == "-loglevel").unwrap();
    if taps { "info" } else { "verbose" }.clone_into(&mut args[level + 1]);
    if taps {
        let graph = args
            .iter_mut()
            .find(|arg| arg.starts_with("[0:0]"))
            .unwrap();
        *graph = graph
            .replace("[0:0]", "[0:0]showinfo@video,")
            .replace(",subtitles=", ",showinfo@caption,subtitles=")
            .replace("[v]", ",showinfo@output[v]");
    } else {
        args.insert(0, "-debug_ts".to_owned());
    }
    let segments = args
        .iter()
        .position(|arg| arg == "-hls_segment_filename")
        .unwrap();
    output
        .join("live%06d.ts")
        .to_str()
        .unwrap()
        .clone_into(&mut args[segments + 1]);
    output
        .join("ffmpeg.m3u8")
        .to_str()
        .unwrap()
        .clone_into(args.last_mut().unwrap());
    assert_ne!(args, base);
    args
}

async fn reconstruct(ffmpeg: &str, args: &[String], output: &Path, actual: &[u8]) -> String {
    tokio::fs::create_dir(output).await.unwrap();
    tokio::fs::write(
        output.join("command.json"),
        serde_json::to_vec_pretty(args).unwrap(),
    )
    .await
    .unwrap();
    let mut command = Command::new(ffmpeg);
    command.args(args);
    let limit = if args.iter().any(|arg| arg == "-debug_ts") {
        1024 * 1024
    } else {
        512 * 1024
    };
    let (_, stderr) = tool_with_stderr(command, limit).await;
    tokio::fs::write(output.join("stderr.log"), &stderr)
        .await
        .unwrap();
    let playlist = tokio::fs::read_to_string(output.join("ffmpeg.m3u8"))
        .await
        .unwrap();
    let names = uris(&playlist).collect::<Vec<_>>();
    assert!(!names.is_empty() && names.len() <= 4);
    let mut bytes = Vec::new();
    for name in names {
        assert!(!name.contains('/') && !name.contains(".."));
        assert_eq!(
            Path::new(name).extension(),
            Some(std::ffi::OsStr::new("ts"))
        );
        let path = output.join(name);
        let length = usize::try_from(tokio::fs::metadata(&path).await.unwrap().len()).unwrap();
        assert!(bytes.len().checked_add(length).unwrap() <= MAX_CAPTURE);
        bytes.extend(tokio::fs::read(path).await.unwrap());
    }
    tokio::fs::write(output.join("captured.ts"), &bytes)
        .await
        .unwrap();
    tokio::fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&json!({
            "public_bytes":actual.len(),"reconstructed_bytes":bytes.len(),
            "public_sha256":sha256(actual),
            "reconstructed_sha256":sha256(&bytes),
            "whole_ts_byte_equal":bytes == actual
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    assert!(
        bytes == actual,
        "entire reconstructed TS must equal public HTTP before trusting clocks; artifacts retained"
    );
    stderr
}

fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(digest, "{byte:02x}").unwrap();
    }
    digest
}

fn field<'a>(line: &'a str, name: &str) -> &'a str {
    line.split_once(name)
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
}

fn integer(line: &str, name: &str) -> i64 {
    field(line, name).parse().unwrap()
}

fn filter_frames(trace: &str, name: &str) -> Vec<i64> {
    let prefix = format!("[showinfo@{name} @");
    let mut clocks = BTreeSet::new();
    let mut frames = Vec::new();
    for line in trace.lines().filter(|line| line.contains(&prefix)) {
        if line.contains("config in time_base:") {
            clocks.insert(field(line, "config in time_base:").trim_end_matches(','));
            assert_eq!(field(line, "frame_rate:"), "25/1");
        }
        if line.contains(" n:") && line.contains(" pts:") {
            assert_eq!(usize::try_from(integer(line, " n:")).unwrap(), frames.len());
            assert_eq!(integer(line, " duration:"), FRAME_TICKS);
            frames.push(integer(line, " pts:"));
            assert!(frames.len() <= MAX_FRAMES);
        }
    }
    assert_eq!(clocks, BTreeSet::from(["1/90000"]));
    assert!(!frames.is_empty());
    assert!(frames
        .windows(2)
        .all(|pair| pair[1] - pair[0] == FRAME_TICKS));
    frames
}

fn debug_frames(trace: &str, stream: &str, marker: &str, value: &str, time_base: &str) -> Vec<i64> {
    let frames = trace
        .lines()
        .filter(|line| line.contains(stream) && line.contains(marker))
        .map(|line| {
            assert_eq!(field(line, " time_base:"), time_base);
            integer(line, value)
        })
        .collect::<Vec<_>>();
    assert!(!frames.is_empty() && frames.len() <= MAX_FRAMES);
    frames
}

fn duplicates(trace: &str) -> usize {
    let summary = trace
        .split(['\r', '\n'])
        .rev()
        .find(|line| line.contains("dup=") && line.contains("drop="))
        .unwrap();
    assert_eq!(integer(summary, "drop="), 0, "no unaccounted frame drops");
    usize::try_from(integer(summary, "dup=")).unwrap()
}

async fn crop_counts(ffmpeg: &str, captured: &Path, output: &Path) -> Vec<usize> {
    let raw = output.join("crop.rgb");
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(captured)
        .args([
            "-map",
            "0:v:0",
            "-vf",
            "crop=320:48:0:132",
            "-fps_mode",
            "passthrough",
            "-frames:v",
            &(MAX_FRAMES + 1).to_string(),
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "-fs",
            &(MAX_FRAMES * CROP_BYTES + 1).to_string(),
        ])
        .arg(&raw);
    tool(command).await;
    let length = usize::try_from(tokio::fs::metadata(&raw).await.unwrap().len()).unwrap();
    assert!(length <= MAX_FRAMES * CROP_BYTES && length.is_multiple_of(CROP_BYTES));
    tokio::fs::read(raw)
        .await
        .unwrap()
        .chunks_exact(CROP_BYTES)
        .map(|frame| {
            frame
                .chunks_exact(3)
                .filter(|pixel| pixel.iter().all(|value| *value > 180))
                .count()
        })
        .collect()
}

struct FilterJoin {
    incoming: Vec<i64>,
    caption: Vec<i64>,
    output: Vec<i64>,
    origin: i64,
    dup: usize,
    represented: usize,
    encoded: Vec<i64>,
}

fn filter_prefixes_match(
    raw: &[i64],
    output: &[i64],
    filtered: &[i64],
    represented: usize,
) -> bool {
    // FFmpeg9 logs filter_raw after pulling from the buffersink; showinfo can
    // process additional lookahead before the recording limit stops that pull.
    // Join every common timestamp, every delivered frame and the next witness.
    represented > 1
        && raw.len() > represented
        && output.len() > represented
        && raw.len() == filtered.len()
        && raw.len() <= MAX_FRAMES
        && output.len() <= MAX_FRAMES
        && (raw.starts_with(output) || output.starts_with(raw))
        && raw.windows(2).all(|pair| pair[1] - pair[0] == FRAME_TICKS)
        && output
            .windows(2)
            .all(|pair| pair[1] - pair[0] == FRAME_TICKS)
}

fn filter_join(taps: &str, debug: &str, original: &Value, seek: u64) -> FilterJoin {
    let incoming = filter_frames(taps, "video");
    let caption = filter_frames(taps, "caption");
    let output = filter_frames(taps, "output");
    assert_eq!(
        incoming, output,
        "Burn must retain every incoming timestamp residual"
    );
    assert_eq!(caption.len(), incoming.len());
    let seek_ticks = i64::try_from(seek).unwrap() * 90;
    for (before, caption) in incoming.iter().zip(&caption) {
        assert_eq!(*caption, *before + seek_ticks);
    }
    assert_eq!(original["streams"][0]["time_base"], "1/90000");
    let origin = format_ticks(original["format"]["start_time"].as_str().unwrap());
    let source_pts = original["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|packet| {
            assert_eq!(packet["duration"], FRAME_TICKS);
            packet["pts"].as_i64().unwrap()
        })
        .collect::<BTreeSet<_>>();
    let absolute = incoming
        .iter()
        .map(|pts| pts + seek_ticks + origin)
        .collect::<Vec<_>>();
    assert!(absolute.iter().all(|pts| source_pts.contains(pts)));
    let decoded = debug_frames(debug, "[vist#0:0/", "decoder -> pts:", " pts:", "1/90000");
    assert!(decoded.starts_with(&incoming));
    let raw = debug_frames(debug, "[fc#1 ", "filter_raw -> pts:", " pts:", "1/90000");
    assert!(raw
        .iter()
        .all(|pts| source_pts.contains(&(pts + seek_ticks + origin))));
    let filtered = debug_frames(debug, "[fc#1 ", "filter -> pts:", " pts:", "1/25");
    let encoded = debug_frames(
        debug,
        "[vost#0:0/",
        "encoder <- type:video",
        " frame_pts:",
        "1/25",
    );
    assert!(encoded
        .iter()
        .enumerate()
        .all(|(n, pts)| *pts == i64::try_from(n).unwrap()));
    let dup = duplicates(debug);
    assert_eq!(dup, duplicates(taps));
    assert_eq!(i64::try_from(dup).unwrap(), filtered[0]);
    let represented = encoded.len().checked_sub(dup).unwrap();
    assert!(represented > 1 && incoming.len() > represented);
    assert!(
        filter_prefixes_match(&raw, &output, &filtered, represented),
        "sink/tap clocks must join every delivered frame and the next outpoint witness"
    );
    assert!(filtered
        .iter()
        .take(represented + 1)
        .enumerate()
        .all(|(n, pts)| *pts == i64::try_from(n + dup).unwrap()));
    FilterJoin {
        incoming,
        caption,
        output,
        origin,
        dup,
        represented,
        encoded,
    }
}

fn packet_join(actual: &Value, debug: &str, encoded: &[i64]) -> i64 {
    assert_eq!(actual["streams"][0]["time_base"], "1/90000");
    let mut actual_pts = actual["packets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|packet| {
            assert_eq!(packet["duration"], FRAME_TICKS);
            packet["pts"].as_i64().unwrap()
        })
        .collect::<Vec<_>>();
    actual_pts.sort_unstable();
    assert_eq!(actual_pts.len(), encoded.len());
    let mut muxed = debug
        .lines()
        .filter(|line| line.contains("[vost#") && line.contains(" muxer <- pts:"))
        .map(|line| {
            assert_eq!(integer(line, " duration:"), FRAME_TICKS);
            integer(line, " pts:")
        })
        .collect::<Vec<_>>();
    muxed.sort_unstable();
    assert_eq!(
        muxed,
        encoded
            .iter()
            .map(|pts| pts * FRAME_TICKS)
            .collect::<Vec<_>>()
    );
    let mux_shift = actual_pts[0] - muxed[0];
    assert!(actual_pts
        .iter()
        .zip(&muxed)
        .all(|(actual, before)| *actual == before + mux_shift));
    mux_shift
}

fn caption_frames(
    caption: &[i64],
    counts: &[usize],
    cues: &[(i64, i64, String)],
    dup: usize,
) -> (usize, usize) {
    let final_cue = cues.last().unwrap();
    assert!(final_cue.2.contains("889 FINAL"));
    assert_eq!(final_cue.1, 8_800);
    let clear_source = caption
        .iter()
        .position(|pts| *pts >= final_cue.1 * 90)
        .unwrap();
    let clear_output = dup + clear_source;
    assert!(clear_output > dup && clear_output < counts.len());
    for (n, white) in counts.iter().enumerate() {
        let source_frame = n.saturating_sub(dup);
        let cue_time = caption[source_frame];
        let ink = cues
            .iter()
            .any(|(start, end, _)| *start * 90 <= cue_time && cue_time < *end * 90);
        if ink {
            assert!(*white > 20, "missing Burn cue at output frame {n}");
        } else {
            assert_eq!(*white, 0, "late Burn cue at output frame {n}");
        }
    }
    assert!(counts[clear_output - 1] > 20);
    assert_eq!(counts[clear_output], 0);
    (clear_source, clear_output)
}

pub(super) async fn check(
    tools: (&str, &str),
    source: &Path,
    sidecar: &Path,
    root: &Path,
    interval: (u64, u64),
) {
    let evidence = root.join("burn-clock");
    tokio::fs::create_dir(&evidence).await.unwrap();
    let captured = root.join("captured.ts");
    assert!(
        tokio::fs::metadata(&captured).await.unwrap().len() <= u64::try_from(MAX_CAPTURE).unwrap()
    );
    let bytes = tokio::fs::read(&captured).await.unwrap();
    let base = expected_args(
        source,
        sidecar,
        &ersatzrs_infra::file_system_layout::fonts_cache_folder(),
        &evidence.join("baseline"),
        interval,
    );
    reconstruct(tools.0, &base, &evidence.join("baseline"), &bytes).await;
    let taps = reconstruct(
        tools.0,
        &logging_derivative(&base, true, &evidence.join("taps")),
        &evidence.join("taps"),
        &bytes,
    )
    .await;
    let debug = reconstruct(
        tools.0,
        &logging_derivative(&base, false, &evidence.join("debug")),
        &evidence.join("debug"),
        &bytes,
    )
    .await;
    let original = packets(Path::new(tools.1), source, "v:0", true).await;
    let FilterJoin {
        incoming,
        caption,
        output,
        origin,
        dup,
        represented,
        encoded,
    } = filter_join(&taps, &debug, &original, interval.0);
    let (seek, end) = interval;
    let actual = packets(Path::new(tools.1), &captured, "v:0", true).await;
    let mux_shift = packet_join(&actual, &debug, &encoded);
    let counts = crop_counts(tools.0, &captured, &evidence).await;
    assert_eq!(counts.len(), encoded.len());
    let independent =
        tokio::fs::read_to_string(source.parent().unwrap().join("independent-889.srt"))
            .await
            .unwrap();
    let cues = cue_blocks(&independent);
    assert_eq!(
        cue_blocks(&tokio::fs::read_to_string(sidecar).await.unwrap()),
        cues
    );
    let (clear_source, clear_output) = caption_frames(&caption, &counts, &cues, dup);
    let last_source_end = caption[represented - 1] + FRAME_TICKS;
    let next_source_end = caption[represented] + FRAME_TICKS;
    let out_ticks = i64::try_from(end).unwrap() * 90;
    tokio::fs::write(evidence.join("clock-qualification.json"), serde_json::to_vec_pretty(&json!({
        "whole_public_baseline_taps_debug_byte_equal":true,
        "source_seek_ms":seek,"source_out_ms":end,"format_origin_ticks":origin,
        "incoming_ticks":incoming,"caption_ticks":caption,"output_ticks":output,
        "output_frames":encoded.len(),"initial_cfr_duplicates":dup,"dropped_frames":0,
        "represented_source_frames":represented,"mux_shift_ticks":mux_shift,
        "white_pixels":counts,"caption_clear_source_ordinal":clear_source,
        "caption_clear_output_ordinal":clear_output,"last_source_frame_end_ticks":last_source_end,
        "next_source_frame_end_ticks":next_source_end,"selected_out_ticks":out_ticks,
        "source_to_output_burn_clock_proven":last_source_end <= out_ticks && next_source_end > out_ticks
    })).unwrap()).await.unwrap();
    assert!(
        last_source_end <= out_ticks,
        "delivered represented source frames must not exceed the selected outpoint"
    );
    assert!(
        next_source_end > out_ticks,
        "capture must cover every complete source frame up to the outpoint"
    );
}

#[test]
fn retained_burn_expected_argv_preserves_the_rounded_source_clock() {
    let args = expected_args(
        Path::new("SOURCE"),
        Path::new("SIDECAR"),
        Path::new("FONTS"),
        Path::new("OUTPUT"),
        (2_143, 9_200),
    );
    insta::assert_snapshot!("retained_burn_restored_clock_argv", args.join("\n"));
}

#[cfg(test)]
#[path = "retained_clock_tests.rs"]
mod tests;

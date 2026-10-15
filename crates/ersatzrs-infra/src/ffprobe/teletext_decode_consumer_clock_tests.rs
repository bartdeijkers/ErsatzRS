//! Join actual native filter clocks without treating logging as a sealed command.

use super::*;

fn logging_derivative(base: &FfmpegCommand, filter_taps: bool) -> FfprobeCommand {
    assert!(base.environment.is_empty());
    let mut args = base.arguments.clone();
    let level = args.iter().position(|arg| arg == "-loglevel").unwrap();
    if filter_taps {
        // showinfo emits checksum fragments separately. Keep independently
        // threaded packet logging out instead of pairing interleaved fragments.
        args[level + 1] = "info".into();
        let graph = args
            .iter()
            .position(|arg| arg == "-filter_complex")
            .unwrap()
            + 1;
        assert_eq!(args[graph].matches("[0:0]").count(), 1);
        assert_eq!(args[graph].matches("[0:2]").count(), 1);
        args[graph] = format!(
            "[0:0]showinfo@video[observed_video];{}",
            args[graph]
                .replace("[0:0]", "[observed_video]")
                .replace("[0:2]", "[0:2]showinfo@caption,")
        );
    } else {
        args[level + 1] = "verbose".into();
        args.insert(0, "-debug_ts".into());
    }
    // Neither derivative retains the baseline command seal. Require complete
    // output equality before using their independently recorded clocks.
    assert_ne!(args, base.arguments);
    FfprobeCommand {
        program: base.executable.clone(),
        args,
    }
}

fn field<'a>(line: &'a str, name: &str) -> &'a str {
    line.split_once(name).unwrap().1.trim_start()
}

fn filter_frames(trace: &str, name: &str) -> serde_json::Value {
    let prefix = format!("[showinfo@{name} @");
    let lines: Vec<_> = trace
        .lines()
        .filter(|line| line.contains(&prefix))
        .collect();
    let mut time_bases = Vec::new();
    let mut frames = Vec::new();
    for line in lines {
        if line.contains("config in time_base:") {
            let ratio = field(line, "config in time_base:")
                .split(',')
                .next()
                .unwrap();
            let (num, den) = ratio.split_once('/').unwrap();
            let ratio = [num.parse::<i64>().unwrap(), den.parse::<i64>().unwrap()];
            assert!(ratio.iter().all(|value| *value > 0));
            if !time_bases.contains(&ratio) {
                time_bases.push(ratio);
            }
        }
        if line.contains(" n:") && line.contains(" pts:") {
            let pts = field(line, " pts:").split_whitespace().next().unwrap();
            let pts = if pts == "NOPTS" {
                None
            } else {
                Some(pts.parse::<i64>().unwrap())
            };
            let n = field(line, " n:")
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<usize>()
                .unwrap();
            assert_eq!(n, frames.len(), "no missing or cross-filter frame ordinals");
            let checksum = field(line, " checksum:").split_whitespace().next().unwrap();
            frames.push(serde_json::json!({"n": n,"pts": pts,
                "checksum":checksum,"native_line":line}));
        }
    }
    assert_eq!(time_bases.len(), 1, "one actual configured filter clock");
    assert!(
        !frames.is_empty(),
        "actual {name} frames, never a fabricated zero"
    );
    serde_json::json!({"time_base":time_bases[0],"frames":frames})
}

async fn original_page_events(
    scope: &LocalProbeCleanupScope,
    extractor: &FfprobeExtractor,
    source: &PreparedLocalTeletextSource,
    work: &TeletextWorkGuard,
) -> serde_json::Value {
    let mut command = extractor
        .teletext_page_decode_command(source.request(), &source.resolved)
        .unwrap();
    command.args.splice(
        0..0,
        ersatzrs_ffmpeg::standard_format_flags()
            .global_options()
            .iter()
            .cloned(),
    );
    command.args.insert(0, "-show_packets".into());
    let entries = command
        .args
        .iter()
        .position(|arg| arg == "-show_entries")
        .unwrap()
        + 1;
    command.args[entries].push_str(":packet=stream_index,pts,dts,pos");
    let output = capture_owned(scope, command, source, work).await;
    serde_json::from_slice(&output).unwrap()
}

async fn capture_derivative(
    scope: &LocalProbeCleanupScope,
    command: FfprobeCommand,
    source: &PreparedLocalTeletextSource,
    work: &TeletextWorkGuard,
    baseline: &[u8],
    evidence: &std::path::Path,
) -> String {
    let diagnostic = scope
        .scope(local::capture_retained_diagnostics(
            command,
            "synthetic generated clock join".into(),
            Some(Instant::now() + Duration::from_secs(30)),
            Some(local::ProbeRetention::new(
                work.hold_work(),
                source.guard.clone(),
            )),
        ))
        .await
        .unwrap();
    std::fs::write(evidence.with_extension("stderr.log"), &diagnostic.stderr).unwrap();
    std::fs::write(evidence.with_extension("ts"), &diagnostic.stdout).unwrap();
    std::fs::write(
        evidence.with_extension("comparison.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "baseline_sha256":hex_text(&Sha256::digest(baseline)),
            "diagnostic_sha256":hex_text(&Sha256::digest(&diagnostic.stdout)),
            "byte_equal":baseline == diagnostic.stdout,
        }))
        .unwrap(),
    )
    .unwrap();
    diagnostic.stderr
}

async fn observe_clock(
    scope: &LocalProbeCleanupScope,
    tools: (&str, &str),
    source: &PreparedLocalTeletextSource,
    caption: &DecodedLocalTeletextPage,
    work: &TeletextWorkGuard,
    seek: Option<Duration>,
    evidence: &std::path::Path,
) -> serde_json::Value {
    let base = burn_command_with_video_buffer(tools.0, source, Some(caption), seek, None);
    let baseline = capture_owned(
        scope,
        FfprobeCommand {
            program: base.executable.clone(),
            args: base.arguments.clone(),
        },
        source,
        work,
    )
    .await;
    // Persist the baseline before diagnostic comparison, including red runs.
    std::fs::write(evidence.with_extension("baseline.ts"), &baseline).unwrap();
    let taps_command = logging_derivative(&base, true);
    let debug_command = logging_derivative(&base, false);
    std::fs::write(
        evidence.with_extension("commands.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"sealed_baseline":base.arguments,
            "unsealed_filter_taps":taps_command.args,
            "unsealed_packet_clocks":debug_command.args}))
        .unwrap(),
    )
    .unwrap();
    let name = evidence.file_name().unwrap().to_str().unwrap();
    let taps = capture_derivative(
        scope,
        taps_command,
        source,
        work,
        &baseline,
        &evidence.with_file_name(format!("{name}-taps")),
    )
    .await;
    let debug = capture_derivative(
        scope,
        debug_command,
        source,
        work,
        &baseline,
        &evidence.with_file_name(format!("{name}-debug")),
    )
    .await;
    for suffix in ["taps", "debug"] {
        let derivative =
            std::fs::read(evidence.with_file_name(format!("{name}-{suffix}.ts"))).unwrap();
        assert_eq!(
            Sha256::digest(&baseline),
            Sha256::digest(&derivative),
            "logging derivative must reproduce the complete sealed output; all artifacts retained"
        );
    }
    let (output, _) = frame_evidence(
        scope,
        tools,
        source,
        &evidence.with_extension("baseline.ts"),
        work,
    )
    .await;
    let packets: Vec<_> = debug
        .lines()
        .filter(|line| line.contains("ist_index:0:2 type:subtitle"))
        .collect();
    let before_cfr: Vec<_> = debug
        .lines()
        .filter(|line| line.contains("filter -> pts:"))
        .collect();
    assert!(!packets.is_empty());
    assert!(!before_cfr.is_empty());
    let report = serde_json::json!({
        "note":"Two complete traces avoid interleaved showinfo checksum/packet fragments. Taps follow automatic trim and include sub2video heartbeats. Neither derivative is sealed; both complete transports equal this no-buffer sealed baseline. This distinct generated encoder policy does not qualify bounded-VBV production output.",
        "transport_sha256":hex_text(&Sha256::digest(&baseline)),
        "video":filter_frames(&taps,"video"),
        "caption":filter_frames(&taps,"caption"),
        "selected_subtitle_packets":packets,"before_cfr":before_cfr,"output":output,
    });
    std::fs::write(
        evidence.with_extension("clock.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    eprintln!(
        "actual generated clock seek={seek:?}: video first={}, caption first={}, output first={}",
        report["video"]["frames"][0], report["caption"]["frames"][0], report["output"]["frames"][0]
    );
    report
}

fn assert_fixture_clock_join(
    report: &serde_json::Value,
    original: &serde_json::Value,
    sought: bool,
) -> serde_json::Value {
    let packets = report["selected_subtitle_packets"].as_array().unwrap();
    let offsets: Vec<_> = packets
        .iter()
        .filter_map(|line| line.as_str().unwrap().split_once(" off:"))
        .map(|(_, value)| {
            value
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<i64>()
                .unwrap()
        })
        .collect();
    let offset = *offsets.first().unwrap();
    assert!(offsets.iter().all(|value| *value == offset));
    assert_eq!(offset, if sought { -1_360_944 } else { -860_944 });
    assert_eq!(report["video"]["time_base"], serde_json::json!([1, 90_000]));
    assert_eq!(
        report["caption"]["time_base"],
        serde_json::json!([1, 1_000_000])
    );
    let video = report["video"]["frames"][0]["pts"].as_i64().unwrap();
    // Match native nearest-away rescaling of this observed microsecond offset.
    let offset_ticks = (offset * 90_000 - 500_000) / 1_000_000;
    let source_video = video - offset_ticks;
    assert_eq!(source_video, if sought { 441_120 } else { 397_920 });

    let frames = report["caption"]["frames"].as_array().unwrap();
    let ink = frames
        .iter()
        .position(|frame| frame["checksum"] != "00000000")
        .unwrap();
    assert!(
        ink > 0,
        "actual transparent decoder initialization precedes ink"
    );
    let bitmap = frames[ink]["pts"].as_i64().unwrap();
    let heartbeat = frames[ink - 1]["pts"].as_i64().unwrap();
    assert!(
        heartbeat > bitmap,
        "delivery order must not be sorted into PTS order"
    );
    let original_events: Vec<_> = original["packets_and_frames"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["type"] == "subtitle")
        .map(|entry| entry["pts"].as_i64().unwrap())
        .collect();
    assert_eq!(original_events.len(), 8);
    let expected_ordinal = if sought { 3 } else { 0 };
    let residual = bitmap - offset - original_events[expected_ordinal];
    assert!(
        residual.abs() <= 1,
        "separate native integer rescalings differ by at most one microsecond"
    );
    let raw_first = packets
        .iter()
        .map(|line| line.as_str().unwrap())
        .find(|line| line.contains("demuxer ->"))
        .unwrap();
    assert_eq!(raw_first.contains("pkt_pts:NOPTS"), sought);
    assert_eq!(report["output"]["frames"][0]["pts"], 0);
    let before_cfr = report["before_cfr"][0].as_str().unwrap();
    assert!(
        field(before_cfr, " pts:")
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<i64>()
            .unwrap()
            > 0,
        "CFR output zero is not the first kept input PTS"
    );
    serde_json::json!({"observed_offset_microseconds":offset,
        "first_kept_source_video_pts_90000":source_video,
        "first_nonblank_caption_pts_microseconds":bitmap,
        "preceding_heartbeat_pts_microseconds":heartbeat,
        "original_page_event_ordinal":expected_ordinal,
        "original_page_event_pts_microseconds":original_events[expected_ordinal],
        "join_residual_microseconds":residual,
        "note":"Pinned synthetic fixture only; no generic seek-landing, cue-coverage or native initialization admission."})
}

#[tokio::test]
#[ignore = "requires explicit native FFmpeg/FFprobe for actual bitmap/video clock join"]
#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in synthetic diagnostic destination"
)]
async fn native_no_buffer_burn_joins_bitmap_video_and_cfr_clocks() {
    let scope = LocalProbeCleanupScope::new();
    let ffmpeg = tool(&scope, "ERSATZRS_TELETEXT_TEST_FFMPEG", "ffmpeg").await;
    let ffprobe = tool(&scope, "FFPROBE_PATH", "ffprobe").await;
    let extractor = FfprobeExtractor::new(ffprobe.clone());
    let directory = tempfile::tempdir().unwrap();
    let evidence = std::env::var_os("ERSATZRS_TELETEXT_BURN_EVIDENCE")
        .map_or_else(|| directory.path().join("evidence"), PathBuf::from);
    std::fs::create_dir_all(&evidence).unwrap();
    let path = staggered_source(&scope, &ffmpeg, directory.path()).await;
    let original = std::fs::read(&path).unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let source = prepare(&scope, &extractor, &path, &work).await;
    let caption = decode(&scope, &extractor, &source, &original, &work).await;
    let events = original_page_events(&scope, &extractor, &source, &work).await;
    std::fs::write(
        evidence.join("original-page888.json"),
        serde_json::to_vec_pretty(&events).unwrap(),
    )
    .unwrap();
    for (name, seek) in [
        ("no-seek", None),
        ("seek-half", Some(Duration::from_millis(500))),
    ] {
        scope
            .scope(source.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()))
            .await
            .unwrap();
        let report = observe_clock(
            &scope,
            (&ffmpeg, &ffprobe),
            &source,
            &caption,
            &work,
            seek,
            &evidence.join(name),
        )
        .await;
        let join = assert_fixture_clock_join(&report, &events, seek.is_some());
        std::fs::write(
            evidence.join(format!("{name}.join.json")),
            serde_json::to_vec_pretty(&join).unwrap(),
        )
        .unwrap();
    }
    assert_eq!(std::fs::read(path).unwrap(), original);
    scope
        .scope(source.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()))
        .await
        .unwrap();
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}

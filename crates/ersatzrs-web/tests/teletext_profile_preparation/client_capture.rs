//! Cold pending and accepted-generation captures for headed client trials.
//! Uses the existing final direct MPEG-TS route, not the private concat wrapper.
//! A capture is not automatic publication or complete engine/client acceptance.

use super::channel_documents::{
    self, address, candidate, install_contract, response, write_response,
};
use super::designs::{budget as document_budget, config, deadline, save, units};
use super::documents::DocumentFixture;
use super::*;
use axum::http::StatusCode;
use ersatzrs_core::repositories::{TeletextGenerationMutation, TeletextPublicationMutation};
use ersatzrs_domain::teletext::{TeletextColourPixelOptions, TeletextFlashPhase};
use ersatzrs_domain::{
    ChannelSubtitleMode, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, StreamingMode,
};
use ersatzrs_ffmpeg::teletext::{TeletextClockConfig, TeletextOutputConfig};
use ersatzrs_ffmpeg::{pipeline::OutputFormat, MpegTsMuxingOptions};
use ersatzrs_web::host::{
    teletext_designs::{
        request_original_teletext_design_publication, RequestedOriginalTeletextPublication,
        SavedOriginalTeletextDesign,
    },
    teletext_generations::{accept_original_teletext_generation, TeletextGenerationTarget},
    teletext_preview::{project_teletext_preview, TeletextPreviewBudget},
};
use ersatzrs_web::transport::iptv::{
    stream::{serve_channel_stream_with_accepted_teletext, serve_channel_stream_with_teletext},
    teletext_accepted_output::{
        load_accepted_teletext_output_intent, TeletextAcceptedOutputIntent,
    },
    teletext_binding::TeletextOutputBinding,
    teletext_source_policy::{TeletextSourcePolicy, TeletextSourcePolicyLimits},
};
use futures_util::StreamExt as _;
use serde_json::{json, Value};
use sqlx::Executor as _;
use std::{collections::BTreeSet, path::PathBuf, process::Stdio, sync::Arc};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    process::Command,
};

const MAX_BYTES: usize = 8 * 1024 * 1024;
const VIDEO_KBPS: u32 = 4000;
const AUDIO_KBPS: u32 = 128;

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit opt-in exact-runtime and create-new synthetic capture selection"
)]
pub(super) fn selected(name: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .expect("set the explicit native teletext capture environment")
}

pub(super) fn muxrate() -> i64 {
    MpegTsMuxingOptions::from_bitrates(Some(VIDEO_KBPS), Some(AUDIO_KBPS)).muxrate_bps
}

fn output_policy() -> TeletextOutputConfig {
    TeletextOutputConfig {
        program: 1,
        elementary_pid: 512,
        clock: TeletextClockConfig {
            pcr_pid: 256,
            muxrate_bps: u64::try_from(muxrate()).unwrap(),
            rate_tolerance_ppm: 1000,
            pcr_jitter_ticks: 14,
            max_pcr_gap_packets: 300,
            max_pcr_gap_ticks: 1_080_000,
            min_pcr_intervals: 2,
            max_window_packets: 1024,
        },
        max_buffer_packets: 8192,
        max_hold: Duration::from_secs(2),
        playout_delay: Duration::from_millis(100),
        max_lateness: Duration::from_millis(100),
    }
}

fn source_policy() -> TeletextSourcePolicy {
    // Explicit bounds for this pinned, synthetic 4.9536-Mbps source only.
    // Program/PCR identities must come from the actual route's source PSI.
    TeletextSourcePolicy::new(TeletextSourcePolicyLimits {
        elementary_pid: 512,
        rate_tolerance_ppm: 1000,
        pcr_jitter_ticks: 14,
        pcr_gap_slack_ticks: 540_000,
        pcr_gap_slack_packets: 36,
        min_pcr_intervals: 2,
        max_window_packets: 1024,
        max_buffer_packets: 8192,
        max_hold: Duration::from_secs(2),
        playout_delay: Duration::from_millis(100),
        max_lateness: Duration::from_millis(100),
        first_byte_timeout: Duration::from_secs(5),
        probe_maximum_bytes: 1024 * 188,
        probe_maximum_time: Duration::from_secs(5),
        startup_maximum_bytes: 8192 * 188,
    })
    .unwrap()
}

pub(super) fn preview_budget() -> TeletextPreviewBudget {
    TeletextPreviewBudget {
        max_pages: 16,
        max_model_records: 32_768,
        max_pixels: 168_000,
        max_diagnostics: 1400,
        max_work: 50_000_000,
    }
}

async fn installed_main_only_pair() -> Fixture {
    let fixture = Fixture::new_persistent().await;
    install_secondary(&fixture).await;
    for (addon, label, page, theme, hash) in [
        (ADDON, "first", 200, "day", "a".repeat(64)),
        (SECONDARY, "second", 201, "night", "b".repeat(64)),
    ] {
        sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256"=? WHERE "AddonId"=?"#)
            .bind(hash)
            .bind(addon)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        let mut original = response(DocumentFixture::Authored, theme, page, 0, "nld");
        original["magazines"][0]["magazine"] = json!(2);
        original["magazines"][0]["presentation"]["panels"] = json!({
            "left":false,"right":false,"status":"level25_and35","width":0
        });
        install_contract(&fixture, addon, label, 2).await;
        write_response(&fixture, label, &original);
        let profile = fixture.profile_for(addon, label, theme, "good").await;
        fixture.bind(1, &profile, None).await;
    }
    fixture
}

async fn configure_source(fixture: &Fixture, source: &Path) {
    configure_source_for_channels(fixture, source, &[1]).await;
}

pub(super) async fn configure_source_for_channels(
    fixture: &Fixture,
    source: &Path,
    channels: &[i64],
) {
    fixture.state.pool.execute(r#"
        UPDATE "Resolution" SET "Width"=160,"Height"=90 WHERE "Id"=1;
        INSERT INTO "MediaSource" ("Id") VALUES (10);
        INSERT INTO "PlexMediaSource" ("Id","ServerName","ClientIdentifier") VALUES (10,'Synthetic','synthetic');
        INSERT INTO "Library" ("Id","MediaSourceId","MediaKind") VALUES (10,10,1);
        INSERT INTO "PlexLibrary" ("Id","ShouldSyncItems") VALUES (10,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (10,10);
    "#).await.unwrap();
    let repositories = make_repositories(fixture.state.pool.clone());
    repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Synthetic copy".to_owned(),
            resolution_id: 1,
            transcode_video: false,
            transcode_audio: false,
            video_format: FFmpegProfileVideoFormat::Copy,
            audio_format: FFmpegProfileAudioFormat::Copy,
            video_bitrate: i32::try_from(VIDEO_KBPS).unwrap(),
            video_buffer_size: 2000,
            audio_bitrate: i32::try_from(AUDIO_KBPS).unwrap(),
            audio_buffer_size: 256,
            audio_channels: 1,
            audio_sample_rate: 48,
            ..Default::default()
        })
        .await
        .unwrap();
    for channel_id in channels {
        let mut channel = repositories
            .channel
            .get_with_profile(*channel_id)
            .await
            .unwrap();
        channel.streaming_mode = StreamingMode::TransportStream;
        channel.preferred_audio_language_code = Some("eng".to_owned());
        channel.subtitle_mode = ChannelSubtitleMode::None;
        repositories.channel.upsert(&channel).await.unwrap();
    }
    let schedule = fixture.root.path().join("synthetic-client-schedule.json");
    let body = serde_json::to_vec(&json!({"startTime":Utc::now().to_rfc3339(),"programs":[{
        "title":"Synthetic local clip", "serverKey":"Synthetic", "duration":60000,
        "file":source.to_str().unwrap(), "ratingKey":"1"
    }]}))
    .unwrap();
    tokio::fs::write(&schedule, body).await.unwrap();
    for channel_id in channels {
        sqlx::query(r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","ScheduleFile","Seed") VALUES (?,?,20,?,0)"#)
            .bind(channel_id).bind(channel_id)
            .bind(schedule.to_str().unwrap()).execute(&fixture.state.pool).await.unwrap();
    }
}

pub(super) async fn generate_source(ffmpeg: &str, root: &Path) -> PathBuf {
    generate_source_with_duration(ffmpeg, root, 12).await
}

/// Explicit finite synthetic source for capture or live headed-client trials.
/// The existing 12-second command and bound remain unchanged. Longer trials
/// write at most 48 MiB and never use the 8-MiB finite-capture consumer.
pub(super) async fn generate_source_with_duration(
    ffmpeg: &str,
    root: &Path,
    seconds: u8,
) -> PathBuf {
    assert!(matches!(seconds, 12 | 60));
    let rate = muxrate().to_string();
    let duration = seconds.to_string();
    let args = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "color=c=red:size=160x90:rate=25",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-t",
        &duration,
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-c:v",
        "mpeg2video",
        "-b:v",
        "1000k",
        "-c:a",
        "aac",
        "-b:a",
        "128k",
        "-metadata:s:a:0",
        "language=eng",
        "-muxrate",
        &rate,
        "-pcr_period",
        "20",
        "-mpegts_start_pid",
        "256",
        "-mpegts_pmt_start_pid",
        "4096",
        "-f",
        "mpegts",
        "pipe:1",
    ];
    let maximum = if seconds == 12 {
        MAX_BYTES
    } else {
        48 * 1024 * 1024
    };
    let bytes = probe(ffmpeg, &args, &[], maximum).await;
    assert!(!bytes.is_empty());
    assert_eq!(bytes.len() % 188, 0);
    assert!(bytes
        .chunks_exact(188)
        .all(|packet| ((u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2])) != 512));
    let path = root.join("synthetic-client-source.ts");
    tokio::fs::write(&path, bytes).await.unwrap();
    path
}

pub(super) async fn check_tools(ffmpeg: &str, ffprobe: &str) {
    assert_eq!(ersatzrs_ffmpeg::ffmpeg_path(), ffmpeg);
    assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), ffprobe);
    for tool in [ffmpeg, ffprobe] {
        let version = String::from_utf8(probe(tool, &["-version"], &[], 65_536).await).unwrap();
        assert_eq!(
            ersatzrs_ffmpeg::version::parse_version(version.lines().next().unwrap())
                .unwrap()
                .major,
            9
        );
    }
}

fn assert_exact_preview(
    saved: &SavedOriginalTeletextDesign,
    requested: &RequestedOriginalTeletextPublication,
) {
    let original = saved.candidate().encoded().unwrap();
    let replay = requested.design().encoded().unwrap();
    assert_eq!(units(original), units(replay));
    assert_eq!(original.magazine_units(), replay.magazine_units());
    assert_eq!(
        requested.design().selection(),
        saved.candidate().selection()
    );
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    for page in [200, 201] {
        let before =
            project_teletext_preview(original, address(page, 0), options, preview_budget())
                .unwrap();
        let after =
            project_teletext_preview(replay, address(page, 0), options, preview_budget()).unwrap();
        assert_eq!(before.raster().width(), 40 * 12);
        assert!(before.raster().diagnostics().is_empty());
        assert_eq!(before.raster(), after.raster());
        assert_eq!(before.navigation(), after.navigation());
        assert_eq!(before.navigation()[0].target.page.get(), page);
        assert_eq!(before.navigation()[0].target.subcode, None);
    }
}

async fn capture(fixture: &Fixture, binding: Arc<TeletextOutputBinding>) -> Vec<u8> {
    let response = Box::pin(serve_channel_stream_with_teletext(
        &fixture.state,
        "1",
        OutputFormat::MpegTs,
        binding,
    ))
    .await;
    capture_response(response).await
}

pub(super) async fn capture_response(response: axum::response::Response) -> Vec<u8> {
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body().into_data_stream();
    tokio::time::timeout(Duration::from_secs(35), async {
        let mut bytes = Vec::new();
        while let Some(chunk) = body.next().await {
            let chunk = chunk.expect("actual final direct output failed its gate or source");
            assert!(bytes.len().checked_add(chunk.len()).unwrap() <= MAX_BYTES);
            bytes.extend_from_slice(&chunk);
        }
        assert!(
            bytes.len() >= 1024 * 1024,
            "capture must contain actual sustained source packets"
        );
        assert_eq!(bytes.len() % 188, 0);
        bytes
    })
    .await
    .expect("bounded direct source response did not complete")
}

async fn bounded_read(
    reader: impl tokio::io::AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::new();
    reader
        .take(u64::try_from(limit.checked_add(1).unwrap()).unwrap())
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "native tool pipe read failed")?;
    if bytes.len() > limit {
        return Err("native tool pipe exceeded fixture bound");
    }
    Ok(bytes)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ToolInput {
    Complete,
    ProbePrefix,
}

fn tool_stage(args: &[&str]) -> &'static str {
    if args.contains(&"-show_streams") {
        "stream probe"
    } else if args.contains(&"--json") {
        "transport analysis"
    } else if args.contains(&"framemd5") {
        "video digest"
    } else if args.contains(&"s16le") {
        "audio decode"
    } else if args.contains(&"-txt_page") {
        "teletext decode"
    } else if args.contains(&"lavfi") {
        "source generation"
    } else {
        "version"
    }
}

fn redacted_stderr(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let mut output = text
        .split_whitespace()
        .map(|word| {
            if word.contains('/') || word.contains('\\') {
                "[path]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    if output.len() > 4096 {
        let mut end = 4096;
        while !output.is_char_boundary(end) {
            end -= 1;
        }
        output.truncate(end);
    }
    output
}

pub(super) async fn probe(executable: &str, args: &[&str], input: &[u8], limit: usize) -> Vec<u8> {
    probe_with_input(executable, args, input, limit, ToolInput::Complete).await
}

/// Select only this owned tool's environment; pipe/deadline/reaping policy is
/// identical to other native probes and never changes the parent host.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) async fn probe_with_environment(
    executable: &str,
    args: &[&str],
    environment: &[(&str, &str)],
    limit: usize,
) -> Vec<u8> {
    probe_with_input_and_environment(
        executable,
        args,
        &[],
        limit,
        ToolInput::Complete,
        environment,
    )
    .await
}

pub(super) async fn probe_with_input(
    executable: &str,
    args: &[&str],
    input: &[u8],
    limit: usize,
    policy: ToolInput,
) -> Vec<u8> {
    probe_with_input_and_environment(executable, args, input, limit, policy, &[]).await
}

// Join every pipe and wait even after a writer error. Only the explicitly
// selected stream-probe caller may accept successful prefix consumption.
async fn probe_with_input_and_environment(
    executable: &str,
    args: &[&str],
    input: &[u8],
    limit: usize,
    policy: ToolInput,
    environment: &[(&str, &str)],
) -> Vec<u8> {
    let name = Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("native tool");
    let stage = tool_stage(args);
    let mut child = Command::new(executable)
        .args(args)
        .envs(environment.iter().copied())
        .kill_on_drop(true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{name} {stage} spawn failed: {:?}", error.kind()));
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let write = async move {
            stdin.write_all(input).await.map_err(|error| error.kind())?;
            stdin.shutdown().await.map_err(|error| error.kind())
        };
        let (write, output, stderr, status) = tokio::join!(
            write,
            bounded_read(stdout, limit),
            bounded_read(stderr, 65_536),
            child.wait()
        );
        let diagnostic = stderr.as_ref().map_or_else(
            |_| "stderr unavailable or exceeded its bound".to_owned(),
            |bytes| redacted_stderr(bytes),
        );
        let failure = |reason| (reason, diagnostic.clone());
        stderr.map_err(failure)?;
        let status = status.map_err(|_| failure("native tool wait failed"))?;
        if !status.success() {
            return Err(failure("selected native tool exit failed"));
        }
        let output = output.map_err(failure)?;
        match write {
            Ok(()) => {}
            Err(std::io::ErrorKind::BrokenPipe)
                if policy == ToolInput::ProbePrefix && !output.is_empty() => {}
            Err(_) => return Err(failure("native tool complete input delivery failed")),
        }
        Ok(output)
    })
    .await
    .unwrap_or_else(|_| Err(("native tool deadline expired", String::new())));
    if result.is_err() {
        let _ = child.start_kill();
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("native tool cleanup deadline expired")
            .expect("native tool cleanup failed");
    }
    result.unwrap_or_else(|(reason, stderr)| {
        panic!("{name} {stage}: {reason}; bounded redacted stderr: {stderr}")
    })
}

async fn pending_output(
    fixture: &Fixture,
) -> ersatzrs_web::host::teletext_designs::RequestedTeletextOutputCandidate {
    let prepared = candidate(fixture, channel_documents::policy())
        .await
        .unwrap();
    let saved = save(fixture, prepared, None).await;
    assert!(fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap()
        .is_none());
    let result = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        None,
        document_budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = result else {
        panic!("synthetic complete originals must yield a pending intent");
    };
    assert_eq!(requested.request().channel_id, 1);
    assert_eq!(requested.request().design_id, saved.head().id);
    assert_eq!(requested.request().revision, saved.head().revision);
    assert_exact_preview(&saved, &requested);
    let output = requested.into_output_candidate().unwrap();
    assert_eq!(
        units(output.service()),
        units(saved.candidate().encoded().unwrap())
    );
    assert_eq!(output.selection(), saved.candidate().selection());
    output
}

async fn accepted_intent(fixture: &Fixture) -> Arc<TeletextAcceptedOutputIntent> {
    let pending = pending_output(fixture).await;
    assert!(load_accepted_teletext_output_intent(
        &fixture.state.application,
        1,
        document_budget(),
        channel_documents::policy(),
        source_policy(),
        deadline(),
    )
    .await
    .unwrap()
    .is_none());
    let result = accept_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            request: pending.request().clone(),
            expected_design_head: pending.request().revision,
            expected_generation_id: None,
        },
        document_budget(),
        channel_documents::policy(),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextGenerationMutation::Accepted(accepted) = result else {
        panic!("reviewed synthetic originals must accept explicitly");
    };
    assert_eq!(units(accepted.service()), units(pending.service()));
    assert_eq!(accepted.selection(), pending.selection());
    let loaded = load_accepted_teletext_output_intent(
        &fixture.state.application,
        1,
        document_budget(),
        channel_documents::policy(),
        source_policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(loaded.candidate().generation(), accepted.generation());
    assert_eq!(
        units(loaded.candidate().service()),
        units(accepted.service())
    );
    Arc::new(loaded)
}

#[tokio::test]
async fn accepted_direct_route_rejects_wrong_channel_and_other_container_before_startup() {
    let fixture = installed_main_only_pair().await;
    let intent = accepted_intent(&fixture).await;
    for (channel, format) in [("2", OutputFormat::MpegTs), ("1", OutputFormat::Nut)] {
        let response = Box::pin(serve_channel_stream_with_accepted_teletext(
            &fixture.state,
            channel,
            format,
            Arc::clone(&intent),
        ))
        .await;
        assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let repositories = make_repositories(fixture.state.pool.clone());
    let mut channel = repositories.channel.get_with_profile(1).await.unwrap();
    channel.streaming_mode = StreamingMode::TransportStreamHybrid;
    repositories.channel.upsert(&channel).await.unwrap();
    let response = Box::pin(serve_channel_stream_with_accepted_teletext(
        &fixture.state,
        "1",
        OutputFormat::MpegTs,
        intent,
    ))
    .await;
    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    // No source or schedule exists: these responses prove rejection before
    // producer admission, not an accidental unavailable-source error.
    channel_documents::assert_requests(&fixture, 2, 2);
    fixture.state.pool.close().await;
}

pub(super) async fn verify_transport(tsanalyze: &str, ffprobe: &str, bytes: &[u8]) {
    verify_transport_with_qualified_authored_pts_gaps(tsanalyze, ffprobe, bytes, 0, None, false)
        .await;
}

/// Permit exactly the independently qualified PMT-Off clock gaps on authored
/// PID 512. This helper does not classify a gap or relax any other transport gate.
/// Retain original analyzer stdout before validation when an output is supplied.
pub(super) async fn verify_transport_with_qualified_authored_pts_gaps(
    tsanalyze: &str,
    ffprobe: &str,
    bytes: &[u8],
    qualified_authored_pts_gaps: usize,
    report_destination: Option<&Path>,
    full_warm_capture: bool,
) -> Vec<u8> {
    let original_report = probe(tsanalyze, &["--json"], bytes, 262_144).await;
    if let Some(path) = report_destination {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("TSDuck report destination must not already exist");
        file.write_all(&original_report).unwrap();
    }
    let report: Value = serde_json::from_slice(&original_report).unwrap();
    assert!(
        report["ts"]["pcr-bitrate"]
            .as_u64()
            .unwrap()
            .abs_diff(u64::try_from(muxrate()).unwrap())
            < 2000
    );
    assert_eq!(
        report["ts"]["packets"]["total"].as_u64().unwrap(),
        u64::try_from(bytes.len() / 188).unwrap(),
        "TSDuck must inspect every captured packet"
    );
    for field in ["invalid-syncs", "transport-errors"] {
        assert_eq!(report["ts"]["packets"][field], 0);
    }
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
    let pids = report["pids"].as_array().unwrap();
    for expected in [256, 257, 4096, 512] {
        assert!(pids.iter().any(|pid| pid["id"] == expected));
    }
    let authored = pids.iter().find(|pid| pid["id"] == 512).unwrap();
    assert_eq!(authored["language"], "nld");
    assert_eq!(authored["pes-stream-id"], 0xbd);
    assert_eq!(
        pids.iter().find(|pid| pid["id"] == 257).unwrap()["language"],
        "eng"
    );
    for expected in [256, 257, 512] {
        assert_eq!(
            pids.iter().find(|pid| pid["id"] == expected).unwrap()["invalid-pes-prefix"],
            0
        );
    }
    for pid in pids {
        for field in ["discontinuities", "pts-leap", "pcr-leap"] {
            if pid["id"] == 512 && field == "pts-leap" {
                assert_eq!(
                    pid["packets"][field],
                    u64::try_from(qualified_authored_pts_gaps).unwrap(),
                    "every analyzer-sized authored PTS gap must have independently qualified PMT-Off clock provenance"
                );
            } else {
                assert_eq!(pid["packets"][field], 0);
            }
        }
    }
    let mut stream_probe = vec![
        "-v",
        "error",
        "-show_streams",
        "-show_programs",
        "-of",
        "json",
        "-f",
        "mpegts",
        "pipe:0",
    ];
    if full_warm_capture {
        // The synthetic60s flow starts Off. Default ffprobe discovery can stop
        // before the later PMT announces teletext; inspect its bounded full epoch.
        stream_probe.splice(
            2..2,
            ["-analyzeduration", "60000000", "-probesize", "50331648"],
        );
    }
    let streams: Value = serde_json::from_slice(
        &probe_with_input(
            ffprobe,
            &stream_probe,
            bytes,
            262_144,
            ToolInput::ProbePrefix,
        )
        .await,
    )
    .unwrap();
    let streams = streams["streams"].as_array().unwrap();
    assert!(streams.iter().any(|stream| stream["codec_type"] == "video"
        && stream["width"] == 160
        && stream["height"] == 90));
    assert!(streams
        .iter()
        .any(|stream| stream["codec_type"] == "audio" && stream["sample_rate"] == "48000"));
    assert!(streams
        .iter()
        .any(|stream| stream["codec_name"] == "dvb_teletext" && stream["id"] == "0x200"));
    original_report
}

pub(super) async fn decode_page_text(ffmpeg: &str, bytes: &[u8], page: &str) -> String {
    decode_page_text_with_discovery(ffmpeg, bytes, page, false).await
}

/// A warm Off-start capture needs bounded discovery through its later PMT.
pub(super) async fn decode_page_text_with_discovery(
    ffmpeg: &str,
    bytes: &[u8],
    page: &str,
    full_warm_capture: bool,
) -> String {
    let mut args = vec![
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-txt_page",
        page,
        "-txt_format",
        "text",
        "-txt_chop_top",
        "0",
        "-txt_duration",
        "1000",
        "-f",
        "mpegts",
        "-i",
        "pipe:0",
        "-map",
        "0:s:0",
        "-c:s",
        "srt",
        "-f",
        "srt",
        "pipe:1",
    ];
    if full_warm_capture {
        args.splice(
            14..14,
            ["-analyzeduration", "60000000", "-probesize", "50331648"],
        );
    }
    let output = probe(ffmpeg, &args, bytes, 262_144).await;
    String::from_utf8(output).unwrap()
}

pub(super) async fn decode_page(ffmpeg: &str, bytes: &[u8], page: &str) {
    let output = decode_page_text(ffmpeg, bytes, page).await;
    assert!(
        output.contains("AZ"),
        "actual page must contain the original enhanced A and unchanged neighbouring Z"
    );
}

async fn video_hashes(ffmpeg: &str, bytes: &[u8]) -> Vec<String> {
    let output = probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-map",
            "0:v:0",
            "-an",
            "-f",
            "framemd5",
            "pipe:1",
        ],
        bytes,
        524_288,
    )
    .await;
    String::from_utf8(output)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.split(',').nth(5).unwrap().trim().to_owned())
        .collect()
}

pub(super) async fn verify_av(ffmpeg: &str, source: &[u8], delivered: &[u8]) {
    verify_av_with_audio_bound(ffmpeg, source, delivered, 2 * 1024 * 1024).await;
}

/// Longer synthetic live captures retain the same source AV oracle under an
/// explicit PCM byte cap; the original finite-capture bound stays unchanged.
pub(super) async fn verify_av_with_audio_bound(
    ffmpeg: &str,
    source: &[u8],
    delivered: &[u8],
    maximum_audio_bytes: usize,
) {
    let hashes: BTreeSet<_> = video_hashes(ffmpeg, source).await.into_iter().collect();
    let actual = video_hashes(ffmpeg, delivered).await;
    assert!(actual.len() >= 20);
    assert!(
        actual.iter().all(|hash| hashes.contains(hash)),
        "output must decode source video, not a capacity slate"
    );
    let audio = probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "pipe:1",
        ],
        delivered,
        maximum_audio_bytes,
    )
    .await;
    let samples: Vec<_> = audio
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()))
        .collect();
    let tones = samples
        .chunks_exact(4800)
        .filter(|chunk| {
            let crossings = chunk
                .windows(2)
                .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
                .count();
            let frequency = crossings * 48_000 / (2 * chunk.len());
            frequency.abs_diff(440) <= 20 && chunk.iter().any(|sample| sample.unsigned_abs() > 100)
        })
        .count();
    assert!(
        tones >= 10,
        "actual audio must retain at least one second of the literal 440Hz source tone"
    );
}

#[tokio::test]
#[ignore = "requires exact FFmpeg/ffprobe 9, TSDuck and explicit create-new synthetic capture destination; headed client trial is separate"]
async fn installed_originals_pending_request_capture_actual_direct_mpegts_for_client_trial() {
    let ffmpeg = selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = selected("FFPROBE_PATH");
    let tsanalyze = selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    let artifact = PathBuf::from(selected("ERSATZRS_TELETEXT_CLIENT_CAPTURE"));
    check_tools(&ffmpeg, &ffprobe).await;
    let mut fixture = installed_main_only_pair().await;
    let source = generate_source(&ffmpeg, fixture.root.path()).await;
    configure_source(&fixture, &source).await;
    let before = config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let output = pending_output(&fixture).await;
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(output.request().clone())
    );
    fixture
        .state
        .application
        .recheck_channel_teletext_document_selection(output.selection(), deadline())
        .await
        .unwrap();
    let binding = Arc::new(
        TeletextOutputBinding::new_authored_shared(
            output_policy(),
            Arc::clone(output.service()),
            Duration::from_secs(5),
        )
        .unwrap(),
    );
    let bytes = capture(&fixture, binding).await;
    verify_transport(&tsanalyze, &ffprobe, &bytes).await;
    let source_bytes = tokio::fs::read(&source).await.unwrap();
    assert!(source_bytes.len() <= MAX_BYTES);
    verify_av(&ffmpeg, &source_bytes, &bytes).await;
    decode_page(&ffmpeg, &bytes, "200").await;
    decode_page(&ffmpeg, &bytes, "201").await;
    assert_eq!(config(&fixture).await, before);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    channel_documents::assert_requests(&fixture, 2, 2);
    assert!(fixture.playout.try_recv().is_err());
    // Opt-in capture is synthetic-only and never overwrites a preexisting file.
    // No stored document, private context or active-publication receipt is exported.
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(artifact)
        .await
        .unwrap();
    file.write_all(&bytes).await.unwrap();
    file.sync_all().await.unwrap();
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires exact FFmpeg/ffprobe 9, TSDuck and explicit create-new synthetic capture destination; headed client trial is separate"]
async fn accepted_originals_reopened_capture_actual_direct_mpegts_for_client_trial() {
    let ffmpeg = selected("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let ffprobe = selected("FFPROBE_PATH");
    let tsanalyze = selected("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    let artifact = PathBuf::from(selected("ERSATZRS_TELETEXT_CLIENT_CAPTURE"));
    check_tools(&ffmpeg, &ffprobe).await;
    let mut fixture = installed_main_only_pair().await;
    let source = generate_source(&ffmpeg, fixture.root.path()).await;
    configure_source(&fixture, &source).await;
    let before = config(&fixture).await;
    let bindings = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let accepted = accepted_intent(&fixture).await;
    let generation = accepted.candidate().generation().clone();
    let compiled_units = units(accepted.candidate().service());
    drop(accepted);
    fixture = fixture.reopen().await;
    let reloaded = Arc::new(
        load_accepted_teletext_output_intent(
            &fixture.state.application,
            1,
            document_budget(),
            channel_documents::policy(),
            source_policy(),
            deadline(),
        )
        .await
        .unwrap()
        .unwrap(),
    );
    assert_eq!(reloaded.candidate().generation(), &generation);
    assert_eq!(units(reloaded.candidate().service()), compiled_units);
    let response = Box::pin(serve_channel_stream_with_accepted_teletext(
        &fixture.state,
        "1",
        OutputFormat::MpegTs,
        reloaded,
    ))
    .await;
    let bytes = capture_response(response).await;
    verify_transport(&tsanalyze, &ffprobe, &bytes).await;
    let source_bytes = tokio::fs::read(&source).await.unwrap();
    assert!(source_bytes.len() <= MAX_BYTES);
    verify_av(&ffmpeg, &source_bytes, &bytes).await;
    decode_page(&ffmpeg, &bytes, "200").await;
    decode_page(&ffmpeg, &bytes, "201").await;
    assert_eq!(config(&fixture).await, before);
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap(),
        bindings
    );
    channel_documents::assert_requests(&fixture, 2, 2);
    assert!(fixture.playout.try_recv().is_err());
    // Keep exactly the successful synthetic stream for a separate headed trial.
    // Originals, private context and durable publication records remain local.
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(artifact)
        .await
        .unwrap();
    file.write_all(&bytes).await.unwrap();
    file.sync_all().await.unwrap();
    fixture.state.pool.close().await;
}

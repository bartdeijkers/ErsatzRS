//! Opt-in shared initializer proof on a genuinely scanned staggered source.
//! Exact fixture masks and complete decoded audio clocks are checked; this is
//! neither an ordinary HTTP route claim nor full teletext engine acceptance.

use std::sync::Arc;

use ersatzrs_domain::HardwareAccelerationKind;
use ersatzrs_ffmpeg::{
    DesiredVideoState, FfmpegCommand, FrameSize, InputFile, PipelineRequest, SourceAudioCodec,
    SourceAudioStream, SourceVideoCodec, SourceVideoStream,
};
use ersatzrs_infra::{
    playback::source_teletext::runtime::{
        self, PlaybackSource, RuntimeSelectionContext, SourceTeletextConsumerRequest,
        SourceTeletextPlaybackInterval,
    },
    remote_stream::{spawn_ffmpeg_with_remote_input, RemoteStreamSpawnContext},
    transcode_capacity::{
        try_reserve_active_transcode, ActiveTranscodeReservation, ActiveTranscodeSlot,
    },
};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio::time::Instant;

use super::*;

const GRAY_FRAME_BYTES: usize = WIDTH * HEIGHT;
const SAMPLE_STRIDE: usize = 5;

async fn staggered_source(ffmpeg: &str, library: &Path) -> PathBuf {
    tokio::fs::create_dir(library).await.unwrap();
    let args = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-itsoffset",
        "3",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=492x250:r=25",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-itsoffset",
        "2",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=660:sample_rate=48000",
        "-f",
        "mpegts",
        "-i",
        "pipe:0",
        "-t",
        "8",
        "-map",
        "0:v:0",
        "-map",
        "1:a:0",
        "-map",
        "3:s:0",
        "-map",
        "2:a:0",
        "-c:v",
        "mpeg2video",
        "-b:v",
        "200k",
        "-c:a",
        "aac",
        "-b:a",
        "64k",
        "-c:s",
        "copy",
        "-metadata:s:a:0",
        "language=eng",
        "-metadata:s:a:1",
        "language=nld",
        "-muxrate",
        "2000000",
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
    let bytes = probe(
        ffmpeg,
        &args,
        ersatzrs_ffmpeg::teletext_probe::TELETEXT_DECODE_FIXTURE,
        MAX_CAPTURE,
    )
    .await
    .unwrap();
    assert_eq!(bytes.len() % 188, 0);
    assert!(bytes.len() / 188 < 32768);
    let path = library.join("Synthetic Staggered Caption.ts");
    tokio::fs::write(&path, bytes).await.unwrap();
    path
}

async fn selected_source(
    state: &AppState,
    source: &Path,
    item: i64,
    version: i64,
    start: Duration,
) -> Arc<PlaybackSource> {
    let file =
        sqlx::query_scalar(r#"SELECT "Id" FROM "MediaFile" WHERE "MediaVersionId"=? AND "Path"=?"#)
            .bind(version)
            .bind(source.to_str().unwrap())
            .fetch_one(&state.pool)
            .await
            .unwrap();
    let locker = state.application.operations().entity_locker();
    let selection = runtime::select(RuntimeSelectionContext {
        pool: &state.pool,
        locker: &locker,
        media_item_id: Some(item),
        media_version_id: Some(version),
        media_file_id: Some(file),
        original_path: source.to_str().unwrap(),
        resolved_path: source.to_str().unwrap(),
        languages: &["eng".into()],
        subtitle_mode: ChannelSubtitleMode::Any,
        streaming_mode: StreamingMode::TransportStream,
        playback_interval: SourceTeletextPlaybackInterval::from_start_and_duration(
            start,
            Duration::from_secs(4),
        ),
        deadline: Instant::now() + Duration::from_secs(15),
    })
    .await
    .unwrap()
    .expect("managed scanned staggered source must select its exact caption page");
    let source = selection.source.expect("exact-page source receipt");
    assert_eq!(source.playback_interval().unwrap().start(), start);
    assert_eq!(locker.active_teletext_work(), 0);
    source
}

fn request(
    ffmpeg: &str,
    source: &PlaybackSource,
    audio: usize,
    buffer: Option<u32>,
    captions: bool,
) -> SourceTeletextConsumerRequest {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.frame_size = FrameSize::new(492, 250);
    desired.scaled_size = desired.frame_size;
    desired.video_bitrate = Some(400);
    desired.video_buffer_size = buffer;
    let mut request = PipelineRequest::new(InputFile::video(source.input().path), desired)
        .with_source_video_stream(
            SourceVideoStream::new(SourceVideoCodec::Mpeg2Video).with_index(0),
        )
        .with_source_audio_streams(vec![
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1),
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3),
        ]);
    request.inputs[1] = request.inputs[1].clone().with_stream_index(audio);
    if captions {
        request.inputs.push(source.input());
    }
    let interval = source.playback_interval().unwrap();
    request.state.start = (!interval.start().is_zero()).then_some(interval.start());
    request.state.duration = Some(interval.duration());
    request.state.report_progress = false;
    request.ffmpeg_executable = Some(ffmpeg.into());
    SourceTeletextConsumerRequest::new(HardwareAccelerationKind::None, request)
}

async fn bounded_pipe(reader: impl AsyncRead + Unpin, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(u64::try_from(limit + 1).unwrap())
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() > limit {
        return Err(std::io::Error::other("synthetic output limit exceeded"));
    }
    Ok(bytes)
}

async fn capture_command(
    command: &FfmpegCommand,
    source: Option<&Arc<PlaybackSource>>,
    reservation: &ActiveTranscodeReservation,
    evidence: &Path,
) -> Vec<u8> {
    std::fs::write(
        evidence.with_extension("command.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "executable":command.executable,"arguments":command.arguments,
            "environment_empty":command.environment.is_empty(),
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        command.environment.is_empty(),
        "fixture records the exact empty environment"
    );
    let mut child = spawn_ffmpeg_with_remote_input(
        command,
        None,
        None,
        RemoteStreamSpawnContext::mpeg_ts("synthetic-source-caption-initializer"),
    )
    .unwrap();
    // No await separates the actual consumer spawn from source/admission
    // attachment. The production adapter owns uncertain cleanup independently.
    if let Some(source) = source {
        if let Err(error) =
            child.attach_local_teletext_source(source.receipt(), reservation.clone())
        {
            child.kill().await.expect("reap failed source attachment");
            panic!("source attachment failed: {error}");
        }
    }
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let captured = tokio::time::timeout(Duration::from_secs(30), async {
        tokio::try_join!(
            bounded_pipe(stdout, MAX_CAPTURE),
            bounded_pipe(stderr, 64 * 1024),
            child.wait(),
        )
    })
    .await;
    let Ok(Ok((output, stderr, status))) = captured else {
        let _ = child.start_kill();
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .expect("bounded consumer cleanup deadline")
            .expect("bounded consumer cleanup");
        panic!("synthetic caption consumer failed or exceeded a bound");
    };
    std::fs::write(evidence.with_extension("stderr"), stderr).unwrap();
    std::fs::write(evidence.with_extension("ts"), &output).unwrap();
    assert!(
        status.success(),
        "synthetic caption consumer rejected command"
    );
    assert!(!output.is_empty());
    assert_eq!(output.len() % 188, 0);
    output
}

struct Observation {
    frames: serde_json::Value,
    masks: Vec<Vec<u8>>,
    audio: Vec<String>,
}

async fn observe(tools: &[String; 2], output: &[u8], evidence: &Path) -> Observation {
    let frames = probe(
        &tools[1],
        &[
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_frames",
            "-show_streams",
            "-show_entries",
            "stream=time_base:frame=pts,best_effort_timestamp,pts_time",
            "-of",
            "json",
            "-i",
            "pipe:0",
        ],
        output,
        MAX_CAPTURE,
    )
    .await
    .unwrap();
    let frames: serde_json::Value = serde_json::from_slice(&frames).unwrap();
    let gray = probe(
        &tools[0],
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-copyts",
            "-i",
            "pipe:0",
            "-map",
            "0:v:0",
            "-an",
            "-vf",
            "select=not(mod(n\\,5))",
            "-fps_mode",
            "passthrough",
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
        output,
        MAX_CAPTURE,
    )
    .await
    .unwrap();
    let count = frames["frames"].as_array().unwrap().len();
    assert!(count > 0);
    assert_eq!(gray.len() % GRAY_FRAME_BYTES, 0);
    assert_eq!(gray.len() / GRAY_FRAME_BYTES, count.div_ceil(SAMPLE_STRIDE));
    let masks: Vec<Vec<u8>> = gray
        .chunks_exact(GRAY_FRAME_BYTES)
        .map(|frame| frame.iter().map(|value| u8::from(*value > 100)).collect())
        .collect();
    let audio = audio_framehash(&tools[0], output).await;
    std::fs::write(
        evidence.with_extension("frames.json"),
        serde_json::to_vec_pretty(&frames).unwrap(),
    )
    .unwrap();
    std::fs::write(evidence.with_extension("gray"), gray).unwrap();
    std::fs::write(evidence.with_extension("masks"), masks.concat()).unwrap();
    std::fs::write(evidence.with_extension("audio.framehash"), audio.join("\n")).unwrap();
    Observation {
        frames,
        masks,
        audio,
    }
}

async fn audio_framehash(ffmpeg: &str, output: &[u8]) -> Vec<String> {
    let bytes = probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-copyts",
            "-i",
            "pipe:0",
            "-map",
            "0:a:0",
            "-vn",
            "-c:a",
            "pcm_f32le",
            "-enc_time_base",
            "demux",
            "-f",
            "framehash",
            "-hash",
            "sha256",
            "pipe:1",
        ],
        output,
        MAX_CAPTURE,
    )
    .await
    .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.lines().any(|line| line == "#tb 0: 1/90000"));
    let rows: Vec<_> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(str::to_owned)
        .collect();
    assert!(!rows.is_empty());
    rows
}

fn assert_observations(off: &Observation, initialized: &Observation, reference: &[u8]) {
    assert!(off.masks.iter().flatten().all(|value| *value == 0));
    assert_eq!(initialized.frames["streams"][0]["time_base"], "1/90000");
    assert_eq!(initialized.frames["frames"][0]["pts"], 0);
    assert_eq!(
        initialized.masks.len(),
        initialized.frames["frames"]
            .as_array()
            .unwrap()
            .len()
            .div_ceil(SAMPLE_STRIDE)
    );
    for mask in &initialized.masks {
        assert_eq!(
            mask, reference,
            "first and every fifth decoded frame retain the exact fixture glyph"
        );
    }
    // Full rows include channel count-independent sample hashes, native absolute
    // DTS/PTS, durations and byte sizes; no rebasing or tolerance hides a change.
    assert_eq!(
        initialized.audio, off.audio,
        "initialized captions must preserve ordinary decoded audio and absolute clocks"
    );
}

async fn record_runtime(tools: &[String; 2], source: &Path, evidence: &Path) {
    let mut runtimes = Vec::new();
    for tool in tools {
        let version = probe(tool, &["-version"], &[], 64 * 1024).await.unwrap();
        runtimes.push(serde_json::json!({
            "program":tool,"sha256":hash(&tokio::fs::read(tool).await.unwrap()),
            "version":String::from_utf8(version).unwrap(),
        }));
    }
    let bytes = tokio::fs::read(source).await.unwrap();
    std::fs::write(evidence.join("runtime.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "tools":runtimes,"source_sha256":hash(&bytes),"source_bytes":bytes.len(),
        "scope":"Shared production initializer on managed-scanned finite synthetic source; no ordinary HTTP, suitable-client or full engine acceptance claim.",
        "foreground_threshold_exclusive":100,"decoded_frame_stride":SAMPLE_STRIDE,
    })).unwrap()).unwrap();
}

fn hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(hash, "{byte:02x}").unwrap();
    }
    hash
}

async fn verify_policy(
    tools: &[String; 2],
    selected: &Arc<PlaybackSource>,
    reservation: &ActiveTranscodeReservation,
    root: &Path,
    audio: usize,
    buffer: Option<u32>,
    reference: &[u8],
) {
    let label = format!(
        "audio{audio}-{}",
        if buffer.is_some() {
            "bounded"
        } else {
            "no-buffer"
        }
    );
    let off = request(&tools[0], selected, audio, buffer, false)
        .build()
        .unwrap()
        .0;
    let off_path = root.join(format!("{label}-off"));
    let off = capture_command(&off, None, reservation, &off_path).await;
    let off = observe(tools, &off, &off_path).await;
    let request = request(&tools[0], selected, audio, buffer, true);
    let (initial, plan) = request.build().unwrap();
    let command = selected
        .initialize_consumer_command(
            &initial,
            plan.as_ref(),
            &request,
            Instant::now() + Duration::from_secs(15),
        )
        .await
        .expect("shared initializer must qualify this scanned finite source");
    let initialized_path = root.join(format!("{label}-initialized"));
    let initialized =
        capture_command(&command, Some(selected), reservation, &initialized_path).await;
    let initialized = observe(tools, &initialized, &initialized_path).await;
    std::fs::write(
        root.join(format!("{label}-comparison.json")),
        serde_json::to_vec_pretty(&serde_json::json!({
            "selected_audio_index":audio,"video_bitrate_kbps":400,"video_buffer_kbps":buffer,
            "first_decoded_frame":initialized.frames["frames"][0],
            "decoded_frame_count":initialized.frames["frames"].as_array().unwrap().len(),
            "sample_count":initialized.masks.len(),"audio_rows":initialized.audio.len(),
            "complete_audio_framehash_rows_equal_off":initialized.audio==off.audio,
            "all_sampled_glyph_masks_equal_reference":initialized.masks.iter().all(|mask|mask==reference),
            "off_samples_blank":off.masks.iter().flatten().all(|value|*value==0),
        })).unwrap(),
    ).unwrap();
    assert_observations(&off, &initialized, reference);
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        1
    );
    println!(
        "shared initializer verified: {label}, frames={}, samples={}, audio_rows={}",
        initialized.frames["frames"].as_array().unwrap().len(),
        initialized.masks.len(),
        initialized.audio.len()
    );
}

async fn run_initializer() {
    isolated_bundled_wrapper().await;
    let tools = [
        exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG"),
        exact_tool("FFPROBE_PATH"),
    ];
    assert_eq!(ersatzrs_ffmpeg::ffmpeg_path(), tools[0]);
    assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), tools[1]);
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "native production source-caption initializer evidence: {}",
        root.display()
    );
    let library = root.join("library");
    let source = staggered_source(&tools[0], &library).await;
    record_runtime(&tools, &source, &root).await;
    let provider = Server::start(Router::new()).await;
    let state = application(&root, &provider).await;
    let (item, version) = scan_source(&state, &library, &tools[1]).await;
    software_profile(&state).await;
    let reservation = try_reserve_active_transcode(
        DEFAULT_ACTIVE_TRANSCODE_LIMIT,
        ActiveTranscodeSlot::new_mpeg_ts("synthetic-source-caption-initializer"),
    )
    .unwrap();
    let reference_source = selected_source(&state, &source, item, version, Duration::ZERO).await;
    let reference_request = request(&tools[0], &reference_source, 1, None, true);
    let (reference_command, reference_plan) = reference_request.build().unwrap();
    reference_source
        .check_consumer_command(&reference_command, reference_plan.as_ref())
        .unwrap();
    reference_source
        .recheck(Instant::now() + Duration::from_secs(15))
        .await
        .unwrap();
    // Independent unseeked shared-input control uses the known fixture decoder
    // recipe, not the initializer under test or an invented timing tolerance.
    let reference = capture_command(
        &reference_command,
        Some(&reference_source),
        &reservation,
        &root.join("reference-unseeked"),
    )
    .await;
    let reference = observe(&tools, &reference, &root.join("reference-unseeked")).await;
    let reference = &reference.masks[0];
    assert_eq!(
        reference.iter().copied().map(usize::from).sum::<usize>(),
        650
    );
    let selected =
        selected_source(&state, &source, item, version, Duration::from_millis(500)).await;
    for audio in [1, 3] {
        for buffer in [Some(800), None] {
            verify_policy(
                &tools,
                &selected,
                &reservation,
                &root,
                audio,
                buffer,
                reference,
            )
            .await;
            assert_eq!(
                state
                    .application
                    .operations()
                    .entity_locker()
                    .active_teletext_work(),
                0
            );
        }
    }
    drop(reservation);
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
    provider.finish().await;
    state.pool.close().await;
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9+libzvbi/libx264, loopback and fresh isolated app-data; managed scan and actual initialized AV children"]
fn scanned_staggered_source_initializer_preserves_first_glyph_and_both_audio_clocks() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_initializer()));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}

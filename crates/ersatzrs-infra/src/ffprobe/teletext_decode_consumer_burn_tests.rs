//! Actual generated Burn diagnostics; sampled frame ordinals are not timestamps.

use super::*;
use ersatzrs_ffmpeg::{FfmpegCommand, FrameSize};

const FRAME_BYTES: usize = 492 * 250;
const SAMPLE_STRIDE: usize = 5;

#[path = "teletext_decode_consumer_clock_tests.rs"]
mod clock;

#[path = "teletext_decode_consumer_repeat_tests.rs"]
mod repeat;

#[path = "teletext_decode_consumer_independent_tests.rs"]
mod independent;

fn burn_command(
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    caption: Option<&DecodedLocalTeletextPage>,
    seek: Option<Duration>,
) -> FfmpegCommand {
    burn_command_with_video_buffer(ffmpeg, source, caption, seek, Some(800))
}

fn burn_command_with_video_buffer(
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    caption: Option<&DecodedLocalTeletextPage>,
    seek: Option<Duration>,
    video_buffer_size: Option<u32>,
) -> FfmpegCommand {
    let request = burn_request(ffmpeg, source, caption, seek, video_buffer_size);
    let (command, plan) = build_pipeline_command_with_source_teletext_plan(
        ersatzrs_domain::HardwareAccelerationKind::None,
        request,
    )
    .unwrap();
    assert!(
        command.environment.is_empty(),
        "capture preserves this empty environment"
    );
    if let Some(caption) = caption {
        let plan = plan.unwrap();
        assert!(plan.matches_command(&command));
        assert_eq!(plan.caption_input(), &caption.native_burn_input());
        assert_eq!(plan.enabled_caption_input_streams(), [0, 1, 2]);
        let timing = plan.timing();
        assert_eq!(timing.input_seek(), seek);
        assert_eq!(timing.duration(), Some(Duration::from_secs(4)));
        assert_eq!(timing.output_seek(), None);
        assert!(!timing.retained_stream_timestamps());
        assert_eq!(
            timing.effective_input_seek_microseconds(),
            Some(i64::try_from(seek.unwrap_or_default().as_micros()).unwrap())
        );
    } else {
        assert!(plan.is_none());
    }
    command
}

fn burn_request(
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    caption: Option<&DecodedLocalTeletextPage>,
    seek: Option<Duration>,
    video_buffer_size: Option<u32>,
) -> PipelineRequest {
    let mut desired = DesiredVideoState::h264_1080p();
    desired.frame_size = FrameSize::new(492, 250);
    desired.scaled_size = desired.frame_size;
    desired.video_bitrate = Some(400);
    desired.video_buffer_size = video_buffer_size;
    let mut request =
        PipelineRequest::new(InputFile::video(source.resolved.to_str().unwrap()), desired)
            .with_source_video_stream(
                SourceVideoStream::new(SourceVideoCodec::Mpeg2Video).with_index(0),
            )
            .with_source_audio_streams(vec![
                SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1)
            ]);
    request.state.start = seek;
    request.state.duration = Some(Duration::from_secs(4));
    request.state.report_progress = false;
    request.ffmpeg_executable = Some(ffmpeg.into());
    if let Some(caption) = caption {
        request.inputs.push(caption.native_burn_input());
    }
    request
}

async fn capture_owned(
    scope: &LocalProbeCleanupScope,
    command: FfprobeCommand,
    source: &PreparedLocalTeletextSource,
    work: &TeletextWorkGuard,
) -> Vec<u8> {
    scope
        .scope(local::capture_retained(
            command,
            "synthetic generated bitmap consumer diagnostic".into(),
            Some(Instant::now() + Duration::from_secs(30)),
            Some(local::ProbeRetention::new(
                work.hold_work(),
                source.guard.clone(),
            )),
        ))
        .await
        .unwrap()
}

async fn frame_evidence(
    scope: &LocalProbeCleanupScope,
    tools: (&str, &str),
    source: &PreparedLocalTeletextSource,
    path: &std::path::Path,
    work: &TeletextWorkGuard,
) -> (serde_json::Value, Vec<Vec<u8>>) {
    let (ffmpeg, ffprobe) = tools;
    let path = path.to_str().unwrap();
    let trace = capture_owned(scope, FfprobeCommand {
        program: ffprobe.into(),
        args: ["-v", "error", "-select_streams", "v:0", "-show_frames", "-show_streams",
            "-show_entries", "stream=time_base:frame=pts,best_effort_timestamp,pts_time,best_effort_timestamp_time",
            "-of", "json", "-i", path].map(str::to_owned).into(),
    }, source, work).await;
    // select retains every fifth decoded frame; it does not synthesize a new
    // five-fps clock. Each sample is joined to its original frame trace below.
    let pixels = capture_owned(
        scope,
        FfprobeCommand {
            program: ffmpeg.into(),
            args: [
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-copyts",
                "-i",
                path,
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
            ]
            .map(str::to_owned)
            .into(),
        },
        source,
        work,
    )
    .await;
    let trace: serde_json::Value = serde_json::from_slice(&trace).unwrap();
    let frames = trace["frames"].as_array().unwrap();
    assert!(!frames.is_empty());
    assert_eq!(pixels.len() % FRAME_BYTES, 0);
    assert_eq!(
        pixels.len() / FRAME_BYTES,
        frames.len().div_ceil(SAMPLE_STRIDE)
    );
    (
        trace,
        pixels
            .chunks_exact(FRAME_BYTES)
            .map(<[u8]>::to_vec)
            .collect(),
    )
}

async fn render(
    scope: &LocalProbeCleanupScope,
    tools: (&str, &str),
    source: &PreparedLocalTeletextSource,
    caption: Option<&DecodedLocalTeletextPage>,
    seek: Option<Duration>,
    evidence: &std::path::Path,
    work: &TeletextWorkGuard,
) -> (serde_json::Value, Vec<Vec<u8>>) {
    scope
        .scope(source.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()))
        .await
        .unwrap();
    let command = burn_command(tools.0, source, caption, seek);
    std::fs::write(
        evidence.with_extension("command.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"executable": command.executable, "arguments": command.arguments}),
        )
        .unwrap(),
    )
    .unwrap();
    let bytes = capture_owned(
        scope,
        FfprobeCommand {
            program: command.executable,
            args: command.arguments,
        },
        source,
        work,
    )
    .await;
    assert!(!bytes.is_empty());
    std::fs::write(evidence.with_extension("ts"), &bytes).unwrap();
    scope
        .scope(source.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()))
        .await
        .unwrap();
    let result = frame_evidence(scope, tools, source, &evidence.with_extension("ts"), work).await;
    std::fs::write(
        evidence.with_extension("frames.json"),
        serde_json::to_vec_pretty(&result.0).unwrap(),
    )
    .unwrap();
    std::fs::write(evidence.with_extension("gray"), result.1.concat()).unwrap();
    result
}

fn comparisons(cases: &[(serde_json::Value, Vec<Vec<u8>>)]) -> serde_json::Value {
    let samples: Vec<_> = cases[0].0["frames"]
        .as_array()
        .unwrap()
        .iter()
        .step_by(SAMPLE_STRIDE)
        .enumerate()
        .map(|(index, frame)| {
            let pixels: Vec<_> = cases.iter().map(|(_, frames)| frames.get(index)).collect();
            let distance = |left: usize, right: usize| {
                pixels[left].zip(pixels[right]).map(|(a, b)| {
                    a.iter()
                        .zip(b)
                        .map(|(x, y)| u64::from(x.abs_diff(*y)))
                        .sum::<u64>()
                })
            };
            serde_json::json!({"sample": index, "off_frame": frame,
                "page888_frame": cases[1].0["frames"].get(index * SAMPLE_STRIDE),
                "page200_frame": cases[2].0["frames"].get(index * SAMPLE_STRIDE),
                "foreground_pixels_off_888_200": pixels.iter().map(|frame| frame.map(|bytes|
                    bytes.iter().filter(|value| **value > 100).count())).collect::<Vec<_>>(),
                "distance_888_off": distance(1,0), "distance_200_off": distance(2,0),
                "distance_888_200": distance(1,2)})
        })
        .collect();
    serde_json::json!({"sample_stride_decoded_frames": SAMPLE_STRIDE,
        "note": "Each case retains its own actual output PTS; sample ordinals are not a shared clock or exact onset. No general cue or seek qualification.",
        "samples": samples})
}

fn assert_observed_pixels(cases: &[(serde_json::Value, Vec<Vec<u8>>)], sought: bool) {
    let foreground = |frame: &[u8]| frame.iter().filter(|value| **value > 100).count();
    assert!(cases[0].1.iter().all(|frame| foreground(frame) == 0));
    assert!(
        foreground(&cases[2].1[0]) > 500,
        "page200 is an independent visible-page control"
    );
    let caption = &cases[1].1;
    if sought {
        // Pinned native diagnostic, not accepted behavior: the sought run
        // starts without page888 pixels and later displays them. Whether the
        // known missing-PTS prefix causes this needs separate event matching.
        assert_eq!(caption[0], cases[0].1[0]);
        assert!(foreground(caption.last().unwrap()) > 500);
    } else {
        assert!(foreground(&caption[0]) > 500);
        assert_ne!(caption[0], cases[2].1[0]);
    }
}

#[tokio::test]
#[ignore = "requires explicit native FFmpeg/FFprobe and generated Burn pixel diagnostics"]
#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in synthetic diagnostic evidence destination"
)]
async fn native_generated_burn_exposes_bitmap_onset_with_seek() {
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
    std::fs::write(evidence.join("staggered-source.ts"), &original).unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let source = prepare(&scope, &extractor, &path, &work).await;
    let caption = decode(&scope, &extractor, &source, &original, &work).await;
    // Same actual held source; change only the requested page and decode it
    // through the real prepared-source API, never manufacture a bitmap receipt.
    let guide_source = PreparedLocalTeletextSource {
        request: SourceTeletextPageDecodeRequest {
            page: TeletextPageNumber::new(200).unwrap(),
            ..source.request.clone()
        },
        original: source.original.clone(),
        resolved: source.resolved.clone(),
        guard: source.guard.clone(),
        native: source.native.clone(),
    };
    let guide = decode(&scope, &extractor, &guide_source, &original, &work).await;
    assert!(std::sync::Arc::ptr_eq(&caption.guard, &guide.guard));
    eprintln!(
        "generated bitmap source SHA256={}, native={:?}, page888={:?}, page200={:?}",
        hex_text(&Sha256::digest(&original)),
        source.native_origin(),
        caption.events(),
        guide.events()
    );
    for (name, seek) in [
        ("no-seek", None),
        ("seek-half", Some(Duration::from_millis(500))),
    ] {
        let mut cases = Vec::new();
        for (page, receipt) in [
            ("off", None),
            ("888", Some(&caption)),
            ("200", Some(&guide)),
        ] {
            cases.push(
                render(
                    &scope,
                    (&ffmpeg, &ffprobe),
                    &source,
                    receipt,
                    seek,
                    &evidence.join(format!("{name}-{page}")),
                    &work,
                )
                .await,
            );
        }
        assert_observed_pixels(&cases, seek.is_some());
        let comparison = comparisons(&cases);
        std::fs::write(
            evidence.join(format!("{name}-comparison.json")),
            serde_json::to_vec_pretty(&comparison).unwrap(),
        )
        .unwrap();
        eprintln!("generated bitmap diagnostic {name}: {comparison}");
    }
    assert_eq!(std::fs::read(path).unwrap(), original);
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}

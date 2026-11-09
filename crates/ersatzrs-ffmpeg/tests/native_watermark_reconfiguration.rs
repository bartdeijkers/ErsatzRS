//! UCU15 row276: a graph rebuild must not consume a still watermark permanently.
use ersatzrs_domain::{HardwareAccelerationKind, WatermarkLocation, WatermarkSize};
use ersatzrs_ffmpeg::{
    build_pipeline_for_acceleration, DesiredVideoState, FrameRate, FrameSize, InputFile,
    OutputFormat, PipelineBuilder, PipelineRequest, PixelFormat, SoftwarePipelineBuilder,
    SourceVideoCodec, SourceVideoStream, WatermarkFadeKind, WatermarkFadePoint, WatermarkOverlay,
};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::Duration,
};
use tokio::process::Command;

// These explicit qualification inputs are not application configuration.
#[allow(
    clippy::disallowed_methods,
    reason = "Ignored runtime proof reads only its explicit tool/artifact inputs."
)]
fn inputs() -> (PathBuf, PathBuf, PathBuf, Option<PathBuf>) {
    let root =
        std::env::var_os("ERSATZRS_RECONFIG_ARTIFACT_DIR").expect("artifact directory required");
    let ff = std::env::var_os("ERSATZRS_NEXT_TEST_FFMPEG").expect("verified FFmpeg required");
    let probe = std::env::var_os("ERSATZRS_NEXT_TEST_FFPROBE").expect("verified ffprobe required");
    (
        root.into(),
        ff.into(),
        probe.into(),
        std::env::var_os("ERSATZRS_RECONFIG_MARK_INPUT").map(PathBuf::from),
    )
}

async fn tool(command: &mut Command, stderr: Option<&Path>) -> Output {
    command.stdin(Stdio::null()).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("bounded test tool timed out")
        .expect("start test tool");
    if let Some(path) = stderr {
        std::fs::write(path, &output.stderr).unwrap();
    }
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn request(source: &Path, mark: &Path) -> PipelineRequest {
    let mut stream = SourceVideoStream::new(SourceVideoCodec::H264);
    stream.frame_size = Some(FrameSize::new(640, 480));
    stream.frame_rate = Some(FrameRate::new(30, 1));
    stream.pixel_format = Some(PixelFormat::Yuv420P);
    stream.pixel_format_name = Some("yuv420p".into());
    stream.sample_aspect_ratio = Some("1:1".into());
    let mut desired = DesiredVideoState::h264_1080p();
    desired.frame_size = FrameSize::new(320, 180);
    desired.scaled_size = FrameSize::new(240, 180);
    desired.frame_rate = Some(FrameRate::new(30, 1));
    desired.video_bitrate = Some(800);
    desired.video_buffer_size = Some(1600);
    let mut request = PipelineRequest::new(InputFile::video(source.to_string_lossy()), desired)
        .without_audio()
        .with_source_video_stream(stream);
    request
        .inputs
        .push(InputFile::watermark(mark.to_string_lossy()));
    request.watermarks.push(WatermarkOverlay {
        location: WatermarkLocation::TopLeft,
        size: WatermarkSize::Scaled,
        width_percent: 10.0,
        horizontal_margin_percent: 5.0,
        vertical_margin_percent: 5.0,
        ..WatermarkOverlay::default()
    });
    request.state.duration = Some(Duration::from_secs(2));
    request.state.report_progress = false;
    request.state.thread_count = Some(2);
    request.state.output_format = OutputFormat::Mp4;
    request
}

fn blue_mean(frame: &[u8], x: usize) -> f64 {
    let mut sum = 0_u32;
    for y in 11..17 {
        for column in x..x + 8 {
            sum += u32::from(frame[(y * 320 + column) * 3 + 2]);
        }
    }
    f64::from(sum) / 48.0
}

async fn presentation_times(probe: &Path, output: &Path, root: &Path) -> Vec<f64> {
    let mut command = Command::new(probe);
    command
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_frames",
            "-show_entries",
            "frame=best_effort_timestamp_time",
            "-of",
            "json",
        ])
        .arg(output);
    let data = tool(&mut command, None).await.stdout;
    std::fs::write(root.join("output-probe.json"), &data).unwrap();
    let data: serde_json::Value = serde_json::from_slice(&data).unwrap();
    data["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| {
            frame["best_effort_timestamp_time"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect()
}

async fn verify_output(
    probe: &Path,
    ff: &Path,
    output: &Path,
    root: &Path,
    reference: Option<&[f64]>,
    fading: bool,
) {
    let times = presentation_times(probe, output, root).await;
    if let Some(reference) = reference {
        assert_eq!(
            times, reference,
            "watermark must preserve the unmarked Native timeline"
        );
    } else {
        assert_eq!(times.len(), 60, "complete two-second30fps output");
        assert!((times[59] - times[0] - 59.0 / 30.0).abs() < 0.002);
    }
    let mut command = Command::new(ff);
    command.args(["-v", "error", "-i"]).arg(output).args([
        "-map",
        "0:v:0",
        "-an",
        "-pix_fmt",
        "rgb24",
        "-fps_mode",
        "passthrough",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    let raw = tool(&mut command, Some(&root.join("decode-stderr.log")))
        .await
        .stdout;
    let frame_bytes = 320 * 180 * 3;
    assert_eq!(raw.len(), times.len() * frame_bytes);
    let deltas: Vec<_> = raw
        .chunks_exact(frame_bytes)
        .map(|f| blue_mean(f, 22) - blue_mean(f, 290))
        .collect();
    std::fs::write(
        root.join("watermark-blue-deltas.json"),
        serde_json::to_vec_pretty(&deltas).unwrap(),
    )
    .unwrap();
    assert!(
        deltas[0] > 150.0,
        "fixture must show the watermark before reconfiguration: {}",
        deltas[0]
    );
    for (i, delta) in deltas
        .iter()
        .enumerate()
        .filter(|(i, _)| !fading || *i < 24)
    {
        assert!(
            *delta > 150.0,
            "watermark lost in output frame{i}: blue difference{delta}"
        );
    }
    if fading {
        assert!(
            deltas[..24].iter().all(|value| *value > 230.0),
            "fade must not advance before its configured programme time"
        );
        assert!(
            *deltas.last().unwrap() < 20.0,
            "fade must finish on the programme clock"
        );
    }
}

async fn transcode(req: PipelineRequest, ff: &Path, root: &Path, name: &str) -> PathBuf {
    let mut generated = SoftwarePipelineBuilder
        .build(req.clone())
        .command(&req.inputs);
    let output = root.join(format!("{name}.mp4"));
    assert!(!output.exists(), "preserve previous runtime evidence");
    assert_eq!(generated.arguments.last().unwrap(), "pipe:1");
    *generated.arguments.last_mut().unwrap() = output.to_string_lossy().into_owned();
    std::fs::write(
        root.join(format!("{name}-command.json")),
        serde_json::to_vec_pretty(&generated.arguments).unwrap(),
    )
    .unwrap();
    let mut command = Command::new(ff);
    command.args(&generated.arguments);
    tool(&mut command, Some(&root.join(format!("{name}-stderr.log")))).await;
    output
}

async fn run_case(enforce_duration: bool, fading: bool) {
    let (root, ff, probe, mark_input) = inputs();
    let source = root.join("reinit.ts");
    let expected = [
        0x0e, 0xef, 0x01, 0x95, 0x9b, 0x08, 0x9e, 0x8e, 0x80, 0xa6, 0x88, 0x65, 0xa8, 0x05, 0xc6,
        0x76, 0x55, 0x32, 0x24, 0x8b, 0x32, 0xa9, 0xb3, 0x59, 0xcc, 0xac, 0x30, 0x95, 0x37, 0x17,
        0x91, 0xc1,
    ];
    assert_eq!(
        Sha256::digest(std::fs::read(&source).unwrap()).as_slice(),
        expected
    );
    let case = root.join(if fading {
        "fading"
    } else if enforce_duration {
        "finite"
    } else {
        "natural"
    });
    std::fs::create_dir_all(&case).unwrap();
    let mark = mark_input.unwrap_or_else(|| case.join("mark.png"));
    if !mark.exists() {
        std::fs::write(&mark, include_bytes!("fixtures/graphics/static-blue.png")).unwrap();
    }
    let mut req = request(&source, &mark);
    if !enforce_duration {
        req.state.duration = None;
    }
    if fading {
        req.inputs[1] = InputFile::watermark_looped(mark.to_string_lossy());
        req.watermarks[0].fade_points.push(WatermarkFadePoint {
            kind: WatermarkFadeKind::Out,
            time: Duration::from_millis(800),
            enable_start: Duration::from_millis(800),
            enable_finish: Duration::from_secs(2),
        });
    }
    let reference = if enforce_duration {
        None
    } else {
        let mut unmarked = req.clone();
        unmarked.inputs.pop();
        unmarked.watermarks.clear();
        let baseline = transcode(unmarked, &ff, &case, "unmarked").await;
        Some(presentation_times(&probe, &baseline, &case).await)
    };
    let output = transcode(req, &ff, &case, "native").await;
    verify_output(&probe, &ff, &output, &case, reference.as_deref(), fading).await;
}

#[tokio::test]
#[ignore = "requires verified FFmpeg9 and the exact synthetic upstream reconfiguration fixture"]
async fn native_still_watermark_survives_source_reconfiguration_for_the_complete_item() {
    run_case(true, false).await;
}

#[tokio::test]
#[ignore = "requires verified FFmpeg9 and the exact synthetic upstream reconfiguration fixture"]
async fn native_still_watermark_does_not_keep_natural_video_eof_alive() {
    run_case(false, false).await;
}

#[tokio::test]
#[ignore = "requires verified FFmpeg9 and the exact synthetic upstream reconfiguration fixture"]
async fn native_still_watermark_fade_keeps_programme_time_across_reconfiguration() {
    run_case(true, true).await;
}

#[test]
fn native_still_watermark_repetition_command_snapshot() {
    let req = request(Path::new("source.ts"), Path::new("mark.png"));
    let generated = SoftwarePipelineBuilder
        .build(req.clone())
        .command(&req.inputs);
    assert!(generated
        .arguments
        .windows(2)
        .any(|a| a == ["-stream_loop", "-1"]));
    insta::assert_snapshot!(generated.arguments.join(" "));
}

#[test]
fn native_watermark_repetition_preserves_explicit_animation_and_input_limits() {
    for input in [
        InputFile::watermark_animated("mark.gif"),
        InputFile::watermark_looped("mark.png"),
        InputFile::watermark("mark.png").with_input_options(["-stream_loop", "3"]),
    ] {
        let mut req = request(Path::new("source.ts"), Path::new("mark.png"));
        req.inputs[1] = input.clone();
        let pipeline = SoftwarePipelineBuilder.build(req);
        let selected = pipeline
            .inputs
            .iter()
            .find(|i| i.path == input.path)
            .unwrap();
        assert_eq!(selected.input_options, input.input_options);
    }
    let mut req = request(Path::new("source.ts"), Path::new("mark.png"));
    req.inputs[1]
        .input_options
        .extend(["-t".into(), "1".into()]);
    let pipeline = SoftwarePipelineBuilder.build(req);
    let input = pipeline
        .inputs
        .iter()
        .find(|i| i.path == "mark.png")
        .unwrap();
    assert_eq!(
        input
            .input_options
            .iter()
            .filter(|arg| arg.as_str() == "-t")
            .count(),
        1
    );
    assert!(!input.input_options.iter().any(|arg| arg == "-stream_loop"));
}

#[test]
fn native_vaapi_multiple_still_watermarks_preserve_natural_eof_via_existing_software_overlay() {
    let mut req = request(Path::new("source.ts"), Path::new("mark.png"));
    req.state.duration = None;
    req.inputs.push(InputFile::watermark("second.png"));
    req.watermarks.push(req.watermarks[0].clone());
    for overlay in &mut req.watermarks {
        overlay.horizontal_margin_percent = 0.0;
        overlay.vertical_margin_percent = 0.0;
    }
    let command = build_pipeline_for_acceleration(HardwareAccelerationKind::Vaapi, req.clone())
        .command(&req.inputs);
    assert!(command
        .arguments
        .windows(2)
        .any(|args| args == ["-codec:v", "h264_vaapi"]));
    let graph = &command.arguments[command
        .arguments
        .iter()
        .position(|arg| arg == "-filter_complex")
        .unwrap()
        + 1];
    assert!(!graph.contains("overlay_vaapi"), "{graph}");
    assert_eq!(graph.matches(":shortest=1").count(), 2, "{graph}");
}

#[test]
#[ignore = "exports a Native CUDA command for the verified native Windows NVIDIA device"]
fn native_cuda_reconfiguration_exports_exact_generated_command() {
    let (root, _, _, _) = inputs();
    let source = root.join("reinit.ts");
    let mark = root.join("mark.png");
    std::fs::write(&mark, include_bytes!("fixtures/graphics/static-blue.png")).unwrap();
    let mut req = request(&source, &mark);
    req.desired.video_preset = Some("p4".into());
    req.runtime_capabilities = Some(ersatzrs_ffmpeg::capability_report_for_tests());
    let mut generated =
        build_pipeline_for_acceleration(HardwareAccelerationKind::Nvenc, req.clone())
            .command(&req.inputs);
    assert!(generated
        .arguments
        .windows(2)
        .any(|args| args == ["-codec:v", "h264_nvenc"]));
    assert!(generated
        .arguments
        .iter()
        .any(|arg| arg.contains("overlay_cuda")));
    assert!(generated
        .arguments
        .windows(2)
        .any(|args| args == ["-filter_hw_device", "nv"]));
    assert!(!generated
        .arguments
        .iter()
        .any(|arg| arg.contains("hwupload_cuda")));
    let folder = root.join("native-cuda");
    std::fs::create_dir_all(&folder).unwrap();
    *generated.arguments.last_mut().unwrap() =
        folder.join("native.mp4").to_string_lossy().into_owned();
    std::fs::write(
        folder.join("native-command.json"),
        serde_json::to_vec_pretty(&generated.arguments).unwrap(),
    )
    .unwrap();
}

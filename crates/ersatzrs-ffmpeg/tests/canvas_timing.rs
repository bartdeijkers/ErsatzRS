use std::{collections::BTreeMap, path::Path, process::Command, time::Duration};

use ersatzrs_domain::{
    GraphicsElement, GraphicsElementConfig, GraphicsElementKind, MotionEndBehavior,
    MotionGraphicsConfig, ScriptGraphicsConfig, SubtitleGraphicsConfig, WatermarkLocation,
};
use ersatzrs_ffmpeg::{
    build_canvas_command, CanvasRenderRequest, FrameRate, FrameSize, GraphicsPipelineOverlay,
    GraphicsRenderContext, GraphicsTemplateRenderer, WatermarkOpacityClock,
};

fn request(root: &Path, element: GraphicsElement, offset_ms: u64) -> CanvasRenderRequest {
    request_with_source_duration(root, element, offset_ms, 2.0)
}

fn request_with_source_duration(
    root: &Path,
    element: GraphicsElement,
    offset_ms: u64,
    source_duration: f64,
) -> CanvasRenderRequest {
    let frame = FrameSize::new(64, 64);
    let context = GraphicsRenderContext::new(frame, frame)
        .with_stream_offset(Duration::from_millis(offset_ms))
        .with_stream_duration(Duration::from_secs(1))
        .with_motion_source_duration(element.id, source_duration);
    let overlays = GraphicsTemplateRenderer::new(root)
        .render_graphics_elements(&[element], &BTreeMap::new(), &context)
        .unwrap()
        .into_iter()
        .map(ersatzrs_ffmpeg::RenderedGraphicsElement::into_pipeline_overlay)
        .collect();
    CanvasRenderRequest {
        resolution: frame,
        content_size: frame,
        frame_rate: FrameRate::new(4, 1).into(),
        duration: Duration::from_secs(1),
        opacity_clock: WatermarkOpacityClock::new(
            0.0,
            Duration::from_millis(offset_ms).as_secs_f64(),
            Some(10.0),
            0.0,
        ),
        overlays,
    }
}

fn script() -> GraphicsElement {
    GraphicsElement {
        id: 1,
        name: "timed-script".into(),
        kind: GraphicsElementKind::Script,
        config: GraphicsElementConfig::Script(ScriptGraphicsConfig {
            command: Some("return 'setcolor red\\nrect 8 8 16 16 fill\\n'".into()),
            start_seconds: Some(2.0),
            duration_seconds: Some(1.0),
            ..ScriptGraphicsConfig::default()
        }),
        ..GraphicsElement::default()
    }
}

fn motion(path: &Path, behavior: MotionEndBehavior) -> GraphicsElement {
    GraphicsElement {
        id: 2,
        name: "timed-motion".into(),
        kind: GraphicsElementKind::Motion,
        location: WatermarkLocation::TopLeft,
        horizontal_margin_percent: Some(0.0),
        vertical_margin_percent: Some(0.0),
        config: GraphicsElementConfig::Motion(MotionGraphicsConfig {
            video_path: Some(path.to_string_lossy().into_owned()),
            start_seconds: Some(1.0),
            hold_seconds: Some(2.0),
            end_behavior: behavior,
        }),
        ..GraphicsElement::default()
    }
}

#[test]
fn canvas_motion_start_at_or_after_stream_finish_skips_the_input() {
    let frame = FrameSize::new(64, 64);
    let context = GraphicsRenderContext::new(frame, frame)
        .with_stream_offset(Duration::from_mins(10))
        .with_stream_duration(Duration::from_secs(1));
    for start in [601.0, 602.0] {
        let mut element = motion(Path::new("motion.nut"), MotionEndBehavior::Loop);
        if let GraphicsElementConfig::Motion(config) = &mut element.config {
            config.start_seconds = Some(start);
        }
        assert!(GraphicsTemplateRenderer::new("graphics")
            .render_graphics_elements(&[element], &BTreeMap::new(), &context)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn canvas_programme_clock_command_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let command = build_canvas_command(request(root.path(), script(), 2250)).unwrap();
    insta::assert_snapshot!(command
        .arguments
        .join("\n")
        .replace(&root.path().to_string_lossy().to_string(), "<graphics>"));
}

#[test]
fn canvas_motion_seek_and_hold_command_snapshots() {
    let root = tempfile::tempdir().unwrap();
    let mut commands = Vec::new();
    for (name, behavior, offset) in [
        ("loop", MotionEndBehavior::Loop, 4250),
        ("hold", MotionEndBehavior::Hold, 3250),
        ("near-eof", MotionEndBehavior::Hold, 2950),
        ("disappear", MotionEndBehavior::Disappear, 3250),
    ] {
        let command = build_canvas_command(request(
            root.path(),
            motion(Path::new("motion.nut"), behavior),
            offset,
        ))
        .unwrap();
        commands.push(format!("{name}:\n{}", command.arguments.join("\n")));
    }
    insta::assert_snapshot!(commands.join("\n\n"));
}

#[allow(
    clippy::disallowed_methods,
    reason = "manual package gate supplies the verified executable"
)]
fn runtime() -> std::ffi::OsString {
    std::env::var_os("ERSATZRS_FFMPEG").expect("packaged FFmpeg executable")
}

fn decode(root: &Path, request: CanvasRenderRequest) -> Vec<u8> {
    let expected_frames = request.duration.as_secs_f64() * request.frame_rate.parsed();
    let encoded = Command::new(runtime())
        .args(build_canvas_command(request).unwrap().arguments)
        .output()
        .unwrap();
    assert!(
        encoded.status.success(),
        "{}",
        String::from_utf8_lossy(&encoded.stderr)
    );
    let nut = root.join("canvas.nut");
    std::fs::write(&nut, encoded.stdout).unwrap();
    let decoded = Command::new(runtime())
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(nut)
        .args(["-pix_fmt", "bgra", "-f", "rawvideo", "pipe:1"])
        .output()
        .unwrap();
    assert!(
        decoded.status.success(),
        "{}",
        String::from_utf8_lossy(&decoded.stderr)
    );
    assert_eq!(decoded.stdout.len() % (64 * 64 * 4), 0);
    let count = u32::try_from(decoded.stdout.len() / (64 * 64 * 4)).unwrap();
    assert!((f64::from(count) - expected_frames).abs() < f64::EPSILON);
    decoded.stdout
}

#[test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
fn canvas_script_and_subtitle_use_programme_time_after_seek() {
    let root = tempfile::tempdir().unwrap();
    let ass = root.path().join("timing.ass");
    std::fs::write(&ass, "[Script Info]\nScriptType: v4.00+\nPlayResX: 64\nPlayResY: 64\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\nStyle: Default,Arial,12,&H000000FF,&H000000FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\nDialogue: 0,0:00:02.00,0:00:03.00,Default,,0,0,0,,{\\an7\\pos(8,8)\\p1}m 0 0 l 16 0 16 16 0 16\n").unwrap();
    let subtitle = GraphicsElement {
        id: 3,
        name: "timed-subtitle".into(),
        kind: GraphicsElementKind::Subtitle,
        config: GraphicsElementConfig::Subtitle(SubtitleGraphicsConfig {
            template: Some(ass.to_string_lossy().into_owned()),
        }),
        ..GraphicsElement::default()
    };
    for element in [script(), subtitle] {
        for (offset, visible) in [(1000, false), (2000, true), (3250, false)] {
            let frames = decode(root.path(), request(root.path(), element.clone(), offset));
            for frame in frames.chunks_exact(64 * 64 * 4) {
                let alpha = frame[(12 * 64 + 12) * 4 + 3];
                assert_eq!(
                    alpha > 240,
                    visible,
                    "kind {:?}, offset {offset}, alpha {alpha}",
                    element.kind
                );
            }
        }
    }
}

#[test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
fn canvas_motion_continues_loop_hold_and_disappear_after_seek() {
    let root = tempfile::tempdir().unwrap();
    let source = motion_source(root.path());
    for (behavior, offset, expected) in [
        (
            MotionEndBehavior::Disappear,
            250,
            ["none", "none", "none", "red"],
        ),
        (
            MotionEndBehavior::Disappear,
            2250,
            ["blue", "blue", "blue", "none"],
        ),
        (MotionEndBehavior::Disappear, 3250, ["none"; 4]),
        (
            MotionEndBehavior::Loop,
            4250,
            ["blue", "blue", "blue", "red"],
        ),
        (MotionEndBehavior::Hold, 3250, ["blue"; 4]),
        (MotionEndBehavior::Hold, 5250, ["none"; 4]),
    ] {
        let frames = decode(
            root.path(),
            request(root.path(), motion(&source, behavior), offset),
        );
        for (frame, expected) in frames.chunks_exact(64 * 64 * 4).zip(expected) {
            let pixel = &frame[(4 * 64 + 4) * 4..(4 * 64 + 4) * 4 + 4];
            match expected {
                "none" => assert_eq!(pixel[3], 0, "{behavior:?} at {offset}: {pixel:?}"),
                "red" => assert!(
                    pixel[2] > 240 && pixel[0] < 10 && pixel[3] == 255,
                    "{behavior:?} at {offset}: {pixel:?}"
                ),
                _ => assert!(
                    pixel[0] > 240 && pixel[2] < 10 && pixel[3] == 255,
                    "{behavior:?} at {offset}: {pixel:?}"
                ),
            }
        }
    }
}

#[test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
fn canvas_motion_hold_recovers_near_eof_and_stops_at_exact_expiry() {
    let root = tempfile::tempdir().unwrap();
    let source = motion_source(root.path());
    for (offset, alpha) in [(2950, 255), (2990, 255), (3000, 255), (5000, 0)] {
        let frames = decode(
            root.path(),
            request(
                root.path(),
                motion(&source, MotionEndBehavior::Hold),
                offset,
            ),
        );
        for frame in frames.chunks_exact(64 * 64 * 4) {
            let pixel = &frame[(4 * 64 + 4) * 4..(4 * 64 + 4) * 4 + 4];
            assert_eq!(pixel[3], alpha, "Hold at {offset}: {pixel:?}");
            if alpha > 0 {
                assert!(
                    pixel[0] > 240 && pixel[2] < 10,
                    "Hold at {offset}: {pixel:?}"
                );
            }
        }
    }
    for behavior in [MotionEndBehavior::Hold, MotionEndBehavior::Disappear] {
        let mut element = motion(&source, behavior);
        if let GraphicsElementConfig::Motion(config) = &mut element.config {
            config.hold_seconds = Some(0.0);
        }
        let frames = decode(root.path(), request(root.path(), element, 2250));
        let alphas: Vec<_> = frames
            .chunks_exact(64 * 64 * 4)
            .map(|f| f[(4 * 64 + 4) * 4 + 3])
            .collect();
        assert_eq!(
            alphas,
            [255, 255, 255, 0],
            "{behavior:?} must stop at source EOF"
        );
    }
}

#[test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
fn canvas_motion_hold_preserves_phase_when_clamped_seek_falls_between_source_frames() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("fractional-clamp.nut");
    let generated = Command::new(runtime())
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=16x16:r=30000/1001,format=bgra",
            "-vf",
            "drawbox=color=blue:t=fill:enable='gte(n,46)'",
            "-frames:v",
            "60",
            "-c:v",
            "ffv1",
            "-f",
            "nut",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    // A 2.002s NTSC source clamps input seek to 1.002s. Its first decoded
    // timestamp is positive; keep that timestamp until the 1.5s phase trim.
    let frames = decode(
        root.path(),
        request_with_source_duration(
            root.path(),
            motion(&source, MotionEndBehavior::Hold),
            2500,
            2.002,
        ),
    );
    let first = &frames[(4 * 64 + 4) * 4..(4 * 64 + 4) * 4 + 4];
    assert_eq!(
        first,
        [0, 0, 255, 255],
        "phase 1.5 must retain source frame45, before the blue frame46"
    );
    let later = &frames[64 * 64 * 4 + (4 * 64 + 4) * 4..64 * 64 * 4 + (4 * 64 + 4) * 4 + 4];
    assert_eq!(later, [255, 0, 0, 255]);
}

#[test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
fn canvas_motion_loop_keeps_full_cycles_after_a_partial_first_cycle() {
    let root = tempfile::tempdir().unwrap();
    let source = motion_source(root.path());
    let mut request = request(root.path(), motion(&source, MotionEndBehavior::Loop), 4250);
    request.duration = Duration::from_secs(3);
    let frames = decode(root.path(), request);
    let colors: Vec<_> = frames
        .chunks_exact(64 * 64 * 4)
        .map(|f| {
            let pixel = &f[(4 * 64 + 4) * 4..(4 * 64 + 4) * 4 + 4];
            assert_eq!(pixel[3], 255);
            if pixel[0] > 240 {
                "blue"
            } else {
                assert!(pixel[2] > 240);
                "red"
            }
        })
        .collect();
    assert_eq!(
        colors,
        [
            "blue", "blue", "blue", "red", "red", "red", "red", "blue", "blue", "blue", "blue",
            "red"
        ]
    );
}

#[test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
fn canvas_watermark_fade_resumes_midway_through_programme_fade() {
    use ersatzrs_ffmpeg::{
        InputFile, InputKind, WatermarkFadeKind, WatermarkFadePoint, WatermarkOverlay,
    };
    let root = tempfile::tempdir().unwrap();
    let mut request = request(root.path(), script(), 2250);
    request.overlays = vec![GraphicsPipelineOverlay::native_image(
        0,
        InputFile::new(
            InputKind::GraphicsImage,
            "color=red:s=16x16:r=4,format=bgra",
            0,
        )
        .with_input_options(["-f", "lavfi"]),
        WatermarkOverlay {
            location: WatermarkLocation::TopLeft,
            horizontal_margin_percent: 0.0,
            vertical_margin_percent: 0.0,
            fade_points: vec![WatermarkFadePoint {
                kind: WatermarkFadeKind::In,
                time: Duration::from_secs(2),
                enable_start: Duration::ZERO,
                enable_finish: Duration::from_secs(5),
            }],
            ..WatermarkOverlay::default()
        },
    )];
    let frames = decode(root.path(), request);
    for (frame, expected) in frames.chunks_exact(64 * 64 * 4).zip([64_u8, 128, 191, 255]) {
        let alpha = frame[(4 * 64 + 4) * 4 + 3];
        assert!(
            alpha.abs_diff(expected) <= 2,
            "expected {expected}, got {alpha}"
        );
    }
}

fn motion_source(root: &Path) -> std::path::PathBuf {
    let source = root.join("motion.nut");
    let generated = Command::new(runtime())
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=16x16:r=4:d=1,format=bgra",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=16x16:r=4:d=1,format=bgra",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0[v]",
            "-map",
            "[v]",
            "-c:v",
            "ffv1",
            "-f",
            "nut",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    source
}

#[test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
fn canvas_motion_fractional_seek_starts_with_an_opaque_frame() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("fractional.nut");
    let generated = Command::new(runtime())
        .args([
            "-nostdin",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=16x16:r=25:d=2,format=bgra",
            "-c:v",
            "ffv1",
            "-f",
            "nut",
        ])
        .arg(&source)
        .output()
        .unwrap();
    assert!(generated.status.success());
    // The motion starts at programme second one; 1.5 means a 0.5-second seek,
    // halfway between the 25 fps source frames at 0.48 and 0.52 seconds.
    for behavior in [MotionEndBehavior::Loop, MotionEndBehavior::Disappear] {
        let frames = decode(
            root.path(),
            request(root.path(), motion(&source, behavior), 1500),
        );
        for frame in frames.chunks_exact(64 * 64 * 4) {
            let pixel = &frame[(4 * 64 + 4) * 4..(4 * 64 + 4) * 4 + 4];
            assert_eq!(
                pixel,
                [0, 0, 255, 255],
                "sought motion must cover the first canvas frame"
            );
        }
    }
}

#[test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
fn canvas_shared_motion_asset_keeps_independent_layer_seek_positions() {
    let root = tempfile::tempdir().unwrap();
    let source = motion_source(root.path());
    let mut first = request(root.path(), motion(&source, MotionEndBehavior::Loop), 2250);
    let mut later = motion(&source, MotionEndBehavior::Loop);
    later.id = 4;
    later.horizontal_margin_percent = Some(50.0);
    if let GraphicsElementConfig::Motion(config) = &mut later.config {
        config.start_seconds = Some(2.0);
    }
    first
        .overlays
        .extend(request(root.path(), later, 2250).overlays);
    let frames = decode(root.path(), first);
    let frame = &frames[..64 * 64 * 4];
    let early = &frame[(4 * 64 + 4) * 4..(4 * 64 + 4) * 4 + 4];
    let late = &frame[(4 * 64 + 36) * 4..(4 * 64 + 36) * 4 + 4];
    assert!(
        early[0] > 240 && early[2] < 10,
        "earlier layer must be blue: {early:?}"
    );
    assert!(
        late[2] > 240 && late[0] < 10,
        "later layer must be red: {late:?}"
    );
}

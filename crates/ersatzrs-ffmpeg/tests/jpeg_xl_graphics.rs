use ersatzrs_domain::{
    GraphicsElement, GraphicsElementConfig, GraphicsElementKind, ImageGraphicsConfig,
    MotionEndBehavior, MotionGraphicsConfig, WatermarkLocation,
};
use ersatzrs_ffmpeg::{
    build_canvas_command, CanvasRenderRequest, FrameRate, FrameSize, GraphicsRenderContext,
    GraphicsTemplateRenderer, JpegXlTiming, WatermarkOpacityClock,
};
use std::{collections::BTreeMap, path::Path, time::Duration};

fn request(
    root: &Path,
    image: bool,
    behavior: MotionEndBehavior,
    offset: f64,
    expression: Option<&str>,
) -> CanvasRenderRequest {
    request_with_timing(
        root,
        image,
        behavior,
        offset,
        expression,
        JpegXlTiming {
            frame_count: 5,
            duration_seconds: 1.0,
            last_frame_seconds: 0.8,
            frame_timestamps: vec![0.0, 0.2, 0.4, 0.6, 0.8],
        },
    )
}

fn request_with_timing(
    root: &Path,
    image: bool,
    behavior: MotionEndBehavior,
    offset: f64,
    expression: Option<&str>,
    timing: JpegXlTiming,
) -> CanvasRenderRequest {
    let source = root.join("animated plate's.jxl");
    let element = GraphicsElement {
        id: 9,
        name: "JPEG XL plate".into(),
        kind: if image {
            GraphicsElementKind::Image
        } else {
            GraphicsElementKind::Motion
        },
        location: WatermarkLocation::TopLeft,
        horizontal_margin_percent: Some(0.0),
        vertical_margin_percent: Some(0.0),
        opacity_expression: expression.map(str::to_owned),
        config: if image {
            GraphicsElementConfig::Image(ImageGraphicsConfig {
                image: Some(source.to_string_lossy().into_owned()),
            })
        } else {
            GraphicsElementConfig::Motion(MotionGraphicsConfig {
                video_path: Some(source.to_string_lossy().into_owned()),
                start_seconds: Some(0.0),
                hold_seconds: Some(2.0),
                end_behavior: behavior,
            })
        },
        ..Default::default()
    };
    let frame = FrameSize::new(64, 32);
    let context = GraphicsRenderContext::new(frame, frame)
        .with_stream_offset(Duration::from_secs_f64(offset))
        .with_stream_duration(Duration::from_secs(2))
        .with_jpeg_xl_timing(9, timing);
    let overlays = GraphicsTemplateRenderer::new(root)
        .render_graphics_elements(&[element], &BTreeMap::new(), &context)
        .unwrap()
        .into_iter()
        .map(ersatzrs_ffmpeg::RenderedGraphicsElement::into_pipeline_overlay)
        .collect();
    CanvasRenderRequest {
        resolution: frame,
        content_size: frame,
        frame_rate: FrameRate::new(5, 1).into(),
        duration: Duration::from_secs(2),
        opacity_clock: WatermarkOpacityClock::new(0.0, offset, Some(10.0), 0.0),
        overlays,
    }
}

fn normalized(value: &str, root: &Path) -> String {
    let raw = root.to_string_lossy();
    value
        .replace(raw.as_ref(), "<graphics>")
        .replace(&raw.replace('\\', "/"), "<graphics>")
        .replace(r"\\?\<graphics>", "<graphics>")
        .replace("//?/<graphics>", "<graphics>")
        .replace(r"<graphics>\", "<graphics>/")
}

#[test]
fn jpeg_xl_native_repeat_seek_and_alpha_command_snapshots() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("animated plate's.jxl"), [0xff, 0x0a]).unwrap();
    let mut cases = Vec::new();
    for (name, image, behavior, expression) in [
        ("motion loop", false, MotionEndBehavior::Loop, None),
        ("image loop", true, MotionEndBehavior::Loop, None),
        (
            "boolean gate",
            false,
            MotionEndBehavior::Loop,
            Some("content_seconds >= 2.4 && content_seconds < 3"),
        ),
        (
            "constant alpha",
            false,
            MotionEndBehavior::Loop,
            Some("0.5"),
        ),
        (
            "continuous alpha",
            false,
            MotionEndBehavior::Loop,
            Some("LinearFadeDuration(content_seconds, 2, 1, 2)"),
        ),
    ] {
        let command =
            build_canvas_command(request(root.path(), image, behavior, 2.4, expression)).unwrap();
        let text = normalized(&command.arguments.join("\n"), root.path());
        assert!(!command.arguments.iter().any(|arg| [
            "-ss",
            "-loop",
            "-stream_loop",
            "-ignore_loop"
        ]
        .contains(&arg.as_str())));
        if matches!(name, "constant alpha" | "boolean gate") {
            assert!(!text.contains("geq="));
        }
        cases.push(format!("{name}:\n{text}"));
    }
    let manifest = normalized(
        &std::fs::read_to_string(root.path().join("9-jpeg-xl.ffconcat")).unwrap(),
        root.path(),
    );
    insta::assert_snapshot!(format!("{}\n\nmanifest:\n{manifest}", cases.join("\n\n")));
}

#[allow(
    clippy::disallowed_methods,
    reason = "native package acceptance supplies the exact verified FFmpeg executable"
)]
async fn run(root: &Path, args: &[String]) -> Vec<u8> {
    use std::process::Stdio;
    let stdout = root.join("native-output.bin");
    let stderr = root.join("native-stderr.txt");
    let mut child = tokio::process::Command::new(
        std::env::var_os("ERSATZRS_FFMPEG").expect("verified FFmpeg 9"),
    )
    .args(args)
    .stdin(Stdio::null())
    .stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
    .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()))
    .kill_on_drop(true)
    .spawn()
    .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(15), child.wait()).await;
    if result.is_err() {
        let _ = child.kill().await;
        let _ = child.wait().await;
        panic!("native JPEG XL command exceeded 15 seconds");
    }
    assert!(
        result.unwrap().unwrap().success(),
        "{}",
        std::fs::read_to_string(stderr).unwrap()
    );
    std::fs::read(stdout).unwrap()
}

async fn encode_jpeg_xl_source(root: &Path, frame_rate: &str) {
    let mut rgba = Vec::new();
    for (index, alpha) in [0, 64, 128, 192, 255].into_iter().enumerate() {
        for _ in 0..64 * 64 {
            rgba.extend([
                if alpha == 0 { 0 } else { 255 },
                u8::try_from(index).unwrap() * 10,
                0,
                alpha,
            ]);
        }
    }
    let raw = root.join("source.rgba");
    std::fs::write(&raw, &rgba).unwrap();
    let source = root.join("animated plate's.jxl");
    let args = [
        "-nostdin",
        "-v",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        "64x64",
        "-framerate",
        frame_rate,
        "-i",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain([raw.to_string_lossy().into_owned()])
    .chain(
        [
            "-c:v",
            "libjxl_anim",
            "-effort",
            "1",
            "-distance",
            "0",
            "-f",
            "image2pipe",
        ]
        .into_iter()
        .map(str::to_owned),
    )
    .chain([source.to_string_lossy().into_owned()])
    .collect::<Vec<_>>();
    run(root, &args).await;
    let args = vec![
        "-v".into(),
        "error".into(),
        "-i".into(),
        source.to_string_lossy().into_owned(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ];
    assert_eq!(run(root, &args).await, rgba);
}

async fn decode_canvas(root: &Path, request: CanvasRenderRequest) -> Vec<u8> {
    let command = build_canvas_command(request).unwrap();
    decode_canvas_command(root, &command).await
}

async fn decode_canvas_command(root: &Path, command: &ersatzrs_ffmpeg::FfmpegCommand) -> Vec<u8> {
    let encoded = run(root, &command.arguments).await;
    let nut = root.join("canvas.nut");
    std::fs::write(&nut, encoded).unwrap();
    let args = vec![
        "-v".into(),
        "error".into(),
        "-i".into(),
        nut.to_string_lossy().into_owned(),
        "-pix_fmt".into(),
        "rgba".into(),
        "-f".into(),
        "rawvideo".into(),
        "pipe:1".into(),
    ];
    run(root, &args).await
}

#[tokio::test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
async fn jpeg_xl_native_decoding_repeats_phase_and_preserves_source_alpha() {
    let root = tempfile::tempdir().unwrap();
    encode_jpeg_xl_source(root.path(), "5").await;
    for (image, behavior, offset, expression, expected) in [
        (
            false,
            MotionEndBehavior::Loop,
            2.4,
            None,
            vec![128_u8, 192, 255, 0, 64, 128, 192, 255, 0, 64],
        ),
        (
            true,
            MotionEndBehavior::Loop,
            2.4,
            None,
            vec![128_u8, 192, 255, 0, 64, 128, 192, 255, 0, 64],
        ),
        (
            false,
            MotionEndBehavior::Hold,
            1.2,
            None,
            vec![255, 255, 255, 255, 255, 255, 255, 255, 255, 0],
        ),
        (false, MotionEndBehavior::Disappear, 1.2, None, vec![0; 10]),
        (
            false,
            MotionEndBehavior::Loop,
            2.4,
            Some("0.5"),
            vec![64, 96, 128, 0, 32, 64, 96, 128, 0, 32],
        ),
        (
            false,
            MotionEndBehavior::Loop,
            2.4,
            Some("0.5 + 0 * content_seconds"),
            vec![64, 96, 128, 0, 32, 64, 96, 128, 0, 32],
        ),
        (
            false,
            MotionEndBehavior::Loop,
            2.4,
            Some("content_seconds < 3"),
            vec![128, 192, 255, 0, 0, 0, 0, 0, 0, 0],
        ),
    ] {
        let decoded = decode_canvas(
            root.path(),
            request(root.path(), image, behavior, offset, expression),
        )
        .await;
        let alphas = decoded
            .chunks_exact(64 * 32 * 4)
            .map(|frame| frame[3])
            .collect::<Vec<_>>();
        assert_eq!(alphas.len(), expected.len());
        for (actual, expected) in alphas.iter().zip(&expected) {
            assert!(i16::from(*actual).abs_diff(i16::from(*expected)) <= 1, "{behavior:?} image={image} offset={offset} expression={expression:?}: {alphas:?} != {expected:?}");
        }
    }
    // Fractional joins retain the in-progress frame and switch on its actual
    // timestamp, including the final frame before the repeat boundary.
    for (offset, expected) in [
        (
            0.45,
            vec![
                128_u8, 128, 128, 128, 192, 192, 192, 192, 192, 255, 255, 255, 255,
            ],
        ),
        (0.95, vec![255, 255, 0, 0, 0, 0, 0, 64, 64, 64, 64, 64, 128]),
    ] {
        let mut canvas = request(root.path(), false, MotionEndBehavior::Loop, offset, None);
        canvas.frame_rate = FrameRate::new(25, 1).into();
        canvas.duration = Duration::from_millis(500);
        let decoded = decode_canvas(root.path(), canvas).await;
        let actual = decoded
            .chunks_exact(64 * 32 * 4)
            .map(|frame| frame[3])
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "fractional join at {offset}");
    }
}

#[tokio::test]
#[ignore = "requires the exact packaged FFmpeg 9 runtime"]
async fn jpeg_xl_native_24fps_join_keeps_microsecond_frame_timing() {
    let root = tempfile::tempdir().unwrap();
    encode_jpeg_xl_source(root.path(), "24").await;
    let mut canvas = request_with_timing(
        root.path(),
        false,
        MotionEndBehavior::Loop,
        0.05,
        None,
        JpegXlTiming {
            frame_count: 5,
            duration_seconds: 0.208_334,
            last_frame_seconds: 0.166_667,
            frame_timestamps: vec![0.0, 0.041_667, 0.083_333, 0.125, 0.166_667],
        },
    );
    canvas.frame_rate = FrameRate::new(25, 1).into();
    canvas.duration = Duration::from_millis(200);
    let command = build_canvas_command(canvas).unwrap();
    assert!(command
        .arguments
        .join(" ")
        .contains("trim=start=0.041667,setpts=PTS-0.05/TB"));
    let decoded = decode_canvas_command(root.path(), &command).await;
    let alpha = decoded
        .chunks_exact(64 * 32 * 4)
        .map(|frame| frame[3])
        .collect::<Vec<_>>();
    assert_eq!(alpha, [64, 128, 192, 255, 0]);
}

#[test]
fn jpeg_xl_numbered_delivery_retains_image2_options() {
    for source in ["plates/frame_%04d.jxl", "plates/frame_%d.JXL"] {
        assert!(!ersatzrs_ffmpeg::image_file_is_jpeg_xl(Path::new(source)));
        assert_eq!(
            ersatzrs_ffmpeg::InputFile::graphics_motion(source, true).input_options,
            ["-stream_loop", "-1"]
        );
        assert_eq!(
            ersatzrs_ffmpeg::InputFile::graphics_image(source).input_options,
            ["-loop", "1"]
        );
    }
    for source in ["plates/compact.jxl", "plates/100%complete.JXL"] {
        assert!(ersatzrs_ffmpeg::image_file_is_jpeg_xl(Path::new(source)));
        assert!(ersatzrs_ffmpeg::InputFile::graphics_image(source)
            .input_options
            .is_empty());
    }
}

//! UCU-10 row213: APNG graphics use the animation demuxer's loop option.

#[macro_use]
#[path = "../../../tests/support/manifest_dir.rs"]
mod test_manifest;

use ersatzrs_ffmpeg::{image_file_is_animated, InputFile};

fn fixture(name: &str) -> std::path::PathBuf {
    test_manifest_dir!()
        .join("tests/fixtures/graphics")
        .join(name)
}

#[test]
fn apng_graphics_detects_animation_even_with_png_extension() {
    let path = fixture("animated-two-colour.png");
    assert!(image_file_is_animated(&path));
    let input = InputFile::graphics_image(path.to_string_lossy());
    assert!(
        input.input_options.is_empty(),
        "bounded renderer supplies APNG repetition"
    );
}

#[test]
fn apng_graphics_static_png_keeps_the_image2_loop_option() {
    let path = fixture("static-blue.png");
    assert!(!image_file_is_animated(&path));
    assert_eq!(
        InputFile::graphics_image(path.to_string_lossy()).input_options,
        ["-loop", "1"]
    );
}

#[test]
fn apng_graphics_rejects_truncated_and_invalid_animation_control() {
    let bytes = std::fs::read(fixture("animated-two-colour.png")).unwrap();
    let control = bytes.windows(4).position(|x| x == b"acTL").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("untrusted.png");
    for cut in [0, 8, 16, control, control + 8, control + 15] {
        std::fs::write(&path, &bytes[..cut]).unwrap();
        assert!(!image_file_is_animated(&path), "truncated at {cut}");
    }
    let mut zero_frames = bytes.clone();
    zero_frames[control + 4..control + 8].fill(0);
    std::fs::write(&path, zero_frames).unwrap();
    assert!(!image_file_is_animated(&path));

    let mut wrong_length = bytes.clone();
    wrong_length[control - 4..control].copy_from_slice(&7_u32.to_be_bytes());
    std::fs::write(&path, wrong_length).unwrap();
    assert!(!image_file_is_animated(&path));

    let mut oversized = bytes[..8].to_vec();
    oversized.extend_from_slice(&u32::MAX.to_be_bytes());
    oversized.extend_from_slice(b"tEXtacTL");
    std::fs::write(&path, oversized).unwrap();
    assert!(!image_file_is_animated(&path));
}

#[test]
fn apng_graphics_does_not_treat_embedded_marker_or_late_control_as_animation() {
    let bytes = std::fs::read(fixture("static-blue.png")).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("misleading.apng");
    let mut with_metadata = bytes[..33].to_vec(); // Complete signature + IHDR.
    with_metadata.extend_from_slice(&4_u32.to_be_bytes());
    with_metadata.extend_from_slice(b"tEXtacTL\0\0\0\0");
    with_metadata.extend_from_slice(&bytes[33..]);
    std::fs::write(&path, &with_metadata).unwrap();
    assert!(!image_file_is_animated(&path));

    // An acTL after image data cannot identify a valid APNG (PNG section 11.3.6.1).
    with_metadata.extend_from_slice(&8_u32.to_be_bytes());
    with_metadata.extend_from_slice(b"acTL\0\0\0\x02\0\0\0\0\0\0\0\0");
    std::fs::write(&path, with_metadata).unwrap();
    assert!(!image_file_is_animated(&path));
}

fn rendered_request(name: &str, root: &std::path::Path) -> ersatzrs_ffmpeg::CanvasRenderRequest {
    use ersatzrs_domain::{
        GraphicsElement, GraphicsElementConfig, GraphicsElementKind, ImageGraphicsConfig,
        WatermarkLocation,
    };
    use ersatzrs_ffmpeg::{
        FrameRate, FrameSize, GraphicsRenderContext, GraphicsTemplateRenderer, ImageTiming,
        WatermarkOpacityClock,
    };
    let element = GraphicsElement {
        id: 9,
        name: "APNG plate".into(),
        kind: GraphicsElementKind::Image,
        location: WatermarkLocation::TopLeft,
        horizontal_margin_percent: Some(0.0),
        vertical_margin_percent: Some(0.0),
        config: GraphicsElementConfig::Image(ImageGraphicsConfig {
            image: Some(fixture(name).to_string_lossy().into_owned()),
        }),
        ..Default::default()
    };
    let frame = FrameSize::new(32, 32);
    let context = GraphicsRenderContext::new(frame, frame)
        .with_stream_duration(std::time::Duration::from_secs(4))
        .with_image_timing(
            9,
            ImageTiming {
                frame_count: 4,
                duration_seconds: 2.0,
                last_frame_seconds: 1.5,
                frame_timestamps: vec![0.0, 0.5, 1.0, 1.5],
            },
        );
    let overlays = GraphicsTemplateRenderer::new(root)
        .render_graphics_elements(&[element], &std::collections::BTreeMap::new(), &context)
        .unwrap()
        .into_iter()
        .map(ersatzrs_ffmpeg::RenderedGraphicsElement::into_pipeline_overlay)
        .collect();
    ersatzrs_ffmpeg::CanvasRenderRequest {
        resolution: frame,
        content_size: frame,
        frame_rate: FrameRate::new(2, 1).into(),
        duration: std::time::Duration::from_secs(4),
        opacity_clock: WatermarkOpacityClock::new(0.0, 0.0, Some(4.0), 0.0),
        overlays,
    }
}

#[test]
fn apng_graphics_canvas_uses_bounded_reopening_instead_of_generic_stream_loop() {
    let root = tempfile::tempdir().unwrap();
    let request = rendered_request("animated-two-colour-finite.png", root.path());
    let command = ersatzrs_ffmpeg::build_canvas_command(request).unwrap();
    let normalized = command
        .arguments
        .join(" ")
        .replace(root.path().to_string_lossy().as_ref(), "<graphics>")
        .replace("<graphics>\\", "<graphics>/");
    insta::assert_snapshot!("apng_graphics_canvas_command", normalized);
    let manifest = std::fs::read_to_string(root.path().join("9-apng.ffconcat")).unwrap();
    assert_eq!(manifest.matches("option ignore_loop 1").count(), 3);
    assert_eq!(manifest.matches("duration 2").count(), 3);
    assert!(!command
        .arguments
        .iter()
        .any(|argument| argument == "-stream_loop"));
}

#[tokio::test]
#[ignore = "requires checksum-verified FFmpeg9 via ERSATZRS_NEXT_TEST_FFMPEG"]
#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in synthetic runtime tool path."
)]
async fn apng_graphics_generated_canvas_repeats_finite_and_infinite_colours() {
    let ffmpeg = std::env::var_os("ERSATZRS_NEXT_TEST_FFMPEG")
        .expect("provide the verified FFmpeg9 test runtime");
    for name in ["animated-two-colour.png", "animated-two-colour-finite.png"] {
        let root = tempfile::tempdir().unwrap();
        let request = rendered_request(name, root.path());
        let args = ersatzrs_ffmpeg::build_canvas_command(request).unwrap();
        let mut command = tokio::process::Command::new(&ffmpeg);
        command.args(args.arguments).kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(20), command.output())
            .await
            .expect("bounded APNG canvas decode")
            .expect("start test runtime");
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let encoded = root.path().join("canvas.nut");
        std::fs::write(&encoded, output.stdout).unwrap();
        let mut decode = tokio::process::Command::new(&ffmpeg);
        decode
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(encoded)
            .args(["-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"])
            .kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(20), decode.output())
            .await
            .expect("bounded encoded canvas decode")
            .expect("start decoder");
        assert!(output.status.success(), "decode the actual FFV1/NUT canvas");
        let frame_bytes = 32 * 32 * 4;
        assert_eq!(
            output.stdout.len(),
            8 * frame_bytes,
            "four-second canvas for {name}"
        );
        let frames: Vec<_> = output.stdout.chunks_exact(frame_bytes).collect();
        assert_eq!(
            &frames[..4],
            &frames[4..],
            "complete cycle repetition for {name}"
        );
        assert!(
            frames[0][2] > 200 && frames[0][0] < 20,
            "first frame is blue"
        );
        assert!(
            frames[2][0] > 200 && frames[2][2] < 20,
            "third frame is red"
        );
    }
}

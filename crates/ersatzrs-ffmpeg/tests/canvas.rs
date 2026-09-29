use std::time::Duration;

use ersatzrs_domain::{WatermarkLocation, WatermarkSize};
use ersatzrs_ffmpeg::{
    build_canvas_command, CanvasRenderRequest, FrameRate, FrameSize, GraphicsPipelineOverlay,
    InputFile, InputKind, WatermarkOpacityClock, WatermarkOverlay,
};

fn request() -> CanvasRenderRequest {
    CanvasRenderRequest {
        resolution: FrameSize::new(64, 64),
        content_size: FrameSize::new(64, 32),
        frame_rate: FrameRate::new(4, 1).into(),
        duration: Duration::from_secs(1),
        opacity_clock: WatermarkOpacityClock::default(),
        overlays: Vec::new(),
    }
}

fn image(color: &str, z_index: i32, opacity: u8) -> GraphicsPipelineOverlay {
    GraphicsPipelineOverlay::native_image(
        z_index,
        InputFile::new(
            InputKind::GraphicsImage,
            format!("color=c={color}:s=16x16:r=4,format=rgba"),
            0,
        )
        .with_input_options(["-f", "lavfi"]),
        WatermarkOverlay {
            location: WatermarkLocation::TopLeft,
            size: WatermarkSize::ActualSize,
            horizontal_margin_percent: 0.0,
            vertical_margin_percent: 0.0,
            place_within_source_content: true,
            opacity,
            ..WatermarkOverlay::default()
        },
    )
}

#[test]
fn canvas_transparent_command_snapshot() {
    // ADR 0105: a finite FFmpeg producer replaces the upstream raw-frame renderer.
    let command = build_canvas_command(request()).unwrap();
    insta::assert_snapshot!(command.arguments.join("\n"));
}

#[test]
fn canvas_decimal_rate_command_snapshot() {
    let mut request = request();
    request.frame_rate = ersatzrs_ffmpeg::GraphicsFrameRate::parse("29.97").unwrap();
    let command = build_canvas_command(request).unwrap();
    insta::assert_snapshot!(command.arguments.join("\n"));
}

#[test]
fn canvas_ordered_graphics_command_snapshot() {
    let mut request = request();
    request.overlays = vec![image("red", 2, 50), image("blue", 1, 100)];
    let command = build_canvas_command(request).unwrap();
    insta::assert_snapshot!(command.arguments.join("\n"));
}

#[test]
fn canvas_rejects_invalid_render_geometry_and_clock() {
    let mut invalid = request();
    invalid.resolution.width = 0;
    assert!(build_canvas_command(invalid).is_err());
    let mut invalid = request();
    invalid.content_size.height = 0;
    assert!(build_canvas_command(invalid).is_err());
    let mut invalid = request();
    invalid.frame_rate = FrameRate::new(4, 0).into();
    assert!(build_canvas_command(invalid).is_err());
    let mut invalid = request();
    invalid.duration = Duration::ZERO;
    assert!(build_canvas_command(invalid).is_err());
}

#[test]
#[ignore = "requires a packaged FFmpeg 9 runtime"]
#[allow(
    clippy::disallowed_methods,
    reason = "the package gate receives the exact runtime from its runner"
)]
fn canvas_packaged_runtime_preserves_alpha_order_geometry_and_duration() {
    let ffmpeg = std::env::var_os("ERSATZRS_FFMPEG").expect("packaged FFmpeg executable");
    for mode in 0..3 {
        let mut request = request();
        request.overlays = match mode {
            0 => vec![],
            1 => vec![image("red", 0, 50)],
            _ => vec![image("red", 2, 50), image("blue", 1, 100)],
        };
        let command = build_canvas_command(request).unwrap();
        let encoded = std::process::Command::new(&ffmpeg)
            .args(command.arguments)
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("canvas.nut");
        std::fs::write(&path, encoded.stdout).unwrap();
        let decoded = std::process::Command::new(&ffmpeg)
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args(["-pix_fmt", "bgra", "-f", "rawvideo", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            decoded.status.success(),
            "{}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        assert_eq!(
            decoded.stdout.len(),
            64 * 64 * 4 * 4,
            "one second at four frames per second"
        );
        for frame in decoded.stdout.chunks_exact(64 * 64 * 4) {
            assert_eq!(frame[3], 0, "top padding must remain transparent");
            assert_eq!(frame[(63 * 64 + 63) * 4 + 3], 0, "background alpha");
            let pixel = &frame[(20 * 64 + 4) * 4..(20 * 64 + 4) * 4 + 4];
            match mode {
                0 => assert_eq!(pixel[3], 0),
                1 => {
                    assert!((126..=129).contains(&pixel[3]), "half opacity: {pixel:?}");
                    assert!(
                        pixel[2] > 245 && pixel[0] < 5,
                        "red straight alpha: {pixel:?}"
                    );
                }
                _ => {
                    assert_eq!(pixel[3], 255);
                    assert!(
                        (120..=135).contains(&pixel[0]) && (120..=135).contains(&pixel[2]),
                        "red above blue: {pixel:?}"
                    );
                }
            }
        }
    }
}

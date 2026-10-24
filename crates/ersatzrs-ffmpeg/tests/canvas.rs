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
fn canvas_region_command_snapshot() {
    let mut request = request();
    request.overlays = vec![image("red", 2, 50), image("blue", 1, 100)];
    let command = ersatzrs_ffmpeg::build_canvas_region_command(
        request,
        ersatzrs_ffmpeg::CanvasRegion {
            x: 4,
            y: 12,
            width: 24,
            height: 24,
        },
    )
    .unwrap();
    insta::assert_snapshot!(command.arguments.join("\n"));
}

#[test]
fn canvas_region_measures_even_union_padding_and_unmeasurable_fallbacks() {
    use ersatzrs_ffmpeg::CanvasRegion;
    let frame = FrameSize::new(1920, 1080);
    let image = FrameSize::new(301, 151);
    let mut overlay = WatermarkOverlay {
        size: WatermarkSize::ActualSize,
        horizontal_margin_percent: 0.0,
        vertical_margin_percent: 0.0,
        ..WatermarkOverlay::default()
    };
    assert_eq!(
        CanvasRegion::for_watermarks(frame, frame, &[(overlay.clone(), image)]),
        Some(CanvasRegion {
            x: 1618,
            y: 928,
            width: 302,
            height: 152
        })
    );
    overlay.place_within_source_content = true;
    assert_eq!(
        CanvasRegion::for_watermarks(
            frame,
            FrameSize::new(1440, 1080),
            &[(overlay.clone(), image)]
        ),
        Some(CanvasRegion {
            x: 1378,
            y: 928,
            width: 302,
            height: 152
        })
    );
    assert!(CanvasRegion::for_watermarks(frame, frame, &[]).is_none());
    assert!(CanvasRegion::for_watermarks(frame, frame, &[(overlay.clone(), frame)]).is_none());
    assert!(
        CanvasRegion::for_watermarks(frame, frame, &[(overlay.clone(), FrameSize::new(0, 1))])
            .is_none()
    );
    overlay.width_percent = f64::NAN;
    assert!(CanvasRegion::for_watermarks(frame, frame, &[(overlay, image)]).is_none());
}

#[test]
fn canvas_region_rejects_empty_and_overflowing_bounds() {
    for (x, y, width, height) in [
        (0, 0, 0, 10),
        (0, 0, 10, 0),
        (u32::MAX, 0, 2, 2),
        (0, u32::MAX, 2, 2),
    ] {
        assert!(ersatzrs_ffmpeg::build_canvas_region_command(
            request(),
            ersatzrs_ffmpeg::CanvasRegion {
                x,
                y,
                width,
                height
            },
        )
        .is_err());
    }
}

#[test]
#[ignore = "requires a packaged FFmpeg 9 runtime"]
#[allow(
    clippy::disallowed_methods,
    reason = "manual gate receives the exact runtime from its runner"
)]
fn canvas_region_encodes_smallest_even_surfaces() {
    let executable = std::env::var_os("ERSATZRS_FFMPEG").expect("packaged FFmpeg executable");
    for (width, height, current_frame) in
        [(2, 2, 64), (2, 8, 64), (8, 2, 64), (4, 4, 64), (16, 16, 8)]
    {
        let mut request = request();
        request.resolution = FrameSize::new(current_frame, current_frame);
        request.content_size = request.resolution;
        if current_frame == 8 {
            // Authored selection changed after publication and the profile
            // shrank. Its new pixel is outside the original signed rectangle.
            request.overlays.push(GraphicsPipelineOverlay::Drawtext {
                z_index: 0,
                filter: "drawbox=x=0:y=0:w=1:h=1:color=green:t=fill".into(),
            });
        }
        let command = ersatzrs_ffmpeg::build_canvas_region_command(
            request,
            ersatzrs_ffmpeg::CanvasRegion {
                x: 2,
                y: 2,
                width,
                height,
            },
        )
        .unwrap();
        let encoded = std::process::Command::new(&executable)
            .args(command.arguments)
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("small.nut");
        std::fs::write(&path, encoded.stdout).unwrap();
        let decoded = std::process::Command::new(&executable)
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
            usize::try_from(width * height * 4 * 4).unwrap()
        );
        assert!(decoded.stdout.chunks_exact(4).all(|pixel| pixel[3] == 0));
    }
}

#[test]
#[ignore = "requires a packaged FFmpeg 9 runtime"]
#[allow(
    clippy::disallowed_methods,
    reason = "manual gate receives the exact runtime from its runner"
)]
fn canvas_region_pixels_equal_full_frame_crop_for_every_anchor_and_mutable_graphics() {
    let executable = std::env::var_os("ERSATZRS_FFMPEG").expect("packaged FFmpeg executable");
    let region = ersatzrs_ffmpeg::CanvasRegion {
        x: 4,
        y: 12,
        width: 48,
        height: 36,
    };
    let directory = tempfile::tempdir().unwrap();
    let decode = |command: ersatzrs_ffmpeg::FfmpegCommand| {
        let encoded = std::process::Command::new(&executable)
            .args(command.arguments)
            .output()
            .unwrap();
        assert!(
            encoded.status.success(),
            "{}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let path = directory.path().join("canvas.nut");
        std::fs::write(&path, encoded.stdout).unwrap();
        let output = std::process::Command::new(&executable)
            .args(["-nostdin", "-v", "error", "-i"])
            .arg(path)
            .args(["-pix_fmt", "bgra", "-f", "rawvideo", "pipe:1"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    };
    for location in [
        WatermarkLocation::TopLeft,
        WatermarkLocation::TopMiddle,
        WatermarkLocation::TopRight,
        WatermarkLocation::LeftMiddle,
        WatermarkLocation::MiddleCenter,
        WatermarkLocation::RightMiddle,
        WatermarkLocation::BottomLeft,
        WatermarkLocation::BottomMiddle,
        WatermarkLocation::BottomRight,
    ] {
        for authored in [false, true] {
            let fixture = || {
                let mut request = request();
                request.overlays = vec![image("red", 2, 50), image("blue", 1, 100)];
                for item in &mut request.overlays {
                    if let GraphicsPipelineOverlay::NativeImage { overlay, .. } = item {
                        overlay.location = location;
                        overlay.size = WatermarkSize::Scaled;
                        overlay.width_percent = 25.0;
                        overlay.opacity_expression = Some("0.5+0.25*sin(content_seconds)".into());
                    }
                }
                if authored {
                    request.overlays.push(GraphicsPipelineOverlay::Drawtext {
                        z_index: 3,
                        filter: "drawbox=x=10:y=18:w=8:h=10:color=green@0.5:t=fill".into(),
                    });
                }
                request
            };
            let full = decode(build_canvas_command(fixture()).unwrap());
            let measured = if authored {
                region
            } else {
                let request = fixture();
                let images: Vec<_> = request
                    .overlays
                    .iter()
                    .filter_map(|overlay| match overlay {
                        GraphicsPipelineOverlay::NativeImage { overlay, .. } => {
                            Some((overlay.clone(), FrameSize::new(16, 16)))
                        }
                        _ => None,
                    })
                    .collect();
                ersatzrs_ffmpeg::CanvasRegion::for_watermarks(
                    request.resolution,
                    request.content_size,
                    &images,
                )
                .unwrap()
            };
            let small =
                decode(ersatzrs_ffmpeg::build_canvas_region_command(fixture(), measured).unwrap());
            assert_canvas_crop(&full, &small, measured, !authored);
        }
    }
}

fn assert_canvas_crop(
    full: &[u8],
    small: &[u8],
    region: ersatzrs_ffmpeg::CanvasRegion,
    contained: bool,
) {
    let (x, y, w, h) = (
        usize::try_from(region.x).unwrap(),
        usize::try_from(region.y).unwrap(),
        usize::try_from(region.width).unwrap(),
        usize::try_from(region.height).unwrap(),
    );
    assert_eq!(full.len(), 64 * 64 * 4 * 4);
    assert_eq!(small.len(), w * h * 4 * 4);
    let cropped: Vec<u8> = full
        .chunks_exact(64 * 64 * 4)
        .flat_map(|frame| {
            (y..y + h).flat_map(move |row| {
                frame[(row * 64 + x) * 4..(row * 64 + x + w) * 4]
                    .iter()
                    .copied()
            })
        })
        .collect();
    assert_eq!(
        small, cropped,
        "measured region must retain exact cropped pixels"
    );
    if contained {
        for frame in full.chunks_exact(64 * 64 * 4) {
            for (index, pixel) in frame.chunks_exact(4).enumerate().filter(|(_, p)| p[3] != 0) {
                assert!(
                    (x..x + w).contains(&(index % 64)) && (y..y + h).contains(&(index / 64)),
                    "measured rectangle clipped a visible watermark pixel {index}: {pixel:?}"
                );
            }
        }
    }
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

#[test]
fn authored_graphics_preview_uses_native_canvas_snapshot() {
    let mut preview = request();
    preview.opacity_clock = WatermarkOpacityClock::new(36000.0, 2.25, Some(30.0), 2.25);
    preview.overlays = vec![
        image("red", 1, 50),
        GraphicsPipelineOverlay::Drawtext {
            z_index: 2,
            filter: "drawtext=font=Roboto:text=Example:fontsize=28:enable='between(t,2,4)'".into(),
        },
    ];
    let command = ersatzrs_ffmpeg::build_canvas_preview_command(preview).unwrap();
    insta::assert_snapshot!(command.arguments.join("\n"));
}

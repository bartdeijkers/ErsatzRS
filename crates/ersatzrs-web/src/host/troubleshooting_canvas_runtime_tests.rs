//! ADR 0105: decoded canvas through the real private HTTP route and official Next.
use super::*;
use ersatzrs_core::repositories::GraphicsRepository as _;
use ersatzrs_domain::{
    DrawtextGraphicsConfig, GraphicsElement, GraphicsElementConfig, ImageGraphicsConfig,
    MotionEndBehavior, MotionGraphicsConfig, ScriptGraphicsConfig, StyleDefinition,
    SubtitleGraphicsConfig, TextGraphicsConfig, TextGraphicsRenderer, WatermarkLocation,
};

async fn assets(fixture: &Fixture) -> (PathBuf, PathBuf, PathBuf) {
    let image = fixture.root.path().join("green.ppm");
    let mut ppm = b"P6\n16 16\n255\n".to_vec();
    ppm.extend([0, 255, 0].repeat(16 * 16));
    tokio::fs::write(&image, ppm).await.unwrap();
    let motion = fixture.root.path().join("motion.nut");
    let mut generate = Command::new(runtime_path("ERSATZRS_NEXT_TEST_FFMPEG"));
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=yellow:s=16x16:r=25",
            "-t",
            "1",
            "-c:v",
            "ffv1",
            "-f",
            "nut",
        ])
        .arg(&motion);
    output(generate).await;
    let ass = fixture.root.path().join("canvas.ass");
    tokio::fs::write(&ass, concat!(
        "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n",
        "[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n",
        "Style: Default,Arial,12,&H00FF00FF,&H00FF00FF,&H00000000,&H00000000,0,0,0,0,100,100,0,0,1,0,0,7,0,0,0,1\n",
        "[Events]\nFormat: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
        "Dialogue: 0,0:00:02.00,0:00:04.00,Default,,0,0,0,,{\\an7\\pos(220,8)\\p1}m 0 0 l 16 0 16 16 0 16\n",
    )).await.unwrap();
    (image, motion, ass)
}

pub(super) async fn install(fixture: &Fixture, query: &mut PlaybackPreviewQuery) {
    let (image, motion, ass) = assets(fixture).await;
    let font =
        FsPath::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts/Roboto-Regular.ttf");
    let script = GraphicsElementConfig::Script(ScriptGraphicsConfig {
        command: Some("return 'setcolor red\\nrect 8 8 32 32 fill\\n'".into()),
        ..Default::default()
    });
    let mut elements = vec![
        GraphicsElement {
            id: 901,
            name: "Synthetic script".into(),
            kind: script.kind(),
            config: script,
            z_index: 1,
            ..Default::default()
        },
        positioned(
            902,
            GraphicsElementConfig::Image(ImageGraphicsConfig {
                image: Some(image.to_str().unwrap().into()),
            }),
            7.5,
            40.0 / 3.0,
        ),
        positioned(
            903,
            GraphicsElementConfig::Motion(MotionGraphicsConfig {
                video_path: Some(motion.to_str().unwrap().into()),
                start_seconds: Some(2.0),
                end_behavior: MotionEndBehavior::Loop,
                ..Default::default()
            }),
            40.0,
            0.0,
        ),
        positioned(
            904,
            GraphicsElementConfig::Subtitle(SubtitleGraphicsConfig {
                template: Some(ass.to_str().unwrap().into()),
            }),
            0.0,
            0.0,
        ),
        positioned(
            905,
            GraphicsElementConfig::Text(TextGraphicsConfig {
                text: Some("VG".into()),
                styles: vec![style("cyan")],
                ..Default::default()
            }),
            2.5,
            100.0 / 3.0,
        ),
        positioned(
            906,
            GraphicsElementConfig::Text(TextGraphicsConfig {
                text: Some("{{ Playback.ElapsedMilliseconds }}".into()),
                renderer: TextGraphicsRenderer::Drawtext,
                drawtext: DrawtextGraphicsConfig {
                    font_file: Some(font.to_str().unwrap().into()),
                    refresh_interval_milliseconds: 100,
                    ..Default::default()
                },
                styles: vec![style("white")],
                ..Default::default()
            }),
            40.0,
            100.0 / 3.0,
        ),
    ];
    let mut translucent = positioned(
        907,
        GraphicsElementConfig::Image(ImageGraphicsConfig {
            image: Some(image.to_str().unwrap().into()),
        }),
        15.0,
        40.0 / 3.0,
    );
    translucent.opacity_percent = Some(50);
    elements.push(translucent);
    for element in elements {
        query.graphics_element.push(element.id);
        fixture
            .state
            .repositories
            .graphics
            .upsert_element(&element)
            .await
            .unwrap();
    }
}

fn style(color: &str) -> StyleDefinition {
    StyleDefinition {
        font_size: Some(21.0),
        text_color: Some(color.into()),
        ..Default::default()
    }
}

fn positioned(id: i64, config: GraphicsElementConfig, x: f64, y: f64) -> GraphicsElement {
    GraphicsElement {
        id,
        name: format!("Synthetic {id}"),
        kind: config.kind(),
        config,
        z_index: 2,
        location: WatermarkLocation::TopLeft,
        horizontal_margin_percent: Some(x),
        vertical_margin_percent: Some(y),
        ..Default::default()
    }
}

pub(super) async fn assert_composite(ffmpeg: &FsPath, playlist: &FsPath) {
    let first = frame(ffmpeg, playlist, "0").await;
    let later = frame(ffmpeg, playlist, "1").await;
    for frame in [&first, &later] {
        let red = pixel(frame, 16, 16);
        assert!(
            red[0] > 180 && red[1] < 40 && red[2] < 40,
            "script missing: {red:?}"
        );
        let green = pixel(frame, 32, 32);
        assert!(
            green[0] < 40 && green[1] > 180 && green[2] < 40,
            "image must cover lower-z script: {green:?}"
        );
        assert_blue(pixel(frame, 80, 24));
        let blend = pixel(frame, 56, 32);
        assert!(
            blend[0] < 30 && (110..=145).contains(&blend[1]) && (110..=145).contains(&blend[2]),
            "Next must composite half-alpha green over blue: {blend:?}"
        );
        assert!(
            region(frame, 8..50, 60..84)
                .filter(|rgb| rgb[0] < 50 && rgb[1] > 180 && rgb[2] > 180)
                .count()
                > 30,
            "drawvg text missing"
        );
        assert!(
            region(frame, 128..210, 60..90)
                .filter(|rgb| rgb.iter().all(|c| *c > 180))
                .count()
                > 10,
            "drawtext text missing"
        );
    }
    assert_blue(pixel(&first, 136, 8));
    assert_blue(pixel(&first, 228, 16));
    let yellow = pixel(&later, 136, 8);
    assert!(
        yellow[0] > 180 && yellow[1] > 180 && yellow[2] < 40,
        "delayed motion missing: {yellow:?}"
    );
    let magenta = pixel(&later, 228, 16);
    assert!(
        magenta[0] > 180 && magenta[1] < 40 && magenta[2] > 180,
        "timed libass graphic missing: {magenta:?}"
    );
    // Preview seeks to source second one: overlays anchored at second two
    // must appear after exactly one output second and ASS must expire at three.
    let expired = frame(ffmpeg, playlist, "3").await;
    assert_blue(pixel(&expired, 228, 16));
    let text_mask = |frame: &[u8]| {
        region(frame, 128..210, 60..90)
            .map(|rgb| rgb.iter().all(|c| *c > 180))
            .collect::<Vec<_>>()
    };
    assert_ne!(
        text_mask(&first),
        text_mask(&later),
        "drawtext must refresh the elapsed clock during the HTTP stream"
    );
}

fn pixel(frame: &[u8], x: usize, y: usize) -> &[u8] {
    &frame[(y * 320 + x) * 3..(y * 320 + x) * 3 + 3]
}

fn region(
    frame: &[u8],
    x: std::ops::Range<usize>,
    y: std::ops::Range<usize>,
) -> impl Iterator<Item = &[u8]> {
    y.flat_map(move |y| x.clone().map(move |x| pixel(frame, x, y)))
}

fn assert_blue(rgb: &[u8]) {
    assert!(
        rgb[0] < 40 && rgb[1] < 40 && rgb[2] > 180,
        "transparent region obscured source: {rgb:?}"
    );
}

async fn frame(ffmpeg: &FsPath, playlist: &FsPath, seek: &str) -> Vec<u8> {
    let mut video = Command::new(ffmpeg);
    video
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(playlist)
        .args([
            "-ss",
            seek,
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let frame = output(video).await;
    assert_eq!(frame.len(), 320 * 180 * 3);
    frame
}

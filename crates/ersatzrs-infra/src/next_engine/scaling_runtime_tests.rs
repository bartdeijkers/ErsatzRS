//! Real decoded aspect-ratio handling through the production Next host.
use super::*;
use ersatzrs_domain::ScalingBehavior;

async fn generate_scaling_clip(ffmpeg: &Path, folder: &Path) -> PathBuf {
    let source = folder.join("source.mkv");
    generate_clip(ffmpeg, &source).await;
    let clip = folder.join("scaling.mkv");
    let mut encode = Command::new(ffmpeg);
    encode.args(["-v", "error", "-nostdin", "-y", "-i"]).arg(source)
        .args(["-map", "0", "-vf",
            "drawbox=x=0:y=0:w=20:h=ih:color=red:t=fill,drawbox=x=iw-20:y=0:w=20:h=ih:color=red:t=fill",
            "-c:v", "libx264", "-preset", "ultrafast", "-threads", "2", "-c:a", "copy"])
        .arg(&clip);
    tool(encode).await;
    std::fs::write(
        folder.join("fixture.nl.srt"),
        "1\n00:00:00,000 --> 00:01:00,000\nScaling subtitle proof\n",
    )
    .unwrap();
    clip
}

async fn assert_scaling_frame(ffmpeg: &Path, segment: &Path, mode: ScalingBehavior) {
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-ss",
            "0.5",
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
    let pixels = tool(decode).await;
    assert_eq!(
        pixels.len(),
        240 * 180 * 3,
        "actual decoded output dimensions"
    );
    let pixel = |x: usize, y: usize| &pixels[(y * 240 + x) * 3..(y * 240 + x) * 3 + 3];
    let black = |p: &[u8]| p.iter().all(|component| *component < 25);
    let blue = |p: &[u8]| p[2] > 200 && p[0] < 35 && p[1] < 35;
    let red = |p: &[u8]| p[0] > 200 && p[1] < 35 && p[2] < 35;
    assert!(
        blue(pixel(120, 90)),
        "center source content must remain blue"
    );
    match mode {
        ScalingBehavior::ScaleAndPad => {
            assert!(black(pixel(120, 5)), "letterbox above fitted source");
            assert!(black(pixel(5, 175)), "letterbox below fitted source");
            assert!(
                red(pixel(5, 90)) && red(pixel(234, 90)),
                "fit preserves both source edges"
            );
            assert!(
                blue(pixel(120, 30)),
                "source starts below the top letterbox"
            );
        }
        ScalingBehavior::Stretch => {
            assert!(blue(pixel(120, 5)), "stretch fills the output height");
            assert!(
                red(pixel(5, 5)) && red(pixel(234, 5)),
                "stretch keeps both edges at full height"
            );
            assert!(
                blue(pixel(24, 90)),
                "horizontal fit reduces each 20px stripe to 15px"
            );
        }
        ScalingBehavior::Crop => {
            assert!(
                blue(pixel(5, 5)) && blue(pixel(234, 5)),
                "crop removes both 20px edges"
            );
            assert!(
                blue(pixel(5, 90)) && blue(pixel(234, 90)),
                "crop fills output without side bars"
            );
        }
    }
    let white = pixels
        .chunks_exact(3)
        .filter(|p| p.iter().all(|component| *component > 180))
        .count();
    assert!(
        white > 20,
        "selected Dutch subtitle must remain burned into the scaled output"
    );
}

async fn assert_selected_audio(ffmpeg: &Path, segment: &Path) {
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let bytes = tool(decode).await;
    let samples: Vec<_> = bytes
        .chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples.windows(2).filter(|p| p[0] <= 0 && p[1] > 0).count();
    let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    assert!(
        (820.0..940.0).contains(&hz),
        "selected Dutch audio must remain 880Hz: {hz}"
    );
}

#[tokio::test]
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic media"]
async fn real_next_scaling_modes_preserve_geometry_selected_audio_and_subtitles() {
    scaling_cases(
        FFmpegProfileVideoFormat::H264,
        &[
            ScalingBehavior::ScaleAndPad,
            ScalingBehavior::Stretch,
            ScalingBehavior::Crop,
        ],
    )
    .await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; decoded MPEG-2 with selected tracks"]
async fn real_next_mpeg2_forces_eight_bits_preserves_geometry_audio_and_subtitles() {
    scaling_cases(
        FFmpegProfileVideoFormat::Mpeg2Video,
        &[ScalingBehavior::ScaleAndPad],
    )
    .await;
}

async fn scaling_cases(format: FFmpegProfileVideoFormat, modes: &[ScalingBehavior]) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = generate_scaling_clip(&ffmpeg, root.path()).await;
    for &mode in modes {
        let profile = FFmpegProfile {
            scaling_behavior: mode,
            video_format: format,
            bit_depth: if format == FFmpegProfileVideoFormat::Mpeg2Video {
                ersatzrs_domain::FFmpegProfileBitDepth::TenBit
            } else {
                ersatzrs_domain::FFmpegProfileBitDepth::EightBit
            },
            audio_format: FFmpegProfileAudioFormat::Aac,
            video_bitrate: 1000,
            video_buffer_size: 2000,
            audio_sample_rate: 48,
            audio_channels: 1,
            ..Default::default()
        };
        let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
        sqlx::query(r#"UPDATE "Resolution" SET "Width"=240, "Height"=180 WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        let host = std::sync::Arc::new(host::NextHost::new(
            pool,
            locker,
            root.path().join("playout"),
        ));
        let output = root.path().join("output");
        let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
        let task = host
            .start(
                registry.clone(),
                host::NextStart {
                    channel_id: 1,
                    preview: false,
                    number: "1".into(),
                    executable: binary.clone(),
                    output: output.clone(),
                    ffmpeg: ffmpeg.clone(),
                    ffprobe: ffprobe.clone(),
                    replacements: Vec::new(),
                    idle_timeout: None,
                    deadline: Instant::now() + Duration::from_secs(30),
                },
            )
            .await
            .unwrap();
        let handle = registry.try_get("1").await.unwrap();
        timeout(Duration::from_secs(30), async {
            loop {
                match handle.startup_readiness() {
                    HlsStartupReadiness::Ready => break,
                    HlsStartupReadiness::Failed => panic!("scaling startup failed: {mode:?}"),
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .unwrap();
        let playlist = handle.trim_playlist().await.unwrap();
        let segment = playlist
            .lines()
            .find(|line| {
                !line.starts_with('#')
                    && Path::new(line)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
            })
            .unwrap();
        assert_scaling_frame(&ffmpeg, &output.join(segment), mode).await;
        assert_selected_audio(&ffmpeg, &output.join(segment)).await;
        assert_video_codec(&ffprobe, &output.join(segment), format).await;
        assert!(registry.stop_channel("1").await);
        task.await.unwrap();
        assert!(!output.exists());
    }
}

async fn assert_video_codec(ffprobe: &Path, segment: &Path, format: FFmpegProfileVideoFormat) {
    let mut probe = Command::new(ffprobe);
    probe
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,pix_fmt",
            "-of",
            "json",
        ])
        .arg(segment);
    let value: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let stream = &value["streams"][0];
    assert_eq!(
        stream["codec_name"],
        if format == FFmpegProfileVideoFormat::Mpeg2Video {
            "mpeg2video"
        } else {
            "h264"
        }
    );
    assert_eq!(
        stream["pix_fmt"], "yuv420p",
        "actual decoded eight-bit output"
    );
}

async fn assert_source_watermark(ffmpeg: &Path, segment: &Path, expected_y: usize) {
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-ss",
            "0.5",
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
    let pixels = tool(decode).await;
    assert_eq!(pixels.len(), 240 * 180 * 3);
    let yellow: Vec<_> = pixels
        .chunks_exact(3)
        .enumerate()
        .filter(|(_, p)| p[0] > 170 && p[1] > 170 && p[2] < 60)
        .map(|(i, _)| (i % 240, i / 240))
        .collect();
    assert!(yellow.len() >= 160, "visible solid 20x10 yellow watermark");
    let left = yellow.iter().map(|p| p.0).min().unwrap();
    let top = yellow.iter().map(|p| p.1).min().unwrap();
    let right = yellow.iter().map(|p| p.0).max().unwrap();
    let bottom = yellow.iter().map(|p| p.1).max().unwrap();
    assert!(left.abs_diff(110) <= 1, "center anchor x={left}");
    assert!(
        top.abs_diff(expected_y) <= 1,
        "content/output edge y={top}, expected={expected_y}"
    );
    assert!((right - left + 1).abs_diff(20) <= 2);
    assert!((bottom - top + 1).abs_diff(10) <= 2);
    let white = pixels
        .chunks_exact(3)
        .filter(|p| p.iter().all(|c| *c > 180))
        .count();
    assert!(
        white > 20,
        "source-relative overlay preserves burned Dutch subtitles"
    );
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One disposable fixture exercises six real output geometries.
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic media"]
async fn real_next_watermark_source_content_edges_follow_padding_but_not_stretch_or_crop() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = generate_scaling_clip(&ffmpeg, root.path()).await;
    let image = root.path().join("yellow.png");
    let mut generate = Command::new(&ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=yellow:s=20x10",
            "-frames:v",
            "1",
            "-threads",
            "1",
        ])
        .arg(&image);
    tool(generate).await;
    for mode in [
        ScalingBehavior::ScaleAndPad,
        ScalingBehavior::Stretch,
        ScalingBehavior::Crop,
    ] {
        for within in [false, true] {
            let profile = FFmpegProfile {
                scaling_behavior: mode,
                video_format: FFmpegProfileVideoFormat::H264,
                audio_format: FFmpegProfileAudioFormat::Aac,
                video_bitrate: 1000,
                video_buffer_size: 2000,
                audio_sample_rate: 48,
                audio_channels: 1,
                ..Default::default()
            };
            let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
            sqlx::query(r#"UPDATE "Resolution" SET "Width"=240, "Height"=180 WHERE "Id"=1"#)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query(
                r#"INSERT INTO "ChannelWatermark" (
                "Id", "Name", "Mode", "ImageSource", "Location", "Size", "WidthPercent",
                "HorizontalMarginPercent", "VerticalMarginPercent", "FrequencyMinutes",
                "DurationSeconds", "Opacity", "PlaceWithinSourceContent", "ZIndex")
                VALUES (1,'Source edge proof',1,1,4,1,10,0,0,0,0,100,?,0)"#,
            )
            .bind(within)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(r#"INSERT INTO "Artwork" ("ArtworkKind", "ChannelId", "Path", "DateAdded", "DateUpdated") VALUES (2,1,?,'2026-09-23','2026-09-23')"#)
                .bind(image.to_str().unwrap()).execute(&pool).await.unwrap();
            sqlx::query(r#"UPDATE "Channel" SET "WatermarkId"=1 WHERE "Id"=1"#)
                .execute(&pool)
                .await
                .unwrap();
            let host = std::sync::Arc::new(host::NextHost::new(
                pool,
                locker,
                root.path().join("playout"),
            ));
            let output = root.path().join("output");
            let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
            let task = host
                .start(
                    registry.clone(),
                    host::NextStart {
                        channel_id: 1,
                        preview: false,
                        number: "1".into(),
                        executable: binary.clone(),
                        output: output.clone(),
                        ffmpeg: ffmpeg.clone(),
                        ffprobe: ffprobe.clone(),
                        replacements: Vec::new(),
                        idle_timeout: None,
                        deadline: Instant::now() + Duration::from_secs(30),
                    },
                )
                .await
                .unwrap();
            let handle = registry.try_get("1").await.unwrap();
            timeout(Duration::from_secs(30), async {
                loop {
                    match handle.startup_readiness() {
                        HlsStartupReadiness::Ready => break,
                        HlsStartupReadiness::Failed => {
                            panic!("watermark content placement startup failed: {mode:?}/{within}")
                        }
                        _ => tokio::time::sleep(Duration::from_millis(10)).await,
                    }
                }
            })
            .await
            .unwrap();
            let playlist = handle.trim_playlist().await.unwrap();
            let segment = playlist
                .lines()
                .find(|line| {
                    !line.starts_with('#')
                        && Path::new(line)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                })
                .unwrap();
            let expected_y = if within && mode == ScalingBehavior::ScaleAndPad {
                22
            } else {
                0
            };
            assert_source_watermark(&ffmpeg, &output.join(segment), expected_y).await;
            assert_selected_audio(&ffmpeg, &output.join(segment)).await;
            assert!(registry.stop_channel("1").await);
            task.await.unwrap();
            assert!(!output.exists());
        }
    }
}

//! Decode real Next overlays alongside selected audio and demand-time subtitles.
use super::*;

async fn seed_watermark(pool: &crate::db::DbPool, image: &Path, scaled: bool, disabled: bool) {
    sqlx::query(
        r#"INSERT INTO "ChannelWatermark" (
            "Id", "Name", "Mode", "ImageSource", "Location", "Size", "WidthPercent",
            "HorizontalMarginPercent", "VerticalMarginPercent", "FrequencyMinutes",
            "DurationSeconds", "Opacity", "PlaceWithinSourceContent", "ZIndex")
            VALUES (1, 'Synthetic red rectangle', 1, 1, ?, ?, 20, 5, 5, 0, 0, ?, 0, 0)"#,
    )
    .bind(if scaled { 3 } else { 2 })
    .bind(i32::from(!scaled))
    .bind(if scaled { 50 } else { 100 })
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "Artwork" ("ArtworkKind", "ChannelId", "Path", "DateAdded", "DateUpdated") VALUES (2,1,?,'2026-09-23','2026-09-23')"#)
        .bind(image.to_str().unwrap()).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "WatermarkId"=1 WHERE "Id"=1"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "PlayoutItem" SET "DisableWatermarks"=?"#)
        .bind(disabled)
        .execute(pool)
        .await
        .unwrap();
}

async fn assert_overlay(ffmpeg: &Path, segment: &Path, scaled: bool, disabled: bool) {
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
            "-vf",
            "crop=320:120:0:0",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(decode).await;
    assert_eq!(pixels.len(), 320 * 120 * 3);
    let red: Vec<_> = pixels
        .chunks_exact(3)
        .enumerate()
        .filter(|(_, p)| p[0] > 65 && p[1] < 40)
        .map(|(index, _)| (index % 320, index / 320))
        .collect();
    if disabled {
        assert!(red.is_empty(), "DisableWatermarks must remove the overlay");
        return;
    }
    let (left, width, height) = if scaled { (16, 64, 32) } else { (264, 40, 20) };
    let min_x = red.iter().map(|p| p.0).min().unwrap();
    let max_x = red.iter().map(|p| p.0).max().unwrap();
    let min_y = red.iter().map(|p| p.1).min().unwrap();
    let max_y = red.iter().map(|p| p.1).max().unwrap();
    assert!(
        min_x.abs_diff(left) <= 2,
        "horizontal margin/corner: {min_x}"
    );
    assert!(min_y.abs_diff(9) <= 2, "vertical margin: {min_y}");
    assert!((max_x - min_x + 1).abs_diff(width) <= 2, "overlay width");
    assert!((max_y - min_y + 1).abs_diff(height) <= 2, "overlay height");
    assert!(red.len() > width * height * 8 / 10, "solid overlay area");
    let index = ((9 + height / 2) * 320 + left + width / 2) * 3;
    let center = &pixels[index..index + 3];
    if scaled {
        // Next 946aa52 applies opacity before adding alpha to RGB images.
        // The maintainer accepts this upstream limitation pending its fix;
        // retain the RGB fixture and observed value without claiming alpha parity.
        println!("Next RGB watermark at configured 50 percent: {center:?}");
    } else {
        assert!(center[0] > 210 && center[2] < 35, "opaque red: {center:?}");
    }
}

#[tokio::test]
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic media"]
async fn real_next_watermark_size_position_dynamic_burn_and_disabled() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let image = root.path().join("red.png");
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
            "color=c=red:s=40x20",
            "-frames:v",
            "1",
            "-threads",
            "1",
        ])
        .arg(&image);
    tool(generate).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    for (dynamic, scaled, disabled) in [
        (false, false, false),
        (true, true, false),
        (true, true, true),
    ] {
        let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
        configure_text_tracks(&pool, true, !dynamic, false).await;
        seed_watermark(&pool, &image, scaled, disabled).await;
        let host = std::sync::Arc::new(host::NextHost::new(
            pool,
            locker,
            root.path().join("playout"),
        ));
        let mut callback = callback_listener(host.clone()).await;
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
                    HlsStartupReadiness::Failed => panic!("watermark startup failed: dynamic={dynamic} scaled={scaled} disabled={disabled}"),
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        }).await.unwrap();
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
        assert_overlay(&ffmpeg, &output.join(segment), scaled, disabled).await;
        assert_text_video_and_audio(&ffmpeg, &output.join(segment), dynamic).await;
        assert!(registry.stop_channel("1").await);
        task.await.unwrap();
        callback.stop().await;
        assert!(!output.exists());
    }
}

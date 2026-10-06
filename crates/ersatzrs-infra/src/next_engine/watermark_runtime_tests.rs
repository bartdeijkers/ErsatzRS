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
        assert!(
            (105..=145).contains(&center[0]) && center[1] < 35 && (105..=145).contains(&center[2]),
            "half-opacity RGB red over blue: {center:?}"
        );
    } else {
        assert!(center[0] > 210 && center[2] < 35, "opaque red: {center:?}");
    }
}

#[tokio::test]
#[ignore = "requires built Next engine and FFmpeg runtime; uses disposable synthetic media"]
async fn real_next_watermark_size_position_dynamic_burn_and_disabled() {
    watermark_runtime_cases(false, false).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; serves a synthetic logo over loopback HTTP"]
async fn real_next_http_channel_logo_watermark_preserves_tracks_and_disable() {
    watermark_runtime_cases(true, false).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolates generated channel-logo cache"]
async fn real_next_generated_channel_logo_preserves_tracks_and_disable() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::watermark_runtime_tests::fixture_child_generated_logo").await;
}

#[tokio::test]
#[ignore = "child of real_next_generated_channel_logo_preserves_tracks_and_disable"]
async fn fixture_child_generated_logo() {
    let _root = prepared_artifact_runtime::temporary_root().await;
    watermark_runtime_cases(false, true).await;
}

#[tokio::test]
#[ignore = "requires FFmpeg 9; isolates generated logo cache and synthetic HTTP artwork"]
async fn real_native_channel_logos_preserve_selected_audio() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::watermark_runtime_tests::fixture_child_native_logos").await;
}

#[tokio::test]
#[ignore = "child of real_native_channel_logos_preserve_selected_audio"]
async fn fixture_child_native_logos() {
    use ersatzrs_ffmpeg::{
        DesiredVideoState, FrameSize, InputFile, PipelineBuilder, PipelineRequest,
        SoftwarePipelineBuilder, SourceAudioCodec, SourceAudioStream, SourceVideoCodec,
        SourceVideoStream,
    };
    let _isolated = prepared_artifact_runtime::temporary_root().await;
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let image = root.path().join("red.png");
    red_logo(&ffmpeg, &image).await;
    let server = httpmock::MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.path("/logo.png");
            then.status(200)
                .header("content-type", "image/png")
                .body(std::fs::read(&image).unwrap());
        })
        .await;
    for generated in [false, true] {
        let (pool, _locker) = prepare_schedule(&clip, false, &FFmpegProfile::default()).await;
        seed_watermark(&pool, &image, false, false).await;
        if generated {
            sqlx::raw_sql(r#"DELETE FROM "Artwork" WHERE "ChannelId"=1;
                UPDATE "ChannelWatermark" SET "Size"=0,"WidthPercent"=50,"Location"=3 WHERE "Id"=1;"#)
                .execute(&pool).await.unwrap();
        } else {
            sqlx::query(r#"UPDATE "Artwork" SET "Path"=? WHERE "ChannelId"=1"#)
                .bind(server.url("/logo.png"))
                .execute(&pool)
                .await
                .unwrap();
        }
        let resolved = crate::playback::resolve_playback_watermarks_by_ids(&pool, 1, vec![1])
            .await
            .unwrap();
        assert_eq!(resolved.len(), 1);
        let mut desired = DesiredVideoState::h264_1080p();
        desired.frame_size = FrameSize::new(320, 180);
        desired.scaled_size = desired.frame_size;
        let mut request = PipelineRequest::new(InputFile::video(clip.to_str().unwrap()), desired)
            .with_source_video_stream(SourceVideoStream::new(SourceVideoCodec::H264))
            .with_source_audio_streams(vec![
                SourceAudioStream::new(SourceAudioCodec::Aac).with_index(2)
            ]);
        request.state.duration = Some(Duration::from_secs(2));
        request.state.report_progress = false;
        request.state.thread_count = Some(1);
        for (input, overlay) in
            crate::playback::watermark_inputs_for_stream(resolved, request.state.duration).await
        {
            request.inputs.push(input);
            request.watermarks.push(overlay);
        }
        let mut args = SoftwarePipelineBuilder
            .build(request.clone())
            .command(&request.inputs)
            .arguments;
        assert_eq!(args.pop().as_deref(), Some("pipe:1"));
        let segment = root.path().join(format!("native-{generated}.ts"));
        let mut encode = Command::new(&ffmpeg);
        encode.args(args).arg(&segment);
        tool(encode).await;
        if generated {
            assert_generated_overlay(&ffmpeg, &segment, false).await;
        } else {
            assert_overlay(&ffmpeg, &segment, false, false).await;
        }
        assert_text_video_and_audio(&ffmpeg, &segment, false).await;
    }
}

async fn red_logo(ffmpeg: &Path, image: &Path) {
    let mut generate = Command::new(ffmpeg);
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
        .arg(image);
    tool(generate).await;
}

async fn logo_server(image: &Path) -> httpmock::MockServer {
    let server = httpmock::MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.path("/logo.png");
            then.status(200)
                .header("content-type", "image/png")
                .body(std::fs::read(image).unwrap());
        })
        .await;
    server
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; synthetic ordered overlays with audio/subtitles"]
async fn real_next_multiple_watermarks_preserve_order_dynamic_tracks_and_disable() {
    watermark_runtime_layers(false, false, Layers::Watermarks).await;
}

async fn watermark_runtime_cases(remote_logo: bool, generated_logo: bool) {
    watermark_runtime_layers(remote_logo, generated_logo, Layers::Single).await;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layers {
    Single,
    Watermarks,
}

async fn watermark_runtime_layers(remote_logo: bool, generated_logo: bool, layers: Layers) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let image = root.path().join("red.png");
    red_logo(&ffmpeg, &image).await;
    let server = logo_server(&image).await;
    for (dynamic, scaled, disabled) in [
        (false, false, false),
        (true, true, false),
        (true, true, true),
    ] {
        let (pool, locker) = prepare_schedule(&clip, false, &overlay_profile()).await;
        configure_text_tracks(&pool, true, !dynamic, false).await;
        seed_watermark(&pool, &image, scaled, disabled).await;
        if layers != Layers::Single {
            seed_lower_watermark(&ffmpeg, root.path(), &pool).await;
        }
        if generated_logo {
            sqlx::raw_sql(r#"DELETE FROM "Artwork" WHERE "ChannelId"=1;
                UPDATE "ChannelWatermark" SET "Size"=0,"WidthPercent"=50,"Location"=3,"Opacity"=100 WHERE "Id"=1;"#)
                .execute(&pool).await.unwrap();
        }
        if remote_logo {
            sqlx::query(r#"UPDATE "Artwork" SET "Path"=? WHERE "ChannelId"=1"#)
                .bind(server.url("/logo.png"))
                .execute(&pool)
                .await
                .unwrap();
        }
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
        let segment = first_transport_segment(&playlist);
        if generated_logo {
            assert_generated_overlay(&ffmpeg, &output.join(segment), disabled).await;
        } else {
            assert_overlay(&ffmpeg, &output.join(segment), scaled, disabled).await;
        }
        if layers != Layers::Single {
            assert_lower_watermark(&ffmpeg, &output.join(segment), disabled).await;
        }
        assert_text_video_and_audio(&ffmpeg, &output.join(segment), dynamic).await;
        assert!(registry.stop_channel("1").await);
        task.await.unwrap();
        callback.stop().await;
        assert!(!output.exists());
    }
}

fn first_transport_segment(playlist: &str) -> &str {
    playlist
        .lines()
        .find(|line| {
            !line.starts_with('#')
                && Path::new(line)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
        })
        .expect("ready playlist must contain a transport stream")
}

async fn seed_lower_watermark(ffmpeg: &Path, root: &Path, pool: &crate::db::DbPool) {
    let green = root.join("green.png");
    let mut generate = Command::new(ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=lime:size=60x30",
            "-frames:v",
            "1",
        ])
        .arg(&green);
    tool(generate).await;
    sqlx::query(
        r#"INSERT INTO "ChannelWatermark" (
        "Id","Name","Mode","ImageSource","Image","Location","Size","WidthPercent",
        "HorizontalMarginPercent","VerticalMarginPercent","FrequencyMinutes",
        "DurationSeconds","Opacity","PlaceWithinSourceContent","ZIndex")
        VALUES (2,'Synthetic lower green',1,0,?,2,1,20,5,5,0,0,100,0,-1)"#,
    )
    .bind(green.to_str().unwrap())
    .execute(pool)
    .await
    .unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId","WatermarkId")
        SELECT "Id",1 FROM "PlayoutItem";
        INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId","WatermarkId")
        SELECT "Id",2 FROM "PlayoutItem";"#,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn assert_lower_watermark(ffmpeg: &Path, segment: &Path, disabled: bool) {
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-ss", "0.5", "-i"])
        .arg(segment)
        .args([
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
    let index = (14 * 320 + 248) * 3;
    let pixel = &pixels[index..index + 3];
    if disabled {
        assert!(pixel[1] < 100, "disabled lower layer remains: {pixel:?}");
    } else {
        assert!(
            pixel[1] > 180 && pixel[0] < 40,
            "lower green layer missing: {pixel:?}"
        );
        // assert_overlay above also proves opaque red covers the overlapping
        // portion, even though the green layer has the larger database ID.
    }
}

async fn assert_generated_overlay(ffmpeg: &Path, segment: &Path, disabled: bool) {
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
            "crop=200:110:0:0",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(decode).await;
    assert_eq!(pixels.len(), 200 * 110 * 3);
    let border = pixels
        .chunks_exact(3)
        .filter(|p| p[0] < 65 && p[1] > 90 && p[2] > 60 && p[2] < 200)
        .count();
    let text = pixels
        .chunks_exact(3)
        .filter(|p| p.iter().all(|c| *c > 175))
        .count();
    if disabled {
        assert_eq!((border, text), (0, 0));
    } else {
        assert!(border > 80, "generated teal border missing: {border}");
        assert!(text > 15, "generated initials missing: {text}");
    }
}

#[path = "intermittent_watermark_runtime_tests.rs"]
mod intermittent_watermark_runtime_tests;

// Mixed image graphics use the production web canvas route; runtime coverage
// lives in troubleshooting_image_graphics_runtime_tests.rs in ersatzrs-web.

fn overlay_profile() -> FFmpegProfile {
    FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    }
}

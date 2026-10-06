//! Scheduled image graphics must use the real canvas route, including dynamic Burn.
use super::*;
use ersatzrs_core::hls::{HlsSessionRegistry as _, HlsStartupReadiness};
use ersatzrs_core::repositories::{GraphicsRepository as _, WatermarkRepository as _};
use ersatzrs_domain::{
    ChannelWatermark, ChannelWatermarkMode, GraphicsElement, GraphicsElementConfig,
    ImageGraphicsConfig, WatermarkLocation, WatermarkSize,
};

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; scheduled image templates through production HTTP canvas"]
async fn real_next_image_graphics_preserve_templates_dynamic_tracks_and_disabled_watermarks() {
    for (burn, disabled) in [(false, false), (true, false), (true, true)] {
        run_case(burn, disabled, false).await;
    }
}

#[tokio::test]
#[ignore = "requires accepted Next and FFmpeg; production copy projection with configured overlays"]
async fn real_next_copied_video_omits_graphics_and_preserves_selected_tracks() {
    run_case(false, false, true).await;
}

async fn run_case(burn: bool, disabled: bool, copy: bool) {
    let fixture = Fixture::new().await;
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    {
        let mut runtime = fixture.state.ffmpeg_runtime.write().await;
        runtime.ffmpeg.path = ffmpeg.to_str().unwrap().into();
        runtime.ffprobe.path = ffprobe.to_str().unwrap().into();
    }
    seed(&fixture, &ffmpeg, burn, disabled).await;
    if copy {
        sqlx::query(r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (999,'Copy fallback',640,360)"#)
            .execute(&fixture.state.pool).await.unwrap();
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "VideoFormat"=99,"AudioFormat"=99,"ResolutionId"=999 WHERE "Id"=1"#)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    let origin = remote_runtime::Origin::start(&fixture, remote_runtime::Mode::Local).await;
    if burn {
        sqlx::query(
            r#"UPDATE "PlayoutItemGraphicsElement" SET "Variables"=? WHERE "PlayoutItemId"=1"#,
        )
        .bind(serde_json::json!({"FixtureImage": origin.image_url()}).to_string())
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    }
    let folder = fixture.root.path().join("scheduled-output");
    let registry = Arc::new(ersatzrs_infra::hls::InMemoryHlsSessionRegistry::new());
    let task = fixture
        .state
        .next_engine
        .start(
            registry.clone(),
            ersatzrs_infra::next_engine::host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: folder.clone(),
                ffmpeg: ffmpeg.clone(),
                ffprobe,
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let handle = registry.try_get("1").await.unwrap();
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => {
                    panic!("image graphics startup failed: burn={burn}, disabled={disabled}")
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
                && FsPath::new(line)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
        })
        .unwrap();
    assert_picture(&ffmpeg, &folder.join(segment), burn, disabled, copy).await;
    if burn {
        assert_decoded_tracks(&ffmpeg, &folder.join(segment)).await;
    } else {
        assert_decoded_audio(&ffmpeg, &folder.join(segment)).await;
        assert_converted_caption(&folder).await;
    }
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    if copy {
        origin.verify_unused_and_stop().await;
    } else {
        origin.verify_revoked_and_stop().await;
    }
    assert!(!folder.exists());
}

async fn assert_converted_caption(folder: &FsPath) {
    let mut entries = tokio::fs::read_dir(folder).await.unwrap();
    let mut found = false;
    while let Some(entry) = entries.next_entry().await.unwrap() {
        if entry.path().extension().is_some_and(|ext| ext == "vtt") {
            found |= tokio::fs::read_to_string(entry.path())
                .await
                .unwrap()
                .contains("Dutch image proof");
        }
    }
    assert!(found, "Convert must publish the selected Dutch cue");
}

async fn seed(fixture: &Fixture, ffmpeg: &FsPath, burn: bool, disabled: bool) {
    let pool = &fixture.state.pool;
    generate_source(fixture, ffmpeg).await;
    let green = fixture.root.path().join("green.ppm");
    let red = fixture.root.path().join("red.ppm");
    for (path, color) in [(&green, [0, 255, 0]), (&red, [255, 0, 0])] {
        let mut ppm = b"P6\n32 32\n255\n".to_vec();
        ppm.extend(color.repeat(32 * 32));
        tokio::fs::write(path, ppm).await.unwrap();
    }
    sqlx::query(r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES (3,'subrip',3,'nld',0,'Dutch',1,0,100)"#).execute(pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "Subtitle" ("MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","DateAdded","DateUpdated") VALUES (20,0,'Dutch',3,'subrip',1,0,0,'nld',0,'2026-09-30','2026-09-30')"#).execute(pool).await.unwrap();
    sqlx::query(
        r#"UPDATE "Channel" SET "StreamingEngine"=1,"NextEngineTextSubtitleMode"=? WHERE "Id"=1"#,
    )
    .bind(i32::from(!burn))
    .execute(pool)
    .await
    .unwrap();
    let at = Utc::now();
    fixture
        .state
        .repositories
        .playout
        .upsert_item(&PlayoutItem {
            id: 1,
            playout_id: 1,
            media_item_id: 10,
            start: at,
            finish: at + TimeDelta::seconds(50),
            in_point: TimeDelta::seconds(1),
            out_point: TimeDelta::seconds(51),
            disable_watermarks: disabled,
            pinned_stream_selection: Some(PinnedStreamSelection {
                media_version_id: 100,
                video_stream_index: 0,
                audio_stream_index: Some(2),
                subtitle_stream_index: Some(3),
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    let element = fixture
        .state
        .repositories
        .graphics
        .create_element(&GraphicsElement {
            name: "Synthetic image template".into(),
            z_index: 5,
            location: WatermarkLocation::TopLeft,
            config: GraphicsElementConfig::Image(ImageGraphicsConfig {
                image: Some("{{ FixtureImage }}".into()),
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId","GraphicsElementId","Variables") VALUES (1,?,?)"#)
        .bind(element).bind(serde_json::json!({"FixtureImage":green.to_str().unwrap()}).to_string()).execute(pool).await.unwrap();
    let watermark = fixture
        .state
        .repositories
        .watermark
        .save_admin(&ChannelWatermark {
            name: "Synthetic red".into(),
            mode: ChannelWatermarkMode::Permanent,
            image: Some(red.to_str().unwrap().into()),
            location: WatermarkLocation::TopRight,
            size: WatermarkSize::ActualSize,
            ..Default::default()
        })
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlayoutItemWatermark" ("PlayoutItemId","WatermarkId") VALUES (1,?)"#,
    )
    .bind(watermark)
    .execute(pool)
    .await
    .unwrap();
}

async fn generate_source(fixture: &Fixture, ffmpeg: &FsPath) {
    let clip = fixture.root.path().join("fixture.mkv");
    generate_clip(ffmpeg, &clip, true).await;
    let subtitle = fixture.root.path().join("fixture.nl.srt");
    tokio::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:01:00,000\nDutch image proof\n",
    )
    .await
    .unwrap();
    let muxed = fixture.root.path().join("muxed.mkv");
    let mut mux = Command::new(ffmpeg);
    mux.args(["-v", "error", "-nostdin", "-stream_loop", "9", "-i"])
        .arg(&clip)
        .arg("-i")
        .arg(&subtitle)
        .args(["-map", "0", "-map", "1:0", "-c", "copy", "-t", "60"])
        .arg(&muxed);
    output(mux).await;
    tokio::fs::remove_file(&clip).await.unwrap();
    tokio::fs::rename(&muxed, &clip).await.unwrap();
}

async fn assert_picture(ffmpeg: &FsPath, segment: &FsPath, burn: bool, disabled: bool, copy: bool) {
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-ss",
            "0.5",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = output(decode).await;
    assert_eq!(pixels.len(), 320 * 180 * 3);
    let top = &pixels[..320 * 120 * 3];
    let green = top
        .chunks_exact(3)
        .filter(|p| p[1] > 180 && p[0] < 40 && p[2] < 40)
        .count();
    if copy {
        assert_eq!(green, 0, "copied video must omit configured graphics");
    } else {
        assert!(
            green > 800,
            "templated image must remain visible when watermarks are disabled"
        );
    }
    let red = top
        .chunks_exact(3)
        .filter(|p| p[0] > 180 && p[1] < 40 && p[2] < 40)
        .count();
    if disabled || copy {
        assert_eq!(red, 0);
    } else {
        assert!(red > 800, "watermark missing");
    }
    let white = pixels[320 * 120 * 3..]
        .chunks_exact(3)
        .filter(|p| p.iter().all(|c| *c > 180))
        .count();
    if burn {
        assert!(white > 30, "dynamic Burn subtitle missing");
    } else {
        assert_eq!(white, 0, "Convert must not burn subtitles");
    }
}

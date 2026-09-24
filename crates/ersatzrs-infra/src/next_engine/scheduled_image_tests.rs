//! Upstream PR #2911: a persisted Image uses the ordinary Next video/silence path.

use super::*;

async fn image_metadata(pool: &crate::db::DbPool) {
    for sql in [
        r#"INSERT INTO "Image" ("Id") VALUES (10)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"ImageId"=10,"Duration"='00:00:00' WHERE "Id"=100"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
        r#"INSERT INTO "ImageMetadata" ("Id","ImageId","Title","DurationSeconds","MetadataKind","DateAdded","DateUpdated") VALUES (30,10,'Synthetic still',60,0,'2026-09-24','2026-09-24')"#,
        r#"DELETE FROM "MediaStream" WHERE "MediaVersionId"=100 AND "Index"<>0"#,
        r#"UPDATE "MediaStream" SET "Codec"='png' WHERE "MediaVersionId"=100"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn scheduled_image_keeps_interval_and_supplies_silence() {
    let (pool, locker, channel, start) = schedule::tests::fixture().await;
    image_metadata(&pool).await;
    let finish = start + chrono::Duration::seconds(60);
    let read = schedule::read_locked(&pool, &locker, &channel, start, finish)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read.items.len(), 1);
    let media = &read.items[0];
    assert_eq!(media.media_version_id, 100);
    assert_eq!(media.media_file_id, 1);
    let item = selection::project_item(
        &pool,
        &channel,
        &media.item,
        media.media_version_id,
        crate::next_engine::protocol::Source::Local {
            path: "synthetic.png".into(),
            in_point_ms: None,
            out_point_ms: None,

            probe_hint: None,
        },
        &Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!((item.start, item.finish), (start, finish));
    let json = serde_json::to_value(item).unwrap();
    assert!(json.get("source").is_none());
    assert_eq!(json["tracks"]["video"]["stream_index"], 0);
    assert_eq!(json["tracks"]["video"]["source"]["path"], "synthetic.png");
    assert_eq!(
        json["tracks"]["audio"]["source"]["probe_hint"]["duration_ms"],
        60_000
    );
    assert!(json["tracks"].get("subtitle").is_none());
}

async fn prepare_image(
    ffmpeg: &Path,
    root: &Path,
) -> (crate::db::DbPool, ersatzrs_core::locking::EntityLocker) {
    let image = root.join("still.png");
    let mut generate = Command::new(ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x180",
            "-frames:v",
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
    // No pinned mute: the Image's actual absence of audio selects silence.
    let (pool, locker) = prepare_schedule(&image, false, &profile).await;
    image_metadata(&pool).await;
    (pool, locker)
}

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9 with disposable app-data/state roots"]
async fn real_next_scheduled_image_holds_picture_with_silent_audio() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let (pool, locker) = prepare_image(&ffmpeg, root.path()).await;
    let host = std::sync::Arc::new(host::NextHost::new(
        pool,
        locker,
        root.path().join("playout"),
    ));
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = root.path().join("output");
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: binary,
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
    let readiness = timeout(Duration::from_secs(30), async {
        while !matches!(
            handle.startup_readiness(),
            HlsStartupReadiness::Ready | HlsStartupReadiness::Failed
        ) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        handle.startup_readiness()
    })
    .await;
    if readiness != Ok(HlsStartupReadiness::Ready) {
        registry.stop_channel("1").await;
        timeout(Duration::from_secs(15), task)
            .await
            .unwrap()
            .unwrap();
        panic!("scheduled image did not become ready: {readiness:?}");
    }
    let playlist = handle.trim_playlist().await.unwrap();
    let segment = output.join(
        playlist
            .lines()
            .find(|line| {
                !line.starts_with('#')
                    && Path::new(line)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
            })
            .unwrap(),
    );
    assert_audio_and_subtitle(&ffmpeg, &ffprobe, &segment, true, "aac").await;
    let mut decode = Command::new(&ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(&segment)
        .args([
            "-ss",
            "2",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=32:18",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(decode).await;
    assert!(registry.stop_channel("1").await);
    timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    assert!(!output.exists());
    assert!(registry.try_get("1").await.is_none());
    assert_eq!(
        pixels.len(),
        32 * 18 * 3,
        "image must last beyond two seconds"
    );
    assert!(
        pixels
            .chunks_exact(3)
            .all(|p| p[2] > 200 && p[0] < 30 && p[1] < 30),
        "decoded output must retain the blue still image, not a fallback frame"
    );
}

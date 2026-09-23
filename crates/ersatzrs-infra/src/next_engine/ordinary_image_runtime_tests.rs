//! Installed producer image subtitles share the single registered input with A/V.
use super::super::super::image_subtitle_runtime_tests as bitmap;
use super::*;

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; installed producer PGS finite/live proof"]
async fn real_next_ordinary_producer_remaps_images_and_preserves_off() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::ordinary_callback_runtime::images::fixture_child_ordinary_images").await;
}

#[tokio::test]
#[ignore = "child of real_next_ordinary_producer_remaps_images_and_preserves_off"]
async fn fixture_child_ordinary_images() {
    for (live, off) in [(false, false), (true, false), (false, true), (true, true)] {
        run_case(live, off).await;
    }
}

async fn source(ffmpeg: &Path, root: &Path) -> PathBuf {
    let clip = bitmap::generate_bitmap_clip(ffmpeg, root).await;
    let reordered = root.join("reordered.mkv");
    let mut mux = Command::new(ffmpeg);
    mux.args(["-v", "error", "-nostdin", "-i"])
        .arg(clip)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "0:s:1",
            "-map",
            "0:a",
            "-map",
            "0:s:0",
            "-c",
            "copy",
            "-metadata:s:a:0",
            "title=English",
            "-metadata:s:a:1",
            "title=Dutch",
            "-disposition:s:0",
            "0",
            "-disposition:s:1",
            "default",
        ])
        .arg(&reordered);
    tool(mux).await;
    let source = root.join("source.mkv");
    tokio::fs::rename(reordered, &source).await.unwrap();
    source
}

async fn configure(pool: &crate::db::DbPool, off: bool) {
    sqlx::raw_sql(r#"
        DELETE FROM "MediaStream" WHERE "MediaStreamKind"=3;
        INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES
          (3,'hdmv_pgs_subtitle',3,'eng',0,'English',1,0,100),(4,'hdmv_pgs_subtitle',3,'nld',0,'Dutch',0,0,100);
        INSERT INTO "Subtitle" ("RemoteStreamMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","DateAdded","DateUpdated") VALUES
          (30,0,'English',3,'hdmv_pgs_subtitle',1,0,0,'eng',0,'2026-09-23','2026-09-23'),(30,0,'Dutch',4,'hdmv_pgs_subtitle',0,0,0,'nld',0,'2026-09-23','2026-09-23');
        UPDATE "Channel" SET "SubtitleMode"=2 WHERE "Id"=1;
    "#).execute(pool).await.unwrap();
    if off {
        let mut item = selection::tests::item();
        item.id = 1;
        item.playout_id = 1;
        item.in_point = chrono::Duration::seconds(6);
        item.out_point = chrono::Duration::seconds(30);
        item.finish = item.start + chrono::Duration::seconds(24);
        item.pinned_stream_selection = Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: None,
        });
        crate::repositories::make_repositories(pool.clone())
            .playout
            .upsert_item(&item)
            .await
            .unwrap();
    }
}

async fn run_case(live: bool, off: bool) {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = prepared_artifact_runtime::temporary_root().await;
    let source = source(&ffmpeg, root.path()).await;
    let (pool, locker) = prepare_schedule(&source, false, &text_profile()).await;
    install(&pool, root.path(), live).await;
    configure(&pool, off).await;
    let host = std::sync::Arc::new(host::NextHost::new(
        pool,
        locker,
        root.path().join("playout"),
    ));
    let mut callback = callback_listener(host.clone()).await;
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = root.path().join("output");
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
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
    let saved = root.path().join("proof.ts");
    let captured = retained_callback_runtime::capture(handle.as_ref(), &output, &saved).await;
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    callback.stop().await;
    captured.unwrap();
    assert!(!output.exists());
    assert_eq!(
        tokio::fs::read_to_string(root.path().join("producer-starts"))
            .await
            .unwrap(),
        "play\n"
    );
    let seek = tokio::fs::read_to_string(root.path().join("producer-seek"))
        .await
        .unwrap();
    if live {
        assert_eq!(seek.trim(), "0");
    } else {
        assert!((7.0..12.0).contains(&vtt_seconds(seek.trim())));
    }
    bitmap::assert_bitmap_output(&ffmpeg, &saved, off).await;
}

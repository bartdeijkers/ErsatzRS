//! Installed ordinary producer with source-owned seek and decoded content proof.
use super::remote_subtitle_runtime_helpers::{capture_vtt, configure_sidecar};
use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;
use sqlx::Row as _;

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated installed ordinary producer"]
async fn real_next_ordinary_producer_preserves_seek_audio_and_single_start() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::ordinary_callback_runtime::fixture_child_ordinary_callback").await;
}

pub(super) async fn generate_source(ffmpeg: &Path, root: &Path) -> PathBuf {
    let original = root.join("original.mkv");
    generate_clip(ffmpeg, &original).await;
    let source = root.join("source.mkv");
    let mut command = Command::new(ffmpeg);
    command.args(["-v","error","-nostdin","-i"]).arg(original)
        .args(["-map","0","-vf","drawbox=color=red:t=fill:enable='lt(t,5)',drawbox=color=green:t=fill:enable='gte(t,12)'",
            "-c:v","libx264","-preset","ultrafast","-g","25","-threads","2",
            "-c:a","copy","-metadata:s:a:0","title=English",
            "-metadata:s:a:1","title=Dutch"])
        .arg(&source);
    tool(command).await;
    source
}

pub(super) async fn install(pool: &crate::db::DbPool, root: &Path, live: bool) {
    let script = r#"#!/bin/sh
set -eu
printf '%s\n' "$1" >> ../producer-starts
printf '%s\n' "$ERSATZRS_REMOTE_STREAM_SEEK" > ../producer-seek
printf '%s\n' "$ERSATZRS_REMOTE_STREAM_URL" > ../producer-url
exec "$FFMPEG_BIN" -v error -nostdin -ss "$ERSATZRS_REMOTE_STREAM_SEEK" -i ../source.mkv -map 0 -c copy -f matroska pipe:1
"#;
    let installed = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
        &root.join("package"),
        script,
        &["example.test"],
    )
    .await;
    let row = sqlx::query(r#"SELECT * FROM "AddonInstallation" LIMIT 1"#)
        .fetch_one(&installed)
        .await
        .unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_str(&row.get::<String, _>("ManifestJson")).unwrap();
    manifest["capabilities"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({"id":"remote-stream.play.v1"}));
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt") VALUES ('org.ersatzrs.addon.fixture',?,1,?,?,?,?,?,?)"#)
        .bind(row.get::<String,_>("Version")).bind(row.get::<String,_>("PackageSha256"))
        .bind(row.get::<String,_>("ContentSha256")).bind(manifest.to_string())
        .bind(row.get::<String,_>("InstallPath")).bind(row.get::<String,_>("InstalledAt"))
        .bind(row.get::<String,_>("UpdatedAt")).execute(pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url","Liveness","IsLive")
          VALUES (10,'org.ersatzrs.addon.fixture','item','https://example.test/item',1,0);
        UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=10 WHERE "Id"=100;
        DELETE FROM "Subtitle";
        DELETE FROM "MovieMetadata" WHERE "MovieId"=10;
        DELETE FROM "Movie" WHERE "Id"=10;
        INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated")
          VALUES (30,10,0,'Synthetic','2026-09-23','2026-09-23');
        UPDATE "MediaFile" SET "Path"='unavailable/published-only.mkv' WHERE "Id"=1;
        UPDATE "Channel" SET "SubtitleMode"=0 WHERE "Id"=1;
    "#).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "RemoteStream" SET "Liveness"=?,"IsLive"=? WHERE "Id"=10"#)
        .bind(if live { 2 } else { 1 })
        .bind(i32::from(live))
        .execute(pool)
        .await
        .unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.in_point = chrono::Duration::seconds(6);
    item.out_point = chrono::Duration::seconds(30);
    item.finish = item.start + chrono::Duration::seconds(24);
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "child of real_next_ordinary_producer_preserves_seek_audio_and_single_start"]
async fn fixture_child_ordinary_callback() {
    for (live, sidecar, relative) in [
        (false, None, false),
        (true, None, false),
        (true, Some(false), false),
        (true, Some(true), false),
        (true, Some(false), true),
        (true, Some(true), true),
        (false, Some(false), false),
        (false, Some(true), false),
        (false, Some(false), true),
        (false, Some(true), true),
    ] {
        run_case(live, sidecar, relative).await;
    }
}

async fn run_case(live: bool, sidecar: Option<bool>, relative: bool) {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = prepared_artifact_runtime::temporary_root().await;
    let source = generate_source(&ffmpeg, root.path()).await;
    let (pool, locker) = prepare_schedule(&source, false, &text_profile()).await;
    install(&pool, root.path(), live).await;
    configure_sidecar(&pool, root.path(), sidecar, relative).await;
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
        .await;
    let Some(task) = task else {
        callback.stop().await;
        panic!("ordinary startup was not admitted");
    };
    let handle = registry.try_get("1").await.unwrap();
    let saved = root.path().join("proof.ts");
    let result = retained_callback_runtime::capture(handle.as_ref(), &output, &saved).await;
    let vtt = if sidecar == Some(true) && result.is_ok() {
        Some(capture_vtt(handle.as_ref(), &output).await)
    } else {
        None
    };
    let stopped = registry.stop_channel("1").await;
    task.await.unwrap();
    callback.stop().await;
    assert!(stopped);
    result.unwrap();
    assert!(!output.exists());
    assert_eq!(
        tokio::fs::read_to_string(root.path().join("producer-starts"))
            .await
            .unwrap(),
        "play\n"
    );
    assert_eq!(
        tokio::fs::read_to_string(root.path().join("producer-url"))
            .await
            .unwrap(),
        "https://example.test/item\n"
    );
    let seek = tokio::fs::read_to_string(root.path().join("producer-seek"))
        .await
        .unwrap();
    if live {
        assert_eq!(seek.trim(), "0");
    } else {
        let seconds = vtt_seconds(seek.trim());
        assert!(
            (7.0..12.0).contains(&seconds),
            "producer seek must include scheduled in-point and elapsed time: {seconds}"
        );
    }
    super::super::retained_nut_runtime_tests::assert_selected_audio(&ffmpeg, &saved).await;
    if let Some(convert) = sidecar {
        assert_text_video_and_audio(&ffmpeg, &saved, !convert).await;
        if convert {
            let (body, duration) = vtt.expect("VTT capture attempted").unwrap();
            assert_text_vtt(&body, true, duration);
        }
    }
    assert_source_clock(&ffmpeg, &saved, live).await;
}

async fn assert_source_clock(ffmpeg: &Path, saved: &Path, live: bool) {
    if live {
        let mut decode = Command::new(ffmpeg);
        decode
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(saved)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-vf",
                "scale=1:1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ]);
        let pixel = tool(decode).await;
        assert_eq!(pixel.len(), 3);
        assert!(
            pixel[0] > 180 && pixel[1] < 40 && pixel[2] < 40,
            "live source must start at producer boundary"
        );
    } else {
        super::super::retained_nut_runtime_tests::assert_blue_video(ffmpeg, saved).await;
    }
}

#[path = "ordinary_image_runtime_tests.rs"]
mod images;

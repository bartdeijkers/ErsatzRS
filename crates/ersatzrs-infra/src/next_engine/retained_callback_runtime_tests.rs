//! Installed retained producer -> registered one-shot reader -> official Next.
//! Actual request intervals drive fixture `FFmpeg` cuts; published source captions
//! use native timeline mapping and provenance before Burn/Convert playback.
use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListLifecycleRepository, AddonMediaListOptionSchema, AddonMediaListOptionSchemaRef,
    AddonMediaListRevisionArtifacts, PlayoutRepository,
};
use sha2::{Digest as _, Sha256};
use sqlx::Row as _;
use std::fmt::Write as _;

#[tokio::test]
#[ignore = "requires official Next, FFmpeg and Python 3; isolated retained producer"]
async fn real_next_retained_callback_uses_one_registered_producer() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::retained_callback_runtime::fixture_child_retained_callback").await;
}

async fn install_producer(pool: &crate::db::DbPool, root: &Path) -> String {
    let source = timeline_fixture::PRODUCER;
    let installation = crate::addon_media_list::tests::fixture_media_list_addon_with_script(
        &root.join("package"),
        source,
        &["example.test"],
    )
    .await;
    let row = sqlx::query(r#"SELECT * FROM "AddonInstallation" LIMIT 1"#)
        .fetch_one(&installation)
        .await
        .unwrap();
    let original: String = row.get("ManifestJson");
    let mut manifest: serde_json::Value = serde_json::from_str(&original).unwrap();
    let declaration = serde_json::json!({"id":"media-list.options.v1","options":[{
        "key":"enabled","label":{"en-US":"Enabled"},"description":{"en-US":"Fixture"},
        "scope":"both","operation_targets":["preparation","playback"],
        "applicability":"finite_only","kind":"boolean","default":true,"requires_preparation":true
    }]});
    manifest["manifest_version"] = serde_json::json!(5);
    manifest["capabilities"].as_array_mut().unwrap().extend([
        serde_json::json!({"id":"media-list.item-runtime.v1"}),
        serde_json::json!({"id":ersatzrs_addon_contract::CAPABILITY_REMOTE_STREAM_PLAY_V4}),
        declaration.clone(),
    ]);
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt") VALUES ('org.ersatzrs.addon.fixture',?,1,?,?,?,?,?,?)"#)
        .bind(row.get::<String,_>("Version"))
        .bind(row.get::<String,_>("PackageSha256"))
        .bind(row.get::<String,_>("ContentSha256"))
        .bind(manifest.to_string())
        .bind(row.get::<String,_>("InstallPath"))
        .bind(row.get::<String,_>("InstalledAt"))
        .bind(row.get::<String,_>("UpdatedAt"))
        .execute(pool).await.unwrap();
    crate::addon_media_list_item_runtime::InstalledItemRuntime::load(
        pool,
        "org.ersatzrs.addon.fixture",
        None,
    )
    .await
    .expect("installed fixture runtime");
    let declaration: ersatzrs_addon_contract::AddonCapability =
        serde_json::from_value(declaration).unwrap();
    let definition_json = serde_json::to_string(&declaration).unwrap();
    let mut digest = String::new();
    for byte in Sha256::digest(definition_json.as_bytes()) {
        write!(digest, "{byte:02x}").unwrap();
    }
    crate::repositories::SqliteAddonMediaListRepository(pool.clone())
        .register_option_schema(&AddonMediaListOptionSchema {
            schema: AddonMediaListOptionSchemaRef {
                addon_id: "org.ersatzrs.addon.fixture".into(),
                capability_id: declaration.id,
                schema_digest: digest.clone(),
            },
            definition_json,
            created_at: chrono::Utc::now(),
        })
        .await
        .unwrap();
    digest
}

async fn publish_retained(pool: &crate::db::DbPool, root: &Path, nut: &Path, convert: bool) {
    prepared_artifact_runtime::publish_fixture(pool, nut).await;
    let digest = install_producer(pool, root).await;
    sqlx::raw_sql(r#"
        UPDATE "RemoteStream" SET "AddonId"='org.ersatzrs.addon.fixture',"Url"='https://example.test/item' WHERE "Id"=10;
        UPDATE "AddonMediaList" SET "AddonId"='org.ersatzrs.addon.fixture',"PublishedRevision"=2 WHERE "Id"=1;
        UPDATE "AddonMediaListItem" SET "SourceUrl"='https://example.test/item' WHERE "Id"=11;
        UPDATE "Channel" SET "SubtitleMode"=0 WHERE "Id"=1;
        UPDATE "MediaVersion" SET "Duration"='00:00:24' WHERE "Id"=100;
        DELETE FROM "AddonInstallation" WHERE "AddonId"='org.example.fixture';
    "#).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "AddonMediaList" SET "DraftOptionSchemaId"=(SELECT "Id" FROM "AddonMediaListOptionSchema" WHERE "SchemaDigest"=?) WHERE "Id"=1"#)
        .bind(digest).execute(pool).await.unwrap();
    let excluded = 12_000..18_000;
    let plan = ersatzrs_core::addon_media_list_playback::AddonMediaListPlaybackTimelinePlan::new(
        30_000,
        0..30_000,
        std::slice::from_ref(&excluded),
        "a".repeat(64),
    )
    .unwrap();
    let files = timeline_fixture::subtitles(pool, root, &plan, convert).await;
    crate::repositories::SqliteAddonMediaListRepository(pool.clone())
        .retain_published_revision(
            1,
            2,
            0,
            chrono::Utc::now(),
            &files,
            &AddonMediaListRevisionArtifacts {
                ids: vec![2],
                subtitles: [("item".into(), vec![2])].into(),
                streaming_plans: [("item".into(), plan)].into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.in_point = chrono::Duration::seconds(8);
    item.out_point = chrono::Duration::seconds(24);
    item.finish = item.start + item.out_point - item.in_point;
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
}

pub(super) async fn capture(
    handle: &dyn ersatzrs_core::hls::HlsSessionHandle,
    output: &Path,
    saved: &Path,
) -> Result<(), String> {
    timeout(Duration::from_secs(30), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => return Err("retained session failed".into()),
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
        let playlist = handle.trim_playlist().await.ok_or("missing playlist")?;
        let segment = playlist
            .lines()
            .find(|line| {
                !line.starts_with('#')
                    && Path::new(line)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
            })
            .ok_or("missing transport segment")?;
        tokio::fs::copy(output.join(segment), saved)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|_| "retained session timed out".to_string())?
}

#[tokio::test]
#[ignore = "child of real_next_retained_callback_uses_one_registered_producer"]
async fn fixture_child_retained_callback() {
    for convert in [false, true] {
        retained_case(convert).await;
    }
}

async fn retained_case(convert: bool) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = prepared_artifact_runtime::temporary_root().await;
    let nut = timeline_fixture::generate_source(&ffmpeg, root.path()).await;
    let (pool, locker) = prepare_schedule(&nut, false, &text_profile()).await;
    publish_retained(&pool, root.path(), &nut, convert).await;
    let host = std::sync::Arc::new(host::NextHost::new(
        pool,
        locker,
        root.path().join("playout"),
    ));
    let mut callback = callback_listener(host.clone()).await;
    let output = root.path().join("output");
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let started = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: binary,
                output: output.clone(),
                ffmpeg: ffmpeg.clone(),
                ffprobe,
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await;
    let Some(task) = started else {
        callback.stop().await;
        panic!("retained startup was not admitted")
    };
    let handle = registry.try_get("1").await.unwrap();
    let saved = root.path().join("proof.ts");
    let result = timeline_fixture::capture_program(handle.as_ref(), &output, &saved).await;

    let stopped = registry.stop_channel("1").await;
    task.await.unwrap();
    callback.stop().await;
    assert!(stopped);
    let captions = result.unwrap();
    assert!(!output.exists());
    assert_eq!(
        tokio::fs::read_to_string(root.path().join("producer-starts"))
            .await
            .unwrap(),
        "play-intervals\n"
    );
    assert_request(root.path()).await;
    super::super::retained_nut_runtime_tests::assert_selected_audio(&ffmpeg, &saved).await;
    timeline_fixture::assert_timeline(&ffmpeg, &saved, &captions, convert).await;
}

async fn assert_request(root: &Path) {
    let request: ersatzrs_addon_contract::RemoteStreamRetainedPlayRequest =
        serde_json::from_slice(&tokio::fs::read(root.join("request.json")).await.unwrap()).unwrap();
    assert_eq!(request.source_url, "https://example.test/item");
    assert_eq!(request.provider_id, "item");
    assert_eq!(request.source_duration_milliseconds, 30_000);
    assert_eq!(request.context_fingerprint, "a".repeat(64));
    assert_eq!(request.max_video_height, Some(180));
    assert_eq!(
        serde_json::to_value(&request.options).unwrap(),
        serde_json::json!({"schema":"media-list.options.v1","values":{"enabled":true}})
    );
    assert_eq!(request.retained_intervals_milliseconds.len(), 2);
    assert!((9_000..12_000).contains(&request.retained_intervals_milliseconds[0].start));
    assert_eq!(request.retained_intervals_milliseconds[0].end, 12_000);
    assert_eq!(request.retained_intervals_milliseconds[1], 18_000..30_000);
}

#[path = "retained_timeline_runtime_fixture.rs"]
mod timeline_fixture;

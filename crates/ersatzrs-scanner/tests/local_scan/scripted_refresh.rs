use super::*;
use chrono::{Duration, Utc};
use ersatzrs_core::{
    locking::EntityLocker,
    repositories::{ChannelRepository, FfmpegProfileRepository},
    scheduling::{PlayoutBuildMode, PlayoutBuildRequest},
};
use ersatzrs_domain::{Channel, FFmpegProfile};
use ersatzrs_infra::{
    maintenance::run_requested_playout_build, repositories::make_repositories,
    scripted_dependencies::resolve_pending,
};

async fn seed_playout(pool: &SqlitePool, script: &Path) {
    let repos = make_repositories(pool.clone());
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'fixture', 1920, 1080)"#).await.unwrap();
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "fixture".to_owned(),
            resolution_id: 1,
            ..FFmpegProfile::default()
        })
        .await
        .unwrap();
    repos
        .channel
        .upsert(&Channel {
            id: 10,
            number: "10".to_owned(),
            name: "Scripted fixture".to_owned(),
            ffmpeg_profile_id: 1,
            ..Channel::default()
        })
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed", "ScheduleFile") VALUES (10, 10, 4, 42, ?)"#)
        .bind(script.to_str().unwrap()).execute(pool).await.unwrap();
}

async fn rebuild(pool: &SqlitePool, mode: PlayoutBuildMode, now: chrono::DateTime<Utc>) {
    run_requested_playout_build(
        pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(10, mode),
        now,
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires native ffprobe and the Big Buck Bunny fixture"]
async fn local_nfo_scan_refreshes_selected_scripted_future_without_replacing_current_programme() {
    let temp = TempDir::new("ersatzrs-scripted-scan");
    let ffprobe = ffprobe_binary(temp.path()).expect("native ffprobe required");
    let fixture = big_buck_bunny_fixture().expect("Big Buck Bunny fixture required");
    let library = temp.path().join("library");
    std::fs::create_dir_all(&library).unwrap();
    link_or_copy_fixture(&fixture, &library.join("Original.mp4"));
    let pool = test_db().await;
    seed_local_library(&pool, &library, 1).await;
    let config = LocalScanConfig {
        deep_scan: false,
        library_id: 1,
        ffprobe_path: ffprobe,
    };
    scan_local_library(&pool, &config).await.unwrap();
    let script = temp.path().join("schedule.lua");
    std::fs::write(&script, "add_search('selected', 'library_id:1 AND tag:Selected')\nadd_search('main', 'library_id:1 AND NOT tag:Selected', 'chronological')\nwhile not is_done() do add_count('selected', 1) add_count('main', 1) end").unwrap();
    seed_playout(&pool, &script).await;
    let now = Utc::now();
    rebuild(&pool, PlayoutBuildMode::Reset, now).await;
    let repos = make_repositories(pool.clone());
    let original = repos
        .scheduling
        .scripted_reference_data(10, std::collections::BTreeMap::new(), None)
        .await
        .unwrap();
    let current = original.existing_items[0].clone();
    scan_local_library(&pool, &config).await.unwrap();
    resolve_pending(&pool, Utc::now()).await.unwrap();
    assert_eq!(count(&pool, "ScanPlayoutRefreshIntent").await, 0);
    link_or_copy_fixture(&fixture, &library.join("Selected.mp4"));
    std::fs::write(
        library.join("Selected.nfo"),
        "<movie><title>Selected upload</title><tag>Selected</tag></movie>",
    )
    .unwrap();
    scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(count(&pool, "ScriptedSourceRefreshWork").await, 1);
    resolve_pending(&pool, Utc::now()).await.unwrap();
    let generation: i64 = sqlx::query_scalar(
        r#"SELECT "Generation" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 10"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    scan_local_library(&pool, &config).await.unwrap();
    resolve_pending(&pool, Utc::now()).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "Generation" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 10"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        generation
    );
    rebuild(
        &pool,
        PlayoutBuildMode::Refresh,
        current.start + Duration::seconds(1),
    )
    .await;
    let refreshed = repos
        .scheduling
        .scripted_reference_data(10, std::collections::BTreeMap::new(), None)
        .await
        .unwrap();
    assert_eq!(refreshed.existing_items[0], current);
    assert!(refreshed
        .existing_items
        .iter()
        .any(|item| item.start >= current.finish
            && item.media_item_id != current.media_item_id
            && item.scheduling_context.as_deref().is_some_and(|context| {
                serde_json::from_str::<serde_json::Value>(context).unwrap()["scriptContent"]
                    == "selected"
            })));
    assert_eq!(count(&pool, "ScanPlayoutRefreshIntent").await, 0);
    let revision: i64 = sqlx::query_scalar(
        r#"SELECT "Revision" FROM "ScriptedLibraryRevision" WHERE "LibraryId" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    std::fs::write(library.join("Broken.mp4"), b"invalid video").unwrap();
    assert!(scan_local_library(&pool, &config).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "Revision" FROM "ScriptedLibraryRevision" WHERE "LibraryId" = 1"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        revision
    );
}

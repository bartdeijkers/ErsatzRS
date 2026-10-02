use super::scripted_sources::{build, database};
use super::*;
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::repositories::PlayoutRepository;
use ersatzrs_core::scheduling::PlayoutBuildMode;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::maintenance::{run_maintenance_once_cancellable, MaintenanceExecution};
use ersatzrs_infra::{
    cache_cleanup::CacheCleanupConfig, db::DatabaseSchemaStatus, health::HealthCheckCoordinator,
};
use std::sync::Arc;
use tokio::sync::RwLock;

async fn snapshot(pool: &SqlitePool, id: i64) -> serde_json::Value {
    let repos = make_repositories(pool.clone());
    let reference = repos
        .scheduling
        .scripted_reference_data(id, BTreeMap::new(), None)
        .await
        .unwrap();
    let mut playout = repos.playout.get_with_anchor(id).await.unwrap();
    // Failure reporting may change; published rows and continuation state may not.
    playout.build_status = None;
    serde_json::json!({
        "items": reference.existing_items,
        "history": reference.existing_history,
        "playout": playout,
    })
}

#[tokio::test]
async fn scripted_publication_rejects_invalid_candidates_without_changing_persisted_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    std::fs::write(&path, "add_playlist('pl', 'nightly', 'prime')\nwatermark_on('bug')\ngraphics_on_path('lower-third.svg')\nwhile not is_done() do add_count('pl', 1) end").unwrap();
    let pool = database("sqlite::memory:", &path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start = common::fixed_time();
    let first = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(48),
        PlayoutBuildMode::Reset,
    )
    .await;
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    common::seed_profile_and_channel(&repos, 904).await;
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (904, 904, 0, 99)"#).await.unwrap();
    let mut other_item = first.added_items[0].clone();
    other_item.id = 900_000;
    other_item.playout_id = 904;
    repos.playout.upsert_item(&other_item).await.unwrap();
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let valid = build(
        &pool,
        playout.clone(),
        start + Duration::hours(1),
        start + Duration::hours(50),
        PlayoutBuildMode::Continue,
    )
    .await;
    let before = snapshot(&pool, 903).await;
    let other_before = snapshot(&pool, 904).await;
    for fault in 0..7 {
        let mut candidate = valid.clone();
        // Include a cleanup request to prove rejection occurs before deletion.
        candidate.remove_before = Some(start + Duration::hours(1));
        match fault {
            0 => candidate.added_items[0].start -= Duration::minutes(1),
            1 => candidate.added_items.swap(0, 1),
            2 => candidate.added_items[0].finish = candidate.added_items[0].start,
            3 => candidate.added_items[0].playout_id = 904,
            4 => candidate.anchor.as_mut().unwrap().next_start += Duration::seconds(1),
            5 => candidate.items_to_remove.push(other_item.id),
            _ => candidate.history_to_remove.push(1),
        }
        let error = repos
            .playout
            .apply_build_result(&candidate, &playout, Duration::zero())
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("Scripted timeline rejected"),
            "{fault}: {error}"
        );
        assert_eq!(snapshot(&pool, 903).await, before, "fault {fault}");
        assert_eq!(snapshot(&pool, 904).await, other_before);
    }
    let mut invalid_replacement = first.clone();
    invalid_replacement.added_items.swap(0, 1);
    assert!(repos
        .playout
        .apply_build_result(&invalid_replacement, &playout, Duration::zero())
        .await
        .is_err());
    assert_eq!(snapshot(&pool, 903).await, before);
    repos
        .playout
        .apply_build_result(&valid, &playout, Duration::zero())
        .await
        .unwrap();
    // A candidate built before another publication is rejected under the write lock.
    let published = snapshot(&pool, 903).await;
    assert!(repos
        .playout
        .apply_build_result(&valid, &playout, Duration::zero())
        .await
        .is_err());
    assert_eq!(snapshot(&pool, 903).await, published);
    assert_eq!(snapshot(&pool, 904).await, other_before);
}

#[tokio::test]
async fn scripted_automatic_maintenance_continues_a_published_future_horizon_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    std::fs::write(&path, "add_marathon('pl', {tvdb='show-guid'}, {}, 'show')\nwhile not is_done() do add_count('pl', 1) end").unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("automatic.db").display()
    );
    let pool = database(&url, &path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start =
        ersatzrs_core::scheduling::truncate_to_persisted_precision(Utc::now() - Duration::hours(1));
    let first = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(48),
        PlayoutBuildMode::Reset,
    )
    .await;
    let oracle = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(51),
        PlayoutBuildMode::Reset,
    )
    .await;
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    drop(repos);
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let before = snapshot(&pool, 903).await;
    let health = HealthCheckCoordinator::new(
        pool.clone(),
        Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests())),
        DatabaseSchemaStatus::default(),
    );
    let report = run_maintenance_once_cancellable(
        pool.clone(),
        &CacheCleanupConfig::default(),
        None,
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
        None,
        MaintenanceExecution {
            health: Some(&health),
            ..MaintenanceExecution::default()
        },
    )
    .await
    .unwrap();
    assert_eq!((report.playouts_built, report.playouts_failed), (1, 0));
    let repos = make_repositories(pool.clone());
    let reference = repos
        .scheduling
        .scripted_reference_data(903, BTreeMap::new(), None)
        .await
        .unwrap();
    assert!(reference.existing_items.len() > first.added_items.len());
    assert_eq!(
        serde_json::to_value(&reference.existing_items[..first.added_items.len()]).unwrap(),
        before["items"]
    );
    assert_eq!(
        serde_json::to_value(&reference.existing_history[..first.added_history.len()]).unwrap(),
        before["history"]
    );
    for (actual, expected) in reference.existing_items.iter().zip(&oracle.added_items) {
        assert_eq!(
            (actual.media_item_id, actual.start, actual.finish),
            (expected.media_item_id, expected.start, expected.finish)
        );
    }
    assert!(reference
        .existing_items
        .windows(2)
        .all(|pair| pair[0].finish == pair[1].start));
    assert!(
        repos
            .playout
            .get_with_anchor(903)
            .await
            .unwrap()
            .build_status
            .unwrap()
            .success
    );

    assert_scripted_failure_and_recovery(&pool, &path, &health).await;
}

async fn assert_scripted_failure_and_recovery(
    pool: &SqlitePool,
    path: &std::path::Path,
    health: &ersatzrs_infra::health::HealthCheckCoordinator,
) {
    let repos = make_repositories(pool.clone());
    let published = snapshot(pool, 903).await;
    std::fs::write(path, "add_marathon('pl', {tvdb='show-guid'}, {}, 'show')\nadd_count('pl', 1)\nwait_until('00:00')\nadd_count('pl', 1)").unwrap();
    let request = ersatzrs_core::scheduling::PlayoutBuildRequest {
        playout_id: 903,
        mode: PlayoutBuildMode::Continue,
    };
    let error = ersatzrs_infra::maintenance::run_requested_playout_build(
        pool,
        &EntityLocker::new(),
        request,
        Utc::now(),
        None,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Scripted timeline rejected:"));
    assert_eq!(snapshot(pool, 903).await, published);
    let row = repos
        .playout
        .list_admin("", 0, 10)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.id == 903)
        .unwrap();
    assert_eq!(row.build_success, Some(false));
    assert!(row
        .build_message
        .unwrap()
        .contains("Scripted timeline rejected:"));
    assert_eq!(repos.playout.count_failed_builds_admin().await.unwrap(), 1);
    let failed_health = health.run(true).await.unwrap();
    let failed = failed_health
        .results
        .iter()
        .find(|row| row.title == "Scripted Playout State")
        .unwrap();
    assert_eq!(
        failed.status,
        ersatzrs_core::health::HealthCheckStatus::Fail
    );
    assert!(
        failed.message.contains("Scripted timeline rejected:"),
        "{failed:?}"
    );

    std::fs::write(path, "add_marathon('pl', {tvdb='show-guid'}, {}, 'show')\nwhile not is_done() do add_count('pl', 1) end").unwrap();
    ersatzrs_infra::maintenance::run_requested_playout_build(
        pool,
        &EntityLocker::new(),
        request,
        Utc::now(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(repos.playout.count_failed_builds_admin().await.unwrap(), 0);
    let recovered = health.run(true).await.unwrap();
    assert_eq!(
        recovered
            .results
            .iter()
            .find(|row| row.title == "Scripted Playout State")
            .unwrap()
            .status,
        ersatzrs_core::health::HealthCheckStatus::Pass
    );
}

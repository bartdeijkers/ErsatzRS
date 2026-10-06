use super::scripted_sources::{build, database};
use super::*;
use ersatzrs_core::repositories::PlayoutPendingWork;
use ersatzrs_core::{repositories::PlayoutRepository, scheduling::PlayoutBuildMode};
use ersatzrs_infra::{
    scan_refresh,
    scripted_dependencies::{record_successful_scan, resolve_pending},
};

async fn fixture(url: &str, path: &std::path::Path) -> SqlitePool {
    std::fs::write(path, "add_search('new', 'library_id:1 AND tag:Selected')\nadd_collection('main', 'prime', 'chronological')\nwhile not is_done() do add_count('new', 1) add_count('main', 1) end").unwrap();
    let pool = database(url, path).await;
    common::seed_media_path(&pool, 2, 2, 2).await;
    publish(&pool, 903).await;
    pool
}

async fn publish(pool: &SqlitePool, id: i64) {
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(id).await.unwrap();
    let start = common::fixed_time();
    let result = build(
        pool,
        playout.clone(),
        start,
        start + Duration::hours(3),
        PlayoutBuildMode::Reset,
    )
    .await;
    repos
        .playout
        .apply_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
}

async fn tag_selected(pool: &SqlitePool) {
    pool.execute(r#"INSERT INTO "Tag" ("MovieMetadataId", "Name") VALUES (200, 'Selected')"#)
        .await
        .unwrap();
}

async fn intent_generation(pool: &SqlitePool) -> Option<i64> {
    sqlx::query_scalar(
        r#"SELECT "Generation" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 903"#,
    )
    .fetch_optional(pool)
    .await
    .unwrap()
}

async fn adaptation_status(pool: &SqlitePool) -> ersatzrs_core::repositories::PlayoutListAdminRow {
    make_repositories(pool.clone())
        .playout
        .list_admin("", 0, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|row| row.id == 903)
        .unwrap()
}

async fn requested_refresh(
    pool: &SqlitePool,
    start: chrono::DateTime<Utc>,
) -> ersatzrs_infra::maintenance::Result<bool> {
    ersatzrs_infra::maintenance::run_requested_playout_build_cancellable(
        pool,
        &ersatzrs_core::locking::EntityLocker::default(),
        ersatzrs_core::scheduling::PlayoutBuildRequest::new(903, PlayoutBuildMode::Refresh),
        start,
        None,
        None,
    )
    .await
}

async fn assert_requested_refresh_defers(pool: &SqlitePool, start: chrono::DateTime<Utc>) {
    let mut tx = ersatzrs_infra::db::begin_write(pool).await.unwrap();
    scan_refresh::persist_unowned_refresh_intents(
        &mut tx,
        ersatzrs_domain::ScanPlayoutRefreshReason::ScriptedSourceChanged,
        &[903],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let error = requested_refresh(pool, start).await.unwrap_err();
    assert!(error.to_string().contains("scan"), "{error}");
    let retry: (i64, i64, String) = sqlx::query_as(r#"SELECT "State", "Attempts", "Diagnostic" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 903"#).fetch_one(pool).await.unwrap();
    assert_eq!(
        retry,
        (0, 1, "scripted-source-publication-deferred".to_owned())
    );
    assert!(scan_refresh::pending_refreshes(pool)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn scripted_publication_waits_for_a_claimed_scan_and_restart_replay() {
    use ersatzrs_core::repositories::{
        ManagedLibraryOperationRepository as _, ManagedLibraryScanSpec,
    };
    use ersatzrs_domain::{ManagedLibraryOperationKind, MediaSourceKind};
    use ersatzrs_infra::repositories::SqliteManagedLibraryOperationRepository;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let url = format!("sqlite://{}", directory.path().join("scan.db").display());
    let pool = fixture(&url, &path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start = common::fixed_time() + Duration::seconds(1);
    let candidate = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(3),
        PlayoutBuildMode::Refresh,
    )
    .await;
    let before = super::scripted_continuation::snapshot(&pool, 903).await;
    let operations = SqliteManagedLibraryOperationRepository(pool.clone());
    let operation = operations
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            1,
            ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: false,
            },
            false,
        )
        .await
        .unwrap()
        .unwrap();
    let claimed = operations
        .claim_operation(operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    // No input has changed yet: this specifically verifies the in-flight fence.
    let error = repos
        .playout
        .apply_build_result(&candidate, &playout, Duration::zero())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("scan"), "{error}");
    assert_eq!(
        super::scripted_continuation::snapshot(&pool, 903).await,
        before
    );
    assert_requested_refresh_defers(&pool, start).await;
    drop(repos);
    drop(operations);
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let repos = make_repositories(pool.clone());
    assert!(
        scan_refresh::recoverable_refreshes(&pool)
            .await
            .unwrap()
            .is_empty(),
        "restart preserves retry eligibility"
    );
    assert!(repos
        .playout
        .apply_build_result(&candidate, &playout, Duration::zero())
        .await
        .is_err());
    // Restart cannot treat an unfinished scan as a stable source snapshot.
    let operations = SqliteManagedLibraryOperationRepository(pool.clone());
    record_successful_scan(&pool, 1).await.unwrap();
    assert_eq!(
        resolve_pending(&pool, Utc::now() + Duration::seconds(2))
            .await
            .unwrap(),
        0,
        "comparison waits until the owning scan finishes"
    );
    let diagnostic: Option<String> = sqlx::query_scalar(
        r#"SELECT "Diagnostic" FROM "ScriptedSourceRefreshWork" WHERE "PlayoutId" = 903"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        diagnostic.is_none(),
        "a running scan is pending work, not a failed comparison"
    );
    assert!(operations.complete_operation(&claimed).await.unwrap());
    assert!(requested_refresh(&pool, start).await.unwrap());
    assert_eq!(intent_generation(&pool).await, None);
}

#[tokio::test]
async fn scripted_publication_rejects_changed_selection_before_scan_comparison() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let pool = fixture("sqlite::memory:", &path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start = common::fixed_time() + Duration::seconds(1);
    let candidate = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(3),
        PlayoutBuildMode::Refresh,
    )
    .await;
    let before = super::scripted_continuation::snapshot(&pool, 903).await;
    tag_selected(&pool).await;
    record_successful_scan(&pool, 1).await.unwrap();
    assert_eq!(intent_generation(&pool).await, None);
    let error = repos
        .playout
        .apply_build_result(&candidate, &playout, Duration::zero())
        .await
        .expect_err("unprocessed scan work must already fence stale publication");
    assert!(error.to_string().contains("source"), "{error}");
    assert_eq!(
        super::scripted_continuation::snapshot(&pool, 903).await,
        before
    );
    assert_eq!(
        resolve_pending(&pool, Utc::now() + Duration::seconds(2))
            .await
            .unwrap(),
        1
    );
    assert_eq!(intent_generation(&pool).await, Some(1));
    let latest = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(3),
        PlayoutBuildMode::Refresh,
    )
    .await;
    repos
        .playout
        .apply_build_result(&latest, &playout, Duration::zero())
        .await
        .unwrap();
}

#[tokio::test]
async fn scripted_publication_accepts_unchanged_selection_after_scan_receipts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let pool = fixture("sqlite::memory:", &path).await;
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(903).await.unwrap();
    let start = common::fixed_time() + Duration::seconds(1);
    let candidate = build(
        &pool,
        playout.clone(),
        start,
        start + Duration::hours(3),
        PlayoutBuildMode::Refresh,
    )
    .await;
    record_successful_scan(&pool, 1).await.unwrap();
    record_successful_scan(&pool, 2).await.unwrap();
    repos
        .playout
        .apply_build_result(&candidate, &playout, Duration::zero())
        .await
        .unwrap();
    assert_eq!(
        resolve_pending(&pool, Utc::now() + Duration::seconds(2))
            .await
            .unwrap(),
        2
    );
    assert_eq!(intent_generation(&pool).await, None);
}

#[tokio::test]
async fn scripted_successful_scan_coalesces_changed_library_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let url = format!("sqlite://{}", directory.path().join("scan.db").display());
    let pool = fixture(&url, &path).await;
    let now = Utc::now() + Duration::seconds(2);
    record_successful_scan(&pool, 1).await.unwrap();
    let status = adaptation_status(&pool).await;
    assert_eq!(status.source_revision, Some(0));
    assert_eq!(status.source_work, PlayoutPendingWork::Pending);
    assert_eq!(status.refresh_work, PlayoutPendingWork::None);
    assert_eq!(resolve_pending(&pool, now).await.unwrap(), 1);
    assert_eq!(intent_generation(&pool).await, None);
    let status = adaptation_status(&pool).await;
    assert_eq!(status.source_revision, Some(0));
    assert_eq!(status.source_work, PlayoutPendingWork::None);
    tag_selected(&pool).await;
    record_successful_scan(&pool, 2).await.unwrap();
    resolve_pending(&pool, now).await.unwrap();
    assert_eq!(
        intent_generation(&pool).await,
        None,
        "another library's successful scan cannot acknowledge this change"
    );
    record_successful_scan(&pool, 1).await.unwrap();
    pool.close().await;
    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let before = super::scripted_continuation::snapshot(&pool, 903).await;
    let (a, b) = tokio::join!(resolve_pending(&pool, now), resolve_pending(&pool, now));
    assert_eq!(a.unwrap() + b.unwrap(), 1);
    assert_eq!(intent_generation(&pool).await, Some(1));
    let status = adaptation_status(&pool).await;
    assert_eq!(status.source_revision, Some(1));
    assert_eq!(status.refresh_work, PlayoutPendingWork::Pending);
    assert_eq!(status.source_work, PlayoutPendingWork::None);
    let pending = scan_refresh::pending_refreshes(&pool).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].reason,
        ersatzrs_domain::ScanPlayoutRefreshReason::ScriptedSourceChanged
    );
    assert_eq!(
        super::scripted_continuation::snapshot(&pool, 903).await,
        before
    );
    record_successful_scan(&pool, 1).await.unwrap();
    resolve_pending(&pool, now).await.unwrap();
    assert_eq!(
        intent_generation(&pool).await,
        Some(1),
        "unchanged scans do not create a new source generation"
    );
    assert_eq!(adaptation_status(&pool).await.source_revision, Some(1));
    pool.execute(r#"DELETE FROM "Tag" WHERE "Name" = 'Selected'"#)
        .await
        .unwrap();
    record_successful_scan(&pool, 1).await.unwrap();
    resolve_pending(&pool, now).await.unwrap();
    assert_eq!(
        intent_generation(&pool).await,
        Some(2),
        "returning to an earlier membership is still a new change"
    );
    assert_eq!(adaptation_status(&pool).await.source_revision, Some(2));
    scan_refresh::fail_refresh(&pool, 903, 2, "rejected")
        .await
        .unwrap();
    let status = adaptation_status(&pool).await;
    assert_eq!(status.refresh_work, PlayoutPendingWork::Failed);
    scan_refresh::complete_refresh(&pool, 903, 2).await.unwrap();
    let status = adaptation_status(&pool).await;
    assert_eq!(status.source_revision, Some(2));
    assert_eq!(status.refresh_work, PlayoutPendingWork::None);
}

#[tokio::test]
async fn scripted_comparison_failure_backs_off_without_blocking_other_playouts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let pool = fixture("sqlite::memory:", &path).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 905).await;
    sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed", "ScheduleFile") VALUES (905, 905, 4, 42, ?)"#).bind(path.to_str().unwrap()).execute(&pool).await.unwrap();
    publish(&pool, 905).await;
    let saved: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT "SourceKey", "Context" FROM "PlayoutScriptedSource" WHERE "PlayoutId" = 903"#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    pool.execute(r#"UPDATE "PlayoutScriptedSource" SET "Context" = '{}' WHERE "PlayoutId" = 903"#)
        .await
        .unwrap();
    tag_selected(&pool).await;
    let now = Utc::now() + Duration::seconds(2);
    record_successful_scan(&pool, 1).await.unwrap();
    assert_eq!(resolve_pending(&pool, now).await.unwrap(), 1);
    assert_eq!(intent_generation(&pool).await, None);
    assert_eq!(
        scan_refresh::pending_refreshes(&pool).await.unwrap()[0].playout_id,
        905
    );
    let failed: (i64, String) = sqlx::query_as(r#"SELECT "Attempts", "Diagnostic" FROM "ScriptedSourceRefreshWork" WHERE "PlayoutId" = 903"#).fetch_one(&pool).await.unwrap();
    assert_eq!(failed, (1, "scripted-source-comparison-failed".to_owned()));
    let status = adaptation_status(&pool).await;
    assert_eq!(status.source_work, PlayoutPendingWork::Failed);
    assert_eq!(status.source_revision, Some(0));
    assert_eq!(
        super::scripted_refresh::scripted_health_status(&pool).await,
        ersatzrs_core::health::HealthCheckStatus::Fail
    );
    let eligibility: String = sqlx::query_scalar(
        r#"SELECT "NextAttempt" FROM "ScriptedSourceRefreshWork" WHERE "PlayoutId" = 903"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    record_successful_scan(&pool, 1).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            r#"SELECT "NextAttempt" FROM "ScriptedSourceRefreshWork" WHERE "PlayoutId" = 903"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        eligibility,
        "new scans cannot shorten comparison backoff"
    );
    for (key, context) in saved {
        sqlx::query(r#"UPDATE "PlayoutScriptedSource" SET "Context" = ? WHERE "PlayoutId" = 903 AND "SourceKey" = ?"#).bind(context).bind(key).execute(&pool).await.unwrap();
    }
    assert_eq!(
        resolve_pending(&pool, now + Duration::seconds(29))
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        resolve_pending(&pool, now + Duration::seconds(31))
            .await
            .unwrap(),
        1
    );
    assert_eq!(intent_generation(&pool).await, Some(1));
    assert_eq!(
        adaptation_status(&pool).await.source_work,
        PlayoutPendingWork::None
    );
    assert_eq!(
        super::scripted_refresh::scripted_health_status(&pool).await,
        ersatzrs_core::health::HealthCheckStatus::Pass
    );
}

#[tokio::test]
async fn scripted_scan_comparison_is_bounded_and_drops_changed_scheduler_ownership() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("schedule.lua");
    let pool = fixture("sqlite::memory:", &path).await;
    let repos = make_repositories(pool.clone());
    for id in 904..920 {
        common::seed_profile_and_channel(&repos, id).await;
        sqlx::query(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed", "ScheduleFile") VALUES (?, ?, 4, 42, ?)"#)
            .bind(id).bind(id).bind(path.to_str().unwrap()).execute(&pool).await.unwrap();
        publish(&pool, id).await;
    }
    record_successful_scan(&pool, 1).await.unwrap();
    pool.execute(r#"UPDATE "Playout" SET "ScheduleKind" = 0 WHERE "Id" = 903"#)
        .await
        .unwrap();
    pool.execute(r#"UPDATE "PlayoutScriptedSource" SET "Context" = 'invalid obsolete source' WHERE "PlayoutId" = 903"#)
        .await
        .unwrap();
    let now = Utc::now() + Duration::seconds(2);
    assert_eq!(resolve_pending(&pool, now).await.unwrap(), 16);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "ScriptedSourceRefreshWork""#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(resolve_pending(&pool, now).await.unwrap(), 1);
    assert!(scan_refresh::pending_refreshes(&pool)
        .await
        .unwrap()
        .is_empty());
}

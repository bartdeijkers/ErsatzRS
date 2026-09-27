mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::maintenance::{ScheduledLibraryScanRequester, ScheduledScanRequestOutcome};
use ersatzrs_core::repositories::{PlayoutRepository, ProgramScheduleRepository};
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
use ersatzrs_domain::{
    ChannelSubtitleMode, CollectionType, PlaybackOrder, PlayoutMode, ProgramScheduleItem,
    ScanPlayoutRefreshReason,
};
use ersatzrs_infra::cache_cleanup::CacheCleanupConfig;
use ersatzrs_infra::db::DatabaseSchemaStatus;
use ersatzrs_infra::maintenance::{
    load_library_refresh_interval, repair_invalid_classic_playouts, run_maintenance_once,
    run_requested_playout_build,
};
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use sqlx::{Executor, Row, SqlitePool};

#[derive(Default)]
struct FakeHlsRegistry {
    playout_updates: Mutex<Vec<String>>,
}

#[async_trait]
impl HlsSessionRegistry for FakeHlsRegistry {
    async fn try_get(&self, _channel_number: &str) -> Option<Arc<dyn HlsSessionHandle>> {
        None
    }

    async fn try_insert(&self, _handle: Arc<dyn HlsSessionHandle>) -> bool {
        false
    }

    async fn insert(&self, _handle: Arc<dyn HlsSessionHandle>) {}

    async fn remove(&self, _channel_number: &str) {}

    async fn register_direct(&self, _handle: Arc<dyn HlsSessionHandle>) {}

    async fn unregister_direct(&self, _handle: &Arc<dyn HlsSessionHandle>) {}

    async fn active_channels(&self) -> Vec<String> {
        Vec::new()
    }

    async fn playout_updated(&self, channel_number: &str) -> bool {
        self.playout_updates
            .lock()
            .expect("fake hls registry lock")
            .push(channel_number.to_owned());
        true
    }
}

struct FakeLibraryScanRequester {
    requests: Mutex<Vec<i64>>,
    outcome: ScheduledScanRequestOutcome,
}

impl FakeLibraryScanRequester {
    fn new(outcome: ScheduledScanRequestOutcome) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            outcome,
        }
    }
}

#[async_trait]
impl ScheduledLibraryScanRequester for FakeLibraryScanRequester {
    async fn request_scheduled_library_scan(&self, library_id: i64) -> ScheduledScanRequestOutcome {
        self.requests
            .lock()
            .expect("fake library scan requester lock")
            .push(library_id);
        self.outcome.clone()
    }
}

#[tokio::test]
async fn maintenance_pass_dispatches_due_libraries_and_records_loop_runs() {
    let pool = common::test_db().await;
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name", "LastScan")
           VALUES (20, 10, 0, 'Movies', NULL)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    // Opt the library refresh loop in (default is "disabled" to match ErsatzTV).
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('scanner.library_refresh_interval', '1')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let library_scans = FakeLibraryScanRequester::new(ScheduledScanRequestOutcome::Accepted);
    let report = run_maintenance_once(
        pool.clone(),
        &CacheCleanupConfig::default(),
        Some(&library_scans),
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
        None,
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
    assert_eq!(
        *library_scans.requests.lock().unwrap(),
        vec![20],
        "the periodic producer must use the application scan boundary"
    );
    let rows = sqlx::query(
        r#"SELECT "Key", "Value"
           FROM "ConfigElement"
           WHERE "Key" LIKE 'maintenance.loop.%'
           ORDER BY "Key""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    // playout, library_refresh, provider_poll, cache_cleanup, artwork_cleanup, subtitle_cleanup, search_index.
    assert_eq!(rows.len(), 7);
    assert!(rows
        .iter()
        .all(|row| !row.get::<String, _>("Value").is_empty()));

    let dispatched_ids = sqlx::query_scalar::<_, String>(
        r#"SELECT "Value"
           FROM "ConfigElement"
           WHERE "Key" = 'maintenance.library_refresh.dispatched_ids'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dispatched_ids, "[20]");
}

#[tokio::test]
async fn maintenance_skips_disabled_library_refresh_loop_by_default() {
    let pool = common::test_db().await;
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name", "LastScan")
           VALUES (20, 10, 0, 'Movies', NULL)"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let report = run_maintenance_once(
        pool.clone(),
        &CacheCleanupConfig::default(),
        None,
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
        None,
    )
    .await
    .unwrap();

    assert!(!report.loops_ran.library_refresh());
    assert_eq!(report.libraries_dispatched, 0);
    let dispatched = sqlx::query(
        r#"SELECT 1 FROM "ConfigElement" WHERE "Key" = 'maintenance.library_refresh.dispatched_ids'"#,
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(dispatched.is_none());
}

#[tokio::test]
async fn maintenance_respects_recent_library_scan_cutoff() {
    let pool = common::test_db().await;
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    // Configure a 24h refresh cadence, then mark the library as having been
    // scanned only one hour ago — the EF datetime format uses a space separator
    // and seven-digit ticks, which the production cutoff string must compare
    // correctly against.
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('scanner.library_refresh_interval', '24')"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let stored = Utc::now() - Duration::hours(1);
    let base = stored.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
    let ticks = stored.timestamp_subsec_nanos() / 100;
    let recent = format!("{base}.{ticks:07}");
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name", "LastScan")
           VALUES (21, 10, 0, 'Movies', ?)"#,
    )
    .bind(recent)
    .execute(&pool)
    .await
    .unwrap();

    let report = run_maintenance_once(
        pool,
        &CacheCleanupConfig::default(),
        None,
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
        None,
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
    assert_eq!(report.libraries_dispatched, 0);
}

#[tokio::test]
async fn maintenance_skips_playouts_with_held_in_memory_lock() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (77, 1, 'Classic', 0)"#,
    )
    .await
    .unwrap();

    let locker = EntityLocker::new();
    assert!(locker.lock_playout(77));

    let report = run_maintenance_once(
        pool,
        &CacheCleanupConfig::default(),
        None,
        &locker,
        DatabaseSchemaStatus::default(),
        None,
    )
    .await
    .unwrap();

    assert!(report.loops_ran.playout());
    assert_eq!(report.playouts_built, 0);
    assert_eq!(report.playouts_skipped_locked, 1);
    assert_eq!(report.playouts_failed, 0);
    // Lock is still held — the maintenance pass must not silently release it.
    assert!(locker.is_playout_locked(77));
}

#[tokio::test]
async fn requested_playout_build_uses_locker_and_writes_status() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_media_path(&pool, 1, 1, 1).await;
    sqlx::query(r#"UPDATE "Channel" SET "PlayoutMode" = 1 WHERE "Id" = 1"#)
        .execute(&pool)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (78, 1, 20, 0)"#,
    )
    .await
    .unwrap();

    let locker = EntityLocker::new();
    let mut intent_tx = ersatzrs_infra::db::begin_write(&pool).await.unwrap();
    ersatzrs_infra::scan_refresh::persist_refresh_intents(
        &mut intent_tx,
        1,
        ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        &[78],
    )
    .await
    .unwrap();
    intent_tx.commit().await.unwrap();
    assert!(locker.lock_playout(78));
    let skipped = run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 78,
            mode: PlayoutBuildMode::Refresh,
        },
        Utc::now(),
        None,
    )
    .await
    .unwrap();
    assert!(!skipped);
    assert_eq!(
        ersatzrs_infra::scan_refresh::pending_refreshes(&pool)
            .await
            .unwrap()
            .len(),
        1,
        "a locked build must leave its durable intent pending for the retry sweep"
    );
    assert!(locker.unlock_playout(78));

    let hls_registry = Arc::new(FakeHlsRegistry::default());
    let built = run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 78,
            mode: PlayoutBuildMode::Reset,
        },
        Utc::now(),
        Some(hls_registry.as_ref()),
    )
    .await
    .unwrap();
    assert!(built);
    assert!(
        ersatzrs_infra::scan_refresh::recoverable_refreshes(&pool)
            .await
            .unwrap()
            .is_empty(),
        "a successful Reset must consume the weaker durable Refresh intent"
    );

    let status = sqlx::query(
        r#"SELECT "Success", "Message"
           FROM "PlayoutBuildStatus"
           WHERE "PlayoutId" = 78"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status.get::<i64, _>("Success"), 1);
    assert_eq!(status.get::<Option<String>, _>("Message"), None);
    assert!(!locker.is_playout_locked(78));
    assert_eq!(
        *hls_registry
            .playout_updates
            .lock()
            .expect("fake hls registry lock"),
        vec!["1".to_owned()]
    );
}

#[tokio::test]
async fn reset_build_regenerates_playout_seed() {
    // ErsatzTV's `ResetPlayout()` regenerates `playout.Seed` on every Reset.
    // Before this fix, ErsatzRS never regenerated an existing playout's seed
    // at all (only at initial creation), so Reset and Refresh reproduced the
    // identical shuffle order forever.
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (79, 1, 20, 12345)"#,
    )
    .await
    .unwrap();

    let locker = EntityLocker::new();
    let built = run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 79,
            mode: PlayoutBuildMode::Reset,
        },
        Utc::now(),
        None,
    )
    .await
    .unwrap();
    assert!(built);

    let seed: i64 = sqlx::query_scalar(r#"SELECT "Seed" FROM "Playout" WHERE "Id" = 79"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(seed, 12345, "Reset should regenerate the playout seed");
}

#[tokio::test]
async fn refresh_and_continue_builds_preserve_playout_seed() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed")
           VALUES (80, 1, 20, 12345)"#,
    )
    .await
    .unwrap();

    let locker = EntityLocker::new();
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Continue] {
        run_requested_playout_build(
            &pool,
            &locker,
            PlayoutBuildRequest {
                playout_id: 80,
                mode,
            },
            Utc::now(),
            None,
        )
        .await
        .unwrap();

        let seed: i64 = sqlx::query_scalar(r#"SELECT "Seed" FROM "Playout" WHERE "Id" = 80"#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            seed, 12345,
            "{mode:?} must not change the playout seed, only Reset should"
        );
    }
}

#[tokio::test]
async fn continue_notifies_only_dependent_next_channels_after_extending_persisted_items() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    for id in 1..=3 {
        common::seed_profile_and_channel(&repos, id).await;
    }
    seed_refresh_rebuild_fixture(&pool, &repos).await;
    pool.execute(r#"UPDATE "Channel" SET "PlayoutSource"=1, "MirrorSourceChannelId"=1, "StreamingEngine"=1 WHERE "Id"=2"#).await.unwrap();
    pool.execute(r#"UPDATE "Channel" SET "StreamingEngine"=1 WHERE "Id"=3"#)
        .await
        .unwrap();
    let now = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let locker = EntityLocker::new();
    run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Reset,
        },
        now,
        None,
    )
    .await
    .unwrap();
    let registry = FakeHlsRegistry::default();
    for (day, expected) in [(1, "2"), (2, "1")] {
        if day == 2 {
            pool.execute(r#"UPDATE "Channel" SET "StreamingEngine"=CASE WHEN "Id"=1 THEN 1 ELSE 0 END WHERE "Id" IN (1,2)"#).await.unwrap();
        }
        let continuation = now + Duration::days(day);
        let previous = repos
            .playout
            .items_in_range(88, continuation, continuation + Duration::days(1))
            .await
            .unwrap();
        assert!(!previous.is_empty());
        let old_finish = previous.iter().map(|item| item.finish).max().unwrap();
        registry.playout_updates.lock().unwrap().clear();
        assert!(run_requested_playout_build(
            &pool,
            &locker,
            PlayoutBuildRequest {
                playout_id: 88,
                mode: PlayoutBuildMode::Continue,
            },
            continuation,
            Some(&registry)
        )
        .await
        .unwrap());
        assert_eq!(
            *registry.playout_updates.lock().unwrap(),
            vec![expected.to_owned()]
        );
        let preserved = repos
            .playout
            .items_in_range(88, continuation, old_finish)
            .await
            .unwrap();
        assert_eq!(
            previous
                .iter()
                .map(|item| (item.id, item.start, item.finish, item.media_item_id))
                .collect::<Vec<_>>(),
            preserved
                .iter()
                .map(|item| (item.id, item.start, item.finish, item.media_item_id))
                .collect::<Vec<_>>(),
            "Continue notifications must not rewrite already scheduled future items"
        );
        let extended = repos
            .playout
            .items_in_range(88, old_finish, continuation + Duration::days(2))
            .await
            .unwrap();
        assert!(
            extended.iter().any(|item| item.finish > old_finish),
            "Continue must actually extend the persisted horizon"
        );
    }
}

#[tokio::test]
async fn cancelled_build_worker_preserves_timeline_and_releases_lock() {
    use ersatzrs_infra::maintenance::{
        run_requested_playout_build_cancellable, PlayoutBuildCancellation,
    };
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;
    let now = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let locker = EntityLocker::new();
    let request = PlayoutBuildRequest {
        playout_id: 88,
        mode: PlayoutBuildMode::Reset,
    };
    run_requested_playout_build(&pool, &locker, request, now, None)
        .await
        .unwrap();
    let before = repos
        .playout
        .items_in_range(88, now, now + Duration::days(3))
        .await
        .unwrap();
    assert!(!before.is_empty());
    let history_before: Vec<(i64, String, String)> = sqlx::query_as(
        r#"SELECT "Id", "When", "Key" FROM "PlayoutHistory" WHERE "PlayoutId" = 88 ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let stopped = Arc::new(AtomicBool::new(false));
    let worker_stopped = stopped.clone();
    let checks = AtomicUsize::new(0);
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let entered_tx = Mutex::new(Some(entered_tx));
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let cancelled: PlayoutBuildCancellation = Arc::new(move || {
        // First check is admission; the second runs on the scheduling worker.
        if checks.fetch_add(1, Ordering::SeqCst) == 1 {
            entered_tx.lock().unwrap().take().unwrap().send(()).unwrap();
            release_rx
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        }
        worker_stopped.load(Ordering::SeqCst)
    });
    let worker_pool = pool.clone();
    let worker_locker = locker.clone();
    let build = tokio::spawn(async move {
        run_requested_playout_build_cancellable(
            &worker_pool,
            &worker_locker,
            request,
            now,
            None,
            Some(&cancelled),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), entered_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(locker.is_playout_locked(88));
    stopped.store(true, Ordering::SeqCst);
    release_tx.send(()).unwrap();
    let error = tokio::time::timeout(std::time::Duration::from_secs(5), build)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("playout build cancelled"));
    assert!(!locker.is_playout_locked(88));
    assert_eq!(
        repos
            .playout
            .items_in_range(88, now, now + Duration::days(3))
            .await
            .unwrap(),
        before
    );
    let history_after: Vec<(i64, String, String)> = sqlx::query_as(
        r#"SELECT "Id", "When", "Key" FROM "PlayoutHistory" WHERE "PlayoutId" = 88 ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(history_after, history_before);
    let status: (bool, String) = sqlx::query_as(
        r#"SELECT "Success", "Message" FROM "PlayoutBuildStatus" WHERE "PlayoutId" = 88"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!status.0);
    assert!(status.1.contains("playout build cancelled"));
}

#[tokio::test]
async fn requested_refresh_rewrites_upcoming_playout_item_playback_data() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;

    let now = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let locker = EntityLocker::new();
    let built = run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Reset,
        },
        now,
        None,
    )
    .await
    .unwrap();
    assert!(built);
    assert_eq!(
        first_playout_item_subtitle_mode(&pool, 88).await,
        Some(1),
        "initial build should materialize the old override"
    );

    sqlx::query(r#"UPDATE "ProgramScheduleItem" SET "SubtitleMode" = 3 WHERE "Id" = 401"#)
        .execute(&pool)
        .await
        .unwrap();
    let refreshed = run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Refresh,
        },
        now,
        None,
    )
    .await
    .unwrap();
    assert!(refreshed);

    let item_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem" WHERE "PlayoutId" = 88"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(item_count > 0);
    assert_eq!(
        first_playout_item_subtitle_mode(&pool, 88).await,
        Some(3),
        "Refresh should clear stale rows and rebuild upcoming items with the new override"
    );
}

#[tokio::test]
async fn reset_and_refresh_preserve_timeline_when_filter_empties_required_source() {
    for mode in [PlayoutBuildMode::Reset, PlayoutBuildMode::Refresh] {
        let pool = common::test_db().await;
        let repos = make_repositories(pool.clone());
        common::seed_profile_and_channel(&repos, 1).await;
        seed_refresh_rebuild_fixture(&pool, &repos).await;

        let start = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
        let locker = EntityLocker::new();
        run_requested_playout_build(
            &pool,
            &locker,
            PlayoutBuildRequest {
                playout_id: 88,
                mode: PlayoutBuildMode::Reset,
            },
            start,
            None,
        )
        .await
        .expect("initial healthy build");
        let before = persisted_playout_item_rows(&pool, 88).await;
        assert!(
            !before.is_empty(),
            "fixture must have a timeline to preserve"
        );

        seed_unschedulable_remote_stream(&pool, 200).await;
        sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (1, 200)"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let error = run_requested_playout_build(
            &pool,
            &locker,
            PlayoutBuildRequest {
                playout_id: 88,
                mode,
            },
            start + Duration::minutes(1),
            None,
        )
        .await
        .expect_err("an emptied required source must refuse a destructive build");
        assert!(
            error
                .to_string()
                .contains("has no schedulable media after invalid Remote Streams were filtered"),
            "unexpected build failure: {error}"
        );

        assert_eq!(
            persisted_playout_item_rows(&pool, 88).await,
            before,
            "{mode:?} must leave the existing materialized timeline untouched"
        );
        let status = sqlx::query(
            r#"SELECT "Success", "Message" FROM "PlayoutBuildStatus" WHERE "PlayoutId" = 88"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status.get::<i64, _>("Success"), 0);
        assert!(status
            .get::<Option<String>, _>("Message")
            .is_some_and(|message| message.contains("has no schedulable media")));
    }
}

#[tokio::test]
async fn requested_classic_refresh_rewrites_near_term_playout_items() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;

    let start = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let locker = EntityLocker::new();
    run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Reset,
        },
        start,
        None,
    )
    .await
    .unwrap();

    sqlx::query(r#"UPDATE "ProgramScheduleItem" SET "SubtitleMode" = 3 WHERE "Id" = 401"#)
        .execute(&pool)
        .await
        .unwrap();
    run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Refresh,
        },
        start + Duration::minutes(1),
        None,
    )
    .await
    .unwrap();

    let near_cutoff = (start + Duration::hours(1))
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let stale_near_term_items: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "PlayoutItem"
           WHERE "PlayoutId" = 88
             AND "Start" < ?
             AND "SubtitleMode" = 1"#,
    )
    .bind(near_cutoff)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stale_near_term_items, 0);
    assert_eq!(
        first_playout_item_subtitle_mode(&pool, 88).await,
        Some(3),
        "classic Refresh must rewrite the near-term window Reset Playout targets"
    );
}

#[tokio::test]
async fn startup_repair_refreshes_only_classic_playouts_with_missing_runtime_state() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;

    let now = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let locker = EntityLocker::new();
    run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Reset,
        },
        now,
        None,
    )
    .await
    .unwrap();
    pool.execute(
        r#"DELETE FROM "CollectionEnumeratorState"
           WHERE "PlayoutProgramScheduleAnchorId" IN (
               SELECT "Id" FROM "PlayoutProgramScheduleAnchor"
               WHERE "PlayoutId" = 88 AND "AnchorDate" IS NULL
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"DELETE FROM "PlayoutProgramScheduleAnchor"
           WHERE "PlayoutId" = 88 AND "AnchorDate" IS NULL"#,
    )
    .await
    .unwrap();
    pool.execute(r#"UPDATE "Playout" SET "ScheduleKind" = 'Classic' WHERE "Id" = 88"#)
        .await
        .unwrap();

    let repaired = repair_invalid_classic_playouts(&pool, &locker, now, None)
        .await
        .unwrap();
    assert_eq!(repaired, 1);
    assert_eq!(classic_undated_anchor_count(&pool, 88).await, 1);
    assert_eq!(
        classic_anchor_state_count(&pool, 88).await,
        classic_anchor_count(&pool, 88).await
    );
    assert!(classic_checkpoint_count(&pool, 88).await >= 1);
    assert_eq!(
        repair_invalid_classic_playouts(&pool, &locker, now, None)
            .await
            .unwrap(),
        0,
        "a repaired playout must pass the next startup audit"
    );
}

#[tokio::test]
async fn startup_repair_refuses_legacy_state_without_clearing_the_window() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;

    let now = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let locker = EntityLocker::new();
    run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Reset,
        },
        now,
        None,
    )
    .await
    .unwrap();
    let item_ids_before = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "PlayoutItem" WHERE "PlayoutId" = 88 ORDER BY "Start", "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    pool.execute(
        r#"DELETE FROM "CollectionEnumeratorState"
           WHERE "PlayoutProgramScheduleAnchorId" IN (
               SELECT "Id" FROM "PlayoutProgramScheduleAnchor"
               WHERE "PlayoutId" = 88 AND "AnchorDate" IS NOT NULL
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"DELETE FROM "PlayoutProgramScheduleAnchor"
           WHERE "PlayoutId" = 88 AND "AnchorDate" IS NOT NULL"#,
    )
    .await
    .unwrap();
    pool.execute(r#"DELETE FROM "PlayoutClassicCheckpoint" WHERE "PlayoutId" = 88"#)
        .await
        .unwrap();

    let repaired = repair_invalid_classic_playouts(&pool, &locker, now, None)
        .await
        .unwrap();
    assert_eq!(repaired, 0);
    let item_ids_after = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "PlayoutItem" WHERE "PlayoutId" = 88 ORDER BY "Start", "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        item_ids_after, item_ids_before,
        "unsafe startup repair must leave the materialized window untouched"
    );
    assert_eq!(classic_undated_anchor_count(&pool, 88).await, 1);
    let status = sqlx::query(
        r#"SELECT "Success", "Message" FROM "PlayoutBuildStatus" WHERE "PlayoutId" = 88"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status.get::<i64, _>("Success"), 0);
    let message = status
        .get::<Option<String>, _>("Message")
        .unwrap_or_default();
    assert!(message.contains("Classic Refresh refused"));
    assert!(message.contains("Reset Playout"));
}

#[tokio::test]
async fn classic_continue_promotes_to_refresh_when_runtime_state_is_missing() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;

    let now = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let locker = EntityLocker::new();
    run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Reset,
        },
        now,
        None,
    )
    .await
    .unwrap();
    pool.execute(
        r#"DELETE FROM "CollectionEnumeratorState"
           WHERE "PlayoutProgramScheduleAnchorId" IN (
               SELECT "Id" FROM "PlayoutProgramScheduleAnchor"
               WHERE "PlayoutId" = 88 AND "AnchorDate" IS NULL
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"DELETE FROM "PlayoutProgramScheduleAnchor"
           WHERE "PlayoutId" = 88 AND "AnchorDate" IS NULL"#,
    )
    .await
    .unwrap();

    let hls_registry = Arc::new(FakeHlsRegistry::default());
    run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 88,
            mode: PlayoutBuildMode::Continue,
        },
        now + Duration::minutes(1),
        Some(hls_registry.as_ref()),
    )
    .await
    .unwrap();

    assert_eq!(classic_undated_anchor_count(&pool, 88).await, 1);
    assert_eq!(
        classic_anchor_state_count(&pool, 88).await,
        classic_anchor_count(&pool, 88).await
    );
    assert_eq!(
        *hls_registry
            .playout_updates
            .lock()
            .expect("fake hls registry lock"),
        vec!["1".to_owned()],
        "promotion to Refresh must invalidate dependent HLS sessions"
    );
}

async fn seed_refresh_rebuild_fixture(pool: &SqlitePool, repos: &Repositories) {
    common::seed_media_path(pool, 1, 1, 1).await;
    common::seed_media_item(pool, 100, 1).await;
    pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (100)"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "MovieMetadata" (
              "Id", "MovieId", "MetadataKind", "Title", "SortTitle",
              "DateAdded", "DateUpdated"
           ) VALUES (
              100, 100, 0, 'Refresh Fixture', 'Refresh Fixture',
              '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z'
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaVersion" (
              "Id", "MovieId", "Name", "Duration", "VideoScanKind",
              "DateAdded", "DateUpdated", "Width", "Height"
           ) VALUES (
              1000, 100, 'main', '00:10:00', 0,
              '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 1920, 1080
           )"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
           VALUES (1, 'Refresh Collection', 0)"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (1, 100)"#)
        .await
        .unwrap();

    repos
        .program_schedule
        .upsert(&common::schedule(301, vec![401]))
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: 401,
            program_schedule_id: 301,
            collection_type: CollectionType::Collection,
            collection_id: Some(1),
            playout_mode: PlayoutMode::One,
            playback_order: PlaybackOrder::Chronological,
            subtitle_mode: Some(ChannelSubtitleMode::Forced),
            sort_order: 1,
            ..ProgramScheduleItem::default()
        })
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed")
           VALUES (88, 1, 301, 1, 0)"#,
    )
    .await
    .unwrap();
}

async fn seed_unschedulable_remote_stream(pool: &SqlitePool, media_item_id: i64) {
    common::seed_media_item(pool, media_item_id, 1).await;
    sqlx::query(
        r#"INSERT INTO "RemoteStream" ("Id", "Url", "Liveness", "IsLive")
           VALUES (?, 'https://example.invalid/stream', 0, 0)"#,
    )
    .bind(media_item_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "RemoteStreamMetadata" (
               "RemoteStreamId", "MetadataKind", "Title", "DateAdded", "DateUpdated"
           ) VALUES (?, 0, 'Unschedulable fixture', '2026-04-15 00:00:00', '2026-04-15 00:00:00')"#,
    )
    .bind(media_item_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
               "RemoteStreamId", "Name", "Duration", "VideoScanKind",
               "DateAdded", "DateUpdated", "Width", "Height"
           ) VALUES (?, 'main', '00:00:00', 0,
                     '2026-04-15 00:00:00', '2026-04-15 00:00:00', 1920, 1080)"#,
    )
    .bind(media_item_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn persisted_playout_item_rows(
    pool: &SqlitePool,
    playout_id: i64,
) -> Vec<(i64, i64, String, String)> {
    sqlx::query(
        r#"SELECT "Id", "MediaItemId", "Start", "Finish"
           FROM "PlayoutItem"
           WHERE "PlayoutId" = ?
           ORDER BY "Id""#,
    )
    .bind(playout_id)
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row| {
        (
            row.get("Id"),
            row.get("MediaItemId"),
            row.get("Start"),
            row.get("Finish"),
        )
    })
    .collect()
}

async fn first_playout_item_subtitle_mode(pool: &SqlitePool, playout_id: i64) -> Option<i64> {
    sqlx::query_scalar::<_, Option<i64>>(
        r#"SELECT "SubtitleMode"
           FROM "PlayoutItem"
           WHERE "PlayoutId" = ?
           ORDER BY "Start", "Id"
           LIMIT 1"#,
    )
    .bind(playout_id)
    .fetch_optional(pool)
    .await
    .unwrap()
    .flatten()
}

async fn classic_anchor_count(pool: &SqlitePool, playout_id: i64) -> i64 {
    sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn classic_anchor_state_count(pool: &SqlitePool, playout_id: i64) -> i64 {
    sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "CollectionEnumeratorState" AS "s"
           JOIN "PlayoutProgramScheduleAnchor" AS "a"
             ON "a"."Id" = "s"."PlayoutProgramScheduleAnchorId"
           WHERE "a"."PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn classic_undated_anchor_count(pool: &SqlitePool, playout_id: i64) -> i64 {
    sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "PlayoutProgramScheduleAnchor"
           WHERE "PlayoutId" = ? AND "AnchorDate" IS NULL"#,
    )
    .bind(playout_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn classic_checkpoint_count(pool: &SqlitePool, playout_id: i64) -> i64 {
    sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutClassicCheckpoint" WHERE "PlayoutId" = ?"#)
        .bind(playout_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn maintenance_reads_scanner_refresh_interval_from_config() {
    let pool = common::test_db().await;
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('scanner.library_refresh_interval', '6')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        load_library_refresh_interval(&pool).await.unwrap(),
        chrono::Duration::hours(6)
    );
}

#[tokio::test]
async fn maintenance_dispatch_uses_application_scan_requester() {
    let pool = common::test_db().await;
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name", "LastScan")
           VALUES (42, 10, 0, 'Movies', NULL)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('scanner.library_refresh_interval', '1')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let library_scans = FakeLibraryScanRequester::new(ScheduledScanRequestOutcome::Accepted);
    let report = run_maintenance_once(
        pool,
        &CacheCleanupConfig::default(),
        Some(&library_scans),
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
        None,
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
    assert_eq!(report.libraries_dispatched, 1);
    assert_eq!(report.libraries_failed, 0);
    assert_eq!(*library_scans.requests.lock().unwrap(), vec![42]);
}

#[tokio::test]
async fn maintenance_dispatch_marks_rejected_application_requests_failed() {
    let pool = common::test_db().await;
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "MediaSourceId", "MediaKind", "Name", "LastScan")
           VALUES (43, 10, 0, 'Movies', NULL)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('scanner.library_refresh_interval', '1')"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let library_scans = FakeLibraryScanRequester::new(ScheduledScanRequestOutcome::Rejected {
        message: "library scanning is disabled".to_owned(),
    });
    let report = run_maintenance_once(
        pool,
        &CacheCleanupConfig::default(),
        Some(&library_scans),
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
        None,
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
    assert_eq!(report.libraries_dispatched, 0);
    assert_eq!(report.libraries_failed, 1);
    assert_eq!(*library_scans.requests.lock().unwrap(), vec![43]);
}

#[tokio::test]
async fn orphan_artwork_maintenance_runs_on_startup_hourly_and_retries_failure() {
    let pool = common::test_db().await;
    let cache = CacheCleanupConfig::default();
    let locker = EntityLocker::new();
    let run = || {
        run_maintenance_once(
            pool.clone(),
            &cache,
            None,
            &locker,
            DatabaseSchemaStatus::default(),
            None,
        )
    };
    sqlx::query(r#"INSERT INTO "Actor" ("Name") VALUES ('orphan')"#)
        .execute(&pool)
        .await
        .unwrap();
    let first = run().await.unwrap();
    assert!(first.loops_ran.artwork_cleanup());
    assert_eq!(first.artwork_cleanup.actors_deleted, 1);
    sqlx::query(r#"INSERT INTO "Actor" ("Name") VALUES ('later orphan')"#)
        .execute(&pool)
        .await
        .unwrap();
    let second = run().await.unwrap();
    assert!(!second.loops_ran.artwork_cleanup());
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = 'maintenance.loop.artwork_cleanup.last_run'"#)
        .bind((Utc::now() - Duration::hours(2)).to_rfc3339())
        .execute(&pool).await.unwrap();
    sqlx::query(r#"CREATE TRIGGER "FailActorCleanup" BEFORE DELETE ON "Actor" BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END"#)
        .execute(&pool).await.unwrap();
    assert!(run().await.is_err());
    sqlx::query(r#"DROP TRIGGER "FailActorCleanup""#)
        .execute(&pool)
        .await
        .unwrap();
    let retry = run().await.unwrap();
    assert!(retry.loops_ran.artwork_cleanup());
    assert_eq!(retry.artwork_cleanup.actors_deleted, 1);
}

#[tokio::test]
async fn orphan_subtitle_maintenance_runs_on_startup_hourly_and_retries_failure() {
    let pool = common::test_db().await;
    let cache = CacheCleanupConfig::default();
    let locker = EntityLocker::new();
    let run = || {
        run_maintenance_once(
            pool.clone(),
            &cache,
            None,
            &locker,
            DatabaseSchemaStatus::default(),
            None,
        )
    };
    sqlx::query(r#"INSERT INTO "Subtitle" ("SubtitleKind","StreamIndex","Default","Forced","DateAdded","DateUpdated") VALUES (0,0,0,0,'2026-09-24','2026-09-24')"#)
        .execute(&pool)
        .await
        .unwrap();
    let first = run().await.unwrap();
    assert!(first.loops_ran.subtitle_cleanup());
    assert_eq!(first.subtitles_deleted, 1);
    sqlx::query(r#"INSERT INTO "Subtitle" ("SubtitleKind","StreamIndex","Default","Forced","DateAdded","DateUpdated") VALUES (0,0,0,0,'2026-09-24','2026-09-24')"#)
        .execute(&pool)
        .await
        .unwrap();
    let second = run().await.unwrap();
    assert!(!second.loops_ran.subtitle_cleanup());
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = 'maintenance.loop.subtitle_cleanup.last_run'"#)
        .bind((Utc::now() - Duration::hours(2)).to_rfc3339())
        .execute(&pool).await.unwrap();
    sqlx::query(r#"CREATE TRIGGER "FailSubtitleCleanup" BEFORE DELETE ON "Subtitle" BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END"#)
        .execute(&pool).await.unwrap();
    assert!(run().await.is_err());
    sqlx::query(r#"DROP TRIGGER "FailSubtitleCleanup""#)
        .execute(&pool)
        .await
        .unwrap();
    let retry = run().await.unwrap();
    assert!(retry.loops_ran.subtitle_cleanup());
    assert_eq!(retry.subtitles_deleted, 1);
}

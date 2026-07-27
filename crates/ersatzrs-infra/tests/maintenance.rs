#![cfg(not(feature = "mysql"))]
mod common;

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::maintenance::{ScheduledLibraryScanRequester, ScheduledScanRequestOutcome};
use ersatzrs_core::repositories::ProgramScheduleRepository;
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
use ersatzrs_domain::{
    ChannelSubtitleMode, CollectionType, PlaybackOrder, PlayoutMode, ProgramScheduleItem,
};
use ersatzrs_infra::cache_cleanup::CacheCleanupConfig;
use ersatzrs_infra::db::DatabaseSchemaStatus;
use ersatzrs_infra::maintenance::{
    load_library_refresh_interval, run_maintenance_once, run_requested_playout_build,
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
    // playout, library_refresh, provider_poll, cache_cleanup, search_index.
    assert_eq!(rows.len(), 5);
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
    assert!(locker.unlock_playout(78));

    let hls_registry = Arc::new(FakeHlsRegistry::default());
    let built = run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 78,
            mode: PlayoutBuildMode::Refresh,
        },
        Utc::now(),
        Some(hls_registry.as_ref()),
    )
    .await
    .unwrap();
    assert!(built);

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
           VALUES (88, 1, 301, 'Classic', 0)"#,
    )
    .await
    .unwrap();
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

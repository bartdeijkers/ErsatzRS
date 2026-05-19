#![cfg(not(feature = "mysql"))]
mod common;

use std::path::PathBuf;

use chrono::{Duration, Utc};
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
use ersatzrs_infra::cache_cleanup::CacheCleanupConfig;
use ersatzrs_infra::db::DatabaseSchemaStatus;
use ersatzrs_infra::maintenance::{
    load_library_refresh_interval, run_maintenance_once, run_requested_playout_build,
    LibraryDispatchConfig,
};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, Row};

/// Build a tiny shell script that records its arguments to `record_path`,
/// then exits with the requested status. The returned path is suitable for
/// `LibraryDispatchConfig::scanner_command` on Unix.
#[cfg(unix)]
fn write_fake_scanner(
    dir: &std::path::Path,
    record_path: &std::path::Path,
    exit_code: i32,
) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("fake-ersatzrs-scanner");
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {record}\nexit {exit}\n",
        record = shell_escape(record_path.to_str().unwrap()),
        exit = exit_code,
    );
    std::fs::write(&script, body).unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}

#[cfg(unix)]
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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

    let report = run_maintenance_once(
        pool.clone(),
        &CacheCleanupConfig::default(),
        &LibraryDispatchConfig::default(),
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
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
        &LibraryDispatchConfig::default(),
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
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
        &LibraryDispatchConfig::default(),
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
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
        &LibraryDispatchConfig::default(),
        &locker,
        DatabaseSchemaStatus::default(),
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
    )
    .await
    .unwrap();
    assert!(!skipped);
    assert!(locker.unlock_playout(78));

    let built = run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest {
            playout_id: 78,
            mode: PlayoutBuildMode::Refresh,
        },
        Utc::now(),
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

#[cfg(unix)]
#[tokio::test]
async fn maintenance_dispatch_invokes_scanner_subprocess() {
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

    let tempdir = tempfile::tempdir().unwrap();
    let record = tempdir.path().join("invocations.log");
    let scanner = write_fake_scanner(tempdir.path(), &record, 0);

    let report = run_maintenance_once(
        pool,
        &CacheCleanupConfig::default(),
        &LibraryDispatchConfig {
            scanner_command: Some(scanner),
            extra_args: vec!["--dry-run".into()],
        },
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
    assert_eq!(report.libraries_dispatched, 1);
    assert_eq!(report.libraries_failed, 0);
    let invocations = std::fs::read_to_string(&record).unwrap();
    assert!(
        invocations.contains("--provider local --library-id 42 --dry-run"),
        "expected scanner invocation, got: {invocations}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn maintenance_dispatch_marks_failed_libraries_when_scanner_exits_nonzero() {
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

    let tempdir = tempfile::tempdir().unwrap();
    let record = tempdir.path().join("invocations.log");
    let scanner = write_fake_scanner(tempdir.path(), &record, 7);

    let report = run_maintenance_once(
        pool,
        &CacheCleanupConfig::default(),
        &LibraryDispatchConfig {
            scanner_command: Some(scanner),
            extra_args: Vec::new(),
        },
        &EntityLocker::new(),
        DatabaseSchemaStatus::default(),
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
    assert_eq!(report.libraries_dispatched, 0);
    assert_eq!(report.libraries_failed, 1);
}

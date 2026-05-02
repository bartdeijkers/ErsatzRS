#![cfg(not(feature = "mysql"))]
mod common;

use chrono::{Duration, Utc};
use ersatzrs_infra::cache_cleanup::CacheCleanupConfig;
use ersatzrs_infra::db::DatabaseSchemaStatus;
use ersatzrs_infra::maintenance::{load_library_refresh_interval, run_maintenance_once};
use sqlx::Row;

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
        DatabaseSchemaStatus::default(),
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
    assert_eq!(report.libraries_dispatched, 1);
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
    // EF stores DateTimeOffset as `YYYY-MM-DD HH:MM:SS[.fffffff]`. The
    // production cutoff query must compare correctly against either the
    // ticked or un-ticked variant, so seed with a 7-digit tick suffix that
    // matches a real ErsatzTV row.
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
        DatabaseSchemaStatus::default(),
    )
    .await
    .unwrap();

    assert!(report.loops_ran.library_refresh());
    assert_eq!(report.libraries_dispatched, 0);
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

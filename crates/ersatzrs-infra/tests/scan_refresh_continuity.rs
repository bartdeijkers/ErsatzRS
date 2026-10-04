mod common;

#[path = "scan_refresh_continuity/provider_collections.rs"]
mod provider_collections;

#[path = "scan_refresh_continuity/classic_midnight.rs"]
mod classic_midnight;
#[path = "scan_refresh_continuity/collection_identity.rs"]
mod collection_identity;
#[path = "scan_refresh_continuity/fixed_start_clock.rs"]
mod fixed_start_clock;

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, Duration, Timelike, Utc};
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::playout_builder::PlayoutBuilder;
use ersatzrs_core::repositories::{PlayoutRepository, ProgramScheduleRepository};
use ersatzrs_core::scheduling::{
    program_schedule_item_smart_shuffle_config_key, PlayoutBuildMode, PlayoutBuildRequest,
    SchedulingRequest, SchedulingSmartShufflePolicy, SchedulingSmartShufflePreset,
    SchedulingSmartShuffleRule, SchedulingSmartShuffleRuleKind,
};
use ersatzrs_domain::{
    CollectionType, EntityId, PlaybackOrder, PlayoutMode, ProgramScheduleItem,
    ScanPlayoutRefreshReason,
};
use ersatzrs_infra::maintenance::run_requested_playout_build;
use ersatzrs_infra::repositories::{make_repositories, Repositories};
use ersatzrs_infra::scan_refresh::{
    self, complete_resolution_in_tx, persist_refresh_intents, DurableScanPlayoutRefresh,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Executor, Row, SqlitePool};

const LIBRARY_ID: EntityId = 1;
const RELATED_MEDIA_ID: EntityId = 101;
const ADDED_MEDIA_ID: EntityId = 102;
const UNRELATED_MEDIA_ID: EntityId = 201;
const RELATED_PLAYLIST_ID: EntityId = 11;
const UNRELATED_PLAYLIST_ID: EntityId = 12;
const RELATED_PLAYOUT_ID: EntityId = 901;
const UNRELATED_PLAYOUT_ID: EntityId = 902;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn recovered_scan_refresh_matches_uninterrupted_classic_oracle() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let baseline_path = directory.path().join("baseline.sqlite");
    let oracle_path = directory.path().join("oracle.sqlite");
    let restarted_path = directory.path().join("restarted.sqlite");
    let build_at = common::fixed_time();
    let refresh_at = build_at + Duration::minutes(1);

    let baseline = open_database(&baseline_path, true).await;
    seed_acceptance_fixture(&baseline).await;
    for playout_id in [RELATED_PLAYOUT_ID, UNRELATED_PLAYOUT_ID] {
        run_classic_build(&baseline, playout_id, PlayoutBuildMode::Reset, build_at).await;
    }

    let unrelated_before = persisted_classic_rows(&baseline, UNRELATED_PLAYOUT_ID).await;
    let related_before = persisted_classic_rows(&baseline, RELATED_PLAYOUT_ID).await;
    assert!(!unrelated_before.playout_items.is_empty());
    assert!(!unrelated_before.playout_anchor.is_empty());
    assert!(!unrelated_before.program_schedule_anchors.is_empty());
    assert!(!unrelated_before.classic_checkpoints.is_empty());
    assert_eq!(
        playout_media_count(&baseline, RELATED_PLAYOUT_ID, ADDED_MEDIA_ID).await,
        0
    );

    baseline.close().await;
    tokio::fs::copy(&baseline_path, &oracle_path)
        .await
        .expect("copy uninterrupted oracle database");
    tokio::fs::copy(&baseline_path, &restarted_path)
        .await
        .expect("copy restart database");

    let oracle = open_database(&oracle_path, false).await;
    let oracle_affected = commit_scan_membership_change(&oracle).await;
    assert_eq!(oracle_affected, [RELATED_PLAYOUT_ID]);
    run_classic_build(
        &oracle,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Refresh,
        refresh_at,
    )
    .await;
    let oracle_rows = persisted_classic_rows(&oracle, RELATED_PLAYOUT_ID).await;
    assert!(
        scan_refresh::recoverable_refreshes(&oracle)
            .await
            .expect("load oracle intents")
            .is_empty(),
        "the uninterrupted worker must consume its durable intent"
    );

    let restarted = open_database(&restarted_path, false).await;
    let restarted_affected = commit_scan_membership_change(&restarted).await;
    assert_eq!(restarted_affected, [RELATED_PLAYOUT_ID]);
    assert_eq!(
        intended_playout_ids(&restarted).await,
        [RELATED_PLAYOUT_ID],
        "scan impact resolution must not admit the unrelated Classic playout"
    );
    assert_unrelated_rows_unchanged(
        &unrelated_before,
        &persisted_classic_rows(&restarted, UNRELATED_PLAYOUT_ID).await,
    );
    restarted.close().await;

    let reopened = open_database(&restarted_path, false).await;
    let recovered = scan_refresh::recoverable_refreshes(&reopened)
        .await
        .expect("recover durable intent after SQLite reopen");
    assert_eq!(
        recovered,
        [DurableScanPlayoutRefresh {
            playout_id: RELATED_PLAYOUT_ID,
            reason: ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        }],
        "the related intent must survive the commit-to-queue crash window"
    );
    assert_eq!(resolution_state(&reopened).await, 1);

    for refresh in recovered {
        run_classic_build(
            &reopened,
            refresh.playout_id,
            PlayoutBuildMode::Refresh,
            refresh_at,
        )
        .await;
    }

    assert!(
        scan_refresh::recoverable_refreshes(&reopened)
            .await
            .expect("load completed restart intents")
            .is_empty(),
        "the recovered worker must consume the completed intent"
    );
    assert!(
        playout_media_count(&reopened, RELATED_PLAYOUT_ID, ADDED_MEDIA_ID).await > 0,
        "the recovered Refresh must materialize the added playlist member"
    );
    let restarted_rows = persisted_classic_rows(&reopened, RELATED_PLAYOUT_ID).await;
    assert!(
        restarted_rows != related_before,
        "the related Classic schedule must actually be refreshed"
    );
    assert!(
        restarted_rows == oracle_rows,
        "recovery must persist the same Classic schedule, anchors, and checkpoints as the uninterrupted oracle"
    );
    assert_unrelated_rows_unchanged(
        &unrelated_before,
        &persisted_classic_rows(&reopened, UNRELATED_PLAYOUT_ID).await,
    );
}

async fn open_database(path: &Path, create: bool) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(create)
                .foreign_keys(true),
        )
        .await
        .expect("open file-backed SQLite database");
    if create {
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate file-backed SQLite database");
        pool.execute(
            r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height")
               VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .await
        .expect("seed default resolution");
    }
    pool
}

async fn seed_acceptance_fixture(pool: &SqlitePool) {
    let repositories = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repositories, RELATED_PLAYOUT_ID).await;
    common::seed_profile_and_channel(&repositories, UNRELATED_PLAYOUT_ID).await;
    common::seed_media_path(pool, 1, LIBRARY_ID, 1).await;
    for (media_id, title) in [
        (RELATED_MEDIA_ID, "Related One"),
        (ADDED_MEDIA_ID, "Related Two"),
        (UNRELATED_MEDIA_ID, "Unrelated"),
    ] {
        common::seed_media_item(pool, media_id, 1).await;
        seed_movie(pool, media_id, title).await;
    }

    pool.execute(
        r#"INSERT INTO "PlaylistGroup" ("Id", "Name", "IsSystem")
           VALUES (10, 'scan-refresh', 1);
           INSERT INTO "Playlist" (
               "Id", "Name", "PlaylistGroupId", "IsSystem", "ManagedSource"
           ) VALUES
               (11, 'related', 10, 1, 'scan:v1:related'),
               (12, 'unrelated', 10, 1, 'scan:v1:unrelated');
           INSERT INTO "PlaylistItem" (
               "Id", "Index", "PlaylistId", "CollectionType", "MediaItemId",
               "IncludeInProgramGuide", "PlaybackOrder", "PlayAll"
           ) VALUES
               (101, 0, 11, 'Movie', 101, 1, 'Chronological', 1),
               (201, 0, 12, 'Movie', 201, 1, 'Chronological', 1);
           INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('playout.days_to_build', '1')"#,
    )
    .await
    .expect("seed managed playlists");

    seed_classic_playout(
        pool,
        &repositories,
        RELATED_PLAYOUT_ID,
        301,
        401,
        RELATED_PLAYLIST_ID,
    )
    .await;
    seed_classic_playout(
        pool,
        &repositories,
        UNRELATED_PLAYOUT_ID,
        302,
        402,
        UNRELATED_PLAYLIST_ID,
    )
    .await;
}

async fn seed_movie(pool: &SqlitePool, media_id: EntityId, title: &str) {
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(media_id)
        .execute(pool)
        .await
        .expect("seed movie");
    sqlx::query(
        r#"INSERT INTO "MovieMetadata" (
               "Id", "MovieId", "MetadataKind", "Title", "SortTitle",
               "DateAdded", "DateUpdated"
           ) VALUES (?, ?, 0, ?, ?, '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z')"#,
    )
    .bind(media_id)
    .bind(media_id)
    .bind(title)
    .bind(title)
    .execute(pool)
    .await
    .expect("seed movie metadata");
    sqlx::query(
        r#"INSERT INTO "MediaVersion" (
               "Id", "MovieId", "Name", "Duration", "VideoScanKind",
               "DateAdded", "DateUpdated", "Width", "Height"
           ) VALUES (
               ?, ?, 'main', '00:10:00', 'Progressive',
               '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z', 1920, 1080
           )"#,
    )
    .bind(media_id * 10)
    .bind(media_id)
    .execute(pool)
    .await
    .expect("seed movie version");
}

async fn seed_classic_playout(
    pool: &SqlitePool,
    repositories: &Repositories,
    playout_id: EntityId,
    schedule_id: EntityId,
    schedule_item_id: EntityId,
    playlist_id: EntityId,
) {
    repositories
        .program_schedule
        .upsert(&common::schedule(schedule_id, vec![schedule_item_id]))
        .await
        .expect("seed program schedule");
    repositories
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: schedule_item_id,
            program_schedule_id: schedule_id,
            collection_type: CollectionType::Playlist,
            playlist_id: Some(playlist_id),
            playout_mode: PlayoutMode::One,
            playback_order: PlaybackOrder::Chronological,
            sort_order: 1,
            ..ProgramScheduleItem::default()
        })
        .await
        .expect("seed program schedule item");
    sqlx::query(
        r#"INSERT INTO "Playout" (
               "Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
           ) VALUES (?, ?, ?, 'Classic', 123)"#,
    )
    .bind(playout_id)
    .bind(playout_id)
    .bind(schedule_id)
    .execute(pool)
    .await
    .expect("seed Classic playout");
}

async fn commit_scan_membership_change(pool: &SqlitePool) -> Vec<EntityId> {
    let reason = ScanPlayoutRefreshReason::RemoteStreamMembershipAdded;
    let generation = scan_refresh::begin_resolution(pool, LIBRARY_ID, reason)
        .await
        .expect("begin scan impact resolution");
    let affected =
        ersatzrs_infra::repositories::SqliteSchedulingContentRepository::new(pool.clone())
            .classic_playouts_affected_by_scan(
                &BTreeSet::from([ADDED_MEDIA_ID]),
                Some(RELATED_PLAYLIST_ID),
            )
            .await
            .expect("resolve affected Classic playouts");

    let mut tx = ersatzrs_infra::db::begin_write(pool)
        .await
        .expect("begin membership transaction");
    sqlx::query(
        r#"INSERT INTO "PlaylistItem" (
               "Index", "PlaylistId", "CollectionType", "MediaItemId",
               "IncludeInProgramGuide", "PlaybackOrder", "PlayAll"
           ) VALUES (1, ?, 'Movie', ?, 1, 'Chronological', 1)"#,
    )
    .bind(RELATED_PLAYLIST_ID)
    .bind(ADDED_MEDIA_ID)
    .execute(&mut *tx)
    .await
    .expect("commit changed playlist membership");
    persist_refresh_intents(&mut tx, LIBRARY_ID, reason, &affected)
        .await
        .expect("persist scan refresh intent");
    complete_resolution_in_tx(&mut tx, LIBRARY_ID, generation, reason)
        .await
        .expect("complete scan impact resolution");
    tx.commit()
        .await
        .expect("commit membership and refresh intent atomically");
    affected
}

async fn run_classic_build(
    pool: &SqlitePool,
    playout_id: EntityId,
    mode: PlayoutBuildMode,
    now: DateTime<Utc>,
) {
    assert!(
        run_requested_playout_build(
            pool,
            &EntityLocker::new(),
            PlayoutBuildRequest::new(playout_id, mode),
            now,
            None,
        )
        .await
        .expect("run scheduler-owned Classic build"),
        "the acceptance fixture must acquire the playout lock"
    );
}

async fn intended_playout_ids(pool: &SqlitePool) -> Vec<EntityId> {
    sqlx::query_scalar(r#"SELECT "PlayoutId" FROM "ScanPlayoutRefreshIntent" ORDER BY "PlayoutId""#)
        .fetch_all(pool)
        .await
        .expect("load persisted intent ids")
}

async fn resolution_state(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar(r#"SELECT "State" FROM "ScanRefreshResolution" WHERE "LibraryId" = ?"#)
        .bind(LIBRARY_ID)
        .fetch_one(pool)
        .await
        .expect("load scan impact resolution state")
}

async fn playout_media_count(pool: &SqlitePool, playout_id: EntityId, media_id: EntityId) -> i64 {
    sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "PlayoutItem"
           WHERE "PlayoutId" = ? AND "MediaItemId" = ?"#,
    )
    .bind(playout_id)
    .bind(media_id)
    .fetch_one(pool)
    .await
    .expect("count materialized playout media")
}

#[derive(PartialEq, Eq)]
struct PersistedClassicRows {
    playout: Vec<Vec<u8>>,
    playout_items: Vec<Vec<u8>>,
    playout_gaps: Vec<Vec<u8>>,
    playout_history: Vec<Vec<u8>>,
    playout_anchor: Vec<Vec<u8>>,
    schedule_item_enumerator: Vec<Vec<u8>>,
    program_schedule_anchors: Vec<Vec<u8>>,
    collection_enumerators: Vec<Vec<u8>>,
    fill_group_indices: Vec<Vec<u8>>,
    fill_group_enumerators: Vec<Vec<u8>>,
    classic_checkpoints: Vec<Vec<u8>>,
    build_status: Vec<Vec<u8>>,
}

async fn persisted_classic_rows(pool: &SqlitePool, playout_id: EntityId) -> PersistedClassicRows {
    PersistedClassicRows {
        playout: direct_rows(pool, "Playout", "Id", playout_id).await,
        playout_items: direct_rows(pool, "PlayoutItem", "PlayoutId", playout_id).await,
        playout_gaps: direct_rows(pool, "PlayoutGap", "PlayoutId", playout_id).await,
        playout_history: direct_rows(pool, "PlayoutHistory", "PlayoutId", playout_id).await,
        playout_anchor: direct_rows(pool, "PlayoutAnchor", "PlayoutId", playout_id).await,
        schedule_item_enumerator: direct_rows(
            pool,
            "ScheduleItemsEnumeratorState",
            "PlayoutAnchorPlayoutId",
            playout_id,
        )
        .await,
        program_schedule_anchors: direct_rows(
            pool,
            "PlayoutProgramScheduleAnchor",
            "PlayoutId",
            playout_id,
        )
        .await,
        collection_enumerators: child_rows(
            pool,
            "CollectionEnumeratorState",
            "PlayoutProgramScheduleAnchorId",
            "PlayoutProgramScheduleAnchor",
            "Id",
            playout_id,
        )
        .await,
        fill_group_indices: direct_rows(
            pool,
            "PlayoutScheduleItemFillGroupIndex",
            "PlayoutId",
            playout_id,
        )
        .await,
        fill_group_enumerators: child_rows(
            pool,
            "FillGroupEnumeratorState",
            "PlayoutScheduleItemFillGroupIndexId",
            "PlayoutScheduleItemFillGroupIndex",
            "Id",
            playout_id,
        )
        .await,
        classic_checkpoints: direct_rows(pool, "PlayoutClassicCheckpoint", "PlayoutId", playout_id)
            .await,
        build_status: direct_rows(pool, "PlayoutBuildStatus", "PlayoutId", playout_id).await,
    }
}

fn assert_unrelated_rows_unchanged(before: &PersistedClassicRows, after: &PersistedClassicRows) {
    assert!(
        before.playout_items == after.playout_items,
        "unrelated PlayoutItem rows changed byte-for-byte"
    );
    assert!(
        before.playout_anchor == after.playout_anchor,
        "unrelated PlayoutAnchor rows changed byte-for-byte"
    );
    assert!(
        before.program_schedule_anchors == after.program_schedule_anchors,
        "unrelated PlayoutProgramScheduleAnchor rows changed byte-for-byte"
    );
    assert!(
        before == after,
        "unrelated persisted Classic runtime state changed"
    );
}

async fn direct_rows(
    pool: &SqlitePool,
    table: &str,
    playout_column: &str,
    playout_id: EntityId,
) -> Vec<Vec<u8>> {
    let columns = table_columns(pool, table).await;
    let projection = quoted_row_projection("row", &columns);
    let sql = format!(
        "SELECT CAST({projection} AS BLOB) FROM {} AS \"row\" WHERE \"row\".{} = ?",
        quote_identifier(table),
        quote_identifier(playout_column),
    );
    let mut rows = sqlx::query_scalar::<_, Vec<u8>>(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(playout_id)
        .fetch_all(pool)
        .await
        .expect("serialize persisted table rows");
    rows.sort();
    rows
}

async fn child_rows(
    pool: &SqlitePool,
    child_table: &str,
    child_foreign_key: &str,
    parent_table: &str,
    parent_primary_key: &str,
    playout_id: EntityId,
) -> Vec<Vec<u8>> {
    let columns = table_columns(pool, child_table).await;
    let projection = quoted_row_projection("child", &columns);
    let sql = format!(
        "SELECT CAST({projection} AS BLOB) FROM {} AS \"child\" \
         JOIN {} AS \"parent\" ON \"parent\".{} = \"child\".{} \
         WHERE \"parent\".\"PlayoutId\" = ?",
        quote_identifier(child_table),
        quote_identifier(parent_table),
        quote_identifier(parent_primary_key),
        quote_identifier(child_foreign_key),
    );
    let mut rows = sqlx::query_scalar::<_, Vec<u8>>(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(playout_id)
        .fetch_all(pool)
        .await
        .expect("serialize persisted child rows");
    rows.sort();
    rows
}

async fn table_columns(pool: &SqlitePool, table: &str) -> Vec<String> {
    let sql = format!("PRAGMA table_info({})", quote_identifier(table));
    let columns = sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .fetch_all(pool)
        .await
        .expect("load table columns")
        .into_iter()
        .map(|row| row.get("name"))
        .collect::<Vec<String>>();
    assert!(!columns.is_empty(), "table {table} must exist");
    columns
}

fn quoted_row_projection(alias: &str, columns: &[String]) -> String {
    columns
        .iter()
        .map(|column| {
            format!(
                "quote({}.{})",
                quote_identifier(alias),
                quote_identifier(column)
            )
        })
        .collect::<Vec<_>>()
        .join(" || char(31) || ")
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Regression: a Classic Refresh must stay accepted after the checkpoint has
/// survived a database round trip.
///
/// `PlayoutClassicCheckpoint."CheckpointDate"` is stored at .NET tick precision
/// while the same instant inside `"Context"` is serialized by serde at full
/// nanosecond precision, so a checkpoint captured at a sub-tick timestamp used
/// to read back as two different instants and
/// `audit_classic_refresh_checkpoint` refused every later Refresh with
/// "checkpoint date and scheduler continuation time do not match". The build
/// after a Reset therefore succeeded exactly once and the playout window was
/// never extended again. Every other Classic checkpoint test drives the
/// scheduler in memory or from whole-second fixtures, where the truncation is
/// lossless, so only a persisted sub-tick build reproduces it.
#[tokio::test]
async fn classic_refresh_survives_checkpoint_round_trip_at_sub_tick_precision() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let database_path = directory.path().join("sub-tick.sqlite");
    // 940 ns is not a whole tick, so the persisted column floors it to 900 ns.
    let build_at = common::fixed_time()
        .with_nanosecond(881_033_940)
        .expect("sub-tick build timestamp");
    assert_ne!(
        build_at.timestamp_subsec_nanos() % 100,
        0,
        "the regression only reproduces below tick precision"
    );

    let pool = open_database(&database_path, true).await;
    seed_acceptance_fixture(&pool).await;

    run_classic_build(&pool, RELATED_PLAYOUT_ID, PlayoutBuildMode::Reset, build_at).await;
    assert!(
        !persisted_classic_rows(&pool, RELATED_PLAYOUT_ID)
            .await
            .classic_checkpoints
            .is_empty(),
        "the Reset build must persist a checkpoint to refresh from"
    );
    assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "reset").await;

    run_classic_build(
        &pool,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Refresh,
        build_at + Duration::minutes(1),
    )
    .await;
    assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "refresh").await;

    pool.close().await;
}

#[tokio::test]
async fn classic_continue_persists_checkpoint_for_new_final_day() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let database_path = directory.path().join("daily-checkpoint.sqlite");
    let pool = open_database(&database_path, true).await;
    seed_acceptance_fixture(&pool).await;
    sqlx::query(
        r#"UPDATE "ConfigElement" SET "Value" = '7' WHERE "Key" = 'playout.days_to_build'"#,
    )
    .execute(&pool)
    .await
    .expect("set seven-day build window");
    sqlx::query(
        r#"UPDATE "MediaVersion" SET "Duration" = '01:40:00' WHERE "MovieId" IN (101, 102)"#,
    )
    .execute(&pool)
    .await
    .expect("seed long items that cross day seams");
    let start = common::fixed_time()
        .date_naive()
        .and_hms_opt(20, 0, 0)
        .expect("20:00")
        .and_utc();
    run_classic_build(&pool, RELATED_PLAYOUT_ID, PlayoutBuildMode::Reset, start).await;
    let continued_at = start + Duration::hours(4);
    run_classic_build(
        &pool,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Continue,
        continued_at,
    )
    .await;

    let dates = sqlx::query_scalar::<_, String>(
        r#"SELECT DISTINCT substr("CheckpointDate", 1, 10)
           FROM "PlayoutClassicCheckpoint" WHERE "PlayoutId" = ?"#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .fetch_all(&pool)
    .await
    .expect("load persisted checkpoint dates");
    let expected_last = (continued_at + Duration::days(7)).date_naive().to_string();
    assert!(
        dates.contains(&expected_last),
        "production Continue did not persist a checkpoint on {expected_last}: {dates:?}"
    );
    let early_day_items = sqlx::query_as::<_, (i64, String, String)>(
        r#"SELECT "MediaItemId", "Start", "Finish" FROM "PlayoutItem"
           WHERE "PlayoutId" = ? AND (substr("Start", 1, 10) = ? OR substr("Finish", 1, 10) = ?)
           ORDER BY "Start" LIMIT 3"#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .bind(&expected_last)
    .bind(&expected_last)
    .fetch_all(&pool)
    .await
    .expect("load early final-day items");
    assert!(
        !early_day_items.is_empty(),
        "the final day needs scheduled items"
    );
    pool.close().await;
    let pool = open_database(&database_path, false).await;
    run_classic_build(
        &pool,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Refresh,
        continued_at + Duration::days(7) + Duration::minutes(30),
    )
    .await;
    assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "refresh").await;
    let after_refresh = sqlx::query_as::<_, (i64, String, String)>(
        r#"SELECT "MediaItemId", "Start", "Finish" FROM "PlayoutItem"
           WHERE "PlayoutId" = ? AND (substr("Start", 1, 10) = ? OR substr("Finish", 1, 10) = ?)
           ORDER BY "Start" LIMIT 3"#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .bind(&expected_last)
    .bind(&expected_last)
    .fetch_all(&pool)
    .await
    .expect("load final-day items after Refresh");
    assert!(
        after_refresh.starts_with(&early_day_items),
        "Refresh dropped or changed the item crossing into the checkpoint day"
    );
    assert!(after_refresh.len() > early_day_items.len());
    assert_eq!(
        after_refresh[early_day_items.len()].1,
        early_day_items.last().expect("prefix item").2
    );
    pool.close().await;
}

#[tokio::test]
async fn classic_continue_preserves_inactive_alternate_anchor_in_sqlite() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let database_path = directory.path().join("alternate-anchor.sqlite");
    let pool = open_database(&database_path, true).await;
    seed_acceptance_fixture(&pool).await;
    let start = common::fixed_time();
    run_classic_build(&pool, RELATED_PLAYOUT_ID, PlayoutBuildMode::Reset, start).await;
    sqlx::query(
        r#"INSERT INTO "ProgramScheduleAlternate" (
               "PlayoutId", "ProgramScheduleId", "Index", "DaysOfWeek",
               "LimitToDateRange", "StartMonth", "StartDay", "EndMonth", "EndDay"
           ) VALUES (?, 302, 0, '["Sunday"]', 0, 1, 1, 12, 31)"#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .execute(&pool)
    .await
    .expect("seed inactive Sunday alternate");
    sqlx::query(
        r#"INSERT INTO "PlayoutProgramScheduleAnchor" (
               "Id", "CollectionType", "PlaylistId", "PlayoutId"
           ) VALUES (9001, 'Playlist', 12, ?)"#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .execute(&pool)
    .await
    .expect("seed alternate collection anchor");
    sqlx::query(
        r#"INSERT INTO "CollectionEnumeratorState" (
               "PlayoutProgramScheduleAnchorId", "Seed", "Index", "Started"
           ) VALUES (9001, 1234, 9, 1)"#,
    )
    .execute(&pool)
    .await
    .expect("seed alternate collection progress");

    pool.close().await;
    let pool = open_database(&database_path, false).await;

    run_classic_build(
        &pool,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Continue,
        start + Duration::hours(1),
    )
    .await;
    assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "continue").await;
    let retained = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT "Seed", "Index" FROM "CollectionEnumeratorState"
           WHERE "PlayoutProgramScheduleAnchorId" = 9001"#,
    )
    .fetch_one(&pool)
    .await
    .expect("inactive alternate progress survives Continue");
    assert_eq!(retained, (1234, 9));
    pool.close().await;
}

#[tokio::test]
async fn classic_shuffled_playlist_restart_matches_continuous_build_and_refresh() {
    assert_grouped_playlist_restart(false).await;
}

#[tokio::test]
async fn classic_marathon_restart_matches_continuous_build_and_refresh() {
    assert_grouped_playlist_restart(true).await;
}

async fn assert_grouped_playlist_restart(marathon: bool) {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let baseline_path = directory.path().join("playlist-baseline.sqlite");
    let oracle_path = directory.path().join("playlist-oracle.sqlite");
    let restarted_path = directory.path().join("playlist-restarted.sqlite");
    let baseline = open_database(&baseline_path, true).await;
    seed_grouped_playlist_fixture(&baseline, marathon).await;
    let start = common::fixed_time();
    run_classic_build(
        &baseline,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Reset,
        start,
    )
    .await;
    assert_build_succeeded(&baseline, RELATED_PLAYOUT_ID, "reset").await;
    if marathon {
        assert_eq!(
            playlist_timeline(&baseline)
                .await
                .iter()
                .take(4)
                .map(|row| row.0)
                .collect::<Vec<_>>(),
            [101, 201, 102, 201]
        );
    }
    baseline.close().await;
    tokio::fs::copy(&baseline_path, &oracle_path).await.unwrap();
    tokio::fs::copy(&baseline_path, &restarted_path)
        .await
        .unwrap();
    let oracle = open_database(&oracle_path, false).await;
    run_classic_build(
        &oracle,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Continue,
        start + Duration::hours(6),
    )
    .await;
    let restarted = open_database(&restarted_path, false).await;
    run_classic_build(
        &restarted,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Continue,
        start + Duration::hours(2),
    )
    .await;
    restarted.close().await;
    let restarted = open_database(&restarted_path, false).await;
    run_classic_build(
        &restarted,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Continue,
        start + Duration::hours(6),
    )
    .await;
    assert_build_succeeded(&restarted, RELATED_PLAYOUT_ID, "continue").await;
    assert_eq!(
        playlist_timeline(&restarted).await,
        playlist_timeline(&oracle).await
    );
    for pool in [&oracle, &restarted] {
        run_classic_build(
            pool,
            RELATED_PLAYOUT_ID,
            PlayoutBuildMode::Refresh,
            start + Duration::hours(15),
        )
        .await;
        assert_build_succeeded(pool, RELATED_PLAYOUT_ID, "refresh").await;
    }
    let timeline = playlist_timeline(&restarted).await;
    assert_eq!(timeline, playlist_timeline(&oracle).await);
    assert!(!timeline.is_empty());
    assert!(timeline
        .iter()
        .all(|row| row.3.as_deref() == Some("nld") && row.4.as_deref() == Some("eng")));
    oracle.close().await;
    restarted.close().await;
}

async fn seed_grouped_playlist_fixture(pool: &SqlitePool, marathon: bool) {
    seed_acceptance_fixture(pool).await;
    pool.execute(
        r#"
        INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
            VALUES (91, 'Playlist child', 1);
        INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex")
            VALUES (91, 101, 0), (91, 102, 1);
        UPDATE "PlaylistItem" SET "CollectionType" = 'Collection',
            "CollectionId" = 91, "MediaItemId" = NULL WHERE "Id" = 101;
        UPDATE "ProgramSchedule" SET "RandomStartPoint" = 1 WHERE "Id" = 301;
        INSERT INTO "PlaylistItem" (
            "Id", "Index", "PlaylistId", "CollectionType", "MediaItemId",
            "IncludeInProgramGuide", "PlaybackOrder", "PlayAll"
        ) VALUES (102, 1, 11, 'Movie', 102, 1, 'Chronological', 1),
                 (103, 2, 11, 'Movie', 201, 1, 'Chronological', 1);
        UPDATE "PlaylistItem" SET "CollectionType" = 'Collection',
            "CollectionId" = 91, "MediaItemId" = NULL, "PlayAll" = 0
            WHERE "Id" IN (101, 102);
        UPDATE "ProgramScheduleItem" SET "MarathonShuffleGroups" = 1,
            "PreferredAudioLanguageCode" = 'nld', "PreferredSubtitleLanguageCode" = 'eng'
            WHERE "Id" = 401;
    "#,
    )
    .await
    .expect("seed shuffled playlist with track preferences");
    if marathon {
        pool.execute(r#"
            INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex") VALUES (91, 201, 2);
            INSERT INTO "Director" ("MovieMetadataId", "Name") VALUES
                (101, 'First Director'), (101, 'Second Director'),
                (102, 'First Director'), (201, 'Other Director');
            UPDATE "ProgramScheduleItem" SET "CollectionType" = 'Collection',
                "CollectionId" = 91, "PlaylistId" = NULL, "PlaybackOrder" = 'Marathon',
                "MarathonGroupBy" = 'Director', "MarathonBatchSize" = 1 WHERE "Id" = 401;
        "#).await.expect("seed director-grouped Marathon");
    }
}

#[tokio::test]
async fn classic_playlist_legacy_refresh_refuses_without_clearing_rows() {
    assert_legacy_playlist_refusal(PlayoutBuildMode::Refresh).await;
}

#[tokio::test]
async fn classic_playlist_legacy_continue_refuses_without_clearing_rows() {
    assert_legacy_playlist_refusal(PlayoutBuildMode::Continue).await;
}

async fn assert_legacy_playlist_refusal(mode: PlayoutBuildMode) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("legacy-playlist.sqlite");
    let pool = open_database(&path, true).await;
    seed_acceptance_fixture(&pool).await;
    let start = common::fixed_time();
    run_classic_build(&pool, RELATED_PLAYOUT_ID, PlayoutBuildMode::Reset, start).await;
    assert_build_succeeded(&pool, RELATED_PLAYOUT_ID, "reset").await;
    sqlx::query(r#"UPDATE "PlayoutAnchor" SET "Context" = json_remove("Context", '$.playlist_sources') WHERE "PlayoutId" = ?"#)
        .bind(RELATED_PLAYOUT_ID).execute(&pool).await.unwrap();
    let rows = sqlx::query(r#"SELECT "CheckpointDate", "Context" FROM "PlayoutClassicCheckpoint" WHERE "PlayoutId" = ?"#)
        .bind(RELATED_PLAYOUT_ID).fetch_all(&pool).await.unwrap();
    assert!(!rows.is_empty());
    for row in rows {
        let mut checkpoint: serde_json::Value =
            serde_json::from_str(&row.get::<String, _>("Context")).unwrap();
        for source in checkpoint["Sources"].as_object_mut().unwrap().values_mut() {
            assert!(source["Playlist"]
                .as_object_mut()
                .unwrap()
                .remove("RemainingMediaIds")
                .is_some());
        }
        sqlx::query(r#"UPDATE "PlayoutClassicCheckpoint" SET "Context" = ? WHERE "PlayoutId" = ? AND "CheckpointDate" = ?"#)
            .bind(checkpoint.to_string()).bind(RELATED_PLAYOUT_ID).bind(row.get::<String, _>("CheckpointDate"))
            .execute(&pool).await.unwrap();
    }
    pool.close().await;
    let pool = open_database(&path, false).await;
    let mut before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    let error = run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(RELATED_PLAYOUT_ID, mode),
        start,
        None,
    )
    .await
    .expect_err("incompatible playlist checkpoint must refuse an unsafe build");
    assert!(error.to_string().contains("playable-cycle state"));
    let status: (i64, String) = sqlx::query_as(
        r#"SELECT "Success", "Message" FROM "PlayoutBuildStatus" WHERE "PlayoutId" = ?"#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status.0, 0);
    assert!(status.1.contains("playable-cycle state"), "{}", status.1);
    let mut after = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    // Only the observable failed build status may change on refusal.
    before.build_status.clear();
    after.build_status.clear();
    assert!(
        before == after,
        "unsafe Refresh must preserve persisted playout rows"
    );
    pool.close().await;
}

type PlaylistTimelineRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

async fn playlist_timeline(pool: &SqlitePool) -> Vec<PlaylistTimelineRow> {
    sqlx::query_as(
        r#"SELECT "MediaItemId", "Start", "Finish", "PreferredAudioLanguageCode",
        "PreferredSubtitleLanguageCode", "SchedulingContext" FROM "PlayoutItem"
        WHERE "PlayoutId" = ? ORDER BY "Start", "Id""#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .fetch_all(pool)
    .await
    .expect("load playlist timeline")
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn classic_smart_shuffle_refresh_matches_uninterrupted_cooldown_across_round_trip() {
    let directory = tempfile::tempdir().expect("temporary database directory");
    let database_path = directory.path().join("smart-shuffle-ticks.sqlite");
    let build_at = common::fixed_time()
        .with_nanosecond(881_033_940)
        .expect("sub-tick build timestamp");
    let pool = open_database(&database_path, true).await;
    seed_acceptance_fixture(&pool).await;
    seed_smart_shuffle_cooldown(&pool).await;

    let oracle_reference =
        ersatzrs_infra::repositories::SqliteSchedulingContentRepository::new(pool.clone())
            .classic_reference_data(RELATED_PLAYOUT_ID, build_at, Duration::zero())
            .await
            .expect("load pre-build Classic reference data");
    run_classic_build(&pool, RELATED_PLAYOUT_ID, PlayoutBuildMode::Reset, build_at).await;

    let repositories = make_repositories(pool.clone());
    let persisted_playout = repositories
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .expect("load seeded playout");
    let first_leg_reference =
        ersatzrs_infra::repositories::SqliteSchedulingContentRepository::new(pool.clone())
            .classic_reference_data(RELATED_PLAYOUT_ID, build_at, Duration::zero())
            .await
            .expect("load first-leg checkpoints");
    let seam_checkpoint = first_leg_reference
        .existing_classic_checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.checkpoint_date > build_at)
        .min_by_key(|checkpoint| checkpoint.checkpoint_date)
        .expect("the first leg must cross a populated daily checkpoint");
    assert!(
        seam_checkpoint.context.contains("release_at"),
        "the regression checkpoint must carry an active duration cooldown"
    );
    assert_json_timestamps_at_tick_precision(
        &serde_json::from_str(&seam_checkpoint.context).expect("checkpoint context JSON"),
    );
    let seam = seam_checkpoint.checkpoint_date;
    let finish = seam + Duration::days(2);

    let mut oracle_playout = persisted_playout.clone();
    oracle_playout.anchor = None;
    let oracle = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: oracle_playout,
            start: build_at,
            finish,
            mode: PlayoutBuildMode::Reset,
            reference_data: oracle_reference,
        })
        .expect("build uninterrupted Smart Shuffle oracle");

    sqlx::query(
        r#"UPDATE "ConfigElement" SET "Value" = '2' WHERE "Key" = 'playout.days_to_build'"#,
    )
    .execute(&pool)
    .await
    .expect("extend Refresh window");
    pool.close().await;

    let reopened = open_database(&database_path, false).await;
    run_classic_build(
        &reopened,
        RELATED_PLAYOUT_ID,
        PlayoutBuildMode::Refresh,
        seam,
    )
    .await;
    assert_build_succeeded(&reopened, RELATED_PLAYOUT_ID, "Smart Shuffle refresh").await;

    let comparison_finish = seam + Duration::hours(2);
    let oracle_comparison_start =
        seam - Duration::nanoseconds(i64::from(seam.timestamp_subsec_nanos()));
    let oracle_comparison_finish = oracle_comparison_start + Duration::hours(2);
    let actual_items = make_repositories(reopened.clone())
        .playout
        .items_in_range(RELATED_PLAYOUT_ID, seam, comparison_finish)
        .await
        .expect("load refreshed future items")
        .into_iter()
        .filter(|item| item.start >= seam && item.start < comparison_finish)
        .map(|item| (item.media_item_id, item.start, item.finish))
        .collect::<Vec<_>>();
    let expected_items = oracle
        .added_items
        .iter()
        .filter(|item| {
            item.start >= oracle_comparison_start && item.start < oracle_comparison_finish
        })
        .map(|item| {
            (
                item.media_item_id,
                ersatzrs_core::scheduling::truncate_to_persisted_precision(item.start),
                ersatzrs_core::scheduling::truncate_to_persisted_precision(item.finish),
            )
        })
        .collect::<Vec<_>>();
    let actual_origin = actual_items.first().expect("refreshed future items").1;
    let expected_origin = expected_items.first().expect("oracle future items").1;
    let actual_signatures = actual_items
        .iter()
        .map(|(media_id, start, finish)| (*media_id, *start - actual_origin, *finish - *start))
        .collect::<Vec<_>>();
    let expected_signatures = expected_items
        .iter()
        .map(|(media_id, start, finish)| (*media_id, *start - expected_origin, *finish - *start))
        .collect::<Vec<_>>();
    assert_eq!(
        actual_signatures, expected_signatures,
        "restored duration cooldowns must preserve future media identities and relative timestamps"
    );

    let actual_reference =
        ersatzrs_infra::repositories::SqliteSchedulingContentRepository::new(reopened.clone())
            .classic_reference_data(RELATED_PLAYOUT_ID, seam, Duration::zero())
            .await
            .expect("reload refreshed checkpoints");
    let actual_contexts = actual_reference
        .existing_classic_checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.checkpoint_date >= seam)
        .take(1)
        .map(|checkpoint| (checkpoint.checkpoint_date, checkpoint.context.clone()))
        .collect::<Vec<_>>();
    let expected_contexts = oracle
        .classic_checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.checkpoint_date >= seam)
        .take(1)
        .map(|checkpoint| (checkpoint.checkpoint_date, checkpoint.context.clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        actual_contexts, expected_contexts,
        "repository round-trip must preserve future checkpoint contexts"
    );

    let actual_anchor = make_repositories(reopened.clone())
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .expect("reload refreshed playout")
        .anchor
        .expect("refreshed anchor");
    let expected_anchor = oracle.anchor.expect("oracle anchor");
    assert_eq!(actual_anchor.next_start, expected_anchor.next_start);
    assert_eq!(actual_anchor.context, expected_anchor.context);
    reopened.close().await;
}

async fn seed_smart_shuffle_cooldown(pool: &SqlitePool) {
    pool.execute(
        r#"INSERT INTO "PlaylistItem" (
               "Id", "Index", "PlaylistId", "CollectionType", "MediaItemId",
               "IncludeInProgramGuide", "PlaybackOrder", "PlayAll"
           ) VALUES
               (102, 1, 11, 'Movie', 102, 1, 'Shuffle', 1),
               (103, 2, 11, 'Movie', 201, 1, 'Shuffle', 1);
           UPDATE "PlaylistItem" SET "PlaybackOrder" = 'Shuffle' WHERE "PlaylistId" = 11;
           UPDATE "ProgramScheduleItem" SET "PlaybackOrder" = 'Shuffle' WHERE "Id" = 401"#,
    )
    .await
    .expect("seed randomized playlist");
    let policy = SchedulingSmartShufflePolicy {
        enabled: true,
        preset: SchedulingSmartShufflePreset::Custom,
        after_duration: Some("00:20:00".to_owned()),
        rules: vec![SchedulingSmartShuffleRule {
            id: "related".to_owned(),
            kind: SchedulingSmartShuffleRuleKind::Filter,
            search: Some("title contains Related".to_owned()),
        }],
        ..SchedulingSmartShufflePolicy::default()
    };
    sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(program_schedule_item_smart_shuffle_config_key(401))
        .bind(serde_json::to_string(&policy).expect("serialize Smart Shuffle policy"))
        .execute(pool)
        .await
        .expect("persist Smart Shuffle policy");
}

fn assert_json_timestamps_at_tick_precision(value: &serde_json::Value) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                assert_json_timestamps_at_tick_precision(value);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                assert_json_timestamps_at_tick_precision(value);
            }
        }
        serde_json::Value::String(value) => {
            if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
                assert_eq!(
                    timestamp.timestamp_subsec_nanos() % 100,
                    0,
                    "checkpoint timestamp is below persisted tick precision: {value}"
                );
            } else if let Ok(nested) = serde_json::from_str(value) {
                assert_json_timestamps_at_tick_precision(&nested);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

async fn assert_build_succeeded(pool: &SqlitePool, playout_id: EntityId, stage: &str) {
    let row = sqlx::query(
        r#"SELECT "Success", COALESCE("Message", '') AS "Message"
           FROM "PlayoutBuildStatus" WHERE "PlayoutId" = ?"#,
    )
    .bind(playout_id)
    .fetch_one(pool)
    .await
    .expect("load persisted build status");
    assert_eq!(
        row.try_get::<i64, _>("Success").expect("success flag"),
        1,
        "{stage} build failed: {}",
        row.try_get::<String, _>("Message").expect("message")
    );
}

#[tokio::test]
async fn classic_filler_chapters_survive_persisted_continue_and_refresh() {
    for mode in [PlayoutBuildMode::Continue, PlayoutBuildMode::Refresh] {
        assert_persisted_filler_chapters(mode).await;
    }
}

async fn assert_persisted_filler_chapters(mode: PlayoutBuildMode) {
    let directory = tempfile::tempdir().expect("temporary filler database");
    let path = directory.path().join("filler.sqlite");
    let pool = open_database(&path, true).await;
    seed_acceptance_fixture(&pool).await;
    seed_continuity_filler_chapters(&pool).await;
    let start = common::fixed_time();
    let first_finish = start + Duration::days(1);
    let finish = start + Duration::days(2);
    let oracle = build_filler_window(&pool, PlayoutBuildMode::Reset, start, finish, false).await;
    let first =
        build_filler_window(&pool, PlayoutBuildMode::Reset, start, first_finish, true).await;
    let seam = if mode == PlayoutBuildMode::Continue {
        first.anchor.as_ref().expect("first anchor").next_start
    } else {
        first
            .classic_checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.checkpoint_date > start)
            .map(|checkpoint| checkpoint.checkpoint_date)
            .min()
            .expect("generated day checkpoint")
    };
    assert!(oracle
        .added_items
        .iter()
        .any(|item| item.filler_kind == ersatzrs_domain::FillerKind::Fallback));
    assert!(oracle
        .added_items
        .iter()
        .any(|item| item.chapter_title.as_deref() == Some("Middle")));
    pool.close().await;
    let reopened = open_database(&path, false).await;
    let result = build_filler_window(&reopened, mode, seam, finish, true).await;
    let actual = make_repositories(reopened.clone())
        .playout
        .items_in_range(RELATED_PLAYOUT_ID, seam, finish)
        .await
        .expect("persisted filler rows")
        .into_iter()
        .filter(|item| item.start >= seam && item.start < finish)
        .map(|mut item| {
            item.id = 0;
            item
        })
        .collect::<Vec<_>>();
    let expected = oracle
        .added_items
        .iter()
        .filter(|item| item.start >= seam && item.start < finish)
        .cloned()
        .collect::<Vec<_>>();
    assert!(!expected.is_empty());
    assert_eq!(
        actual, expected,
        "{mode:?} must preserve chapter seeks, contexts, track overrides and timeline"
    );
    let persisted = make_repositories(reopened.clone())
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .expect("reloaded filler anchor")
        .anchor
        .expect("persisted filler anchor");
    assert_eq!(persisted.context, result.anchor.as_ref().unwrap().context);
    assert_eq!(persisted.context, oracle.anchor.as_ref().unwrap().context);
    let filler = actual
        .iter()
        .find(|item| item.media_item_id == UNRELATED_MEDIA_ID)
        .unwrap();
    assert_eq!(filler.preferred_audio_language_code.as_deref(), Some("nld"));
    assert_eq!(
        filler.preferred_subtitle_language_code.as_deref(),
        Some("eng")
    );
    reopened.close().await;
}

async fn build_filler_window(
    pool: &SqlitePool,
    mode: PlayoutBuildMode,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    persist: bool,
) -> ersatzrs_core::scheduling::SchedulingResult {
    let repositories = make_repositories(pool.clone());
    let playout = repositories
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .expect("load filler playout");
    let reference_data = repositories
        .scheduling
        .classic_reference_data(RELATED_PLAYOUT_ID, start, Duration::zero())
        .await
        .expect("load filler reference data");
    let result = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start,
            finish,
            mode,
            reference_data,
        })
        .expect("build filler window");
    if persist {
        repositories
            .playout
            .apply_build_result(&result, &playout, Duration::zero())
            .await
            .expect("atomically persist filler window and checkpoints");
    }
    result
}

async fn seed_continuity_filler_chapters(pool: &SqlitePool) {
    pool.execute(r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
        VALUES (91, 'Chapter filler', 1), (92, 'Empty padding', 1);
        INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "CustomIndex",
            "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode")
        VALUES (91, 201, 0, 'nld', 'eng');
        INSERT INTO "MediaChapter" ("Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title")
        VALUES (9101, 2010, 0, '00:00:00', '00:02:00', 'Opening'),
               (9102, 2010, 1, '00:03:00', '00:06:00', 'Middle'),
               (9103, 2010, 2, '00:07:00', '00:09:00', 'Ending');
        INSERT INTO "FillerPreset" ("Id", "Name", "FillerMode", "FillerKind", "CollectionType",
            "CollectionId", "Count", "PadToNearestMinute", "AllowWatermarks", "UseChaptersAsMediaItems")
        VALUES (77, 'Chapter fallback', 'Count', 'Fallback', 'Collection', 91, 1, NULL, 0, 1),
               (78, 'Clock padding', 'Pad', 'PostRoll', 'Collection', 92, NULL, 15, 0, 0);
        UPDATE "ProgramScheduleItem" SET "PreRollFillerId" = 77, "PostRollFillerId" = 78,
            "FallbackFillerId" = 77 WHERE "Id" = 401;"#).await.expect("seed chapter filler continuity fixture");
}

#[tokio::test]
async fn classic_filler_chapter_membership_preserves_unconsumed_identity_after_restart() {
    for remove_consumed in [false, true] {
        let directory = tempfile::tempdir().expect("temporary membership database");
        let path = directory.path().join("membership.sqlite");
        let pool = open_database(&path, true).await;
        seed_acceptance_fixture(&pool).await;
        seed_continuity_filler_chapters(&pool).await;
        pool.execute(
            r#"UPDATE "ProgramScheduleItem" SET "PostRollFillerId" = NULL WHERE "Id" = 401"#,
        )
        .await
        .expect("isolate chapter cursor from padding");
        let start = common::fixed_time();
        let first = build_filler_window(
            &pool,
            PlayoutBuildMode::Reset,
            start,
            start + Duration::minutes(1),
            true,
        )
        .await;
        assert_eq!(
            first.added_items[0].chapter_title.as_deref(),
            Some("Opening")
        );
        let seam = first.anchor.unwrap().next_start;
        if remove_consumed {
            pool.execute(r#"DELETE FROM "MediaChapter" WHERE "Id" = 9101"#)
                .await
                .expect("remove consumed chapter before cursor");
        } else {
            pool.execute(
                r#"INSERT INTO "MediaChapter"
                ("Id", "MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title")
                VALUES (9104, 2010, 3, '00:02:00', '00:02:30', 'New chapter')"#,
            )
            .await
            .expect("insert chapter before unconsumed cursor");
        }
        pool.close().await;
        let reopened = open_database(&path, false).await;
        let continued = build_filler_window(
            &reopened,
            PlayoutBuildMode::Continue,
            seam,
            seam + Duration::minutes(1),
            true,
        )
        .await;
        let next = &continued.added_items[0];
        assert_eq!(
            next.chapter_title.as_deref(),
            Some("Middle"),
            "membership mutation must preserve next surviving chapter (remove={remove_consumed})"
        );
        assert_eq!(next.in_point, Duration::minutes(3));
        assert_eq!(next.out_point, Duration::minutes(6));
        assert_eq!(next.preferred_audio_language_code.as_deref(), Some("nld"));
        let persisted = make_repositories(reopened.clone())
            .playout
            .items_in_range(RELATED_PLAYOUT_ID, seam, seam + Duration::minutes(1))
            .await
            .expect("load continued chapter");
        assert_eq!(persisted[0].chapter_title, next.chapter_title);
        assert_eq!(persisted[0].in_point, next.in_point);
        reopened.close().await;
    }
}

#[tokio::test]
async fn classic_filler_legacy_refresh_refuses_missing_identity_without_clearing_rows() {
    for remove_indices in [false, true] {
        let directory = tempfile::tempdir().expect("legacy filler database");
        let path = directory.path().join("legacy.sqlite");
        let pool = open_database(&path, true).await;
        seed_acceptance_fixture(&pool).await;
        seed_continuity_filler_chapters(&pool).await;
        let start = common::fixed_time();
        build_filler_window(
            &pool,
            PlayoutBuildMode::Reset,
            start,
            start + Duration::minutes(30),
            true,
        )
        .await;
        remove_filler_identity_checkpoints(&pool, remove_indices).await;
        let before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
        let repositories = make_repositories(pool.clone());
        let reference_data = repositories
            .scheduling
            .classic_reference_data(RELATED_PLAYOUT_ID, start, Duration::zero())
            .await
            .expect("load legacy filler checkpoint");
        let playout = repositories
            .playout
            .get_with_anchor(RELATED_PLAYOUT_ID)
            .await
            .expect("load legacy filler playout");
        let error = PlayoutBuilder::default()
            .build(SchedulingRequest {
                playout,
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect_err("legacy filler ordinal cannot prove identity continuity");
        assert!(
            error.to_string().contains("filler identity checkpoint"),
            "{error}"
        );
        assert!(
            before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await,
            "refused Refresh must preserve all persisted rows"
        );
        pool.close().await;
    }
}

async fn remove_filler_identity_checkpoints(pool: &SqlitePool, remove_indices: bool) {
    let rows = sqlx::query(
        r#"SELECT "CheckpointDate", "Context" FROM "PlayoutClassicCheckpoint" WHERE "PlayoutId" = ?"#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .fetch_all(pool)
    .await
    .expect("load produced checkpoints");
    assert!(!rows.is_empty());
    for row in rows {
        let mut checkpoint: serde_json::Value =
            serde_json::from_str(&row.get::<String, _>("Context"))
                .expect("produced checkpoint JSON");
        let mut anchor_context: serde_json::Value = serde_json::from_str(
            checkpoint["Anchor"]["context"]
                .as_str()
                .expect("produced anchor context"),
        )
        .expect("produced filler context JSON");
        let object = anchor_context.as_object_mut().unwrap();
        assert!(object.remove("filler_sources").is_some());
        if remove_indices {
            assert!(object.remove("filler_indices").is_some());
        }
        checkpoint["Anchor"]["context"] = serde_json::Value::String(anchor_context.to_string());
        sqlx::query(r#"UPDATE "PlayoutClassicCheckpoint" SET "Context" = ? WHERE "PlayoutId" = ? AND "CheckpointDate" = ?"#)
            .bind(checkpoint.to_string())
            .bind(RELATED_PLAYOUT_ID)
            .bind(row.get::<String, _>("CheckpointDate"))
            .execute(pool)
            .await
            .expect("persist legacy checkpoint shape");
    }
}

#[tokio::test]
async fn classic_filler_refresh_allows_new_and_removed_presets() {
    for add_preset in [false, true] {
        let directory = tempfile::tempdir().expect("changed filler database");
        let path = directory.path().join("changed.sqlite");
        let pool = open_database(&path, true).await;
        seed_acceptance_fixture(&pool).await;
        seed_continuity_filler_chapters(&pool).await;
        let start = common::fixed_time();
        build_filler_window(
            &pool,
            PlayoutBuildMode::Reset,
            start,
            start + Duration::minutes(30),
            true,
        )
        .await;
        if add_preset {
            pool.execute(r#"INSERT INTO "FillerPreset" ("Id", "Name", "FillerMode", "FillerKind",
                "CollectionType", "CollectionId", "Count", "AllowWatermarks", "UseChaptersAsMediaItems")
                VALUES (79, 'New preroll', 'Count', 'PreRoll', 'Collection', 91, 1, 0, 1);
                UPDATE "ProgramScheduleItem" SET "PreRollFillerId" = 79 WHERE "Id" = 401;"#)
                .await.expect("configure previously unused preset");
        } else {
            pool.execute(
                r#"UPDATE "ProgramScheduleItem" SET "PreRollFillerId" = NULL,
                "PostRollFillerId" = NULL, "FallbackFillerId" = NULL WHERE "Id" = 401"#,
            )
            .await
            .expect("remove filler references from schedule");
        }
        let refreshed = build_filler_window(
            &pool,
            PlayoutBuildMode::Refresh,
            start,
            start + Duration::hours(1),
            true,
        )
        .await;
        assert!(!refreshed.added_items.is_empty());
        let new_preset_used = refreshed.added_items.iter().any(|item| {
            item.scheduling_context
                .as_deref()
                .is_some_and(|context| context.contains("\"FillerPresetId\":79"))
        });
        assert_eq!(new_preset_used, add_preset);
        if !add_preset {
            assert!(refreshed
                .added_items
                .iter()
                .all(|item| item.filler_kind == ersatzrs_domain::FillerKind::None));
        }
        pool.close().await;
    }
}

#[tokio::test]
async fn classic_filler_expression_rejection_preserves_persisted_playout_and_redacts_input() {
    let directory = tempfile::tempdir().expect("expression rejection database");
    let path = directory.path().join("expression.sqlite");
    let pool = open_database(&path, true).await;
    seed_acceptance_fixture(&pool).await;
    seed_continuity_filler_chapters(&pool).await;
    pool.execute(
        r#"INSERT INTO "MediaChapter"
        ("MediaVersionId", "ChapterId", "StartTime", "EndTime", "Title") VALUES
        (1010, 0, '00:00:00', '00:05:00', 'Synthetic hidden chapter title'),
        (1010, 1, '00:05:00', '00:10:00', 'Second half');
        INSERT INTO "FillerPreset" ("Id", "Name", "FillerMode", "FillerKind",
            "CollectionType", "CollectionId", "Count", "AllowWatermarks",
            "UseChaptersAsMediaItems", "Expression")
        VALUES (79, 'Expression midroll', 'Count', 'MidRoll', 'Collection', 91, 1, 0, 1, 'true');
        UPDATE "ProgramScheduleItem" SET "PreRollFillerId" = NULL,
            "PostRollFillerId" = NULL, "MidRollFillerId" = 79 WHERE "Id" = 401;"#,
    )
    .await
    .expect("seed valid chapter expression schedule");
    let start = common::fixed_time();
    let built = build_filler_window(
        &pool,
        PlayoutBuildMode::Reset,
        start,
        start + Duration::minutes(30),
        true,
    )
    .await;
    assert!(
        built.added_items.iter().any(|item| {
            item.media_item_id == RELATED_MEDIA_ID && item.in_point == Duration::minutes(5)
        }),
        "baseline must genuinely split the primary programme"
    );
    let before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    let marker = "synthetic-private-expression-marker";
    let expressions = [
        format!("'{marker}' = ("),
        format!(r"false and ('\uD800' = '{marker}')"),
        format!("false and (#2026-02-30# = #2026-02-28# or '{marker}' = '')"),
        format!("'{marker}' = '{marker}'{}", " ".repeat(65_537)),
        format!(
            "{}'{marker}' = '{marker}'{}",
            "(".repeat(33),
            ")".repeat(33)
        ),
    ];
    for expression in expressions {
        sqlx::query(r#"UPDATE "FillerPreset" SET "Expression" = ? WHERE "Id" = 79"#)
            .bind(&expression)
            .execute(&pool)
            .await
            .expect("persist rejected expression");
        let repositories = make_repositories(pool.clone());
        let reference_data = repositories
            .scheduling
            .classic_reference_data(RELATED_PLAYOUT_ID, start, Duration::zero())
            .await
            .expect("hydrate persisted expression");
        assert_eq!(
            reference_data.classic_filler_presets[&79]
                .expression
                .as_deref(),
            Some(expression.as_str())
        );
        let playout = repositories
            .playout
            .get_with_anchor(RELATED_PLAYOUT_ID)
            .await
            .expect("load existing scheduled playout");
        let error = PlayoutBuilder::default()
            .build(SchedulingRequest {
                playout,
                start,
                finish: start + Duration::hours(1),
                mode: PlayoutBuildMode::Refresh,
                reference_data,
            })
            .expect_err("invalid or excessive expression must fail before replacement");
        assert!(matches!(
            error,
            ersatzrs_core::scheduling::SchedulingError::InvalidClassicFillerExpression(401)
        ));
        let diagnostic = format!("{error} {error:?}");
        assert!(!diagnostic.contains(marker));
        assert!(!diagnostic.contains("Synthetic hidden chapter title"));
        assert!(!diagnostic.contains(&expression));
        assert!(
            before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await,
            "expression rejection must retain all existing items, anchors and checkpoints"
        );
    }
    pool.close().await;
}

#[tokio::test]
async fn yaml_marathon_history_survives_restart_cleanup_and_refresh() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("yaml-history.sqlite");
    let pool = open_database(&path, true).await;
    seed_grouped_playlist_fixture(&pool, true).await;
    common::seed_media_item(&pool, 103, 1).await;
    seed_movie(&pool, 103, "Third group").await;
    pool.execute(
        r#"INSERT INTO "Director" ("MovieMetadataId", "Name") VALUES (103, 'Third Director')"#,
    )
    .await
    .unwrap();
    let yaml = "content:\n  - key: marathon\n    marathon: Movies\n    searches: ['type:movie']\n    group_by: director\n    item_order: chronological\n    shuffle_groups: true\nplayout:\n  - count: '1'\n    content: marathon\n  - repeat: true\n";
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind" = 'Sequential', "ScheduleFile" = ?, "Seed" = 12345 WHERE "Id" = ?"#)
        .bind(yaml).bind(RELATED_PLAYOUT_ID).execute(&pool).await.unwrap();
    let start = common::fixed_time();
    let finish = start + Duration::hours(4);
    let full = build_yaml_history_window(&pool, yaml, PlayoutBuildMode::Reset, start, finish).await;
    let first = build_yaml_history_window(
        &pool,
        yaml,
        PlayoutBuildMode::Reset,
        start,
        start + Duration::minutes(110),
    )
    .await;
    persist_yaml_history_result(&pool, &first).await;
    pool.close().await;
    let pool = open_database(&path, false).await;
    let seam = first.anchor.as_ref().unwrap().next_start;
    let continued =
        build_yaml_history_window(&pool, yaml, PlayoutBuildMode::Continue, seam, finish).await;
    assert_eq!(
        continued.added_items,
        full.added_items[first.added_items.len()..]
    );
    persist_yaml_history_result(&pool, &continued).await;
    pool.close().await;
    let pool = open_database(&path, false).await;
    let refreshed =
        build_yaml_history_window(&pool, yaml, PlayoutBuildMode::Refresh, seam, finish).await;
    let timeline = |items: &[ersatzrs_domain::PlayoutItem]| {
        items
            .iter()
            .map(|item| (item.media_item_id, item.start, item.finish))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        timeline(&refreshed.added_items),
        timeline(&continued.added_items)
    );
    persist_yaml_history_result(&pool, &refreshed).await;
    assert_eq!(
        playlist_timeline(&pool).await.len(),
        refreshed.added_items.len()
    );
    assert_yaml_legacy_history_refusal(&pool, seam).await;
    pool.close().await;
}

async fn build_yaml_history_window(
    pool: &SqlitePool,
    yaml: &str,
    mode: PlayoutBuildMode,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> ersatzrs_core::scheduling::SchedulingResult {
    let repos = make_repositories(pool.clone());
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let reference_data = repos
        .scheduling
        .yaml_reference_data(RELATED_PLAYOUT_ID, yaml, Duration::zero())
        .await
        .unwrap();
    PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout,
            start,
            finish,
            mode,
            reference_data,
        })
        .expect("build YAML history window")
}

async fn persist_yaml_history_result(
    pool: &SqlitePool,
    result: &ersatzrs_core::scheduling::SchedulingResult,
) {
    let repos = make_repositories(pool.clone());
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    repos
        .playout
        .apply_build_result(result, &playout, Duration::zero())
        .await
        .unwrap();
}

async fn assert_yaml_legacy_history_refusal(pool: &SqlitePool, start: DateTime<Utc>) {
    sqlx::query(r#"UPDATE "PlayoutHistory" SET "Details" = json_extract("Details", '$.media_details') WHERE "PlayoutId" = ? AND "ChildKey" IS NULL"#)
        .bind(RELATED_PLAYOUT_ID).execute(pool).await.unwrap();
    let before = playlist_timeline(pool).await;
    let error = run_requested_playout_build(
        pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(RELATED_PLAYOUT_ID, PlayoutBuildMode::Refresh),
        start,
        None,
    )
    .await
    .expect_err("legacy YAML history must refuse Refresh");
    assert!(error.to_string().contains("cannot safely restore"));
    assert_eq!(
        playlist_timeline(pool).await,
        before,
        "refusal must not clear scheduled items"
    );
    let status: (i64, String) = sqlx::query_as(
        r#"SELECT "Success", "Message" FROM "PlayoutBuildStatus" WHERE "PlayoutId" = ?"#,
    )
    .bind(RELATED_PLAYOUT_ID)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(status.0, 0);
    assert!(status.1.contains("Reset Playout"));
    run_requested_playout_build(
        pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(RELATED_PLAYOUT_ID, PlayoutBuildMode::Reset),
        start,
        None,
    )
    .await
    .expect("explicit Reset rebuilds compatible YAML history");
    assert_build_succeeded(pool, RELATED_PLAYOUT_ID, "YAML Reset").await;
}

mod common;

use std::collections::BTreeSet;
use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_core::repositories::ProgramScheduleRepository;
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutBuildRequest};
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
            PlayoutBuildRequest { playout_id, mode },
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

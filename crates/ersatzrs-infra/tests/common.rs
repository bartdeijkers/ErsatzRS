#![allow(dead_code)]

use chrono::{DateTime, Duration, TimeZone, Utc};
use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
use ersatzrs_domain::{
    Channel, CollectionEnumeratorState, CollectionType, FFmpegProfile, Library, LibraryMediaKind,
    MediaItem, MediaSourceKind, PlaybackOrder, Playout, PlayoutAnchor, PlayoutBuildStatus,
    PlayoutGap, PlayoutHistory, PlayoutItem, PlayoutProgramScheduleAnchor,
    PlayoutScheduleItemFillGroupIndex, PlayoutTemplate, ProgramSchedule,
};
use ersatzrs_infra::repositories::Repositories;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Executor, SqlitePool};
use uuid::Uuid;

/// Create a migrated in-memory database for repository tests.
///
/// # Panics
///
/// Panics when the in-memory database cannot be created, foreign keys cannot be
/// enabled, or the test migration cannot run.
pub async fn test_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("create in-memory sqlite pool");

    pool.execute("PRAGMA foreign_keys = ON")
        .await
        .expect("enable sqlite foreign keys");

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    // Seed a default Resolution row required by FFmpegProfile's NOT NULL FK.
    pool.execute(
        r#"INSERT OR IGNORE INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#,
    )
    .await
    .expect("seed default resolution");

    pool
}

#[must_use]
pub fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap()
}

#[must_use]
pub fn profile(id: i64, name: &str) -> FFmpegProfile {
    FFmpegProfile {
        id,
        name: name.to_owned(),
        resolution_id: 1,
        ..FFmpegProfile::default()
    }
}

#[must_use]
pub fn channel(id: i64, number: &str, is_enabled: bool) -> Channel {
    Channel {
        id,
        unique_id: Uuid::from_u128(u128::try_from(id).unwrap_or_default()),
        number: number.to_owned(),
        sort_number: f64::from(i32::try_from(id).unwrap_or_default()),
        name: format!("Channel {number}"),
        ffmpeg_profile_id: 1,
        is_enabled,
        ..Channel::default()
    }
}

/// # Panics
/// Panics if seeding the fixture profile or channel fails.
pub async fn seed_profile_and_channel(repos: &Repositories, channel_id: i64) {
    repos
        .ffmpeg_profile
        .upsert(&profile(1, "default"))
        .await
        .unwrap();
    repos
        .channel
        .upsert(&channel(channel_id, &channel_id.to_string(), true))
        .await
        .unwrap();
}

/// # Panics
/// Panics if seeding the fixture media source, library, or path fails.
pub async fn seed_media_path(pool: &SqlitePool, source_id: i64, library_id: i64, path_id: i64) {
    sqlx::query(r#"INSERT OR REPLACE INTO "MediaSource" ("Id") VALUES (?)"#)
        .bind(source_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT OR REPLACE INTO "LocalMediaSource" ("Id") VALUES (?)"#)
        .bind(source_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT OR REPLACE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
             VALUES (?, 'local', 'Movies', ?)"#,
    )
    .bind(library_id)
    .bind(source_id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT OR REPLACE INTO "LocalLibrary" ("Id") VALUES (?)"#)
        .bind(library_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT OR REPLACE INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (?, ?, '/media')"#,
    )
    .bind(path_id)
    .bind(library_id)
    .execute(pool)
    .await
    .unwrap();
}

/// # Panics
/// Panics if seeding the fixture media item fails.
pub async fn seed_media_item(pool: &SqlitePool, item_id: i64, path_id: i64) {
    sqlx::query(
        r#"INSERT OR REPLACE INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, ?, 0)"#,
    )
    .bind(item_id)
    .bind(path_id)
    .execute(pool)
    .await
    .unwrap();
}

pub async fn seed_media_graph(pool: &SqlitePool) {
    seed_media_path(pool, 1, 1, 1).await;
    seed_media_item(pool, 1, 1).await;
}

#[must_use]
pub fn library(id: i64, source_id: i64, path_id: i64) -> Library {
    Library {
        id,
        name: "Movies".to_owned(),
        media_kind: LibraryMediaKind::Movies,
        media_source_id: source_id,
        media_source_kind: MediaSourceKind::Plex,
        path_ids: vec![path_id],
        ..Library::default()
    }
}

#[must_use]
pub fn media_item(id: i64, library_path_id: i64) -> MediaItem {
    MediaItem {
        id,
        library_path_id,
        ..MediaItem::default()
    }
}

#[must_use]
pub fn schedule(id: i64, item_ids: Vec<i64>) -> ProgramSchedule {
    ProgramSchedule {
        id,
        name: format!("schedule {id}"),
        item_ids,
        ..ProgramSchedule::default()
    }
}

#[must_use]
pub fn playout_item(id: i64, playout_id: i64, media_item_id: i64) -> PlayoutItem {
    let start = fixed_time();
    PlayoutItem {
        id,
        media_item_id,
        playout_id,
        start,
        finish: start + Duration::minutes(30),
        ..PlayoutItem::default()
    }
}

#[must_use]
pub fn build_status(playout_id: i64) -> PlayoutBuildStatus {
    PlayoutBuildStatus {
        playout_id,
        last_build: fixed_time(),
        success: true,
        message: None,
    }
}

#[must_use]
pub fn playout(id: i64, channel_id: i64, schedule_id: i64) -> Playout {
    Playout {
        id,
        channel_id,
        program_schedule_id: Some(schedule_id),
        on_demand_checkpoint: Some(fixed_time() + Duration::minutes(5)),
        ..Playout::default()
    }
}

#[must_use]
pub fn playout_anchor() -> PlayoutAnchor {
    PlayoutAnchor {
        schedule_items_enumerator_state: CollectionEnumeratorState {
            seed: 77,
            index: 5,
            started: true,
        },
        next_start: fixed_time() + Duration::minutes(45),
        multiple_remaining: Some(2),
        duration_finish: Some(fixed_time() + Duration::hours(2)),
        in_flood: true,
        in_duration_filler: false,
        next_guide_group: 9,
        next_instruction_index: 4,
        context: Some("runtime-state".to_owned()),
    }
}

#[must_use]
pub fn playout_gap(id: i64, playout_id: i64) -> PlayoutGap {
    PlayoutGap {
        id,
        playout_id,
        start: fixed_time() + Duration::hours(3),
        finish: fixed_time() + Duration::hours(3) + Duration::minutes(15),
    }
}

#[must_use]
pub fn playout_history(id: i64, playout_id: i64) -> PlayoutHistory {
    PlayoutHistory {
        seed: None,
        id,
        playout_id,
        block_id: None,
        playback_order: PlaybackOrder::Chronological,
        key: "history-key".to_owned(),
        child_key: Some("child-key".to_owned()),
        when: fixed_time() + Duration::minutes(10),
        finish: Some(fixed_time() + Duration::minutes(40)),
        index: 3,
        details: "{\"kind\":\"episode\"}".to_owned(),
        is_current_child: true,
    }
}

#[must_use]
pub fn program_schedule_anchor(id: i64, playout_id: i64) -> PlayoutProgramScheduleAnchor {
    PlayoutProgramScheduleAnchor {
        id,
        playout_id,
        anchor_date: Some(fixed_time() + Duration::days(1)),
        collection_type: CollectionType::Collection,
        collection_id: Some(99),
        multi_collection_id: None,
        smart_collection_id: None,
        rerun_collection_id: None,
        media_item_id: None,
        playlist_id: None,
        search_query: Some("title:fixture".to_owned()),
        fake_collection_key: Some("anchor-key".to_owned()),
        enumerator_state: CollectionEnumeratorState {
            seed: 33,
            index: 7,
            started: true,
        },
    }
}

#[must_use]
pub fn fill_group_index(
    id: i64,
    playout_id: i64,
    program_schedule_item_id: i64,
) -> PlayoutScheduleItemFillGroupIndex {
    PlayoutScheduleItemFillGroupIndex {
        id,
        playout_id,
        program_schedule_item_id,
        enumerator_state: CollectionEnumeratorState {
            seed: 55,
            index: 11,
            started: true,
        },
    }
}

#[must_use]
pub fn playout_template(
    playout_id: i64,
    template_id: i64,
    deco_template_id: i64,
) -> PlayoutTemplate {
    PlayoutTemplate {
        playout_id,
        template_id,
        updated_ticks: 1234,
        deco_template_id: Some(deco_template_id),
        index: 2,
        days_of_week: vec![1, 2, 3],
        days_of_month: vec![1, 15],
        months_of_year: vec![4],
        limit_to_date_range: true,
        start_month: 4,
        start_day: 1,
        start_year: Some(2026),
        end_month: 4,
        end_day: 30,
        end_year: Some(2026),
    }
}

/// # Panics
/// Panics if seeding the fixture collection or collection item fails.
pub async fn seed_collection_target(pool: &SqlitePool) {
    seed_media_graph(pool).await;
    pool.execute(
        r#"INSERT OR REPLACE INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (99, 'fixture', 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT OR REPLACE INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (99, 1)"#,
    )
    .await
    .unwrap();
}

#[must_use]
pub fn collection_type() -> CollectionType {
    CollectionType::Collection
}

/// Inspect typed tag identity through the persisted provider projection.
///
/// # Panics
///
/// Panics when the fixture database cannot read the expected typed tag rows.
pub async fn collection_tag_rows(pool: &SqlitePool, owner: &str) -> Vec<(i64, String, String)> {
    sqlx::query_as(
        r#"SELECT "Id", "Name", "ExternalCollectionId" FROM "Tag"
        WHERE "ExternalTypeId" = ? ORDER BY "Id""#,
    )
    .bind(owner)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Assert that collection membership stays within one provider source.
///
/// # Panics
/// Panics when the query fails or membership belongs to another source.
pub async fn assert_collection_member_source(pool: &SqlitePool, expected: i64) {
    let sources: Vec<i64> = sqlx::query_scalar(
        r#"SELECT DISTINCT l."MediaSourceId"
        FROM "CollectionItem" ci JOIN "MediaItem" m ON m."Id" = ci."MediaItemId"
        JOIN "LibraryPath" p ON p."Id" = m."LibraryPathId"
        JOIN "Library" l ON l."Id" = p."LibraryId""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        sources,
        vec![expected],
        "collection members belong to the requested server"
    );
}

/// Verify that equal provider IDs/names produce independent collections and tags.
///
/// # Panics
/// Panics for an unsupported fixture table, failed query or ownership mismatch.
pub async fn assert_source_owned_collections(
    pool: &SqlitePool,
    table: &str,
    tag_type: &str,
    members_per_source: usize,
) {
    assert!(matches!(
        table,
        "PlexCollection" | "JellyfinCollection" | "EmbyCollection"
    ));
    let rows: Vec<(i64, i64, i64, String, String)> =
        sqlx::query_as(ersatzrs_infra::db::assert_sql_safe(format!(
            r#"SELECT p."Id", p."MediaSourceId", l."MediaSourceId", p."Name", c."Name"
            FROM "{table}" p JOIN "Collection" c ON c."Id" = p."Id"
            JOIN "CollectionItem" ci ON ci."CollectionId" = p."Id"
            JOIN "MediaItem" m ON m."Id" = ci."MediaItemId"
            JOIN "LibraryPath" lp ON lp."Id" = m."LibraryPathId"
            JOIN "Library" l ON l."Id" = lp."LibraryId" ORDER BY p."Id""#
        )))
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), members_per_source * 2);
    let ids: std::collections::BTreeSet<_> = rows.iter().map(|row| row.0).collect();
    assert_eq!(ids.len(), 2);
    let display_names: std::collections::BTreeSet<_> = rows.iter().map(|row| &row.4).collect();
    assert_eq!(display_names.len(), 2);
    for (_, owner, member_source, provider_name, _) in rows {
        assert_eq!(owner, member_source);
        assert_eq!(provider_name, "Favorites");
    }
    let tags = collection_tag_rows(pool, tag_type).await;
    assert_eq!(tags.len(), members_per_source * 2);
    assert!(tags.iter().all(|(_, name, _)| name == "Favorites"));
}

/// Model retained membership that predates source isolation without touching tags.
///
/// # Panics
/// Panics for an unsupported fixture table or a failed fixture insert.
pub async fn add_foreign_collection_member(pool: &SqlitePool, table: &str) {
    assert!(matches!(
        table,
        "PlexCollection" | "JellyfinCollection" | "EmbyCollection"
    ));
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
        r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId")
        SELECT owner."Id", ci."MediaItemId" FROM "{table}" owner
        JOIN "{table}" other ON other."MediaSourceId" = 2
        JOIN "CollectionItem" ci ON ci."CollectionId" = other."Id"
        WHERE owner."MediaSourceId" = 1 ORDER BY ci."Id" LIMIT 1"#
    )))
    .execute(pool)
    .await
    .unwrap();
}

#![allow(dead_code)]
#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]

use chrono::{DateTime, Duration, TimeZone, Utc};
use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
use ersatzrs_domain::{
    Channel, CollectionType, FFmpegProfile, Library, LibraryMediaKind, MediaItem, MediaSourceKind,
    PlayoutBuildStatus, PlayoutItem, ProgramSchedule,
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

    pool
}

pub fn fixed_time() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap()
}

pub fn profile(id: i64, name: &str) -> FFmpegProfile {
    FFmpegProfile {
        id,
        name: name.to_owned(),
        ..FFmpegProfile::default()
    }
}

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

pub async fn seed_media_path(pool: &SqlitePool, source_id: i64, library_id: i64, path_id: i64) {
    pool.execute(
        format!(
            "INSERT OR REPLACE INTO media_sources (id, name, kind) VALUES ({source_id}, 'local', 'Local')"
        )
        .as_str(),
    )
    .await
    .unwrap();
    pool.execute(
        format!(
            "INSERT OR REPLACE INTO libraries (id, name, media_kind, media_source_id, media_source_kind)
             VALUES ({library_id}, 'local', 'Movies', {source_id}, 'Local')"
        )
        .as_str(),
    )
    .await
    .unwrap();
    pool.execute(
        format!(
            "INSERT OR REPLACE INTO library_paths (id, library_id, path) VALUES ({path_id}, {library_id}, '/media')"
        )
        .as_str(),
    )
    .await
    .unwrap();
}

pub async fn seed_media_item(pool: &SqlitePool, item_id: i64, path_id: i64) {
    pool.execute(
        format!(
            "INSERT OR REPLACE INTO media_items (id, library_path_id, state) VALUES ({item_id}, {path_id}, 'Normal')"
        )
        .as_str(),
    )
    .await
    .unwrap();
}

pub async fn seed_media_graph(pool: &SqlitePool) {
    seed_media_path(pool, 1, 1, 1).await;
    seed_media_item(pool, 1, 1).await;
}

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

pub fn media_item(id: i64, library_path_id: i64) -> MediaItem {
    MediaItem {
        id,
        library_path_id,
        ..MediaItem::default()
    }
}

pub fn schedule(id: i64, item_ids: Vec<i64>) -> ProgramSchedule {
    ProgramSchedule {
        id,
        name: format!("schedule {id}"),
        item_ids,
        ..ProgramSchedule::default()
    }
}

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

pub fn build_status(playout_id: i64) -> PlayoutBuildStatus {
    PlayoutBuildStatus {
        playout_id,
        last_build: fixed_time(),
        success: true,
        message: None,
    }
}

pub async fn seed_collection_target(pool: &SqlitePool) {
    seed_media_graph(pool).await;
    pool.execute("INSERT OR REPLACE INTO collections (id, name, collection_type) VALUES (99, 'fixture', 'Collection')")
        .await
        .unwrap();
    pool.execute(
        "INSERT OR REPLACE INTO collection_items (id, collection_id, media_item_id, sort_order)
         VALUES (99, 99, 1, 0)",
    )
    .await
    .unwrap();
}

pub fn collection_type() -> CollectionType {
    CollectionType::Collection
}

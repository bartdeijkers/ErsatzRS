#![cfg(not(feature = "mysql"))]
//! Regression coverage for the media-server missing-item state machine
//! (`crate::media_server`): trashing items no longer on the server and flagging
//! items whose local file is missing as `RemoteOnly`/`Unavailable`.
mod common;

use ersatzrs_infra::media_server::{
    flag_missing_media_server_items, flag_unavailable_media_server_items,
};
use sqlx::{Row, SqlitePool};

async fn seed_library(pool: &SqlitePool) {
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "JellyfinMediaSource" ("Id", "ServerName") VALUES (1, 'srv')"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (1, 'J', 1, 1)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, 'jf')"#)
        .execute(pool)
        .await
        .unwrap();
}

/// Seed a `JellyfinMovie` media item (no media file). Returns the `MediaItem` id.
async fn seed_movie(pool: &SqlitePool, item_id: &str, state: i64) -> i64 {
    let id: i64 =
        sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (1, ?)"#)
            .bind(state)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid();
    sqlx::query(r#"INSERT INTO "Movie" ("Id") VALUES (?)"#)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "JellyfinMovie" ("Id", "ItemId", "Etag") VALUES (?, ?, 'e')"#)
        .bind(id)
        .bind(item_id)
        .execute(pool)
        .await
        .unwrap();
    id
}

/// Attach a `MediaVersion` + `MediaFile` at `path` to an existing media item.
async fn attach_file(pool: &SqlitePool, item_id: i64, path: &str) {
    let version_id: i64 = sqlx::query(
        r#"INSERT INTO "MediaVersion"
           ("DateAdded", "DateUpdated", "Duration", "Height", "VideoScanKind", "Width", "MovieId")
           VALUES ('2024-01-01 00:00:00', '2024-01-01 00:00:00', '00:30:00', 1080, 1, 1920, ?)"#,
    )
    .bind(item_id)
    .execute(pool)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (?, ?, ?)"#,
    )
    .bind(version_id)
    .bind(path)
    .bind(format!("{item_id}:{path}"))
    .execute(pool)
    .await
    .unwrap();
}

async fn state(pool: &SqlitePool, id: i64) -> i64 {
    sqlx::query(r#"SELECT "State" FROM "MediaItem" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("State")
}

#[tokio::test]
async fn trash_flags_items_absent_from_server() {
    let pool = common::test_db().await;
    seed_library(&pool).await;
    let present = seed_movie(&pool, "present", 0).await;
    let gone = seed_movie(&pool, "gone", 0).await;

    let mut incoming = std::collections::HashSet::new();
    incoming.insert("present".to_string());

    let flagged = flag_missing_media_server_items(&pool, "JellyfinMovie", "ItemId", 1, &incoming)
        .await
        .unwrap();

    assert_eq!(flagged, 1, "one item flagged");
    assert_eq!(state(&pool, present).await, 0, "present item normal");
    assert_eq!(state(&pool, gone).await, 1, "absent item FileNotFound");
}

#[tokio::test]
async fn unavailable_flags_present_item_with_missing_local_file_as_remote_only() {
    let pool = common::test_db().await;
    seed_library(&pool).await;

    // One item whose file exists, one whose file is gone, one previously
    // RemoteOnly whose file came back.
    let here = seed_movie(&pool, "here", 0).await;
    attach_file(
        &pool,
        here,
        concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
    )
    .await;
    let moved = seed_movie(&pool, "moved", 0).await;
    attach_file(&pool, moved, "/definitely/not/here/movie.mkv").await;
    let restored = seed_movie(&pool, "restored", 3).await;
    attach_file(
        &pool,
        restored,
        concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
    )
    .await;

    let incoming: std::collections::HashSet<String> = ["here", "moved", "restored"]
        .into_iter()
        .map(String::from)
        .collect();

    flag_unavailable_media_server_items(&pool, "JellyfinMovie", "ItemId", 1, &incoming, &[], true)
        .await
        .unwrap();

    assert_eq!(state(&pool, here).await, 0, "present file stays normal");
    assert_eq!(state(&pool, moved).await, 3, "missing file -> RemoteOnly");
    assert_eq!(state(&pool, restored).await, 0, "returned file restored");
}

#[tokio::test]
async fn unavailable_flags_present_item_with_missing_local_file_as_unavailable_without_remote_streaming(
) {
    let pool = common::test_db().await;
    seed_library(&pool).await;

    let moved = seed_movie(&pool, "moved", 0).await;
    attach_file(&pool, moved, "/definitely/not/here/no-remote.mkv").await;
    let restored = seed_movie(&pool, "restored", 2).await;
    attach_file(
        &pool,
        restored,
        concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
    )
    .await;

    let incoming: std::collections::HashSet<String> = ["moved", "restored"]
        .into_iter()
        .map(String::from)
        .collect();

    flag_unavailable_media_server_items(&pool, "JellyfinMovie", "ItemId", 1, &incoming, &[], false)
        .await
        .unwrap();

    assert_eq!(state(&pool, moved).await, 2, "missing file -> Unavailable");
    assert_eq!(state(&pool, restored).await, 0, "returned file restored");
}

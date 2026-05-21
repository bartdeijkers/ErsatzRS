//! Slice 4 wiring tests for `extract_embedded_show_subtitles`.
//!
//! These cover the config gate and the empty-work early return without
//! spawning ffmpeg or writing to the real subtitle cache. The full
//! extract-and-flip-to-`IsExtracted=1` path requires a real media file with an
//! embedded subtitle and writes to `data_local_dir()`; it is exercised by the
//! manual verification step in the plan rather than in CI.

#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_infra::ffmpeg_config::{
    CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES, CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
};
use ersatzrs_infra::subtitles::extract_embedded_show_subtitles;
use sqlx::{Row, SqlitePool};

/// Seeds Show(10) → Season(11) → Episode(12) → `EpisodeMetadata` with one
/// extractable embedded `subrip` subtitle (`IsExtracted=0`) and a resolvable
/// media file path. Returns the `Subtitle` row id.
async fn seed_extractable_episode(pool: &SqlitePool) -> i64 {
    common::seed_media_path(pool, 1, 1, 1).await;
    for id in [10_i64, 11, 12] {
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }
    sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (10)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (11, 10, 1)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (12, 11)"#)
        .execute(pool)
        .await
        .unwrap();
    let metadata = sqlx::query(
        r#"INSERT INTO "EpisodeMetadata"
           ("EpisodeId", "MetadataKind", "DateAdded", "DateUpdated")
           VALUES (12, 2, '2026-01-01 00:00:00', '2026-01-01 00:00:00')"#,
    )
    .execute(pool)
    .await
    .unwrap();
    let metadata_id = metadata.last_insert_rowid();

    // A head media version + file so input-path resolution can succeed.
    let version = sqlx::query(
        r#"INSERT INTO "MediaVersion"
           ("EpisodeId", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind")
           VALUES (12, '2026-01-01 00:00:00', '2026-01-01 00:00:00', '00:30:00', 720, 1280, 0)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    let version_id = version.last_insert_rowid();
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
           VALUES (?, '/nonexistent/show/s01e01.mkv', 'hash')"#,
    )
    .bind(version_id)
    .execute(pool)
    .await
    .unwrap();

    // Embedded text subtitle that still needs extraction.
    let subtitle = sqlx::query(
        r#"INSERT INTO "Subtitle"
           ("EpisodeMetadataId", "SubtitleKind", "StreamIndex", "Codec",
            "Default", "Forced", "SDH", "IsExtracted", "DateAdded", "DateUpdated")
           VALUES (?, 0, 2, 'subrip', 0, 0, 0, 0, '2026-01-01 00:00:00', '2026-01-01 00:00:00')"#,
    )
    .bind(metadata_id)
    .execute(pool)
    .await
    .unwrap();
    subtitle.last_insert_rowid()
}

async fn is_extracted(pool: &SqlitePool, subtitle_id: i64) -> i64 {
    sqlx::query(r#"SELECT "IsExtracted" FROM "Subtitle" WHERE "Id" = ?"#)
        .bind(subtitle_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("IsExtracted")
}

#[tokio::test]
async fn extraction_is_noop_when_extract_key_disabled() {
    let pool = common::test_db().await;
    let subtitle_id = seed_extractable_episode(&pool).await;

    // Default config: use=true (implicit), extract=false (implicit). The gate
    // must short-circuit before touching ffmpeg or the subtitle cache.
    extract_embedded_show_subtitles(&pool, 10).await.unwrap();

    assert_eq!(
        is_extracted(&pool, subtitle_id).await,
        0,
        "subtitle must not be marked extracted when extract key is off"
    );
}

#[tokio::test]
async fn extraction_is_noop_when_use_key_disabled() {
    let pool = common::test_db().await;
    let subtitle_id = seed_extractable_episode(&pool).await;

    // use=false overrides extract=true.
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, 'False')"#)
        .bind(CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, 'True')"#)
        .bind(CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES)
        .execute(&pool)
        .await
        .unwrap();

    extract_embedded_show_subtitles(&pool, 10).await.unwrap();

    assert_eq!(is_extracted(&pool, subtitle_id).await, 0);
}

#[tokio::test]
async fn extraction_returns_ok_when_no_episodes_have_text_subtitles() {
    let pool = common::test_db().await;

    // Enable both keys, but seed nothing extractable. The function must reach
    // the gate-passing branch and return Ok with no episodes to process.
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, 'True')"#)
        .bind(CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, 'True')"#)
        .bind(CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES)
        .execute(&pool)
        .await
        .unwrap();

    extract_embedded_show_subtitles(&pool, 999).await.unwrap();
}

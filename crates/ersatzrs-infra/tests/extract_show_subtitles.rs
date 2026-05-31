//! Slice 4 wiring tests for `extract_embedded_show_subtitles`.
//!
//! These cover the config gate and the empty-work early return without
//! spawning ffmpeg or writing to the real subtitle cache. The real-media smoke
//! test below generates a tiny MKV with an embedded text subtitle when `FFmpeg`
//! is available and verifies the full extract-and-flip-to-`IsExtracted=1` path.

#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_infra::ffmpeg_config::{
    CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES, CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
};
use ersatzrs_infra::file_system_layout::subtitle_cache_folder;
use ersatzrs_infra::subtitles::extract_embedded_show_subtitles;
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use tokio::process::Command;

/// Seeds Show(10) → Season(11) → Episode(12) → `EpisodeMetadata` with one
/// extractable embedded `subrip` subtitle (`IsExtracted=0`) and a resolvable
/// media file path. Returns the `Subtitle` row id.
async fn seed_extractable_episode(pool: &SqlitePool) -> i64 {
    seed_extractable_episode_with_path(pool, "/nonexistent/show/s01e01.mkv").await
}

async fn seed_extractable_episode_with_path(pool: &SqlitePool, media_path: &str) -> i64 {
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
           VALUES (?, ?, 'hash')"#,
    )
    .bind(version_id)
    .bind(media_path)
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

async fn enable_embedded_subtitle_extraction(pool: &SqlitePool) {
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, 'True')"#)
        .bind(CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, 'True')"#)
        .bind(CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES)
        .execute(pool)
        .await
        .unwrap();
}

async fn is_extracted(pool: &SqlitePool, subtitle_id: i64) -> i64 {
    sqlx::query(r#"SELECT "IsExtracted" FROM "Subtitle" WHERE "Id" = ?"#)
        .bind(subtitle_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("IsExtracted")
}

async fn subtitle_path(pool: &SqlitePool, subtitle_id: i64) -> Option<String> {
    sqlx::query(r#"SELECT "Path" FROM "Subtitle" WHERE "Id" = ?"#)
        .bind(subtitle_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Path")
}

async fn generate_embedded_subtitle_fixture(dir: &Path) -> Option<PathBuf> {
    let srt_path = dir.join("smoke.srt");
    let mkv_path = dir.join("subtitle-smoke.mkv");
    std::fs::write(
        &srt_path,
        "1\n00:00:00,000 --> 00:00:00,800\nReal subtitle smoke\n",
    )
    .unwrap();

    let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path_from_env();
    let output = Command::new(&ffmpeg).arg("-version").output().await.ok()?;
    if !output.status.success() {
        eprintln!(
            "skipping real-media subtitle extraction smoke: ffmpeg is unavailable at {ffmpeg}"
        );
        return None;
    }

    let output = Command::new(&ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=size=160x90:duration=1:rate=25",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=channel_layout=stereo:sample_rate=48000",
            "-i",
        ])
        .arg(&srt_path)
        .args([
            "-map", "0:v", "-map", "1:a", "-map", "2:s", "-t", "1", "-c:v", "libx264", "-pix_fmt",
            "yuv420p", "-c:a", "aac", "-c:s", "srt",
        ])
        .arg(&mkv_path)
        .output()
        .await
        .ok()?;

    if output.status.success() {
        Some(mkv_path)
    } else {
        eprintln!(
            "skipping real-media subtitle extraction smoke: ffmpeg fixture generation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        None
    }
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
    enable_embedded_subtitle_extraction(&pool).await;

    extract_embedded_show_subtitles(&pool, 999).await.unwrap();
}

#[tokio::test]
async fn extraction_smoke_extracts_real_embedded_text_subtitle() {
    let media_dir = tempdir().unwrap();
    let Some(mkv_path) = generate_embedded_subtitle_fixture(media_dir.path()).await else {
        return;
    };

    let pool = common::test_db().await;
    let subtitle_id = seed_extractable_episode_with_path(&pool, &mkv_path.to_string_lossy()).await;
    enable_embedded_subtitle_extraction(&pool).await;

    extract_embedded_show_subtitles(&pool, 10).await.unwrap();

    assert_eq!(is_extracted(&pool, subtitle_id).await, 1);
    let relative_path = subtitle_path(&pool, subtitle_id)
        .await
        .expect("extracted subtitle path should be recorded");
    let output_path = subtitle_cache_folder().join(&relative_path);
    let extracted = std::fs::read_to_string(&output_path).unwrap_or_else(|error| {
        panic!(
            "expected extracted subtitle at {}: {error}",
            output_path.display()
        )
    });

    assert!(extracted.contains("Real subtitle smoke"));

    let _ = std::fs::remove_file(output_path);
}

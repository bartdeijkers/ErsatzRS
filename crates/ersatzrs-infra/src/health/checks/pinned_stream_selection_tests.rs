use super::*;
use sqlx::Executor;

async fn song_fixture(cover: bool) -> Pool<Db> {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    for statement in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#,
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (1, 'Synthetic songs', 4, 1)"#,
        r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, 'media')"#,
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (10, 1, 0), (11, 1, 0)"#,
        r#"INSERT INTO "Song" ("Id") VALUES (10), (11)"#,
        r#"INSERT INTO "SongMetadata" ("Id", "SongId", "Title", "MetadataKind", "DateAdded", "DateUpdated")
           VALUES (20, 10, 'Synthetic song', 0, '2026-10-07', '2026-10-07'),
                  (21, 11, 'Different song', 0, '2026-10-07', '2026-10-07')"#,
        r#"INSERT INTO "MediaVersion" ("Id", "DateAdded", "DateUpdated", "Duration", "SongId", "Height", "Width", "VideoScanKind")
           VALUES (100, '2026-10-07', '2026-10-07', '00:01:00', 10, 0, 0, 0)"#,
        r#"INSERT INTO "MediaStream" ("Id", "Index", "Codec", "MediaStreamKind", "Language", "Channels", "Default", "Forced", "MediaVersionId")
           VALUES (1, 23, 'flac', 2, 'nld', 2, 1, 0, 100)"#,
        r#"INSERT INTO "Subtitle" ("Id", "SongMetadataId", "SubtitleKind", "Title", "StreamIndex", "Codec", "Language", "Default", "Forced", "SDH", "IsExtracted", "Path", "DateAdded", "DateUpdated")
           VALUES (1, 20, 1, 'Japanese lyrics', 41, 'subrip', 'jpn', 0, 0, 0, 0, 'media/lyrics.ja.srt', '2026-10-07', '2026-10-07'),
                  (2, 21, 1, 'Other lyrics', 42, 'subrip', 'nld', 0, 0, 0, 0, 'media/other.nl.srt', '2026-10-07', '2026-10-07')"#,
        r#"INSERT INTO "PlaylistGroup" ("Id", "Name") VALUES (1, 'Synthetic')"#,
        r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId") VALUES (1, 0, 'Synthetic songs', 1)"#,
        r#"INSERT INTO "PlaylistItem" ("Id", "Index", "PlaylistId", "CollectionType", "MediaItemId", "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex", "SubtitleStreamIndex")
           VALUES (1, 0, 1, 10, 10, 100, 0, 23, 41)"#,
    ] {
        pool.execute(statement).await.unwrap();
    }
    if cover {
        pool.execute(
            r#"INSERT INTO "MediaStream" ("Id", "Index", "Codec", "MediaStreamKind", "Channels", "Default", "Forced", "MediaVersionId", "AttachedPic")
               VALUES (2, 7, 'mjpeg', 1, 0, 0, 0, 100, 1)"#,
        )
        .await
        .unwrap();
    }
    pool
}

async fn assert_status(pool: &Pool<Db>, expected: HealthCheckStatus) {
    let result = PinnedStreamSelectionHealthCheck::new(pool.clone())
        .check()
        .await
        .unwrap();
    assert_eq!(result.status, expected);
    if expected == HealthCheckStatus::Fail {
        assert_eq!(result.brief_message, "1 stale pinned stream selection(s)");
    }
}

#[tokio::test]
async fn pinned_song_generated_video_and_international_sidecar_pass_with_or_without_cover() {
    for cover in [false, true] {
        let pool = song_fixture(cover).await;
        let subtitle_streams: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaStream" WHERE "MediaStreamKind" = 3"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            subtitle_streams, 0,
            "sidecar admission must use SongMetadata"
        );
        assert_status(&pool, HealthCheckStatus::Pass).await;
        pool.execute(
            r#"UPDATE "PlaylistItem" SET "AudioStreamIndex" = NULL, "SubtitleStreamIndex" = NULL"#,
        )
        .await
        .unwrap();
        assert_status(&pool, HealthCheckStatus::Pass).await;
    }
}

#[tokio::test]
async fn pinned_song_generated_video_keeps_version_and_all_track_pins_strict() {
    let pool = song_fixture(true).await;
    for mutation in [
        r#"UPDATE "PlaylistItem" SET "VideoStreamIndex" = 7"#,
        r#"UPDATE "PlaylistItem" SET "VideoStreamIndex" = 9"#,
        r#"UPDATE "PlaylistItem" SET "VideoStreamIndex" = NULL"#,
        r#"UPDATE "PlaylistItem" SET "AudioStreamIndex" = 99"#,
        r#"UPDATE "PlaylistItem" SET "SubtitleStreamIndex" = 99"#,
        r#"UPDATE "PlaylistItem" SET "SubtitleStreamIndex" = 42"#,
        r#"UPDATE "PlaylistItem" SET "MediaItemId" = 11"#,
        r#"UPDATE "PlaylistItem" SET "MediaItemId" = NULL"#,
        r#"UPDATE "PlaylistItem" SET "MediaVersionId" = 999"#,
    ] {
        pool.execute(
            r#"UPDATE "PlaylistItem" SET "MediaItemId" = 10, "MediaVersionId" = 100,
               "VideoStreamIndex" = 0, "AudioStreamIndex" = 23, "SubtitleStreamIndex" = 41"#,
        )
        .await
        .unwrap();
        pool.execute(mutation).await.unwrap();
        assert_status(&pool, HealthCheckStatus::Fail).await;
    }
    pool.execute(r#"UPDATE "PlaylistItem" SET "MediaVersionId" = NULL"#)
        .await
        .unwrap();
    assert_status(&pool, HealthCheckStatus::Pass).await;
}

#[tokio::test]
async fn pinned_generated_video_requires_raw_audio_and_never_replaces_real_video() {
    let pool = song_fixture(true).await;
    pool.execute(
        r#"INSERT INTO "MediaStream" ("Id", "Index", "Codec", "MediaStreamKind", "Channels", "Default", "Forced", "MediaVersionId")
           VALUES (3, 6, 'h264', 1, 0, 1, 0, 100)"#,
    )
    .await
    .unwrap();
    assert_status(&pool, HealthCheckStatus::Fail).await;
    pool.execute(r#"UPDATE "PlaylistItem" SET "VideoStreamIndex" = 6"#)
        .await
        .unwrap();
    assert_status(&pool, HealthCheckStatus::Pass).await;
    pool.execute(r#"UPDATE "PlaylistItem" SET "VideoStreamIndex" = 7"#)
        .await
        .unwrap();
    assert_status(&pool, HealthCheckStatus::Fail).await;
    pool.execute(r#"DELETE FROM "MediaStream" WHERE "MediaStreamKind" != 1 OR "AttachedPic" = 0"#)
        .await
        .unwrap();
    pool.execute(r#"UPDATE "PlaylistItem" SET "VideoStreamIndex" = 0, "AudioStreamIndex" = NULL"#)
        .await
        .unwrap();
    assert_status(&pool, HealthCheckStatus::Fail).await;
    pool.execute(r#"DELETE FROM "MediaStream""#).await.unwrap();
    assert_status(&pool, HealthCheckStatus::Fail).await;
}

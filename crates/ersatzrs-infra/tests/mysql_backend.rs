#![cfg(all(feature = "mysql", feature = "mysql-integration"))]

use ersatzrs_core::repositories::{ChannelRepository, ConfigRepository, FfmpegProfileRepository};
use ersatzrs_infra::db;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Row;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn mysql_backend_runs_migrations_and_representative_queries() {
    #[allow(clippy::disallowed_methods)]
    let Ok(database_url) = std::env::var("ERSATZRS_MYSQL_TEST_URL") else {
        eprintln!("skipping MySQL integration test: ERSATZRS_MYSQL_TEST_URL is not set");
        return;
    };

    let pool = db::init(&database_url)
        .await
        .expect("initialize MySQL database");
    let repos = make_repositories(pool.clone());

    let key = format!("mysql.integration.{}", uuid::Uuid::new_v4());
    repos
        .config
        .set(&key, "ok")
        .await
        .expect("write config value");
    assert_eq!(
        repos.config.get(&key).await.expect("read config value"),
        Some("ok".to_string())
    );

    sqlx::query(
        r#"REPLACE INTO "Resolution" ("Id", "Name", "Width", "Height")
           VALUES (7001, 'mysql-test', 1920, 1080)"#,
    )
    .execute(&pool)
    .await
    .expect("seed resolution");

    sqlx::query(
        r#"REPLACE INTO "FFmpegProfile" (
               "Id", "Name", "ThreadCount", "TranscodeVideo", "TranscodeAudio",
               "ResolutionId", "ScalingBehavior", "VideoFormat", "AudioFormat",
               "VideoBitrate", "VideoBufferSize", "AudioBitrate", "AudioBufferSize",
               "NormalizeLoudnessMode", "AudioChannels", "AudioSampleRate",
               "HardwareAcceleration", "QsvExtraHardwareFrames", "VaapiDriver",
               "TonemapAlgorithm", "AllowBFrames", "BitDepth", "NormalizeFramerate",
               "NormalizeColors", "DeinterlaceVideo", "PadMode"
           ) VALUES (
               7001, 'mysql-test', 0, 1, 1,
               7001, 0, 1, 1,
               4000, 8000, 192, 384,
               0, 2, 48000,
               0, 0, 0,
               0, 0, 0, 0,
               1, 0, 0
           )"#,
    )
    .execute(&pool)
    .await
    .expect("seed ffmpeg profile");

    sqlx::query(
        r#"REPLACE INTO "Channel" (
               "Id", "UniqueId", "Number", "SortNumber", "Name", "Group",
               "FFmpegProfileId", "StreamingMode", "StreamSelectorMode",
               "SubtitleMode", "MusicVideoCreditsMode", "SongVideoMode",
               "PlayoutSource", "PlayoutMode", "TranscodeMode", "IdleBehavior",
               "IsEnabled", "ShowInEpg"
           ) VALUES (
               7001, '00000000-0000-0000-0000-000000007001', '7001', 7001.0,
               'mysql-test', 'ErsatzTV', 7001, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1
           )"#,
    )
    .execute(&pool)
    .await
    .expect("seed channel");

    let channel = repos
        .channel
        .get_by_number("7001")
        .await
        .expect("read channel by number");
    assert_eq!(channel.name, "mysql-test");

    let profile = repos
        .ffmpeg_profile
        .get(7001)
        .await
        .expect("read ffmpeg profile");
    assert_eq!(profile.name, "mysql-test");

    sqlx::query(r#"REPLACE INTO "MediaSource" ("Id") VALUES (7001)"#)
        .execute(&pool)
        .await
        .expect("seed media source");
    sqlx::query(r#"REPLACE INTO "LocalMediaSource" ("Id") VALUES (7001)"#)
        .execute(&pool)
        .await
        .expect("seed local media source");
    sqlx::query(
        r#"REPLACE INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (7001, 'mysql-test', 0, 7001)"#,
    )
    .execute(&pool)
    .await
    .expect("seed library");
    sqlx::query(r#"REPLACE INTO "LocalLibrary" ("Id") VALUES (7001)"#)
        .execute(&pool)
        .await
        .expect("seed local library");
    sqlx::query(
        r#"REPLACE INTO "LibraryPath" ("Id", "LibraryId", "Path")
           VALUES (7001, 7001, '/tmp')"#,
    )
    .execute(&pool)
    .await
    .expect("seed library path");

    let inserted =
        sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (7001, 0)"#)
            .execute(&pool)
            .await
            .expect("insert media item");
    assert!(db::last_insert_id(inserted) > 0);

    let playout_count: i64 =
        sqlx::query(r#"SELECT COUNT(*) AS "Count" FROM "Channel" WHERE "Number" = ?"#)
            .bind("7001")
            .fetch_one(&pool)
            .await
            .expect("run bound MySQL query")
            .try_get("Count")
            .expect("decode count");
    assert_eq!(playout_count, 1);
}

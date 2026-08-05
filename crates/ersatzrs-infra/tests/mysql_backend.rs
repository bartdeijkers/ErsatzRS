#![cfg(all(feature = "mysql", feature = "mysql-integration"))]

use ersatzrs_core::repositories::{
    ChannelRepository, ConfigRepository, FfmpegProfileRepository, PlayoutRepository,
    ProgramScheduleRepository,
};
use ersatzrs_domain::{
    CollectionEnumeratorState, CollectionType, Playout, PlayoutProgramScheduleAnchor,
    PlayoutScheduleItemFillGroupIndex, ProgramSchedule,
};
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

    let video_tune_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
           WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'FFmpegProfile'
             AND COLUMN_NAME = 'VideoTune'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("query VideoTune column");
    assert_eq!(video_tune_count, 1, "missing FFmpegProfile.VideoTune");

    for table in [
        "EpisodeMetadata",
        "MusicVideoMetadata",
        "SongMetadata",
        "ImageMetadata",
    ] {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM INFORMATION_SCHEMA.COLUMNS
               WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
                 AND COLUMN_NAME = 'ContentRating'"#,
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("query uniform content-rating column");
        assert_eq!(count, 1, "missing ContentRating on {table}");
    }
    for table in ["MusicLabel", "MetadataLanguage"] {
        let table_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM INFORMATION_SCHEMA.TABLES
               WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?"#,
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("query uniform facet table");
        assert_eq!(table_count, 1, "missing {table}");

        let index_count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(DISTINCT INDEX_NAME) FROM INFORMATION_SCHEMA.STATISTICS
               WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
                 AND INDEX_NAME LIKE 'IX_%'"#,
        )
        .bind(table)
        .fetch_one(&pool)
        .await
        .expect("query uniform facet indexes");
        assert_eq!(index_count, 10, "unexpected owner-index count for {table}");
    }

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
               "NormalizeColors", "DeinterlaceVideo", "PadMode", "VideoPreset",
               "VideoTune"
           ) VALUES (
               7001, 'mysql-test', 0, 1, 1,
               7001, 0, 1, 1,
               4000, 8000, 192, 384,
               0, 2, 48000,
               0, 0, 0,
               0, 0, 0, 0,
               1, 0, 0, 'p7', 'll'
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
    assert_eq!(profile.video_preset.as_deref(), Some("p7"));
    assert_eq!(profile.video_tune.as_deref(), Some("ll"));

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

    sqlx::query(
        r#"REPLACE INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder")
           VALUES (7101, 'mysql-runtime-state', 0)"#,
    )
    .execute(&pool)
    .await
    .expect("seed runtime-state collection");
    for (schedule_id, item_id) in [(7101, 7101), (7102, 7102)] {
        repos
            .program_schedule
            .upsert(&ProgramSchedule {
                id: schedule_id,
                name: format!("mysql-runtime-state-{schedule_id}"),
                item_ids: vec![item_id],
                ..ProgramSchedule::default()
            })
            .await
            .expect("seed runtime-state schedule");
    }
    for (playout_id, schedule_id) in [(7101, 7101), (7102, 7102)] {
        sqlx::query(
            r#"REPLACE INTO "Playout" (
                   "Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed"
               ) VALUES (?, 7001, ?, 1, 0)"#,
        )
        .bind(playout_id)
        .bind(schedule_id)
        .execute(&pool)
        .await
        .expect("seed runtime-state playout");
        repos
            .playout
            .replace_runtime_state(
                &Playout {
                    id: playout_id,
                    channel_id: 7001,
                    program_schedule_id: Some(schedule_id),
                    ..Playout::default()
                },
                &[],
                &[],
                &[PlayoutProgramScheduleAnchor {
                    id: 0,
                    playout_id,
                    collection_type: CollectionType::Collection,
                    collection_id: Some(7101),
                    enumerator_state: CollectionEnumeratorState { seed: 1, index: 2 },
                    ..PlayoutProgramScheduleAnchor::default()
                }],
                &[PlayoutScheduleItemFillGroupIndex {
                    id: 0,
                    playout_id,
                    program_schedule_item_id: schedule_id,
                    enumerator_state: CollectionEnumeratorState { seed: 3, index: 4 },
                }],
            )
            .await
            .expect("persist database-assigned runtime-state identities");
    }

    let anchor_ids = sqlx::query(
        r#"SELECT "Id" FROM "PlayoutProgramScheduleAnchor"
           WHERE "PlayoutId" IN (7101, 7102) ORDER BY "PlayoutId""#,
    )
    .fetch_all(&pool)
    .await
    .expect("load generated anchor identities")
    .into_iter()
    .map(|row| row.try_get::<i64, _>("Id").expect("decode anchor identity"))
    .collect::<Vec<_>>();
    let fill_group_ids = sqlx::query(
        r#"SELECT "Id" FROM "PlayoutScheduleItemFillGroupIndex"
           WHERE "PlayoutId" IN (7101, 7102) ORDER BY "PlayoutId""#,
    )
    .fetch_all(&pool)
    .await
    .expect("load generated fill-group identities")
    .into_iter()
    .map(|row| {
        row.try_get::<i64, _>("Id")
            .expect("decode fill-group identity")
    })
    .collect::<Vec<_>>();
    assert_eq!(anchor_ids.len(), 2);
    assert_ne!(anchor_ids[0], anchor_ids[1]);
    assert_eq!(fill_group_ids.len(), 2);
    assert_ne!(fill_group_ids[0], fill_group_ids[1]);
}

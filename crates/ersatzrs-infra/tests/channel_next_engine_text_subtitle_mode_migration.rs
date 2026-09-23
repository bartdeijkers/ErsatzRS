mod common;

use ersatzrs_core::repositories::FfmpegProfileRepository;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{sqlite::SqlitePoolOptions, Executor};

#[tokio::test]
async fn channel_next_engine_text_subtitle_mode_migration_defaults_burn_and_preserves_choice() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_609_230_847)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height")
           VALUES (1, 'test', 1920, 1080)"#,
    )
    .await
    .unwrap();
    make_repositories(pool.clone())
        .ffmpeg_profile
        .upsert(&common::profile(1, "test"))
        .await
        .unwrap();
    // Seed through the old schema, independently of the current Channel model.
    pool.execute(
        r#"INSERT INTO "Channel" (
            "Id", "UniqueId", "Number", "Name", "SortNumber", "FFmpegProfileId",
            "IdleBehavior", "MusicVideoCreditsMode", "PlayoutMode", "PlayoutSource",
            "SongVideoMode", "StreamSelectorMode", "StreamingMode", "SubtitleMode",
            "TranscodeMode", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode")
           VALUES (1, 'engine-migration-fixture', '1', 'Existing', 1, 1,
                   0, 0, 0, 0, 0, 0, 5, 1, 0, 'nl', 'en')"#,
    )
    .await
    .unwrap();
    migrations.run(&pool).await.unwrap();
    let row: (i32, i32, String, String) = sqlx::query_as(
        r#"SELECT "NextEngineTextSubtitleMode", "StreamingMode", "PreferredAudioLanguageCode",
                  "PreferredSubtitleLanguageCode" FROM "Channel" WHERE "Id" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, (0, 5, "nl".to_owned(), "en".to_owned()));
    pool.execute(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode" = 1 WHERE "Id" = 1"#)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    let engine: i32 =
        sqlx::query_scalar(r#"SELECT "NextEngineTextSubtitleMode" FROM "Channel" WHERE "Id" = 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        engine, 1,
        "rerunning migrations must preserve explicit choice"
    );
    assert!(pool
        .execute(r#"UPDATE "Channel" SET "NextEngineTextSubtitleMode" = NULL WHERE "Id" = 1"#)
        .await
        .is_err());
}

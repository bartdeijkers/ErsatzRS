mod common;

use ersatzrs_core::repositories::FfmpegProfileRepository;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{sqlite::SqlitePoolOptions, Executor};

#[tokio::test]
async fn channel_teletext_migration_preserves_existing_editions_and_leaves_other_channels_off() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_611_251_520)
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
           VALUES (1, 'atlas-migration-fixture', '1', 'Existing', 1, 1,
                   0, 0, 0, 0, 0, 0, 5, 1, 0, 'nl', 'en'),
                  (2, 'no-atlas', '2', 'Other', 2, 1,
                   0, 0, 0, 0, 0, 0, 5, 1, 0, 'nl', 'en')"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "ChannelTeletextAtlas" ("ChannelId","Revision","GuidePages") VALUES (1,1,'[]')"#)
        .await.unwrap();
    migrations.run(&pool).await.unwrap();
    let choices: Vec<bool> =
        sqlx::query_scalar(r#"SELECT "TeletextEnabled" FROM "Channel" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(choices, [true, false]);
    pool.execute(r#"UPDATE "Channel" SET "TeletextEnabled"=0 WHERE "Id"=1"#)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    let preserved: (bool, String) = sqlx::query_as(
        r#"SELECT c."TeletextEnabled", a."GuidePages"
        FROM "Channel" c JOIN "ChannelTeletextAtlas" a ON a."ChannelId"=c."Id" WHERE c."Id"=1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(preserved, (false, "[]".to_owned()));
}

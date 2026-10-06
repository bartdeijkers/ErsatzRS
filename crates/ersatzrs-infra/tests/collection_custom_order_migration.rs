use sqlx::{sqlite::SqlitePoolOptions, Executor};

#[tokio::test]
async fn collection_custom_order_migration_repairs_only_non_movie_settings_once() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|m| m.version < 202_609_301_355)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId") VALUES (1, 1, 1);
        INSERT INTO "LibraryPath" ("Id", "LibraryId") VALUES (1, 1);
        INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (1,1,0), (2,1,0), (3,1,0);
        INSERT INTO "Movie" ("Id") VALUES (1);
        INSERT INTO "Song" ("Id") VALUES (2);
        INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES
        (1, 'Synthetic movies', 1), (2, 'Synthetic mixed', 1), (3, 'Synthetic song', 1),
        (4, 'Synthetic empty', 1), (5, 'Synthetic disabled mixed', 0), (6, 'Synthetic unknown kind', 1);
        INSERT INTO "CollectionItem" ("Id", "CollectionId", "MediaItemId", "CustomIndex", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode", "SubtitleMode") VALUES
        (1,1,1,7,'eng','nld',2), (2,1,1,8,'nld','eng',3), (3,2,1,4,'eng','nld',2),
        (4,2,2,5,'nld','eng',3), (5,3,2,9,'eng','nld',2), (6,5,2,10,'eng','nld',2), (7,6,3,11,'eng','nld',2);"#,
    ).await.unwrap();
    let before: Vec<(i64,i64,i64,i64,String,String,i64)> = sqlx::query_as(
        r#"SELECT "Id", "CollectionId", "MediaItemId", "CustomIndex", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode", "SubtitleMode" FROM "CollectionItem" ORDER BY "Id""#,
    ).fetch_all(&pool).await.unwrap();
    migrations.run(&pool).await.unwrap();
    let settings: Vec<(i64, bool)> =
        sqlx::query_as(r#"SELECT "Id", "UseCustomPlaybackOrder" FROM "Collection" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        settings,
        [
            (1, true),
            (2, false),
            (3, false),
            (4, true),
            (5, false),
            (6, false)
        ]
    );
    let after: Vec<(i64,i64,i64,i64,String,String,i64)> = sqlx::query_as(
        r#"SELECT "Id", "CollectionId", "MediaItemId", "CustomIndex", "PreferredAudioLanguageCode", "PreferredSubtitleLanguageCode", "SubtitleMode" FROM "CollectionItem" ORDER BY "Id""#,
    ).fetch_all(&pool).await.unwrap();
    assert_eq!(
        after, before,
        "membership, duplicate identities and playback overrides are retained"
    );
    pool.execute(r#"UPDATE "Collection" SET "UseCustomPlaybackOrder" = 1 WHERE "Id" = 2"#)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    let retained: bool =
        sqlx::query_scalar(r#"SELECT "UseCustomPlaybackOrder" FROM "Collection" WHERE "Id" = 2"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        retained,
        "compatibility repair runs once, not on every startup"
    );
}

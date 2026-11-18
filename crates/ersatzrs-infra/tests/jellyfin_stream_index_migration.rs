//! Add mapping storage without rewriting persisted track choices or cached data.
use sqlx::{sqlite::SqlitePoolOptions, Executor};

#[tokio::test]
async fn row295_migration_preserves_legacy_pins_and_disabled_library_choices() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys=ON").await.unwrap();
    let all = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version < 202_611_162_205)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1),(2);
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1);
        INSERT INTO "EmbyMediaSource" ("Id") VALUES (2);
        INSERT INTO "Library" ("Id","MediaSourceId","Name","MediaKind","LastScan") VALUES (1,1,'Jellyfin',1,'2026-11-15'),(2,2,'Emby',1,'2026-11-15');
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,0);
        INSERT INTO "EmbyLibrary" ("Id","ShouldSyncItems") VALUES (2,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'jf');
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (10,1);
        INSERT INTO "Movie" ("Id") VALUES (10);
        INSERT INTO "JellyfinMovie" ("Id","Etag") VALUES (10,'same');
        INSERT INTO "MovieMetadata" ("Id","MovieId","MetadataKind","DateAdded","DateUpdated") VALUES (20,10,1,'2026-11-15','2026-11-15');
        INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind") VALUES (100,10,'2026-11-15','2026-11-15','00:10:00',180,320,0);
        INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","StreamIndex","Path","Default","Forced","IsExtracted","DateAdded","DateUpdated") VALUES (77,20,1,4,'cached.ass',0,0,1,'2026-11-15','2026-11-15');
        INSERT INTO "PlaylistGroup" ("Id","Name") VALUES (50,'pins');
        INSERT INTO "Playlist" ("Id","PlaylistGroupId","Name","IsSystem") VALUES (51,50,'pins',0);
        INSERT INTO "PlaylistItem" ("PlaylistId","Index","CollectionType","MediaItemId","MediaVersionId","VideoStreamIndex","AudioStreamIndex","SubtitleStreamIndex") VALUES (51,0,'Movie',10,100,2,3,4);
    "#).execute(&pool).await.unwrap();
    all.run(&pool).await.unwrap();
    let pin:(i64,i64,i64,i64)=sqlx::query_as(r#"SELECT "MediaVersionId","VideoStreamIndex","AudioStreamIndex","SubtitleStreamIndex" FROM "PlaylistItem""#).fetch_one(&pool).await.unwrap();
    assert_eq!(pin, (100, 2, 3, 4));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "ProviderStreamIndexVersion" FROM "MediaVersion" WHERE "Id"=100"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "ShouldSyncItems" FROM "JellyfinLibrary" WHERE "Id"=1"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(r#"SELECT "LastScan" FROM "Library" WHERE "Id"=1"#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "0001-01-01 00:00:00"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(r#"SELECT "LastScan" FROM "Library" WHERE "Id"=2"#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "2026-11-15"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(r#"SELECT "Etag" FROM "JellyfinMovie" WHERE "Id"=10"#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "same"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(r#"SELECT "Path" FROM "Subtitle" WHERE "Id"=77"#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "cached.ass"
    );
    all.run(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "ProviderStreamIndex""#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

use sqlx::{sqlite::SqlitePoolOptions, Executor, Row};

#[tokio::test]
async fn durable_publication_migration_adopts_a_stranded_legacy_journal() {
    const DURABLE_PUBLICATION_VERSION: i64 = 202_609_021_200;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open publication migration database");
    pool.execute("PRAGMA foreign_keys = ON")
        .await
        .expect("enable foreign keys");
    let migrations = sqlx::migrate!("../../migrations");
    let previous = sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < DURABLE_PUBLICATION_VERSION)
            .cloned()
            .collect(),
    );
    previous
        .run(&pool)
        .await
        .expect("migrate the legacy publication schema");
    sqlx::raw_sql(
        r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
            VALUES (1, 2, 1, 'Managed add-on list');
        INSERT INTO "AddonInstallation" (
            "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
            "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
        ) VALUES (
            'org.example.legacy', '1.0.0', 1, 'package', 'content', '{}',
            'addons/legacy', '2026-09-02T00:00:00Z', '2026-09-02T00:00:00Z'
        );
        INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ReviewState",
            "PublishedRevision", "PublicationJournalJson", "PublicationStartedAt"
        ) VALUES (
            1, 'org.example.legacy', 'legacy-list', 'https://example.invalid/list',
            'Legacy list', 3, 7,
            '{"previous_review_state":"published","previous_presentation_kind":"remote_streams","previous_items":[],"previous_playlist":null,"managed_folder":"managed/legacy-list","backup_folder":"managed/.ersatzrs-publication-1-legacy","previous_materializations":[]}',
            '2026-09-01T00:00:00Z'
        );
        INSERT INTO "AddonMediaListLibrary" (
            "AddonMediaListId", "LibraryId", "FolderName", "ManifestPath"
        ) VALUES (1, 1, 'Legacy list', 'managed/legacy-list');
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed stranded legacy publication");

    migrations
        .run(&pool)
        .await
        .expect("apply durable publication migration");

    let row = sqlx::query(
        r#"SELECT "operation"."Kind", "operation"."State", "job"."Phase",
                  "job"."SavedRevision", "job"."TargetPublishedRevision",
                  "job"."OriginalFailureCode"
           FROM "ManagedLibraryOperation" AS "operation"
           JOIN "AddonMediaListPublicationJob" AS "job"
             ON "job"."OperationId" = "operation"."Id"
           WHERE "job"."AddonMediaListId" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .expect("load adopted recovery job");
    assert_eq!(row.get::<i32, _>("Kind"), 6);
    assert_eq!(row.get::<i32, _>("State"), 0);
    assert_eq!(row.get::<i32, _>("Phase"), 4);
    assert_eq!(row.get::<i32, _>("SavedRevision"), 7);
    assert_eq!(row.get::<i32, _>("TargetPublishedRevision"), 8);
    assert_eq!(
        row.get::<String, _>("OriginalFailureCode"),
        "legacy_interrupted_publication"
    );
}

#[tokio::test]
async fn destructive_migration_removes_legacy_trakt_schema_and_data() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    sqlx::raw_sql(
        r#"
        CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY, "Value" TEXT);
        CREATE TABLE "AddonInstallation" ("AddonId" TEXT PRIMARY KEY);
        CREATE TABLE "MediaItem" ("Id" INTEGER PRIMARY KEY);
        CREATE TABLE "Playlist" ("Id" INTEGER PRIMARY KEY, "IsSystem" INTEGER NOT NULL);
        CREATE TABLE "TraktList" (
            "Id" INTEGER PRIMARY KEY,
            "PlaylistId" INTEGER NULL,
            FOREIGN KEY ("PlaylistId") REFERENCES "Playlist" ("Id") ON DELETE SET NULL
        );
        CREATE TABLE "TraktListItem" (
            "Id" INTEGER PRIMARY KEY,
            "TraktListId" INTEGER NOT NULL,
            FOREIGN KEY ("TraktListId") REFERENCES "TraktList" ("Id") ON DELETE CASCADE
        );
        CREATE TABLE "TraktListItemGuid" (
            "Id" INTEGER PRIMARY KEY,
            "TraktListItemId" INTEGER NOT NULL,
            FOREIGN KEY ("TraktListItemId") REFERENCES "TraktListItem" ("Id") ON DELETE CASCADE
        );
        INSERT INTO "Playlist" ("Id", "IsSystem") VALUES (9, 1);
        INSERT INTO "TraktList" ("Id", "PlaylistId") VALUES (1, 9);
        INSERT INTO "TraktListItem" ("Id", "TraktListId") VALUES (2, 1);
        INSERT INTO "TraktListItemGuid" ("Id", "TraktListItemId") VALUES (3, 2);
        INSERT INTO "ConfigElement" ("Key", "Value")
            VALUES ('pages.trakt.lists_page_size', '25');
        INSERT INTO "ConfigElement" ("Key", "Value")
            VALUES ('pages.addon_media_lists.page_size', '50');
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::raw_sql(include_str!(
        "../../../migrations/202608131200_addon_media_lists.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let table_names =
        sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect::<Vec<_>>();
    assert!(!table_names.iter().any(|name| name.starts_with("Trakt")));
    assert!(table_names.iter().any(|name| name == "AddonMediaList"));
    assert!(table_names.iter().any(|name| name == "AddonMediaListItem"));
    assert!(table_names
        .iter()
        .any(|name| name == "AddonMediaListItemGuid"));

    let playlist_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"Playlist\"")
        .fetch_one(&pool)
        .await
        .unwrap();
    let legacy_config_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM \"ConfigElement\" WHERE \"Key\" LIKE '%trakt%' OR \"Key\" = 'pages.addon_media_lists.page_size'",
    )
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((playlist_count, legacy_config_count), (0, 0));
}

/// AMM-H-01 repairs only byte-exact duplicates in add-on-projected television.
/// Variants, unrelated artwork, malformed multi-owner rows, and any duplicate
/// set referenced by an actor must survive both the migration and a restart.
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn artwork_cardinality_repair_is_narrow_restart_safe_and_reports_counts() {
    let directory = tempfile::tempdir().expect("create migration test directory");
    let database_path = directory.path().join("artwork-repair.db");
    let database_url = format!("sqlite://{}?mode=rwc", database_path.display());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("open migration test database");
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    sqlx::raw_sql(
        r#"
        CREATE TABLE "ConfigElement" (
            "Id" INTEGER PRIMARY KEY AUTOINCREMENT,
            "Key" TEXT NULL,
            "Value" TEXT NULL
        );
        CREATE TABLE "ShowMetadata" ("Id" INTEGER PRIMARY KEY, "ShowId" INTEGER NOT NULL);
        CREATE TABLE "Season" ("Id" INTEGER PRIMARY KEY, "ShowId" INTEGER NOT NULL);
        CREATE TABLE "SeasonMetadata" ("Id" INTEGER PRIMARY KEY, "SeasonId" INTEGER NOT NULL);
        CREATE TABLE "EpisodeMetadata" ("Id" INTEGER PRIMARY KEY, "EpisodeId" INTEGER NOT NULL);
        CREATE TABLE "AddonMediaListTelevisionShow" ("ShowId" INTEGER NOT NULL);
        CREATE TABLE "AddonMediaListItemEpisode" ("EpisodeId" INTEGER NOT NULL);
        CREATE TABLE "Artwork" (
            "Id" INTEGER PRIMARY KEY,
            "ArtistMetadataId" INTEGER NULL,
            "ArtworkKind" INTEGER NOT NULL,
            "BlurHash43" TEXT NULL,
            "BlurHash54" TEXT NULL,
            "BlurHash64" TEXT NULL,
            "ChannelId" INTEGER NULL,
            "DateAdded" TEXT NOT NULL,
            "DateUpdated" TEXT NOT NULL,
            "EpisodeMetadataId" INTEGER NULL,
            "ImageMetadataId" INTEGER NULL,
            "MovieMetadataId" INTEGER NULL,
            "MusicVideoMetadataId" INTEGER NULL,
            "OriginalContentType" TEXT NULL,
            "OtherVideoMetadataId" INTEGER NULL,
            "Path" TEXT NULL,
            "RemoteStreamMetadataId" INTEGER NULL,
            "SeasonMetadataId" INTEGER NULL,
            "ShowMetadataId" INTEGER NULL,
            "SongMetadataId" INTEGER NULL,
            "SourcePath" TEXT NULL
        );
        CREATE TABLE "Actor" (
            "Id" INTEGER PRIMARY KEY,
            "ArtworkId" INTEGER NULL,
            FOREIGN KEY ("ArtworkId") REFERENCES "Artwork" ("Id") ON DELETE CASCADE
        );

        INSERT INTO "ShowMetadata" ("Id", "ShowId") VALUES (10, 1);
        INSERT INTO "Season" ("Id", "ShowId") VALUES (2, 1);
        INSERT INTO "SeasonMetadata" ("Id", "SeasonId") VALUES (20, 2);
        INSERT INTO "EpisodeMetadata" ("Id", "EpisodeId") VALUES (30, 3), (31, 4);
        INSERT INTO "AddonMediaListTelevisionShow" ("ShowId") VALUES (1);
        INSERT INTO "AddonMediaListItemEpisode" ("EpisodeId") VALUES (3);

        INSERT INTO "Artwork" (
            "Id", "ArtworkKind", "DateAdded", "DateUpdated",
            "EpisodeMetadataId", "Path", "SourcePath"
        ) VALUES
            (1, 1, 'old', 'old', 30, '/media/thumb.jpg', NULL),
            (2, 1, 'new', 'new', 30, '/media/thumb.jpg', NULL),
            (3, 1, 'new', 'new', 30, '/media/variant.jpg', NULL),
            (4, 1, 'new', 'new', 30, '/media/thumb.jpg', 'provider-a');

        INSERT INTO "Artwork" (
            "Id", "ArtworkKind", "DateAdded", "DateUpdated", "ShowMetadataId", "Path"
        ) VALUES
            (5, 0, 'old', 'old', 10, '/media/poster.jpg'),
            (6, 0, 'new', 'new', 10, '/media/poster.jpg');
        INSERT INTO "Actor" ("Id", "ArtworkId") VALUES (100, 6);

        INSERT INTO "Artwork" (
            "Id", "ArtworkKind", "DateAdded", "DateUpdated", "SeasonMetadataId", "Path"
        ) VALUES
            (7, 0, 'old', 'old', 20, '/media/Season.jpg'),
            (8, 0, 'new', 'new', 20, '/media/season.jpg');

        INSERT INTO "Artwork" (
            "Id", "ArtworkKind", "DateAdded", "DateUpdated", "EpisodeMetadataId", "Path"
        ) VALUES
            (9, 1, 'old', 'old', 31, '/other/thumb.jpg'),
            (10, 1, 'new', 'new', 31, '/other/thumb.jpg');

        INSERT INTO "Artwork" (
            "Id", "ArtworkKind", "DateAdded", "DateUpdated",
            "EpisodeMetadataId", "ShowMetadataId", "Path"
        ) VALUES
            (11, 1, 'old', 'old', 30, 10, '/malformed/thumb.jpg'),
            (12, 1, 'new', 'new', 30, 10, '/malformed/thumb.jpg');
        "#,
    )
    .execute(&pool)
    .await
    .expect("seed legacy artwork shapes");

    sqlx::raw_sql(include_str!(
        "../../../migrations/202608241400_artwork_cardinality_repair.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply artwork cardinality repair");

    pool.close().await;
    let reopened = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("reopen repaired database");
    reopened.execute("PRAGMA foreign_keys = ON").await.unwrap();

    let ids = sqlx::query_scalar::<_, i64>(r#"SELECT "Id" FROM "Artwork" ORDER BY "Id""#)
        .fetch_all(&reopened)
        .await
        .expect("load repaired artwork ids");
    assert_eq!(ids, [1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT "ArtworkId" FROM "Actor" WHERE "Id" = 100"#)
            .fetch_one(&reopened)
            .await
            .expect("load retained actor reference"),
        6
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            r#"SELECT "Value" FROM "ConfigElement"
               WHERE "Key" = 'migration.artwork_cardinality_repair'"#,
        )
        .fetch_one(&reopened)
        .await
        .expect("load aggregate repair report"),
        r#"{"removedRows":1,"skippedReferencedRows":1}"#
    );
    assert!(sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&reopened)
        .await
        .expect("check repaired foreign keys")
        .is_empty());
}

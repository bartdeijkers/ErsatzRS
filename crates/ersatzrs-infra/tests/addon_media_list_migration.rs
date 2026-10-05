use sqlx::{sqlite::SqlitePoolOptions, Executor, Row, SqlitePool};

#[tokio::test]
async fn selection_migration_preserves_legacy_settings_revisions_and_manual_inclusion() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|m| m.version < 202_610_051_730)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    seed_legacy_lifecycle_rows(&pool).await;
    sqlx::raw_sql(r#"UPDATE "AddonMediaList" SET "RefreshCadence"=5,"DeliveryMode"=1,"NextRefreshAt"='2026-10-06T12:00:00Z' WHERE "Id"=1;
        INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","Kind","Rank","DisplayTitle","Title","Included","IncludedByUser","Published") VALUES
        (10,1,'pin',4,0,'Pin','Pin',1,1,1),(11,1,'exclude',4,1,'Exclude','Exclude',0,1,0),(12,1,'auto',4,2,'Auto','Auto',1,0,1);"#).execute(&pool).await.unwrap();
    let before: (i32,i32,i32,i32,String)=sqlx::query_as(r#"SELECT "DraftRevision","PublishedRevision","RefreshCadence","DeliveryMode","NextRefreshAt" FROM "AddonMediaList" WHERE "Id"=1"#).fetch_one(&pool).await.unwrap();
    migrations.run(&pool).await.unwrap();
    assert_eq!(sqlx::query_as::<_,(i32,i32,i32,i32,String)>(r#"SELECT "DraftRevision","PublishedRevision","RefreshCadence","DeliveryMode","NextRefreshAt" FROM "AddonMediaList" WHERE "Id"=1"#).fetch_one(&pool).await.unwrap(),before);
    assert_eq!(sqlx::query_as::<_,(i64,Option<bool>,bool,bool)>(r#"SELECT "Id","SelectionOverride","Included","Published" FROM "AddonMediaListItem" ORDER BY "Id""#).fetch_all(&pool).await.unwrap(),[(10,Some(true),true,true),(11,Some(false),false,false),(12,None,true,true)]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "AddonMediaListSelection""#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn disk_warning_migration_defaults_existing_lists_without_revising_them() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    let previous = sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_609_080_900)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.unwrap();
    seed_legacy_lifecycle_rows(&pool).await;
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version <= 202_609_080_900)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    let row: (i64, i64, i64) = sqlx::query_as(
        r#"SELECT "DiskUsageWarningGb", "DraftRevision", "PublishedRevision"
           FROM "AddonMediaList" WHERE "Id" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, (25, 7, 5));
    pool.execute(r#"UPDATE "AddonMediaList" SET "DiskUsageWarningGb" = 0 WHERE "Id" = 1"#)
        .await
        .unwrap();
    for invalid in [-1_i64, i64::from(u32::MAX) + 1] {
        assert!(sqlx::query(
            r#"UPDATE "AddonMediaList" SET "DiskUsageWarningGb" = ? WHERE "Id" = 1"#
        )
        .bind(invalid)
        .execute(&pool)
        .await
        .is_err());
    }
}

#[tokio::test]
async fn lifecycle_foundation_backfills_only_truthful_legacy_state() {
    const LIFECYCLE_FOUNDATION_VERSION: i64 = 202_609_041_500;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open lifecycle migration database");
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    let previous = sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < LIFECYCLE_FOUNDATION_VERSION)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.expect("migrate legacy schema");
    seed_legacy_lifecycle_rows(&pool).await;

    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_609_161_200)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .expect("apply lifecycle foundation migration");
    assert_lifecycle_foundation_backfill(&pool).await;
}

async fn seed_legacy_lifecycle_rows(pool: &SqlitePool) {
    sqlx::raw_sql(
        r#"
        INSERT INTO "AddonInstallation" (
            "AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
            "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt"
        ) VALUES (
            'org.example.lifecycle', '1.0.0', 1, 'package', 'content', '{}',
            'addons/lifecycle', '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z'
        );
        INSERT INTO "AddonMediaList" (
            "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "AutoRefresh",
            "PublishedRevision", "DraftRevision", "LastUpdate"
        ) VALUES
            (1, 'org.example.lifecycle', 'daily', 'https://example.invalid/daily',
             'Daily', 1, 5, 7, '2026-09-03T12:00:00Z'),
            (2, 'org.example.lifecycle', 'manual', 'https://example.invalid/manual',
             'Manual', 0, 0, 0, NULL);
        INSERT INTO "AddonMediaListImportJob" (
            "AddonMediaListId", "Generation", "State", "RecordCapability",
            "CreatedAt", "UpdatedAt"
        ) VALUES (
            1, 1, 3, 'media-list.list.v1',
            '2026-09-03T12:00:00Z', '2026-09-03T12:00:00Z'
        );
        "#,
    )
    .execute(pool)
    .await
    .expect("seed legacy lifecycle rows");
}

async fn assert_lifecycle_foundation_backfill(pool: &SqlitePool) {
    let lists = sqlx::query(
        r#"SELECT "Id", "RefreshCadence", "RefreshPublicationPolicy",
                  "LastRefreshSucceededAt", "NextRefreshAt", "DraftOptionSchemaId",
                  "DraftOptionValuesJson", "ProcessedArtifactByteLimit"
           FROM "AddonMediaList" ORDER BY "Id""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(lists[0].get::<i32, _>("RefreshCadence"), 0);
    assert_eq!(lists[1].get::<i32, _>("RefreshCadence"), 4);
    assert_eq!(lists[0].get::<i32, _>("RefreshPublicationPolicy"), 0);
    assert_eq!(
        lists[0].get::<String, _>("LastRefreshSucceededAt"),
        "2026-09-03T12:00:00Z"
    );
    assert_eq!(
        lists[0].get::<String, _>("NextRefreshAt"),
        "2026-09-04 12:00:00"
    );
    assert!(lists[1].get::<Option<String>, _>("NextRefreshAt").is_none());
    assert!(lists[0]
        .get::<Option<i64>, _>("DraftOptionSchemaId")
        .is_none());
    assert_eq!(lists[0].get::<String, _>("DraftOptionValuesJson"), "{}");
    assert!(lists[0]
        .get::<Option<i64>, _>("ProcessedArtifactByteLimit")
        .is_none());

    let import = sqlx::query(
        r#"SELECT "Purpose", "Mode", "IncrementalStrategy", "ExaminedCount",
                  "ArchivedSkippedCount", "EmittedCount", "CheckpointId"
           FROM "AddonMediaListImportJob""#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(import.get::<i32, _>("Purpose"), 0);
    assert_eq!(import.get::<i32, _>("Mode"), 0);
    assert_eq!(import.get::<i32, _>("IncrementalStrategy"), 0);
    assert_eq!(import.get::<i64, _>("ExaminedCount"), 0);
    assert_eq!(import.get::<i64, _>("ArchivedSkippedCount"), 0);
    assert_eq!(import.get::<i64, _>("EmittedCount"), 0);
    assert!(import.get::<Option<i64>, _>("CheckpointId").is_none());

    let revision = sqlx::query(
        r#"SELECT "Revision", "SourceDraftRevision", "OptionSchemaId",
                  "OptionValuesJson", "CompletedAt"
           FROM "AddonMediaListRevision""#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(revision.get::<i32, _>("Revision"), 5);
    assert!(revision
        .get::<Option<i32>, _>("SourceDraftRevision")
        .is_none());
    assert!(revision.get::<Option<i64>, _>("OptionSchemaId").is_none());
    assert_eq!(revision.get::<String, _>("OptionValuesJson"), "{}");
    assert!(revision.get::<Option<String>, _>("CompletedAt").is_none());

    let fabricated_history_count: i64 = sqlx::query_scalar(
        r#"SELECT
               (SELECT COUNT(*) FROM "AddonMediaListOptionSchema") +
               (SELECT COUNT(*) FROM "AddonMediaListImportCheckpoint") +
               (SELECT COUNT(*) FROM "AddonMediaListLifecycleJob") +
               (SELECT COUNT(*) FROM "AddonMediaListItemTombstone") +
               (SELECT COUNT(*) FROM "AddonMediaListArtifact") +
               (SELECT COUNT(*) FROM "AddonToolMaintenanceJob")"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(fabricated_history_count, 0);
}

#[tokio::test]
async fn adaptive_refresh_migration_adds_bounded_cascade_owned_state() {
    const ADAPTIVE_REFRESH_VERSION: i64 = 202_609_041_900;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open adaptive refresh migration database");
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    let previous = sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < ADAPTIVE_REFRESH_VERSION)
            .cloned()
            .collect(),
    );
    previous
        .run(&pool)
        .await
        .expect("migrate pre-adaptive schema");
    seed_legacy_lifecycle_rows(&pool).await;

    migrations
        .run(&pool)
        .await
        .expect("apply adaptive refresh migration");

    let fabricated_state: i64 = sqlx::query_scalar(
        r#"SELECT
               (SELECT COUNT(*) FROM "AddonMediaListRefreshState") +
               (SELECT COUNT(*) FROM "AddonMediaListSourceChangeObservation")"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fabricated_state, 0);

    sqlx::raw_sql(
        r#"
        UPDATE "AddonMediaList" SET "RefreshCadence" = 5 WHERE "Id" = 1;
        INSERT INTO "AddonMediaListRefreshState" (
            "AddonMediaListId", "Generation", "State", "Validator",
            "MissCount", "AttemptCount", "NextEligibleAt", "LastOutcome",
            "UpdatedAt"
        ) VALUES (
            1, 3, 3, 'opaque-validator', 2, 1,
            '2026-09-04T12:00:00Z', 2, '2026-09-04T11:00:00Z'
        );
        INSERT INTO "AddonMediaListSourceChangeObservation" (
            "AddonMediaListId", "SourceUpdatedAt"
        ) VALUES
            (1, '2026-09-02T12:00:00Z'),
            (1, '2026-09-03T12:00:00Z');
        "#,
    )
    .execute(&pool)
    .await
    .expect("persist adaptive refresh state");

    let state = sqlx::query(
        r#"SELECT "list"."RefreshCadence", "refresh"."Generation",
                  "refresh"."State", "refresh"."MissCount",
                  "refresh"."AttemptCount", "refresh"."LastOutcome"
           FROM "AddonMediaList" AS "list"
           JOIN "AddonMediaListRefreshState" AS "refresh"
             ON "refresh"."AddonMediaListId" = "list"."Id"
           WHERE "list"."Id" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state.get::<i32, _>("RefreshCadence"), 5);
    assert_eq!(state.get::<i64, _>("Generation"), 3);
    assert_eq!(state.get::<i32, _>("State"), 3);
    assert_eq!(state.get::<i64, _>("MissCount"), 2);
    assert_eq!(state.get::<i64, _>("AttemptCount"), 1);
    assert_eq!(state.get::<i32, _>("LastOutcome"), 2);

    let negative_attempt = sqlx::query(
        r#"INSERT INTO "AddonMediaListRefreshState" (
               "AddonMediaListId", "Generation", "State", "MissCount",
               "AttemptCount", "UpdatedAt"
           ) VALUES (2, 0, 0, 0, -1, '2026-09-04T12:00:00Z')"#,
    )
    .execute(&pool)
    .await;
    assert!(negative_attempt.is_err());

    sqlx::query(r#"DELETE FROM "AddonMediaList" WHERE "Id" = 1"#)
        .execute(&pool)
        .await
        .expect("delete owning list");
    let owned_rows: i64 = sqlx::query_scalar(
        r#"SELECT
               (SELECT COUNT(*) FROM "AddonMediaListRefreshState") +
               (SELECT COUNT(*) FROM "AddonMediaListSourceChangeObservation")"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owned_rows, 0);
}

#[tokio::test]
async fn import_v2_migration_freezes_legacy_jobs_to_the_v1_transport() {
    const IMPORT_V2_VERSION: i64 = 202_609_042_000;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open import v2 migration database");
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    let previous = sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < IMPORT_V2_VERSION)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.expect("migrate pre-v2 schema");
    seed_legacy_lifecycle_rows(&pool).await;
    sqlx::query(
        r#"INSERT INTO "AddonMediaListImportCheckpoint" (
               "AddonMediaListId", "RelativePath", "Generation", "Sha256",
               "EntryCount", "PromotedAt"
           ) VALUES (1, 'legacy/archive.txt', 1, 'legacy-digest', 1,
                     '2026-09-04T12:00:00Z')"#,
    )
    .execute(&pool)
    .await
    .expect("seed legacy committed checkpoint");

    migrations
        .run(&pool)
        .await
        .expect("apply import v2 migration");

    let migrated_job = sqlx::query(
        r#"SELECT "ImportCapability", "FinalizationPhase"
           FROM "AddonMediaListImportJob""#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let capability: String = migrated_job.get("ImportCapability");
    assert_eq!(capability, "media-list.import.v1");
    assert_eq!(migrated_job.get::<i32, _>("FinalizationPhase"), 0);

    let legacy_checkpoint = sqlx::query(
        r#"SELECT "DataRoot", "Committed" FROM "AddonMediaListImportCheckpoint"
           WHERE "AddonMediaListId" = 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(legacy_checkpoint
        .get::<Option<String>, _>("DataRoot")
        .is_none());
    assert_eq!(legacy_checkpoint.get::<i32, _>("Committed"), 1);

    sqlx::query(
        r#"INSERT INTO "AddonMediaListImportCheckpoint" (
               "AddonMediaListId", "RelativePath", "Generation", "Sha256",
               "EntryCount", "PromotedAt", "DataRoot", "Committed"
           ) VALUES (1, 'private/job/archive.txt', 2, 'job-digest', 2,
                     '2026-09-04T12:01:00Z', 'private-data', 0)"#,
    )
    .execute(&pool)
    .await
    .expect("persist parallel job checkpoint");
    let checkpoint_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListImportCheckpoint""#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoint_count, 2);
}

#[tokio::test]
async fn reappearance_marker_migration_preserves_existing_tombstones() {
    const REAPPEARANCE_VERSION: i64 = 202_609_042_100;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open reappearance migration database");
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    let previous = sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < REAPPEARANCE_VERSION)
            .cloned()
            .collect(),
    );
    previous
        .run(&pool)
        .await
        .expect("migrate pre-reappearance schema");
    seed_legacy_lifecycle_rows(&pool).await;
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItem" (
               "Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title", "Kind", "Rank"
           ) VALUES (10, 1, 'fixture-item', 'Fixture item', 'Fixture item', 4, 0)"#,
    )
    .execute(&pool)
    .await
    .expect("seed tombstone item");
    sqlx::query(
        r#"INSERT INTO "AddonMediaListItemTombstone" (
               "AddonMediaListItemId", "MissingSince", "LastObservedAt",
               "ObservationCount", "LastReconcileGeneration"
           ) VALUES (10, '2026-09-03T00:00:00Z', '2026-09-03T00:00:00Z', 1, 4)"#,
    )
    .execute(&pool)
    .await
    .expect("seed existing tombstone");

    migrations
        .run(&pool)
        .await
        .expect("apply reappearance migration");

    let row = sqlx::query(
        r#"SELECT "ObservationCount", "LastReconcileGeneration", "ReappearedAt"
           FROM "AddonMediaListItemTombstone" WHERE "AddonMediaListItemId" = 10"#,
    )
    .fetch_one(&pool)
    .await
    .expect("load migrated tombstone");
    assert_eq!(row.get::<i32, _>("ObservationCount"), 1);
    assert_eq!(row.get::<i32, _>("LastReconcileGeneration"), 4);
    assert!(row.get::<Option<String>, _>("ReappearedAt").is_none());
    let item_generation: Option<i32> = sqlx::query_scalar(
        r#"SELECT "LastReconcileGeneration" FROM "AddonMediaListItem" WHERE "Id" = 10"#,
    )
    .fetch_one(&pool)
    .await
    .expect("load migrated item generation");
    assert_eq!(item_generation, None);
}

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
                  "job"."OriginalFailureCode", "event"."Sequence" AS "EventSequence",
                  "event"."Kind" AS "EventKind", "event"."Phase" AS "EventPhase"
           FROM "ManagedLibraryOperation" AS "operation"
           JOIN "AddonMediaListPublicationJob" AS "job"
             ON "job"."OperationId" = "operation"."Id"
           JOIN "AddonMediaListPublicationEvent" AS "event"
             ON "event"."PublicationJobId" = "job"."Id"
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
    assert_eq!(row.get::<i32, _>("EventSequence"), 1);
    assert_eq!(row.get::<i32, _>("EventKind"), 0);
    assert_eq!(row.get::<i32, _>("EventPhase"), 4);
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

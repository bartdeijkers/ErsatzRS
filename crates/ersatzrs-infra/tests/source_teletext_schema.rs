use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};

const MIGRATION: i64 = 202_610_101_930;

// Dynamic statements below compose literal test identifiers/assignments only.
// No external input enters the audited AssertSqlSafe queries.

async fn empty_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    pool
}

async fn current_pool() -> SqlitePool {
    let pool = empty_pool().await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    seed_ordinary_rows(&pool).await;
    pool
}

async fn seed_ordinary_rows(pool: &SqlitePool) {
    sqlx::raw_sql(
        r#"INSERT INTO "MediaSource" ("Id") VALUES (31);
        INSERT INTO "MediaVersion" (
            "Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind")
            VALUES (1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '00:01:00', 576, 720, 0),
                   (2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '00:01:00', 576, 720, 0);
        INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash")
            VALUES (21, 1, 'fixture/part-one.ts', 'synthetic-part-one'),
                   (22, 1, 'fixture/part-two.ts', 'synthetic-part-two'),
                   (23, 2, 'fixture/other.ts', 'synthetic-other');
        INSERT INTO "MediaStream" (
            "Id", "MediaVersionId", "Index", "Codec", "MediaStreamKind", "Language",
            "Channels", "Title", "Default", "Forced")
            VALUES (11, 1, 4, 'dvb_teletext', 3, 'nld', 0, 'Reported data', 0, 0),
                   (12, 1, 3, 'aac', 2, 'eng', 2, 'English', 1, 0),
                   (13, 1, 7, 'subrip', 3, 'nld', 0, 'Dutch subtitles', 0, 1),
                   (14, 1, 3, 'aac', 2, 'deu', 6, 'German alternate', 0, 0),
                   (15, 2, 4, 'dvb_teletext', 3, 'eng', 0, 'Other data', 0, 0);"#,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_metadata(
    pool: &SqlitePool,
    version: i64,
    stream: i64,
    file: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO "SourceTeletextMetadata" (
            "MediaVersionId", "MediaStreamId", "MediaFileId", "StreamIndex", "BindingStatus",
            "Provenance", "PidStatus", "ProgramStatus", "DescriptorStatus", "ObservedPagesStatus")
            VALUES (?, ?, ?, 4, 0, 1, 0, 2, 2, 0)"#,
    )
    .bind(version)
    .bind(stream)
    .bind(file)
    .execute(pool)
    .await
    .map(|result| result.last_insert_rowid())
}

async fn insert_children(pool: &SqlitePool, parent: i64) {
    sqlx::query(
        r#"INSERT INTO "SourceTeletextDescriptor" (
            "SourceTeletextMetadataId", "ProjectionOrdinal", "Language", "DescriptorType",
            "WireMagazine", "WirePage") VALUES (?, 0, 'nld', 1, 2, 0)"#,
    )
    .bind(parent)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "SourceTeletextProgram" (
            "SourceTeletextMetadataId", "MembershipOrdinal", "ProgramId", "ProgramNumber",
            "PmtPid", "PcrPid") VALUES (?, 0, 100, 100, 4096, 256)"#,
    )
    .bind(parent)
    .execute(pool)
    .await
    .unwrap();
}

async fn bind_metadata(pool: &SqlitePool, parent: i64, revision: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"UPDATE "SourceTeletextMetadata" SET "BindingStatus" = 2, "MediaSourceId" = 31,
           "Provider" = 'Local', "SourceRevision" = ? WHERE "Id" = ?"#,
    )
    .bind(revision)
    .bind(parent)
    .execute(pool)
    .await
    .map(|_| ())
}

type OrdinaryTrack = (i64, i64, i64, String, i64, String, i64, String, i64, i64);

async fn ordinary_tracks(pool: &SqlitePool) -> Vec<OrdinaryTrack> {
    sqlx::query_as(
        r#"SELECT "Id", "MediaVersionId", "Index", "Codec", "MediaStreamKind", "Language",
                  "Channels", "Title", "Default", "Forced" FROM "MediaStream" ORDER BY "Id""#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn metadata_counts(pool: &SqlitePool) -> (i64, i64, i64) {
    sqlx::query_as(
        r#"SELECT (SELECT COUNT(*) FROM "SourceTeletextMetadata"),
                  (SELECT COUNT(*) FROM "SourceTeletextDescriptor"),
                  (SELECT COUNT(*) FROM "SourceTeletextProgram")"#,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn source_teletext_schema_fresh_migration_keeps_acquisition_absent() {
    let pool = current_pool().await;
    assert_eq!(metadata_counts(&pool).await, (0, 0, 0));
    assert_eq!(ordinary_tracks(&pool).await.len(), 5);
    let foreign_key_failures = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(foreign_key_failures.is_empty());
    let observed_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'SourceTeletextPage'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(observed_tables, 0);
}

#[tokio::test]
async fn source_teletext_schema_upgrade_preserves_ordinary_tracks_and_files() {
    let pool = empty_pool().await;
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < MIGRATION)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    seed_ordinary_rows(&pool).await;
    let before = ordinary_tracks(&pool).await;
    let before_files: Vec<(i64, i64, String, String)> = sqlx::query_as(
        r#"SELECT "Id", "MediaVersionId", "Path", "PathHash" FROM "MediaFile" ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    migrations.run(&pool).await.unwrap();
    migrations.run(&pool).await.unwrap();
    assert_eq!(ordinary_tracks(&pool).await, before);
    let after_files = sqlx::query_as::<_, (i64, i64, String, String)>(
        r#"SELECT "Id", "MediaVersionId", "Path", "PathHash" FROM "MediaFile" ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(after_files, before_files);
    assert_eq!(
        before[1].2, before[3].2,
        "ordinary duplicate Index remains valid"
    );
    assert_eq!(metadata_counts(&pool).await, (0, 0, 0));
    insert_metadata(&pool, 1, 11, 21).await.unwrap();
}

#[tokio::test]
async fn source_teletext_schema_requires_same_version_and_distinguishes_files() {
    let pool = current_pool().await;
    assert!(insert_metadata(&pool, 1, 11, 23).await.is_err());
    assert!(insert_metadata(&pool, 1, 15, 21).await.is_err());
    assert!(insert_metadata(&pool, 2, 11, 23).await.is_err());
    assert!(insert_metadata(&pool, 1, 999, 21).await.is_err());
    assert!(insert_metadata(&pool, 1, 11, 999).await.is_err());
    let first = insert_metadata(&pool, 1, 11, 21).await.unwrap();
    let second = insert_metadata(&pool, 1, 11, 22).await.unwrap();
    assert_ne!(first, second);
    assert!(insert_metadata(&pool, 1, 11, 21).await.is_err());
    assert_eq!(metadata_counts(&pool).await, (2, 0, 0));
    let before = ordinary_tracks(&pool).await;
    sqlx::query(
        r#"INSERT INTO "MediaStream" (
            "Id", "MediaVersionId", "Index", "Codec", "MediaStreamKind", "Language",
            "Channels", "Title", "Default", "Forced")
            VALUES (16, 1, 3, 'aac', 1, 'fra', 2, 'French alternate', 0, 0)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(ordinary_tracks(&pool).await[..5], before);
}

#[tokio::test]
async fn source_teletext_schema_provider_file_upsert_can_reassign_existing_version() {
    let pool = current_pool().await;
    let first = insert_metadata(&pool, 1, 11, 21).await.unwrap();
    let other = insert_metadata(&pool, 1, 11, 22).await.unwrap();
    bind_metadata(&pool, first, "synthetic-before-move")
        .await
        .unwrap();
    insert_children(&pool, first).await;
    insert_children(&pool, other).await;
    let tracks = ordinary_tracks(&pool).await;
    // This is the PathHash-conflict SQL produced by the existing Plex and
    // Jellyfin/Emby ensure_media_file upsert; it preserves the old file ID.
    pool.execute(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
           VALUES (2, 'fixture/part-one.ts', 'synthetic-part-one')
           ON CONFLICT("PathHash") DO UPDATE SET
               "MediaVersionId" = excluded."MediaVersionId", "Path" = excluded."Path""#,
    )
    .await
    .unwrap();
    let file: (i64, i64, String, String) = sqlx::query_as(
        r#"SELECT "Id", "MediaVersionId", "Path", "PathHash" FROM "MediaFile" WHERE "Id" = 21"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        file,
        (
            21,
            2,
            "fixture/part-one.ts".into(),
            "synthetic-part-one".into()
        )
    );
    assert_eq!(ordinary_tracks(&pool).await, tracks);
    assert_eq!(metadata_counts(&pool).await, (1, 1, 1));
    let survivor: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "SourceTeletextMetadata""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(survivor, other);
}

#[tokio::test]
async fn source_teletext_schema_file_identity_changes_invalidate_only_that_file() {
    for assignment in [
        r#""Path" = 'fixture/renamed.ts'"#,
        r#""Path" = NULL"#,
        r#""PathHash" = 'synthetic-renamed'"#,
    ] {
        let pool = current_pool().await;
        let first = insert_metadata(&pool, 1, 11, 21).await.unwrap();
        let other = insert_metadata(&pool, 1, 11, 22).await.unwrap();
        insert_children(&pool, first).await;
        insert_children(&pool, other).await;
        let tracks = ordinary_tracks(&pool).await;
        pool.execute(
            r#"UPDATE "MediaFile" SET "MediaVersionId" = "MediaVersionId", "Path" = "Path",
               "PathHash" = "PathHash" WHERE "Id" = 21"#,
        )
        .await
        .unwrap();
        assert_eq!(metadata_counts(&pool).await, (2, 2, 2));
        let sql = format!(r#"UPDATE "MediaFile" SET {assignment} WHERE "Id" = 21"#);
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(metadata_counts(&pool).await, (1, 1, 1), "{assignment}");
        assert_eq!(ordinary_tracks(&pool).await, tracks);
        let survivor: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(survivor, other);
    }
}

#[tokio::test]
async fn source_teletext_schema_stream_identity_changes_invalidate_only_that_stream() {
    for assignment in [
        r#""MediaVersionId" = 2"#,
        r#""Index" = 5"#,
        r#""Codec" = 'replacement'"#,
        r#""Codec" = NULL"#,
        r#""MediaStreamKind" = 1"#,
        r#""Language" = 'eng'"#,
        r#""Language" = NULL"#,
    ] {
        let pool = current_pool().await;
        let first = insert_metadata(&pool, 1, 11, 21).await.unwrap();
        let second_file = insert_metadata(&pool, 1, 11, 22).await.unwrap();
        let other = insert_metadata(&pool, 2, 15, 23).await.unwrap();
        for parent in [first, second_file, other] {
            insert_children(&pool, parent).await;
        }
        pool.execute(
            r#"UPDATE "MediaStream" SET "MediaVersionId" = "MediaVersionId", "Index" = "Index",
               "Codec" = "Codec", "MediaStreamKind" = "MediaStreamKind", "Language" = "Language"
               WHERE "Id" = 11"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"UPDATE "MediaStream" SET "Title" = 'Renamed', "Default" = 1, "Forced" = 1 WHERE "Id" = 11"#,
        )
        .await
        .unwrap();
        assert_eq!(metadata_counts(&pool).await, (3, 3, 3));
        let other_tracks: Vec<_> = ordinary_tracks(&pool)
            .await
            .into_iter()
            .filter(|track| track.0 != 11)
            .collect();
        let sql = format!(r#"UPDATE "MediaStream" SET {assignment} WHERE "Id" = 11"#);
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(metadata_counts(&pool).await, (1, 1, 1), "{assignment}");
        let survivor: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(survivor, other);
        let remaining: Vec<_> = ordinary_tracks(&pool)
            .await
            .into_iter()
            .filter(|track| track.0 != 11)
            .collect();
        assert_eq!(remaining, other_tracks);
    }
}

#[tokio::test]
async fn source_teletext_schema_failed_parent_changes_roll_back_trigger_cleanup() {
    let pool = current_pool().await;
    let parent = insert_metadata(&pool, 1, 11, 21).await.unwrap();
    bind_metadata(&pool, parent, "synthetic-before-update")
        .await
        .unwrap();
    insert_children(&pool, parent).await;
    let tracks = ordinary_tracks(&pool).await;
    for update in [
        r#"UPDATE "MediaFile" SET "MediaVersionId" = 999 WHERE "Id" = 21"#,
        r#"UPDATE "MediaFile" SET "PathHash" = 'synthetic-part-two' WHERE "Id" = 21"#,
        r#"UPDATE "MediaStream" SET "MediaVersionId" = 999 WHERE "Id" = 11"#,
    ] {
        assert!(pool.execute(update).await.is_err());
        assert_eq!(metadata_counts(&pool).await, (1, 1, 1));
        assert_eq!(ordinary_tracks(&pool).await, tracks);
    }
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path" = 'fixture/rolled-back.ts' WHERE "Id" = 21"#)
        .execute(&mut *transaction)
        .await
        .unwrap();
    let cleared: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
        .fetch_one(&mut *transaction)
        .await
        .unwrap();
    assert_eq!(cleared, 0);
    transaction.rollback().await.unwrap();
    assert_eq!(metadata_counts(&pool).await, (1, 1, 1));
    let path: String = sqlx::query_scalar(r#"SELECT "Path" FROM "MediaFile" WHERE "Id" = 21"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(path, "fixture/part-one.ts");
}

#[tokio::test]
async fn source_teletext_schema_cascades_each_owned_parent_and_keeps_other_tracks() {
    for delete in [
        r#"DELETE FROM "SourceTeletextMetadata" WHERE "Id" = 1"#,
        r#"DELETE FROM "MediaStream" WHERE "Id" = 11"#,
        r#"DELETE FROM "MediaFile" WHERE "Id" = 21"#,
        r#"DELETE FROM "MediaVersion" WHERE "Id" = 1"#,
        r#"DELETE FROM "MediaSource" WHERE "Id" = 31"#,
    ] {
        let pool = current_pool().await;
        let parent = insert_metadata(&pool, 1, 11, 21).await.unwrap();
        assert_eq!(parent, 1);
        bind_metadata(&pool, parent, "synthetic-revision")
            .await
            .unwrap();
        insert_children(&pool, parent).await;
        assert_eq!(metadata_counts(&pool).await, (1, 1, 1));
        pool.execute(delete).await.unwrap();
        assert_eq!(metadata_counts(&pool).await, (0, 0, 0), "{delete}");
        let remaining = ordinary_tracks(&pool).await;
        assert!(remaining.iter().any(|track| track.0 == 15));
        if !delete.contains("MediaVersion") {
            assert!(remaining.iter().any(|track| track.0 == 12));
            assert!(remaining.iter().any(|track| track.0 == 13));
            assert!(remaining.iter().any(|track| track.0 == 14));
        }
    }
}

#[tokio::test]
async fn source_teletext_schema_preserves_repeated_descriptors_and_memberships() {
    let pool = current_pool().await;
    let parent = insert_metadata(&pool, 1, 11, 21).await.unwrap();
    insert_children(&pool, parent).await;
    pool.execute(
        r#"INSERT INTO "SourceTeletextDescriptor" VALUES
            (1, 1, 'nld', 1, 2, 0), (1, 2, 'eng', 31, 0, 250)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "SourceTeletextProgram" VALUES
            (1, 1, 100, 100, 4096, 256), (1, 2, 200, 200, 4097, 8191)"#,
    )
    .await
    .unwrap();
    let descriptors: Vec<(i64, String, i64, i64, i64)> = sqlx::query_as(
        r#"SELECT "ProjectionOrdinal", "Language", "DescriptorType", "WireMagazine", "WirePage"
           FROM "SourceTeletextDescriptor" ORDER BY "ProjectionOrdinal""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        descriptors,
        vec![
            (0, "nld".into(), 1, 2, 0),
            (1, "nld".into(), 1, 2, 0),
            (2, "eng".into(), 31, 0, 250)
        ]
    );
    let programs: Vec<(i64, i64, i64, i64, i64)> = sqlx::query_as(
        r#"SELECT "MembershipOrdinal", "ProgramId", "ProgramNumber", "PmtPid", "PcrPid"
           FROM "SourceTeletextProgram" ORDER BY "MembershipOrdinal""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        programs,
        vec![
            (0, 100, 100, 4096, 256),
            (1, 100, 100, 4096, 256),
            (2, 200, 200, 4097, 8191)
        ]
    );
    assert!(pool
        .execute(r#"INSERT INTO "SourceTeletextDescriptor" VALUES (1, 1, 'deu', 2, 7, 0)"#)
        .await
        .is_err());
    assert!(pool
        .execute(r#"INSERT INTO "SourceTeletextProgram" VALUES (1, 1, 300, 300, 4098, 257)"#)
        .await
        .is_err());
    assert_eq!(metadata_counts(&pool).await, (1, 3, 3));
}

#[tokio::test]
async fn source_teletext_schema_independent_states_reject_incomplete_or_invalid_values() {
    let pool = current_pool().await;
    insert_metadata(&pool, 1, 11, 21).await.unwrap();
    for assignment in [
        r#""BindingStatus" = 2"#,
        r#""BindingStatus" = 0, "MediaSourceId" = 31"#,
        r#""BindingStatus" = 1, "Provider" = 'Local'"#,
        r#""BindingStatus" = 1, "SourceRevision" = 'revision'"#,
        r#""BindingStatus" = 2, "MediaSourceId" = 31, "Provider" = 'Local'"#,
        r#""BindingStatus" = 2, "MediaSourceId" = 31, "SourceRevision" = 'revision'"#,
        r#""BindingStatus" = 3"#,
        r#""BindingStatus" = 0.5"#,
        r#""PidStatus" = 2"#,
        r#""PidStatus" = 1, "Pid" = 256"#,
        r#""PidStatus" = 2, "Pid" = 8192"#,
        r#""PidStatus" = 2, "Pid" = -1"#,
        r#""PidStatus" = 2, "Pid" = 256.5"#,
        r#""PidStatus" = 3"#,
        r#""ProgramStatus" = 3"#,
        r#""ProgramStatus" = 1.5"#,
        r#""DescriptorStatus" = 3"#,
        r#""DescriptorStatus" = 0, "TrailingUnpairedPairs" = 1"#,
        r#""TrailingUnpairedPairs" = 0"#,
        r#""TrailingUnpairedPairs" = 4294967296"#,
        r#""TrailingUnpairedPairs" = 1.5"#,
        r#""Provenance" = 2"#,
        r#""Provenance" = 0.5"#,
        r#""ObservedPagesStatus" = 1"#,
        r#""ObservedPagesStatus" = 0.5"#,
        r#""StreamIndex" = -1"#,
        r#""StreamIndex" = 2147483648"#,
        r#""StreamIndex" = 4.5"#,
    ] {
        let sql = format!(r#"UPDATE "SourceTeletextMetadata" SET {assignment} WHERE "Id" = 1"#);
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .execute(&pool)
                .await
                .is_err(),
            "{assignment}"
        );
    }
    pool.execute(
        r#"UPDATE "SourceTeletextMetadata" SET "BindingStatus" = 1, "Provenance" = 0,
           "PidStatus" = 2, "Pid" = 8191, "ProgramStatus" = 0, "DescriptorStatus" = 1"#,
    )
    .await
    .unwrap();
    let states: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT "BindingStatus", "Provenance", "PidStatus", "Pid", "ProgramStatus", "DescriptorStatus"
           FROM "SourceTeletextMetadata""#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(states, (1, 0, 2, 8191, 0, 1));
    pool.execute(
        r#"UPDATE "SourceTeletextMetadata" SET "DescriptorStatus" = 2,
           "TrailingUnpairedPairs" = 4294967295, "ProgramStatus" = 2"#,
    )
    .await
    .unwrap();
    assert_eq!(
        metadata_counts(&pool).await,
        (1, 0, 0),
        "reported-empty stays independent from NotAcquired pages"
    );
}

#[tokio::test]
async fn source_teletext_schema_binding_revision_bounds_are_utf8_bytes() {
    let pool = current_pool().await;
    let parent = insert_metadata(&pool, 1, 11, 21).await.unwrap();
    let revision = "é".repeat(512);
    bind_metadata(&pool, parent, &revision).await.unwrap();
    assert!(bind_metadata(&pool, parent, &"é".repeat(513))
        .await
        .is_err());
    assert!(bind_metadata(&pool, parent, "").await.is_err());
    let saved: String =
        sqlx::query_scalar(r#"SELECT "SourceRevision" FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved, revision);
    for assignment in [
        r#""MediaSourceId" = NULL"#,
        r#""MediaSourceId" = 999"#,
        r#""MediaSourceId" = 0"#,
        r#""Provider" = NULL"#,
        r#""Provider" = 'Unknown'"#,
        r#""SourceRevision" = NULL"#,
        r#""SourceRevision" = X'72'"#,
        r#""BindingStatus" = 0"#,
    ] {
        let sql = format!(r#"UPDATE "SourceTeletextMetadata" SET {assignment} WHERE "Id" = 1"#);
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .execute(&pool)
                .await
                .is_err(),
            "{assignment}"
        );
    }
    for provider in ["Local", "Plex", "Jellyfin", "Emby"] {
        sqlx::query(r#"UPDATE "SourceTeletextMetadata" SET "Provider" = ?"#)
            .bind(provider)
            .execute(&pool)
            .await
            .unwrap();
    }
    // Opaque revision text is not normalized; the domain permits an embedded NUL.
    bind_metadata(&pool, parent, "\0").await.unwrap();
    let saved: String =
        sqlx::query_scalar(r#"SELECT "SourceRevision" FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved, "\0");
}

#[tokio::test]
async fn source_teletext_schema_descriptor_bounds_preserve_reserved_bits() {
    let pool = current_pool().await;
    let parent = insert_metadata(&pool, 1, 11, 21).await.unwrap();
    insert_children(&pool, parent).await;
    for assignment in [
        r#""ProjectionOrdinal" = -1"#,
        r#""ProjectionOrdinal" = 4294967296"#,
        r#""ProjectionOrdinal" = 0.5"#,
        r#""DescriptorType" = 32"#,
        r#""DescriptorType" = -1"#,
        r#""DescriptorType" = 1.5"#,
        r#""WireMagazine" = 8"#,
        r#""WireMagazine" = -1"#,
        r#""WireMagazine" = 2.5"#,
        r#""WirePage" = 256"#,
        r#""WirePage" = -1"#,
        r#""WirePage" = 0.5"#,
        r#""Language" = 'NLD'"#,
        r#""Language" = 'nl'"#,
        r#""Language" = 'n1d'"#,
        r#""Language" = 'éa'"#,
        r#""Language" = X'6e6c64'"#,
        r#""Language" = 'nl' || CHAR(0)"#,
    ] {
        let sql = format!(r#"UPDATE "SourceTeletextDescriptor" SET {assignment}"#);
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(sql))
                .execute(&pool)
                .await
                .is_err(),
            "{assignment}"
        );
    }
    pool.execute(
        r#"UPDATE "SourceTeletextDescriptor" SET "ProjectionOrdinal" = 4294967295,
           "DescriptorType" = 31, "WireMagazine" = 0, "WirePage" = 255"#,
    )
    .await
    .unwrap();
    let raw: (i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT "ProjectionOrdinal", "DescriptorType", "WireMagazine", "WirePage"
           FROM "SourceTeletextDescriptor""#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(raw, (4_294_967_295, 31, 0, 255));
}

#[tokio::test]
async fn source_teletext_schema_program_bounds_keep_raw_membership_values() {
    let pool = current_pool().await;
    let parent = insert_metadata(&pool, 1, 11, 21).await.unwrap();
    insert_children(&pool, parent).await;
    for field in ["MembershipOrdinal", "ProgramId", "ProgramNumber"] {
        for value in ["-1", "4294967296", "0.5"] {
            let sql = format!(r#"UPDATE "SourceTeletextProgram" SET "{field}" = {value}"#);
            assert!(
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .execute(&pool)
                    .await
                    .is_err(),
                "{field}={value}"
            );
        }
    }
    for field in ["PmtPid", "PcrPid"] {
        for value in ["-1", "8192", "0.5"] {
            let sql = format!(r#"UPDATE "SourceTeletextProgram" SET "{field}" = {value}"#);
            assert!(
                sqlx::query(sqlx::AssertSqlSafe(sql))
                    .execute(&pool)
                    .await
                    .is_err(),
                "{field}={value}"
            );
        }
    }
    pool.execute(
        r#"UPDATE "SourceTeletextProgram" SET "MembershipOrdinal" = 4294967295,
           "ProgramId" = 4294967295, "ProgramNumber" = 4294967295, "PmtPid" = 0, "PcrPid" = 8191"#,
    )
    .await
    .unwrap();
    let raw: (i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT "MembershipOrdinal", "ProgramId", "ProgramNumber", "PmtPid", "PcrPid"
           FROM "SourceTeletextProgram""#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(raw, (4_294_967_295, 4_294_967_295, 4_294_967_295, 0, 8191));
}

#[tokio::test]
async fn source_teletext_schema_failed_replacement_rolls_back_metadata_and_tracks() {
    let pool = current_pool().await;
    let parent = insert_metadata(&pool, 1, 11, 21).await.unwrap();
    bind_metadata(&pool, parent, "synthetic-original")
        .await
        .unwrap();
    insert_children(&pool, parent).await;
    let tracks = ordinary_tracks(&pool).await;
    let mut transaction = pool.begin().await.unwrap();
    sqlx::query(r#"DELETE FROM "SourceTeletextMetadata" WHERE "Id" = ?"#)
        .bind(parent)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaStream" SET "Language" = 'fra' WHERE "Id" = 12"#)
        .execute(&mut *transaction)
        .await
        .unwrap();
    let replacement = sqlx::query(
        r#"INSERT INTO "SourceTeletextMetadata" (
            "MediaVersionId", "MediaStreamId", "MediaFileId", "StreamIndex", "BindingStatus",
            "Provenance", "PidStatus", "ProgramStatus", "DescriptorStatus", "ObservedPagesStatus")
            VALUES (1, 11, 21, 4, 0, 1, 0, 2, 2, 0)"#,
    )
    .execute(&mut *transaction)
    .await
    .unwrap()
    .last_insert_rowid();
    assert!(sqlx::query(
        r#"INSERT INTO "SourceTeletextDescriptor" VALUES (?, 0, 'nld', 32, 2, 0)"#,
    )
    .bind(replacement)
    .execute(&mut *transaction)
    .await
    .is_err());
    transaction.rollback().await.unwrap();
    assert_eq!(metadata_counts(&pool).await, (1, 1, 1));
    assert_eq!(ordinary_tracks(&pool).await, tracks);
    let restored: (i64, String) =
        sqlx::query_as(r#"SELECT "Id", "SourceRevision" FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(restored, (parent, "synthetic-original".to_owned()));
    let descriptor: (String, i64, i64, i64) = sqlx::query_as(
        r#"SELECT "Language", "DescriptorType", "WireMagazine", "WirePage" FROM "SourceTeletextDescriptor""#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(descriptor, ("nld".to_owned(), 1, 2, 0));
}

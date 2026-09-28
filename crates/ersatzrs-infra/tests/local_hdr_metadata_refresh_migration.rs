use sqlx::{sqlite::SqlitePoolOptions, Executor};

#[tokio::test]
async fn local_hdr_metadata_refresh_migration_is_scoped_and_runs_once() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|m| m.version < 202_609_282_140)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    pool.execute(
        r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1), (2);
        INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId") VALUES (1, 1, 1), (2, 1, 2);
        INSERT INTO "LocalLibrary" ("Id") VALUES (1);
        INSERT INTO "LibraryPath" ("Id", "LibraryId") VALUES (1, 1), (2, 2);
    "#,
    )
    .await
    .unwrap();
    // Local PQ/HLG, local SDR, provider PQ, and misleading non-video PQ.
    for (id, library, kind, transfer) in [
        (1_i64, 1_i64, 1, "smpte2084"),
        (2, 1, 1, "ARIB-STD-B67"),
        (3, 1, 1, "bt709"),
        (4, 2, 1, "smpte2084"),
        (5, 1, 2, "smpte2084"),
    ] {
        sqlx::query(r#"INSERT INTO "LibraryFolder" ("Id", "LibraryPathId", "Etag") VALUES (?, ?, 'cached')"#)
            .bind(id).bind(library).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "MediaVersion" ("Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind")
            VALUES (?, '2026-09-01', '2026-09-02', '00:00:01', 64, 64, 0)"#)
            .bind(id).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "LibraryFolderId", "PathHash") VALUES (?, ?, ?, ?)"#)
            .bind(id).bind(id).bind(id).bind(format!("synthetic-{id}")).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "MediaStream" ("Id", "Index", "MediaStreamKind", "MediaVersionId", "ColorTransfer", "Channels", "Default", "Forced")
            VALUES (?, 0, ?, ?, ?, 0, 0, 0)"#)
            .bind(id).bind(kind).bind(id).bind(transfer).execute(&pool).await.unwrap();
    }
    migrations.run(&pool).await.unwrap();
    let versions: Vec<(i64, String)> =
        sqlx::query_as(r#"SELECT "Id", "DateUpdated" FROM "MediaVersion" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    let folders: Vec<(i64, Option<String>)> =
        sqlx::query_as(r#"SELECT "Id", "Etag" FROM "LibraryFolder" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    for (id, updated) in versions {
        assert_eq!(
            updated,
            if id <= 2 {
                "1970-01-01 00:00:00"
            } else {
                "2026-09-02"
            }
        );
    }
    for (id, etag) in folders {
        assert_eq!(etag.as_deref(), if id <= 2 { None } else { Some("cached") });
    }
    // Neither media stream identities nor file identities are rebuilt.
    let streams: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "MediaStream" ORDER BY "Id""#)
        .fetch_all(&pool)
        .await
        .unwrap();
    let files: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "MediaFile" ORDER BY "Id""#)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(streams, [1, 2, 3, 4, 5]);
    assert_eq!(files, streams);
    pool.execute(
        r#"UPDATE "LibraryFolder" SET "Etag" = 'rescanned';
        UPDATE "MediaVersion" SET "DateUpdated" = '2026-09-28';"#,
    )
    .await
    .unwrap();
    migrations.run(&pool).await.unwrap();
    let refreshed: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryFolder" WHERE "Etag" = 'rescanned'"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        refreshed, 5,
        "a later startup must retain successful scan gates"
    );
}

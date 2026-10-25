mod common;

use ersatzrs_infra::db::assert_sql_safe;
use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};

const REPAIR_VERSION: i64 = 202_610_240_415;
type StoredStream = (i64, Option<String>, Option<String>, i64, Option<i64>);

async fn legacy() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    let all = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version < REPAIR_VERSION)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(
        r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","MediaSourceId","Name","MediaKind","LastScan")
            VALUES (1,1,'Provider',1,'2026-10-23'),(2,1,'Local',1,'2026-10-23');
        INSERT INTO "LocalLibrary" ("Id") VALUES (2);
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,0);
        INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (1,1),(2,2);
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (900,1),(901,1);
        INSERT INTO "Show" ("Id") VALUES (900);
        INSERT INTO "Season" ("Id","ShowId") VALUES (901,900);
    "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn cases(pool: &SqlitePool) -> Vec<(String, i64, bool)> {
    let mut cases = Vec::new();
    let mut id = 10_i64;
    for (provider, kind) in [
        ("Plex", "Movie"),
        ("Plex", "Episode"),
        ("Plex", "OtherVideo"),
        ("Emby", "Movie"),
        ("Emby", "Episode"),
        ("Jellyfin", "Movie"),
        ("Jellyfin", "Episode"),
    ] {
        for (codec, transfer, stream_kind, affected) in [
            ("hevc", None, 1, true),
            ("AV1", Some(""), 1, true),
            ("HEVC", Some("SMPTE2084"), 1, true),
            ("av1", Some("arib-std-b67"), 1, true),
            ("hevc", Some("bt709"), 1, false),
            ("h264", Some("smpte2084"), 1, false),
            ("hevc", None, 2, false),
            ("h264", Some("bt709"), 1, true),
            ("h264", None, 1, true),
        ] {
            common::seed_media_item(pool, id, 1).await;
            let season = if kind == "Episode" {
                ",\"SeasonId\""
            } else {
                ""
            };
            let value = if kind == "Episode" { ",901" } else { "" };
            pool.execute(assert_sql_safe(format!(r#"
                INSERT INTO "{kind}" ("Id"{season}) VALUES ({id}{value});
                INSERT INTO "{provider}{kind}" ("Id","Etag") VALUES ({id},'cached');
                INSERT INTO "MediaVersion" ("Id","{kind}Id","DateAdded","DateUpdated","Duration","Width","Height","VideoScanKind")
                    VALUES ({id},{id},'2026-10-23','2026-10-23','00:01:00',320,180,0);
            "#))).await.unwrap();
            sqlx::query(r#"INSERT INTO "MediaStream" ("Id","MediaVersionId","Index","Codec","ColorTransfer","MediaStreamKind","Channels","Default","Forced","Language","MaxCll")
                VALUES (?,?,0,?,?,?,2,0,0,'nld',1000)"#)
                .bind(id).bind(id).bind(codec).bind(transfer).bind(stream_kind)
                .execute(pool).await.unwrap();
            if codec == "h264" && affected {
                sqlx::query(r#"UPDATE "MediaStream" SET "DolbyVisionProfile"=?,"DolbyVisionMetadataState"=2 WHERE "Id"=?"#)
                    .bind(transfer.map(|_| 0)).bind(id).execute(pool).await.unwrap();
            }
            cases.push((format!("{provider}{kind}"), id, affected));
            id += 1;
        }
    }
    // A local HDR stream is not a provider Etag and must not be rewritten.
    sqlx::raw_sql(r#"
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (800,2);
        INSERT INTO "Movie" ("Id") VALUES (800);
        INSERT INTO "MediaVersion" ("Id","MovieId","DateAdded","DateUpdated","Duration","Width","Height","VideoScanKind")
            VALUES (800,800,'2026-10-23','2026-10-23','00:01:00',320,180,0);
        INSERT INTO "MediaStream" ("Id","MediaVersionId","Index","Codec","ColorTransfer","MediaStreamKind","Channels","Default","Forced","MaxCll")
            VALUES (800,800,0,'hevc','smpte2084',1,0,0,0,1000);
    "#).execute(pool).await.unwrap();
    cases
}

async fn etag(pool: &SqlitePool, table: &str, id: i64) -> Option<String> {
    sqlx::query_scalar(assert_sql_safe(format!(
        r#"SELECT "Etag" FROM "{table}" WHERE "Id"=?"#
    )))
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn hdr_repair_only_invalidates_possible_provider_dv_and_runs_once() {
    let pool = legacy().await;
    let cases = cases(&pool).await;
    let streams: Vec<StoredStream> = sqlx::query_as(
        r#"SELECT "Id","Codec","ColorTransfer","MediaStreamKind","MaxCll" FROM "MediaStream" ORDER BY "Id""#,
    ).fetch_all(&pool).await.unwrap();
    let all = sqlx::migrate!("../../migrations");
    all.run(&pool).await.unwrap();
    for (table, id, affected) in &cases {
        assert_eq!(
            etag(&pool, table, *id).await.as_deref(),
            (!affected).then_some("cached")
        );
        pool.execute(assert_sql_safe(format!(
            r#"UPDATE "{table}" SET "Etag"='rescanned' WHERE "Id"={id}"#
        )))
        .await
        .unwrap();
    }
    assert_eq!(sqlx::query_as::<_, StoredStream>(
        r#"SELECT "Id","Codec","ColorTransfer","MediaStreamKind","MaxCll" FROM "MediaStream" ORDER BY "Id""#
    ).fetch_all(&pool).await.unwrap(), streams);
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
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "Library" WHERE "LastScan"='2026-10-23'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    all.run(&pool).await.unwrap();
    for (table, id, _) in &cases {
        assert_eq!(etag(&pool, table, *id).await.as_deref(), Some("rescanned"));
    }
}

#[tokio::test]
async fn hdr_repair_rolls_back_all_provider_invalidations_on_failure() {
    let pool = legacy().await;
    let cases = cases(&pool).await;
    pool.execute(r#"CREATE TRIGGER fail_hdr_repair BEFORE UPDATE ON "JellyfinEpisode" BEGIN SELECT RAISE(ABORT,'synthetic failure'); END"#)
        .await.unwrap();
    let all = sqlx::migrate!("../../migrations");
    assert!(all.run(&pool).await.is_err());
    for (table, id, _) in &cases {
        assert_eq!(etag(&pool, table, *id).await.as_deref(), Some("cached"));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM _sqlx_migrations WHERE version=?")
            .bind(REPAIR_VERSION)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    pool.execute("DROP TRIGGER fail_hdr_repair").await.unwrap();
    all.run(&pool).await.unwrap();
    for (table, id, affected) in &cases {
        assert_eq!(
            etag(&pool, table, *id).await.as_deref(),
            (!affected).then_some("cached")
        );
    }
}

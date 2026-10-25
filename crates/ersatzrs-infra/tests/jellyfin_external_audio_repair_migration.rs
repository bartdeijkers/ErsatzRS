mod common;

use ersatzrs_infra::db::assert_sql_safe;
use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};

const REPAIR_VERSION: i64 = 202_610_240_341;

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
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1),(2),(3);
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1);
        INSERT INTO "EmbyMediaSource" ("Id") VALUES (2);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (3);
        INSERT INTO "Library" ("Id","MediaSourceId","Name","MediaKind","LastScan") VALUES
          (1,1,'Jellyfin movies',1,'2026-10-23 10:00:00'),(2,1,'Jellyfin shows',2,'2026-10-23 10:00:00'),
          (3,2,'Emby',1,'2026-10-23 10:00:00'),(4,3,'Local',1,'2026-10-23 10:00:00');
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,1),(2,0);
        INSERT INTO "EmbyLibrary" ("Id","ShouldSyncItems") VALUES (3,1);
        INSERT INTO "LocalLibrary" ("Id") VALUES (4);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'jf-movies'),(2,2,'jf-shows'),(3,3,'emby'),(4,4,'local');
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (900,2),(901,2);
        INSERT INTO "Show" ("Id") VALUES (900);
        INSERT INTO "Season" ("Id","ShowId") VALUES (901,900);
    "#).execute(&pool).await.unwrap();
    pool
}

async fn seed_items(pool: &SqlitePool) -> Vec<(String, i64, bool)> {
    let mut cases = Vec::new();
    let mut id = 10_i64;
    for provider in ["Jellyfin", "Emby"] {
        for kind in ["Movie", "Episode"] {
            for index in [Some(-1), Some(1), None] {
                let path = if provider == "Emby" {
                    3
                } else if kind == "Movie" {
                    1
                } else {
                    2
                };
                common::seed_media_item(pool, id, path).await;
                let subtype = if kind == "Episode" {
                    format!(r#"INSERT INTO "Episode" ("Id","SeasonId") VALUES ({id},901);"#)
                } else {
                    format!(r#"INSERT INTO "Movie" ("Id") VALUES ({id});"#)
                };
                pool.execute(assert_sql_safe(subtype)).await.unwrap();
                pool.execute(assert_sql_safe(format!(r#"
                    INSERT INTO "{provider}{kind}" ("Id","Etag") VALUES ({id},'unchanged');
                    INSERT INTO "{kind}Metadata" ("Id","{kind}Id","MetadataKind","DateAdded","DateUpdated") VALUES ({id},{id},1,'2026-10-23','2026-10-23');
                    INSERT INTO "MediaVersion" ("Id","{kind}Id","DateAdded","DateUpdated","Duration","Width","Height","VideoScanKind") VALUES ({id},{id},'2026-10-23','2026-10-23','00:01:00',320,180,0);
                    INSERT INTO "Subtitle" ("{kind}MetadataId","SubtitleKind","StreamIndex","Path","Default","Forced","IsExtracted","DateAdded","DateUpdated") VALUES ({id},99,0,'cached.nl.ass',0,0,1,'2026-10-23','2026-10-23');
                "#))).await.unwrap();
                if let Some(index) = index {
                    sqlx::query(r#"INSERT INTO "MediaStream" ("MediaVersionId","Index","Codec","MediaStreamKind","Channels","Default","Forced","Language") VALUES (?,?,'aac',2,2,1,0,'nld')"#)
                        .bind(id).bind(index).execute(pool).await.unwrap();
                }
                cases.push((
                    format!("{provider}{kind}"),
                    id,
                    provider == "Jellyfin" && index.is_some_and(|index| index < 0),
                ));
                id += 1;
            }
        }
    }
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
async fn jellyfin_audio_repair_invalidates_only_affected_items_and_runs_once() {
    let pool = legacy().await;
    let cases = seed_items(&pool).await;
    let tracks: Vec<(i64, i64, i64, String)> = sqlx::query_as(
        r#"SELECT "Id","Index","MediaStreamKind","Language" FROM "MediaStream" ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let subtitles: Vec<(i64, Option<String>, i64)> =
        sqlx::query_as(r#"SELECT "Id","Path","IsExtracted" FROM "Subtitle" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    let all = sqlx::migrate!("../../migrations");
    all.run(&pool).await.unwrap();
    for (table, id, affected) in &cases {
        assert_eq!(
            etag(&pool, table, *id).await.as_deref(),
            (!affected).then_some("unchanged")
        );
    }
    assert_eq!(
        sqlx::query_as::<_, (i64, String)>(
            r#"SELECT "Id","LastScan" FROM "Library" ORDER BY "Id""#
        )
        .fetch_all(&pool)
        .await
        .unwrap(),
        [
            (1, "0001-01-01 00:00:00".into()),
            (2, "0001-01-01 00:00:00".into()),
            (3, "2026-10-23 10:00:00".into()),
            (4, "2026-10-23 10:00:00".into())
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "ShouldSyncItems" FROM "JellyfinLibrary" WHERE "Id"=2"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_as::<_, (i64, i64, i64, String)>(
            r#"SELECT "Id","Index","MediaStreamKind","Language" FROM "MediaStream" ORDER BY "Id""#
        )
        .fetch_all(&pool)
        .await
        .unwrap(),
        tracks
    );
    assert_eq!(
        sqlx::query_as::<_, (i64, Option<String>, i64)>(
            r#"SELECT "Id","Path","IsExtracted" FROM "Subtitle" ORDER BY "Id""#
        )
        .fetch_all(&pool)
        .await
        .unwrap(),
        subtitles
    );
    for (table, id, _) in &cases {
        sqlx::query(assert_sql_safe(format!(
            r#"UPDATE "{table}" SET "Etag"='rescanned' WHERE "Id"=?"#
        )))
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.execute(r#"UPDATE "Library" SET "LastScan"='2026-10-24 10:00:00'"#)
        .await
        .unwrap();
    all.run(&pool).await.unwrap();
    for (table, id, _) in &cases {
        assert_eq!(etag(&pool, table, *id).await.as_deref(), Some("rescanned"));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "Library" WHERE "LastScan"='2026-10-24 10:00:00'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        4
    );
}

#[tokio::test]
async fn jellyfin_audio_repair_rolls_back_and_can_retry() {
    let pool = legacy().await;
    let cases = seed_items(&pool).await;
    pool.execute(r#"CREATE TRIGGER reject_scan_repair BEFORE UPDATE OF "LastScan" ON "Library" BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;"#).await.unwrap();
    let all = sqlx::migrate!("../../migrations");
    assert!(all.run(&pool).await.is_err());
    for (table, id, _) in &cases {
        assert_eq!(etag(&pool, table, *id).await.as_deref(), Some("unchanged"));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "_sqlx_migrations" WHERE "version"=?"#
        )
        .bind(REPAIR_VERSION)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    pool.execute("DROP TRIGGER reject_scan_repair")
        .await
        .unwrap();
    all.run(&pool).await.unwrap();
    for (table, id, affected) in &cases {
        assert_eq!(
            etag(&pool, table, *id).await.as_deref(),
            (!affected).then_some("unchanged")
        );
    }
}

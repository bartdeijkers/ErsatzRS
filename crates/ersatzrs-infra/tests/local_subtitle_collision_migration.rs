mod common;

use ersatzrs_infra::db::assert_sql_safe;
use sqlx::{sqlite::SqlitePoolOptions, Executor, SqlitePool};

const REPAIR_VERSION: i64 = 202_609_241_530;

async fn seed_item(pool: &SqlitePool, kind: &str, id: i64, case: i64) {
    common::seed_media_item(pool, id, 1).await;
    // Identifiers are restricted to the four local video owners below; IDs are integers.
    let subtype = match kind {
        "Episode" => format!(r#"INSERT INTO "Episode" ("Id","SeasonId") VALUES ({id},901)"#),
        "MusicVideo" => format!(r#"INSERT INTO "MusicVideo" ("Id","ArtistId") VALUES ({id},902)"#),
        "Movie" | "OtherVideo" => format!(r#"INSERT INTO "{kind}" ("Id") VALUES ({id})"#),
        _ => panic!("unsupported fixture kind"),
    };
    sqlx::raw_sql(assert_sql_safe(subtype))
        .execute(pool)
        .await
        .unwrap();
    sqlx::raw_sql(assert_sql_safe(format!(r#"
        INSERT INTO "{kind}Metadata" ("Id","{kind}Id","MetadataKind","DateAdded","DateUpdated")
            VALUES ({id},{id},1,'2026-09-24','2026-09-24');
        INSERT INTO "LibraryFolder" ("Id","LibraryPathId","Etag") VALUES ({id},1,'unchanged');
        INSERT INTO "MediaVersion" ("Id","{kind}Id","DateAdded","DateUpdated","Duration","Width","Height","VideoScanKind")
            VALUES ({id},{id},'2026-09-24','2026-09-24','00:01:00',320,180,1);
        INSERT INTO "MediaFile" ("MediaVersionId","LibraryFolderId","PathHash") VALUES ({id},{id},'fixture-{id}');
    "#))).execute(pool).await.unwrap();
    let paths = match case {
        0 => vec![None],
        1 => vec![Some("")],
        2 => vec![Some("cached.srt"), Some("duplicate.srt")],
        _ => vec![Some("valid.srt")],
    };
    for path in paths {
        sqlx::query(assert_sql_safe(format!(r#"INSERT INTO "Subtitle" ("{kind}MetadataId","SubtitleKind","StreamIndex","Path","Language","Codec","Default","Forced","SDH","IsExtracted","DateAdded","DateUpdated")
            VALUES (?, ?, 0, ?, 'eng', 'subrip', 0, 0, 0, 1, '2026-09-24','2026-09-24')"#)))
            .bind(id).bind(i32::from(case != 2)).bind(path)
            .execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn local_subtitle_collision_migration_invalidates_only_affected_folders_once() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < REPAIR_VERSION)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    common::seed_media_graph(&pool).await;
    for id in [900, 901, 902] {
        common::seed_media_item(&pool, id, 1).await;
    }
    pool.execute(
        r#"INSERT INTO "Show" ("Id") VALUES (900);
        INSERT INTO "Season" ("Id","ShowId") VALUES (901,900);
        INSERT INTO "Artist" ("Id") VALUES (902);"#,
    )
    .await
    .unwrap();
    for (offset, kind) in ["Movie", "Episode", "MusicVideo", "OtherVideo"]
        .iter()
        .enumerate()
    {
        for case in 0..4 {
            let id = i64::try_from(offset).unwrap() * 4 + case + 10;
            seed_item(&pool, kind, id, case).await;
        }
    }
    let before: Vec<(i64, Option<String>, i64)> =
        sqlx::query_as(r#"SELECT "Id","Path","IsExtracted" FROM "Subtitle" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    migrations.run(&pool).await.unwrap();
    let folders: Vec<(i64, Option<String>)> =
        sqlx::query_as(r#"SELECT "Id","Etag" FROM "LibraryFolder" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(folders.len(), 16);
    for (id, etag) in folders {
        assert_eq!(etag.as_deref(), ((id - 10) % 4 == 3).then_some("unchanged"));
    }
    let after: Vec<(i64, Option<String>, i64)> =
        sqlx::query_as(r#"SELECT "Id","Path","IsExtracted" FROM "Subtitle" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "invalidation preserves subtitle rows and cached extractions"
    );
    pool.execute(r#"UPDATE "LibraryFolder" SET "Etag" = 'rescanned'"#)
        .await
        .unwrap();
    migrations.run(&pool).await.unwrap();
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryFolder" WHERE "Etag" = 'rescanned'"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 16, "a restart must not repeat the data migration");
}

#[tokio::test]
async fn local_subtitle_collision_migration_accepts_a_fresh_database() {
    let pool = common::test_db().await;
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "LibraryFolder""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

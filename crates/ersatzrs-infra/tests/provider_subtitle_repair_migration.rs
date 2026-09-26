mod common;

use ersatzrs_infra::db::assert_sql_safe;
use sqlx::{sqlite::SqlitePoolOptions, Executor};

const REPAIR_VERSION: i64 = 202_609_260_808;

#[tokio::test]
async fn provider_subtitle_repair_targets_external_streams_and_runs_once() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|m| m.version < REPAIR_VERSION)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    common::seed_media_graph(&pool).await;
    common::seed_media_item(&pool, 900, 1).await;
    common::seed_media_item(&pool, 901, 1).await;
    pool.execute(
        r#"INSERT INTO "Show" ("Id") VALUES (900);
        INSERT INTO "Season" ("Id","ShowId") VALUES (901,900);"#,
    )
    .await
    .unwrap();
    let mut items = Vec::new();
    let mut id = 10_i64;
    for provider in ["Jellyfin", "Emby"] {
        for kind in ["Movie", "Episode"] {
            // External, embedded-only and no-stream items; none needs a surviving
            // Subtitle row for the migration to discover the lost provider track.
            for stream_kind in [Some(5), Some(3), None] {
                common::seed_media_item(&pool, id, 1).await;
                let subtype = if kind == "Episode" {
                    format!(r#"INSERT INTO "Episode" ("Id","SeasonId") VALUES ({id},901);"#)
                } else {
                    format!(r#"INSERT INTO "Movie" ("Id") VALUES ({id});"#)
                };
                pool.execute(assert_sql_safe(subtype)).await.unwrap();
                pool.execute(assert_sql_safe(format!(r#"
                    INSERT INTO "{provider}{kind}" ("Id","Etag") VALUES ({id},'unchanged');
                    INSERT INTO "{kind}Metadata" ("Id","{kind}Id","MetadataKind","DateAdded","DateUpdated")
                        VALUES ({id},{id},1,'2026-09-26','2026-09-26');
                    INSERT INTO "MediaVersion" ("Id","{kind}Id","DateAdded","DateUpdated","Duration","Width","Height","VideoScanKind")
                        VALUES ({id},{id},'2026-09-26','2026-09-26','00:01:00',320,180,1);
                    INSERT INTO "Subtitle" ("{kind}MetadataId","SubtitleKind","StreamIndex","Path","Default","Forced","IsExtracted","DateAdded","DateUpdated")
                        VALUES ({id},99,0,'cached.ass',0,0,1,'2026-09-26','2026-09-26');
                "#))).await.unwrap();
                if let Some(stream_kind) = stream_kind {
                    sqlx::query(r#"INSERT INTO "MediaStream" ("Index","MediaStreamKind","Channels","Default","Forced","MediaVersionId") VALUES (2,?,0,0,0,?)"#)
                        .bind(stream_kind).bind(id).execute(&pool).await.unwrap();
                }
                items.push((format!("{provider}{kind}"), id, stream_kind == Some(5)));
                id += 1;
            }
        }
    }
    let before: Vec<(i64, Option<String>, i64)> =
        sqlx::query_as(r#"SELECT "Id","Path","IsExtracted" FROM "Subtitle" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    migrations.run(&pool).await.unwrap();
    for (table, id, affected) in &items {
        let etag: Option<String> = sqlx::query_scalar(assert_sql_safe(format!(
            r#"SELECT "Etag" FROM "{table}" WHERE "Id" = ?"#,
        )))
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(etag.as_deref(), (!affected).then_some("unchanged"));
    }
    let after: Vec<(i64, Option<String>, i64)> =
        sqlx::query_as(r#"SELECT "Id","Path","IsExtracted" FROM "Subtitle" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(before, after, "repair preserves cached subtitle rows");
    for (table, id, _) in &items {
        sqlx::query(assert_sql_safe(format!(
            r#"UPDATE "{table}" SET "Etag" = 'rescanned' WHERE "Id" = ?"#
        )))
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    migrations.run(&pool).await.unwrap();
    for (table, id, _) in &items {
        let etag: String = sqlx::query_scalar(assert_sql_safe(format!(
            r#"SELECT "Etag" FROM "{table}" WHERE "Id" = ?"#
        )))
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(etag, "rescanned", "restart must not repeat repair");
    }
}

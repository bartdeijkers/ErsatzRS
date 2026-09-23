mod common;

use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
use ersatzrs_infra::artwork_cleanup::{clean_orphaned_artwork, OrphanedArtworkCleanupReport};
use ersatzrs_infra::db::assert_sql_safe;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, SqlitePool};

const OWNERS: [&str; 10] = [
    "Artist",
    "Episode",
    "Image",
    "Movie",
    "MusicVideo",
    "OtherVideo",
    "RemoteStream",
    "Season",
    "Show",
    "Song",
];

async fn artwork(pool: &SqlitePool, id: i64) {
    sqlx::query(
        r#"INSERT INTO "Artwork" ("Id", "ArtworkKind", "Path", "DateAdded", "DateUpdated")
           VALUES (?, 0, 'synthetic.png', '2026-01-01', '2026-01-01')"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(assert_sql_safe(format!(
        r#"SELECT COUNT(*) FROM "{table}""#
    )))
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn artwork_cleanup_preserves_every_live_owner_channel_logo_and_actor_image() {
    let pool = common::test_db().await;
    common::seed_media_graph(&pool).await;
    // These synthetic subtypes share a MediaItem identity only to keep the
    // foreign-key-valid ownership fixture small; every owner FK is exercised.
    for owner in [
        "Artist",
        "Show",
        "Image",
        "Movie",
        "OtherVideo",
        "RemoteStream",
        "Song",
    ] {
        pool.execute(assert_sql_safe(format!(
            r#"INSERT INTO "{owner}" ("Id") VALUES (1)"#
        )))
        .await
        .unwrap();
    }
    pool.execute(
        r#"INSERT INTO "Season" ("Id", "ShowId") VALUES (1, 1);
        INSERT INTO "Episode" ("Id", "SeasonId") VALUES (1, 1);
        INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (1, 1);"#,
    )
    .await
    .unwrap();
    for (index, owner) in OWNERS.iter().enumerate() {
        pool.execute(assert_sql_safe(format!(
            r#"INSERT INTO "{owner}Metadata" ("Id", "{owner}Id", "MetadataKind", "DateAdded", "DateUpdated")
               VALUES (1, 1, 0, '2026-01-01', '2026-01-01')"#,
        ))).await.unwrap();
        let id = i64::try_from(index).unwrap() + 1;
        artwork(&pool, id).await;
        artwork(&pool, 100 + id).await;
        sqlx::query(assert_sql_safe(format!(
            r#"UPDATE "Artwork" SET "{owner}MetadataId" = 1 WHERE "Id" = ?"#,
        )))
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(assert_sql_safe(format!(
            r#"INSERT INTO "Actor" ("Id", "ArtworkId", "{owner}MetadataId") VALUES (?, ?, 1)"#,
        )))
        .bind(id)
        .bind(100 + id)
        .execute(&pool)
        .await
        .unwrap();
    }
    let repos = make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&common::profile(1, "synthetic"))
        .await
        .unwrap();
    repos
        .channel
        .upsert(&common::channel(1, "1", true))
        .await
        .unwrap();
    artwork(&pool, 500).await;
    pool.execute(r#"UPDATE "Artwork" SET "ChannelId" = 1 WHERE "Id" = 500"#)
        .await
        .unwrap();
    artwork(&pool, 900).await;
    artwork(&pool, 901).await;
    pool.execute(r#"INSERT INTO "Actor" ("Id", "ArtworkId") VALUES (900, 900)"#)
        .await
        .unwrap();

    assert_eq!(
        clean_orphaned_artwork(&pool, 5000).await.unwrap(),
        OrphanedArtworkCleanupReport {
            actors_deleted: 1,
            artwork_deleted: 2,
        }
    );
    assert_eq!(count(&pool, "Actor").await, 10);
    assert_eq!(count(&pool, "Artwork").await, 21);
    let ids: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "Artwork" ORDER BY "Id""#)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        ids,
        (1..=10).chain(101..=110).chain([500]).collect::<Vec<_>>()
    );
    assert_eq!(
        clean_orphaned_artwork(&pool, 5000).await.unwrap(),
        OrphanedArtworkCleanupReport::default()
    );
}

async fn seed_orphans(pool: &SqlitePool, amount: i64) {
    sqlx::query(
        r#"WITH RECURSIVE "Numbers"("Id") AS (
        SELECT 1 UNION ALL SELECT "Id" + 1 FROM "Numbers" WHERE "Id" < ?
    ) INSERT INTO "Artwork" ("Id", "ArtworkKind", "DateAdded", "DateUpdated")
      SELECT "Id", 0, '2026-01-01', '2026-01-01' FROM "Numbers""#,
    )
    .bind(amount)
    .execute(pool)
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "Actor" ("Id", "ArtworkId") SELECT "Id", "Id" FROM "Artwork""#)
        .await
        .unwrap();
}

#[tokio::test]
async fn artwork_cleanup_retains_upstream_batch_limit_and_repeated_pass_semantics() {
    let pool = common::test_db().await;
    seed_orphans(&pool, 5001).await;
    for limit in [0, -1, i32::MIN] {
        assert_eq!(
            clean_orphaned_artwork(&pool, limit).await.unwrap(),
            OrphanedArtworkCleanupReport::default()
        );
    }
    assert_eq!(count(&pool, "Actor").await, 5001);
    assert_eq!(count(&pool, "Artwork").await, 5001);
    assert_eq!(
        clean_orphaned_artwork(&pool, 1).await.unwrap(),
        OrphanedArtworkCleanupReport {
            actors_deleted: 5000,
            artwork_deleted: 5000,
        }
    );
    assert_eq!(count(&pool, "Actor").await, 1);
    assert_eq!(count(&pool, "Artwork").await, 1);
    assert_eq!(
        clean_orphaned_artwork(&pool, 5000).await.unwrap(),
        OrphanedArtworkCleanupReport {
            actors_deleted: 1,
            artwork_deleted: 1,
        }
    );
    assert_eq!(
        clean_orphaned_artwork(&pool, i32::MAX).await.unwrap(),
        OrphanedArtworkCleanupReport::default()
    );
}

#[tokio::test]
async fn artwork_cleanup_rolls_back_failed_actor_batch_before_touching_artwork() {
    let pool = common::test_db().await;
    seed_orphans(&pool, 201).await;
    pool.execute(
        r#"CREATE TRIGGER "FailActorCleanup" BEFORE DELETE ON "Actor"
        WHEN OLD."Id" = 101 BEGIN SELECT RAISE(ABORT, 'synthetic cleanup fault'); END"#,
    )
    .await
    .unwrap();
    assert!(clean_orphaned_artwork(&pool, 5000).await.is_err());
    assert_eq!(count(&pool, "Actor").await, 201);
    assert_eq!(count(&pool, "Artwork").await, 201);
    pool.execute(r#"DROP TRIGGER "FailActorCleanup""#)
        .await
        .unwrap();
    assert_eq!(
        clean_orphaned_artwork(&pool, 5000).await.unwrap(),
        OrphanedArtworkCleanupReport {
            actors_deleted: 201,
            artwork_deleted: 201,
        }
    );
}

#[tokio::test]
async fn artwork_cleanup_rolls_back_failed_artwork_batch_and_can_resume() {
    let pool = common::test_db().await;
    seed_orphans(&pool, 201).await;
    pool.execute(
        r#"CREATE TRIGGER "FailArtworkCleanup" BEFORE DELETE ON "Artwork"
        WHEN OLD."Id" = 101 BEGIN SELECT RAISE(ABORT, 'synthetic cleanup fault'); END"#,
    )
    .await
    .unwrap();
    assert!(clean_orphaned_artwork(&pool, 5000).await.is_err());
    assert_eq!(count(&pool, "Actor").await, 0);
    assert_eq!(count(&pool, "Artwork").await, 201);
    pool.execute(r#"DROP TRIGGER "FailArtworkCleanup""#)
        .await
        .unwrap();
    assert_eq!(
        clean_orphaned_artwork(&pool, 5000).await.unwrap(),
        OrphanedArtworkCleanupReport {
            actors_deleted: 0,
            artwork_deleted: 201,
        }
    );
}

#[tokio::test]
async fn artwork_cleanup_continues_past_one_batch_when_limit_allows() {
    let pool = common::test_db().await;
    seed_orphans(&pool, 5001).await;
    assert_eq!(
        clean_orphaned_artwork(&pool, 5001).await.unwrap(),
        OrphanedArtworkCleanupReport {
            actors_deleted: 5001,
            artwork_deleted: 5001,
        }
    );
    assert_eq!(count(&pool, "Actor").await, 0);
    assert_eq!(count(&pool, "Artwork").await, 0);
}

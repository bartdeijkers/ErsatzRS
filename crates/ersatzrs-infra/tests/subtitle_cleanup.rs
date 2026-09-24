mod common;

use ersatzrs_infra::{db::assert_sql_safe, subtitle_cleanup::clean_orphaned_subtitles};
use sqlx::{Executor, SqlitePool};

async fn subtitle(pool: &SqlitePool, id: i64, kind: i64, path: Option<&str>) {
    sqlx::query(r#"INSERT INTO "Subtitle" ("Id","SubtitleKind","StreamIndex","Path","Default","Forced","DateAdded","DateUpdated")
        VALUES (?, ?, 0, ?, 0, 0, '2026-09-24', '2026-09-24')"#)
        .bind(id).bind(kind).bind(path).execute(pool).await.unwrap();
}

#[tokio::test]
async fn subtitle_cleanup_preserves_every_owner_and_removes_only_invalid_rows() {
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

    for (index, owner) in [
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
    ]
    .iter()
    .enumerate()
    {
        pool.execute(assert_sql_safe(format!(r#"INSERT INTO "{owner}Metadata" ("Id","{owner}Id","MetadataKind","DateAdded","DateUpdated")
            VALUES (1,1,0,'2026-09-24','2026-09-24')"#))).await.unwrap();
        let base = i64::try_from(index).unwrap() * 10;
        // Embedded and generated rows can lack paths; valid sidecars must survive.
        for (offset, kind, path) in [
            (1, 0, None),
            (2, 99, None),
            (3, 1, Some("valid.srt")),
            (4, 1, None),
            (5, 1, Some("")),
        ] {
            subtitle(&pool, base + offset, kind, path).await;
            sqlx::query(assert_sql_safe(format!(
                r#"UPDATE "Subtitle" SET "{owner}MetadataId" = 1 WHERE "Id" = ?"#
            )))
            .bind(base + offset)
            .execute(&pool)
            .await
            .unwrap();
        }
    }
    subtitle(&pool, 1001, 0, Some("cache.srt")).await;
    subtitle(&pool, 1002, 99, None).await;
    subtitle(&pool, 1003, 1, None).await;
    assert_eq!(clean_orphaned_subtitles(&pool, 100_000).await.unwrap(), 23);
    let ids: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "Subtitle" ORDER BY "Id""#)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        ids,
        (0..10)
            .flat_map(|base| (1..=3).map(move |offset| base * 10 + offset))
            .collect::<Vec<_>>()
    );
    assert_eq!(clean_orphaned_subtitles(&pool, 100_000).await.unwrap(), 0);
}

#[tokio::test]
async fn subtitle_cleanup_is_bounded_and_rolls_back_failed_batch() {
    let pool = common::test_db().await;
    for id in 1..=3 {
        subtitle(&pool, id, 0, None).await;
    }
    assert_eq!(clean_orphaned_subtitles(&pool, 0).await.unwrap(), 0);
    pool.execute(
        r#"CREATE TRIGGER "FailSubtitleCleanup" BEFORE DELETE ON "Subtitle" WHEN OLD."Id" = 2
        BEGIN SELECT RAISE(ABORT, 'synthetic failure'); END"#,
    )
    .await
    .unwrap();
    assert!(clean_orphaned_subtitles(&pool, 100_000).await.is_err());
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Subtitle""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3, "failed statement must restore earlier deletions");
    pool.execute(r#"DROP TRIGGER "FailSubtitleCleanup""#)
        .await
        .unwrap();
    assert_eq!(clean_orphaned_subtitles(&pool, 2).await.unwrap(), 2);
    assert_eq!(clean_orphaned_subtitles(&pool, 2).await.unwrap(), 1);
    assert_eq!(clean_orphaned_subtitles(&pool, 2).await.unwrap(), 0);
}

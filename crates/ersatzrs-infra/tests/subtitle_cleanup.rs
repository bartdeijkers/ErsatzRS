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
    assert_eq!(clean_orphaned_subtitles(&pool, 100_000).await.unwrap(), 11);
    let ids: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "Subtitle" ORDER BY "Id""#)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(
        ids,
        (0..10)
            .flat_map(|base| {
                let last = if [1, 3, 4, 5].contains(&base) { 3 } else { 5 };
                (1..=last).map(move |offset| base * 10 + offset)
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(clean_orphaned_subtitles(&pool, 100_000).await.unwrap(), 0);
}

#[tokio::test]
async fn subtitle_cleanup_preserves_provider_sidecars_without_paths() {
    let pool = common::test_db().await;
    pool.execute(
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId")
            VALUES (1,'provider','Movies',1);
        INSERT INTO "JellyfinLibrary" ("Id","ShouldSyncItems") VALUES (1,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (1,1);
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (1,1,0);
        INSERT INTO "Movie" ("Id") VALUES (1);
        INSERT INTO "JellyfinMovie" ("Id","ItemId") VALUES (1,'synthetic-movie');
        INSERT INTO "MovieMetadata" ("Id","MovieId","MetadataKind","DateAdded","DateUpdated")
            VALUES (1,1,0,'2026-09-26','2026-09-26');"#,
    )
    .await
    .unwrap();
    for (id, path) in [(1, None), (2, Some(""))] {
        subtitle(&pool, id, 1, path).await;
        sqlx::query(r#"UPDATE "Subtitle" SET "MovieMetadataId" = 1, "StreamIndex" = ?, "IsExtracted" = 1 WHERE "Id" = ?"#)
            .bind(id).bind(id).execute(&pool).await.unwrap();
    }
    for _ in 0..2 {
        assert_eq!(clean_orphaned_subtitles(&pool, 100_000).await.unwrap(), 0);
        let rows: Vec<(i64, Option<String>, i64)> = sqlx::query_as(
            r#"SELECT "StreamIndex","Path","IsExtracted" FROM "Subtitle" ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows, vec![(1, None, 1), (2, Some(String::new()), 1)]);
    }
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

use super::{load_content_cache, ContentCache};
use crate::xmltv::load_programme_content_in_transaction;
use crate::{
    db::DbPool,
    xmltv_read::{GuideReadBudget, GuideReadLimits, GuideReadUsage},
};
use sqlx::Executor;

async fn database() -> (tempfile::TempDir, DbPool) {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite:{}", root.path().join("metadata.db").display());
    let pool = crate::db::init(&url).await.unwrap();
    for sql in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (10)"#,
        r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (10)"#,
        r#"INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (10,1,10,'Synthetic')"#,
        r#"INSERT INTO "LocalLibrary" ("Id") VALUES (10)"#,
        r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (10,10,'media')"#,
    ] {
        pool.execute(sql).await.unwrap();
    }
    (root, pool)
}

async fn rich_movie(pool: &DbPool) {
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (20,10)"#,
        r#"INSERT INTO "Movie" ("Id") VALUES (20)"#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","DateAdded","DateUpdated","MetadataKind","Title","Plot","ContentRating") VALUES (30,20,'2026-01-01','2026-01-01',0,'T','é','PG')"#,
        r#"INSERT INTO "Genre" ("MovieMetadataId","Name") VALUES (30,'Drama'),(30,'Drama')"#,
        r#"INSERT INTO "Artwork" ("Id","ArtworkKind","MovieMetadataId","Path","DateAdded","DateUpdated") VALUES (40,0,30,'art/é.jpg','2026-01-01','2026-01-01')"#,
        r#"INSERT INTO "MetadataGuid" ("MovieMetadataId","Guid") VALUES (30,'g')"#,
    ] {
        pool.execute(sql).await.unwrap();
    }
}

fn budget(records: usize, source_bytes: usize) -> GuideReadBudget {
    GuideReadBudget::new(GuideReadLimits {
        max_records: records,
        max_source_bytes: source_bytes,
    })
}

async fn read(
    pool: &DbPool,
    ids: &[i64],
    limits: &mut GuideReadBudget,
) -> Result<ContentCache, sqlx::Error> {
    // Read-only transaction: the metadata reader performs SELECTs only.
    let mut transaction = pool.begin().await.unwrap();
    load_programme_content_in_transaction(&mut transaction, ids, limits).await
}

#[tokio::test]
async fn exact_utf8_payloads_and_duplicate_hydration_rows_match_legacy_content() {
    let (_root, pool) = database().await;
    rich_movie(&pool).await;
    let legacy = load_content_cache(&pool, &[20]).await.unwrap();
    // UTF-8 values plus selected scalar representations: base=9, genres=14, artwork=15, GUID=3.
    let mut exact = budget(5, 41);
    let actual = read(&pool, &[20], &mut exact).await.unwrap();
    assert_eq!(actual, legacy);
    assert_eq!(
        exact.usage(),
        GuideReadUsage {
            records: 5,
            source_bytes: 41
        }
    );
    let (_, content) = actual.get(&20).unwrap();
    assert_eq!(content.description.as_deref(), Some("é"));
    assert_eq!(content.genres, ["Drama"]);
    assert_eq!(content.icon_artwork.as_ref().unwrap().path, "art/é.jpg");
    for (records, bytes) in [(4, 41), (5, 40), (1, 8)] {
        let mut limited = budget(records, bytes);
        assert!(read(&pool, &[20], &mut limited).await.is_err());
        assert!(limited.usage().records <= records);
        assert!(limited.usage().source_bytes <= bytes);
    }
}

#[tokio::test]
async fn four_hundred_and_first_record_charges_the_next_binding_batch() {
    let (_root, pool) = database().await;
    for sql in [
        r#"WITH RECURSIVE ids(n) AS (SELECT 20 UNION ALL SELECT n+1 FROM ids WHERE n<420) INSERT INTO "MediaItem" ("Id","LibraryPathId") SELECT n,10 FROM ids"#,
        r#"INSERT INTO "Movie" ("Id") SELECT "Id" FROM "MediaItem""#,
        r#"INSERT INTO "MovieMetadata" ("Id","MovieId","DateAdded","DateUpdated","MetadataKind","Title") SELECT "Id","Id",'2026-01-01','2026-01-01',0,'T' FROM "Movie""#,
    ] {
        pool.execute(sql).await.unwrap();
    }
    let ids: Vec<i64> = (20..=420).collect();
    let mut first_batch = budget(400, 2640);
    assert_eq!(
        read(&pool, &ids[..400], &mut first_batch)
            .await
            .unwrap()
            .len(),
        400
    );
    assert_eq!(
        first_batch.usage(),
        GuideReadUsage {
            records: 400,
            source_bytes: 2640
        }
    );
    let mut identifiers = budget(400, 2647);
    assert!(read(&pool, &ids, &mut identifiers).await.is_err());
    assert_eq!(
        identifiers.usage(),
        GuideReadUsage {
            records: 0,
            source_bytes: 0
        }
    );
    let mut insufficient = budget(401, 2640);
    assert!(read(&pool, &ids, &mut insufficient).await.is_err());
    assert_eq!(
        insufficient.usage(),
        GuideReadUsage {
            records: 400,
            source_bytes: 2640
        }
    );
    let mut exact = budget(401, 2647);
    let actual = read(&pool, &ids, &mut exact).await.unwrap();
    assert_eq!(actual.len(), 401);
    assert_eq!(actual, load_content_cache(&pool, &ids).await.unwrap());
    assert_eq!(
        exact.usage(),
        GuideReadUsage {
            records: 401,
            source_bytes: 2647
        }
    );
    // A duplicate metadata row is charged although projection keeps the first.
    pool.execute(r#"INSERT INTO "MovieMetadata" ("Id","MovieId","DateAdded","DateUpdated","MetadataKind","Title") VALUES (421,20,'2026-01-01','2026-01-01',0,'T')"#)
        .await.unwrap();
    let mut row_limited = budget(401, 2653);
    assert!(read(&pool, &ids, &mut row_limited).await.is_err());
    assert_eq!(
        row_limited.usage(),
        GuideReadUsage {
            records: 401,
            source_bytes: 2646
        }
    );
    let mut all_rows = budget(402, 2653);
    let deduplicated = read(&pool, &ids, &mut all_rows).await.unwrap();
    assert_eq!(deduplicated, actual);
    assert_eq!(
        all_rows.usage(),
        GuideReadUsage {
            records: 402,
            source_bytes: 2653
        }
    );
}

#[tokio::test]
async fn subtype_json_and_studio_queries_share_the_movie_hydration_budget() {
    let (_root, pool) = database().await;
    rich_movie(&pool).await;
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (21,10)"#,
        r#"INSERT INTO "Song" ("Id") VALUES (21)"#,
        r#"INSERT INTO "SongMetadata" ("Id","SongId","DateAdded","DateUpdated","MetadataKind","Title","Artists","AlbumArtists","Track","Comment","Album","ReleaseDate") VALUES (31,21,'2026-01-01','2026-01-01',0,'S','["é"]','[]','01','c','é','2026')"#,
        r#"INSERT INTO "Studio" ("SongMetadataId","Name") VALUES (31,'Label')"#,
    ] {
        pool.execute(sql).await.unwrap();
    }
    // Movie=41 bytes; song JSON/text and scalar fields=22; studio=7.
    let mut exact = budget(7, 70);
    let actual = read(&pool, &[20, 21], &mut exact).await.unwrap();
    assert_eq!(actual, load_content_cache(&pool, &[20, 21]).await.unwrap());
    assert_eq!(
        exact.usage(),
        GuideReadUsage {
            records: 7,
            source_bytes: 70
        }
    );
    let song = &actual.get(&21).unwrap().1;
    assert_eq!(song.artists, ["é"]);
    assert_eq!(song.studios, ["Label"]);
    assert_eq!(song.track.as_deref(), Some("01"));
    for (records, bytes) in [(6, 70), (7, 62)] {
        let mut limited = budget(records, bytes);
        assert!(read(&pool, &[20, 21], &mut limited).await.is_err());
        assert!(limited.usage().records <= records);
        assert!(limited.usage().source_bytes <= bytes);
    }
}

#[tokio::test]
async fn separate_cache_loads_share_one_remaining_budget() {
    let (_root, pool) = database().await;
    rich_movie(&pool).await;
    let mut aggregate = budget(10, 82);
    let first = read(&pool, &[20], &mut aggregate).await.unwrap();
    let second = read(&pool, &[20], &mut aggregate).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(
        aggregate.usage(),
        GuideReadUsage {
            records: 10,
            source_bytes: 82
        }
    );
    assert!(read(&pool, &[20], &mut aggregate).await.is_err());
    assert_eq!(
        aggregate.usage(),
        GuideReadUsage {
            records: 10,
            source_bytes: 82
        }
    );
}

#[tokio::test]
async fn corrupt_numeric_affinity_payload_is_bounded_before_typed_hydration() {
    let (_root, pool) = database().await;
    rich_movie(&pool).await;
    sqlx::query(r#"UPDATE "MovieMetadata" SET "Year"=? WHERE "MovieId"=20"#)
        .bind("x".repeat(4096))
        .execute(&pool)
        .await
        .unwrap();
    let mut limited = budget(5, 40);
    let error = read(&pool, &[20], &mut limited).await.unwrap_err();
    let sqlx::Error::Decode(source) = error else {
        panic!("expected source budget rejection")
    };
    assert_eq!(
        source.to_string(),
        "guide source source bytes count 4105 exceeds limit 40"
    );
    assert_eq!(
        limited.usage(),
        GuideReadUsage {
            records: 0,
            source_bytes: 0
        }
    );
}

async fn publish_metadata_change(pool: &DbPool) {
    pool.execute(r#"UPDATE "MovieMetadata" SET "Plot"='changed' WHERE "MovieId"=20"#)
        .await
        .unwrap();
    pool.execute(r#"UPDATE "Genre" SET "Name"='New' WHERE "MovieMetadataId"=30"#)
        .await
        .unwrap();
}

#[tokio::test]
async fn borrowed_wal_transaction_pins_metadata_across_committed_pool_updates() {
    let (_root, pool) = database().await;
    rich_movie(&pool).await;
    let legacy = load_content_cache(&pool, &[20]).await.unwrap();
    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(journal.to_lowercase(), "wal");
    // Read-only transaction: the concurrent writer uses a separate pooled connection.
    let mut transaction = pool.begin().await.unwrap();
    let mut aggregate = budget(10, 82);
    assert_eq!(
        load_programme_content_in_transaction(&mut transaction, &[20], &mut aggregate)
            .await
            .unwrap(),
        legacy
    );
    publish_metadata_change(&pool).await;
    assert_eq!(
        load_programme_content_in_transaction(&mut transaction, &[20], &mut aggregate)
            .await
            .unwrap(),
        legacy
    );
    assert_eq!(
        aggregate.usage(),
        GuideReadUsage {
            records: 10,
            source_bytes: 82
        }
    );
    transaction.rollback().await.unwrap();
    let current = load_content_cache(&pool, &[20]).await.unwrap();
    assert_eq!(
        current.get(&20).unwrap().1.description.as_deref(),
        Some("changed")
    );
    assert_eq!(current.get(&20).unwrap().1.genres, ["New"]);
}

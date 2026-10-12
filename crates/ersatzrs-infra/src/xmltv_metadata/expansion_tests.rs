use super::{
    bounded_read_tests::{database, read},
    load_content_cache,
};
use crate::{
    db::DbPool,
    xmltv_read::{GuideExpansionLimits, GuideExpansionUsage, GuideReadBudget, GuideReadLimits},
};
use ersatzrs_core::{
    repositories::ArtworkReference,
    xmltv::{IconSizeHint, XmltvProgrammeContent},
};
use sqlx::Executor;
use std::mem::size_of;

fn cache_slot() -> usize {
    size_of::<(i64, (Option<String>, XmltvProgrammeContent))>()
}
fn budget(nodes: usize, bytes: usize) -> GuideReadBudget {
    GuideReadBudget::new_with_expansion(
        GuideReadLimits {
            max_records: 100,
            max_source_bytes: 64 * 1024,
        },
        GuideExpansionLimits {
            max_nodes: nodes,
            max_bytes: bytes,
        },
    )
}

async fn song(pool: &DbPool, artists: &str) {
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (20,10)"#,
        r#"INSERT INTO "Song" ("Id") VALUES (20)"#,
    ] {
        pool.execute(sql).await.unwrap();
    }
    sqlx::query(r#"INSERT INTO "SongMetadata" ("Id","SongId","DateAdded","DateUpdated","MetadataKind","Title","Artists") VALUES (30,20,'2026-01-01','2026-01-01',0,'S',?)"#)
        .bind(artists).execute(pool).await.unwrap();
}

fn song_fixed_bytes() -> usize {
    cache_slot() + size_of::<(i64, i64)>() + 2 * size_of::<i64>()
}

#[tokio::test]
async fn empty_string_slots_are_bounded_before_decoding_even_when_retained_list_is_empty() {
    let (_root, pool) = database().await;
    let raw = serde_json::to_string(&vec![""; 5000]).unwrap();
    song(&pool, &raw).await;
    let legacy = load_content_cache(&pool, &[20]).await.unwrap();
    assert!(legacy.get(&20).unwrap().1.artists.is_empty());
    let mut limited = budget(100, usize::MAX);
    let error = read(&pool, &[20], &mut limited).await.unwrap_err();
    assert!(error
        .to_string()
        .contains("expansion nodes count 5001 exceeds limit 100"));
    assert_eq!(
        limited.expansion_usage(),
        Some(GuideExpansionUsage {
            nodes: 1,
            bytes: cache_slot()
        })
    );
    let mut exact = budget(5004, song_fixed_bytes() + 5000 * size_of::<String>());
    assert_eq!(read(&pool, &[20], &mut exact).await.unwrap(), legacy);
    assert_eq!(exact.expansion_usage().unwrap().nodes, 5004);
}

#[tokio::test]
async fn escaped_lists_copy_join_and_filtered_slots_have_inclusive_model_limits() {
    for (raw, title, payload_bytes) in [
        (r#"["","  ","\u00e9"]"#, "é", 8),
        (r#"["\u00e9","B"]"#, "é, B", 11),
    ] {
        let (_root, pool) = database().await;
        song(&pool, raw).await;
        let bytes = song_fixed_bytes() + 4 * size_of::<String>() + payload_bytes;
        let mut exact = budget(9, bytes);
        let result = read(&pool, &[20], &mut exact).await.unwrap();
        assert_eq!(result, load_content_cache(&pool, &[20]).await.unwrap());
        assert_eq!(result.get(&20).unwrap().0.as_deref(), Some(title));
        assert_eq!(
            exact.expansion_usage(),
            Some(GuideExpansionUsage { nodes: 9, bytes })
        );
        for (nodes, cap) in [(8, bytes), (9, bytes - 1)] {
            let mut limited = budget(nodes, cap);
            assert!(read(&pool, &[20], &mut limited).await.is_err());
            assert!(limited.expansion_usage().unwrap().nodes <= nodes);
            assert!(limited.expansion_usage().unwrap().bytes <= cap);
        }
    }
}

#[tokio::test]
async fn malformed_large_prefixes_remain_one_literal_without_allocating_a_parsed_prefix() {
    let prefix = serde_json::to_string(&vec![""; 5000]).unwrap();
    for raw in [
        format!("{prefix} trailing"),
        format!("{},null]", &prefix[..prefix.len() - 1]),
    ] {
        let (_root, pool) = database().await;
        song(&pool, &raw).await;
        let mut exact = budget(
            7,
            song_fixed_bytes() + 2 * size_of::<String>() + 2 * raw.len(),
        );
        let result = read(&pool, &[20], &mut exact).await.unwrap();
        assert_eq!(result, load_content_cache(&pool, &[20]).await.unwrap());
        assert_eq!(result.get(&20).unwrap().0.as_deref(), Some(raw.as_str()));
        assert_eq!(result.get(&20).unwrap().1.artists, [raw]);
        assert_eq!(exact.expansion_usage().unwrap().nodes, 7);
    }
}

#[tokio::test]
async fn missing_identifiers_still_reserve_cache_and_separate_calls_never_refund_model_cost() {
    let (_root, pool) = database().await;
    let mut shared = budget(2, 2 * cache_slot());
    for expected in [1, 2] {
        assert!(read(&pool, &[999], &mut shared).await.unwrap().is_empty());
        assert_eq!(
            shared.expansion_usage(),
            Some(GuideExpansionUsage {
                nodes: expected,
                bytes: expected * cache_slot()
            })
        );
        assert_eq!(shared.usage().records, 0);
    }
    assert!(read(&pool, &[999], &mut shared).await.is_err());
    let mut limited = budget(1, usize::MAX);
    assert!(read(&pool, &[998, 999], &mut limited).await.is_err());
    assert_eq!(limited.usage().records, 0);
    assert_eq!(
        limited.expansion_usage(),
        Some(GuideExpansionUsage { nodes: 0, bytes: 0 })
    );
}

async fn episodes(pool: &DbPool) {
    for sql in [
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (20,10),(21,10),(22,10),(23,10)"#,
        r#"INSERT INTO "Show" ("Id") VALUES (20)"#,
        r#"INSERT INTO "Season" ("Id","ShowId","SeasonNumber") VALUES (21,20,1)"#,
        r#"INSERT INTO "Episode" ("Id","SeasonId") VALUES (22,21),(23,21)"#,
        r#"INSERT INTO "ShowMetadata" ("Id","ShowId","DateAdded","DateUpdated","MetadataKind","Title") VALUES (30,20,'2026-01-01','2026-01-01',0,'T')"#,
        r#"INSERT INTO "EpisodeMetadata" ("Id","EpisodeId","DateAdded","DateUpdated","MetadataKind","Title") VALUES (31,22,'2026-01-01','2026-01-01',0,'E'),(32,23,'2026-01-01','2026-01-01',0,'F')"#,
    ] {
        pool.execute(sql).await.unwrap();
    }
    sqlx::query(r#"INSERT INTO "Genre" ("ShowMetadataId","Name") VALUES (30,?)"#)
        .bind("é".repeat(256))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "MetadataGuid" ("ShowMetadataId","Guid") VALUES (30,?)"#)
        .bind("g".repeat(257))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "Artwork" ("Id","ArtworkKind","ShowMetadataId","Path","DateAdded","DateUpdated") VALUES (40,0,30,?,'2026-01-01','2026-01-01')"#)
        .bind(format!("art/{}", "p".repeat(257))).execute(pool).await.unwrap();
}

#[tokio::test]
async fn shared_show_lists_and_artwork_are_charged_before_each_episode_copy() {
    let (_root, pool) = database().await;
    episodes(&pool).await;
    let fixed = 2 * cache_slot()
        + 4 * size_of::<(i64, i64)>()
        + 4 * size_of::<i64>()
        + 2 * size_of::<(i64, Vec<String>)>()
        + 2 * size_of::<String>()
        + size_of::<(i64, (ArtworkReference, IconSizeHint))>();
    let copied = 2 * size_of::<String>() + 512 + 257 + 261;
    let mut exact = budget(21, fixed + 2 * copied);
    let result = read(&pool, &[22, 23], &mut exact).await.unwrap();
    assert_eq!(result, load_content_cache(&pool, &[22, 23]).await.unwrap());
    assert_eq!(
        exact.expansion_usage(),
        Some(GuideExpansionUsage {
            nodes: 21,
            bytes: fixed + 2 * copied
        })
    );
    assert_eq!(
        result.get(&22).unwrap().1.genres,
        result.get(&23).unwrap().1.genres
    );
    let mut one_copy = budget(21, fixed + copied);
    let error = read(&pool, &[22, 23], &mut one_copy).await.unwrap_err();
    assert!(error.to_string().contains("expansion bytes"));
    assert_eq!(one_copy.usage().records, 5);
    assert_eq!(
        one_copy.expansion_usage(),
        Some(GuideExpansionUsage {
            nodes: 18,
            bytes: fixed + copied
        })
    );
}

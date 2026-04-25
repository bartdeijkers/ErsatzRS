#![cfg(not(feature = "mysql"))]
mod common;

use chrono::Duration;
use ersatzrs_core::repositories::BlockRepository;
use ersatzrs_domain::Block;
use ersatzrs_infra::repositories::make_repositories;
use sqlx::{Executor, Row};

#[tokio::test]
async fn block_repository_loads_items_and_lists_empty_navigation_fields() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.execute(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'group')"#)
        .await
        .unwrap();

    repos
        .block
        .upsert(&Block {
            id: 1,
            block_group_id: Some(1),
            name: "block".to_owned(),
            updated_ticks: 42,
            duration: Duration::minutes(30),
            item_ids: vec![20],
            ..Block::default()
        })
        .await
        .unwrap();

    assert_eq!(
        repos.block.get_with_items(1).await.unwrap().item_ids,
        vec![20]
    );
    let listed = repos.block.list_for_group(1).await.unwrap();
    assert_eq!(listed[0].id, 1);
    assert_eq!(listed[0].updated_ticks, 42);
    assert!(listed[0].item_ids.is_empty());
}

#[tokio::test]
async fn block_repository_preserves_existing_item_target_fields() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.execute(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'group')"#)
        .await
        .unwrap();

    repos
        .block
        .upsert(&Block {
            id: 1,
            block_group_id: Some(1),
            name: "block".to_owned(),
            updated_ticks: 42,
            duration: Duration::minutes(30),
            item_ids: Vec::new(),
            ..Block::default()
        })
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "BlockItem" (
            "Id", "Index", "BlockId", "CollectionType", "PlaybackOrder",
            "SearchQuery", "SearchTitle", "IncludeInProgramGuide", "DisableWatermarks"
         ) VALUES (20, 5, 1, 9, 2, 'genre:comedy', 'Comedy', 1, 1)"#,
    )
    .await
    .unwrap();

    repos
        .block
        .upsert(&Block {
            id: 1,
            block_group_id: Some(1),
            name: "block".to_owned(),
            updated_ticks: 43,
            duration: Duration::minutes(45),
            item_ids: vec![20],
            ..Block::default()
        })
        .await
        .unwrap();

    let row = sqlx::query(
        r#"SELECT "CollectionType", "PlaybackOrder", "Index", "SearchQuery", "SearchTitle",
                  "IncludeInProgramGuide", "DisableWatermarks"
           FROM "BlockItem"
           WHERE "Id" = 20"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(row.get::<i64, _>("CollectionType"), 9);
    assert_eq!(row.get::<i64, _>("PlaybackOrder"), 2);
    assert_eq!(row.get::<i64, _>("Index"), 0);
    assert_eq!(
        row.get::<Option<String>, _>("SearchQuery"),
        Some("genre:comedy".to_owned())
    );
    assert_eq!(
        row.get::<Option<String>, _>("SearchTitle"),
        Some("Comedy".to_owned())
    );
    assert_eq!(row.get::<i64, _>("IncludeInProgramGuide"), 1);
    assert_eq!(row.get::<i64, _>("DisableWatermarks"), 1);
}

mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{BlockAdminRow, BlockItemAdmin, BlockRepository};
use ersatzrs_core::scheduling::{SchedulingSmartShufflePolicy, SchedulingSmartShufflePreset};
use ersatzrs_domain::{Block, BlockItem, BlockStopScheduling, CollectionType, PlaybackOrder};
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
async fn block_repository_round_trips_typed_items_and_overlays() {
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
            ..Block::default()
        })
        .await
        .unwrap();
    seed_overlay_targets(&pool).await;

    repos
        .block
        .upsert_item(&BlockItem {
            id: 20,
            block_id: 1,
            collection_type: CollectionType::SearchQuery,
            search_query: Some("title:news".to_owned()),
            playback_order: PlaybackOrder::Shuffle,
            sort_order: 4,
            watermark_ids: vec![501],
            graphics_element_ids: vec![601],
            ..BlockItem::default()
        })
        .await
        .unwrap();

    let loaded = repos.block.get_item(20).await.unwrap();
    assert_eq!(loaded.collection_type, CollectionType::SearchQuery);
    assert_eq!(loaded.search_query, Some("title:news".to_owned()));
    assert_eq!(loaded.playback_order, PlaybackOrder::Shuffle);
    assert_eq!(loaded.sort_order, 4);
    assert_eq!(loaded.watermark_ids, vec![501]);
    assert_eq!(loaded.graphics_element_ids, vec![601]);

    let listed = repos.block.list_items(1).await.unwrap();
    assert_eq!(
        listed.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![20]
    );
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

#[tokio::test]
async fn block_admin_repository_round_trips_and_copies_complete_item_graph() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    let group_id = repos.block.create_group_admin("group").await.unwrap();
    let block_id = repos.block.create_admin(group_id, "block").await.unwrap();
    seed_overlay_targets(&pool).await;
    let policy = SchedulingSmartShufflePolicy::for_preset(SchedulingSmartShufflePreset::Balanced);

    repos
        .block
        .save_admin(
            &BlockAdminRow {
                id: block_id,
                group_id,
                group_name: "ignored".to_owned(),
                name: "block updated".to_owned(),
                minutes: 60,
                stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
            },
            &[BlockItemAdmin {
                id: 0,
                index: 0,
                collection_type: CollectionType::SearchQuery,
                collection_id: None,
                multi_collection_id: None,
                smart_collection_id: None,
                media_item_id: None,
                search_title: Some("News".to_owned()),
                search_query: Some("title:news".to_owned()),
                playback_order: PlaybackOrder::Shuffle,
                smart_shuffle: Some(policy.clone()),
                include_in_program_guide: true,
                disable_watermarks: true,
                collection_name: "News".to_owned(),
                watermark_ids: vec![501],
                graphics_element_ids: vec![601],
            }],
        )
        .await
        .unwrap();

    let loaded = repos.block.list_items_admin(block_id).await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].smart_shuffle, Some(policy.clone()));
    assert_eq!(loaded[0].watermark_ids, vec![501]);
    assert_eq!(loaded[0].graphics_element_ids, vec![601]);
    assert!(loaded[0].include_in_program_guide);
    assert!(loaded[0].disable_watermarks);

    let copy_id = repos
        .block
        .copy_admin(block_id, group_id, "copy")
        .await
        .unwrap();
    let copied = repos.block.list_items_admin(copy_id).await.unwrap();
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0].smart_shuffle, Some(policy));
    assert_eq!(copied[0].watermark_ids, vec![501]);
    assert_eq!(copied[0].graphics_element_ids, vec![601]);

    repos.block.delete_admin(block_id).await.unwrap();
    assert!(repos.block.delete_admin(block_id).await.is_err());
}

#[tokio::test]
async fn block_admin_repository_propagates_closed_pool_failures() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;

    assert!(repos.block.list_groups_admin().await.is_err());
    assert!(repos.block.list_admin("").await.is_err());
    assert!(repos.block.block_exists_admin(1).await.is_err());
}

async fn seed_overlay_targets(pool: &sqlx::SqlitePool) {
    pool.execute(
        r#"INSERT INTO "ChannelWatermark" (
            "Id", "DurationSeconds", "FrequencyMinutes", "HorizontalMarginPercent",
            "ImageSource", "Location", "Mode", "Name", "Opacity",
            "PlaceWithinSourceContent", "Size", "VerticalMarginPercent",
            "WidthPercent", "ZIndex"
        ) VALUES (501, 30, 10, 0.0, 0, 0, 1, 'wm', 100, 0, 0, 0.0, 10.0, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "GraphicsElement" ("Id", "Kind", "Name") VALUES (601, 1, 'ge')"#)
        .await
        .unwrap();
}

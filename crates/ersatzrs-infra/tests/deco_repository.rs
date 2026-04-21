#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{DecoRepository, GraphicsRepository};
use ersatzrs_domain::{Deco, GraphicsElement};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn deco_repository_loads_watermarks_and_graphics_for_explicit_get() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.execute(r#"INSERT INTO "DecoGroup" ("Id", "Name") VALUES (1, 'group')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "ChannelWatermark" (
            "Id", "Name", "Mode", "ImageSource", "Location", "Size", "WidthPercent",
            "HorizontalMarginPercent", "VerticalMarginPercent", "FrequencyMinutes",
            "DurationSeconds", "Opacity", "PlaceWithinSourceContent", "ZIndex"
         ) VALUES (10, 'wm', 'Permanent', 'Text', 'TopLeft', 'Scaled', 10, 0, 0, 0, 0, 100, 0, 0)"#,
    )
    .await
    .unwrap();
    repos
        .graphics
        .upsert_element(&GraphicsElement {
            id: 11,
            name: "bug".to_owned(),
            ..GraphicsElement::default()
        })
        .await
        .unwrap();
    // Collection(5) must exist before inserting Deco with DefaultFillerCollectionId=5 (NOT NULL FK)
    pool.execute(
        r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (5, 'filler', 0)"#,
    )
    .await
    .unwrap();

    repos
        .deco
        .upsert(&Deco {
            id: 1,
            deco_group_id: Some(1),
            name: "deco".to_owned(),
            default_filler_mode: ersatzrs_domain::DecoMode::Override,
            default_filler_collection_type: Some(ersatzrs_domain::CollectionType::Collection),
            default_filler_collection_id: Some(5),
            default_filler_trim_to_fit: true,
            watermark_ids: vec![10],
            graphics_element_ids: vec![11],
            ..Deco::default()
        })
        .await
        .unwrap();

    let loaded = repos.deco.get_with_watermarks(1).await.unwrap();
    assert_eq!(loaded.watermark_ids, vec![10]);
    assert_eq!(loaded.graphics_element_ids, vec![11]);
    assert_eq!(loaded.default_filler_collection_id, Some(5));
    assert!(loaded.default_filler_trim_to_fit);

    let listed = repos.deco.list_for_group(1).await.unwrap();
    assert_eq!(listed[0].id, 1);
    assert!(listed[0].watermark_ids.is_empty());
    assert!(listed[0].graphics_element_ids.is_empty());
}

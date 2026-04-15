mod common;

use nastv_core::repositories::{DecoRepository, GraphicsRepository};
use nastv_domain::{Deco, GraphicsElement};
use nastv_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn deco_repository_loads_watermarks_and_graphics_for_explicit_get() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.execute("INSERT INTO deco_groups (id, name) VALUES (1, 'group')")
        .await
        .unwrap();
    pool.execute(
        "INSERT INTO channel_watermarks (
            id, name, mode, image_source, location, size, width_percent,
            horizontal_margin_percent, vertical_margin_percent, frequency_minutes,
            duration_seconds, opacity, place_within_source_content, z_index
         ) VALUES (10, 'wm', 'Permanent', 'Text', 'TopLeft', 'Scaled', 10, 0, 0, 0, 0, 100, 0, 0)",
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

    repos
        .deco
        .upsert(&Deco {
            id: 1,
            deco_group_id: Some(1),
            name: "deco".to_owned(),
            watermark_ids: vec![10],
            graphics_element_ids: vec![11],
            ..Deco::default()
        })
        .await
        .unwrap();

    let loaded = repos.deco.get_with_watermarks(1).await.unwrap();
    assert_eq!(loaded.watermark_ids, vec![10]);
    assert_eq!(loaded.graphics_element_ids, vec![11]);

    let listed = repos.deco.list_for_group(1).await.unwrap();
    assert_eq!(listed[0].id, 1);
    assert!(listed[0].watermark_ids.is_empty());
    assert!(listed[0].graphics_element_ids.is_empty());
}

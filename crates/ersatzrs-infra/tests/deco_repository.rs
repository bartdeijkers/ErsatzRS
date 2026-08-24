mod common;

use ersatzrs_core::repositories::{DecoAdminRecord, DecoRepository, DecoTemplateItemAdmin};
use ersatzrs_domain::{CollectionType, DecoMode};
use ersatzrs_infra::repositories::make_repositories;

#[tokio::test]
async fn deco_admin_repository_round_trips_complete_deco_and_template_graphs() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool);

    let group_id = repos.deco.create_group_admin("group").await.unwrap();
    let deco_id = repos
        .deco
        .create_admin(group_id, "station deco")
        .await
        .unwrap();
    repos
        .deco
        .save_admin(&DecoAdminRecord {
            id: deco_id,
            group_id,
            group_name: "ignored".to_owned(),
            name: "station deco updated".to_owned(),
            watermark_mode: DecoMode::Inherit,
            watermark_ids: Vec::new(),
            use_watermark_during_filler: false,
            graphics_elements_mode: DecoMode::Disable,
            graphics_element_ids: Vec::new(),
            use_graphics_elements_during_filler: false,
            break_content_mode: DecoMode::Inherit,
            break_content: Vec::new(),
            default_filler_mode: DecoMode::Inherit,
            default_filler_collection_type: CollectionType::Collection,
            default_filler_collection_id: None,
            default_filler_media_item_id: None,
            default_filler_multi_collection_id: None,
            default_filler_smart_collection_id: None,
            default_filler_trim_to_fit: true,
            dead_air_fallback_mode: DecoMode::Disable,
            dead_air_fallback_collection_type: CollectionType::Collection,
            dead_air_fallback_collection_id: None,
            dead_air_fallback_media_item_id: None,
            dead_air_fallback_multi_collection_id: None,
            dead_air_fallback_smart_collection_id: None,
        })
        .await
        .unwrap();

    let loaded = repos.deco.get_admin(deco_id).await.unwrap().unwrap();
    assert_eq!(loaded.name, "station deco updated");
    assert_eq!(loaded.graphics_elements_mode, DecoMode::Disable);
    assert!(loaded.default_filler_trim_to_fit);

    let template_group_id = repos
        .deco
        .create_template_group_admin("templates")
        .await
        .unwrap();
    let template_id = repos
        .deco
        .create_template_admin(template_group_id, "weekday")
        .await
        .unwrap();
    repos
        .deco
        .save_template_admin(
            template_id,
            "weekday updated",
            &[DecoTemplateItemAdmin {
                deco_id,
                deco_name: String::new(),
                start_minutes: 60,
                end_minutes: 24 * 60,
            }],
        )
        .await
        .unwrap();

    let items = repos
        .deco
        .list_template_items_admin(template_id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].deco_id, deco_id);
    assert_eq!(items[0].start_minutes, 60);
    assert_eq!(items[0].end_minutes, 0);
}

#[tokio::test]
async fn deco_admin_repository_propagates_closed_pool_failures() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;

    assert!(repos.deco.list_groups_admin().await.is_err());
    assert!(repos.deco.list_admin("").await.is_err());
    assert!(repos.deco.editor_options_admin().await.is_err());
    assert!(repos.deco.list_template_groups_admin().await.is_err());
}

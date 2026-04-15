mod common;

use chrono::Duration;
use nastv_core::repositories::{BlockRepository, TemplateRepository};
use nastv_domain::{Block, Template};
use nastv_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn template_repository_loads_items_from_child_rows_and_lists_empty_navigation_fields() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.execute("INSERT INTO template_groups (id, name) VALUES (1, 'group')")
        .await
        .unwrap();
    repos
        .block
        .upsert(&Block {
            id: 10,
            name: "block".to_owned(),
            duration: Duration::minutes(30),
            ..Block::default()
        })
        .await
        .unwrap();
    repos
        .template
        .upsert(&Template {
            id: 1,
            template_group_id: Some(1),
            name: "template".to_owned(),
            item_ids: Vec::new(),
        })
        .await
        .unwrap();
    pool.execute(
        "INSERT INTO template_items (id, template_id, block_id, sort_order) VALUES (20, 1, 10, 0)",
    )
    .await
    .unwrap();

    assert_eq!(
        repos.template.get_with_items(1).await.unwrap().item_ids,
        vec![20]
    );
    let listed = repos.template.list_for_group(1).await.unwrap();
    assert_eq!(listed[0].id, 1);
    assert!(listed[0].item_ids.is_empty());
}

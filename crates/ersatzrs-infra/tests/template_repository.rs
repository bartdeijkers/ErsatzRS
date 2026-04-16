mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{BlockRepository, TemplateRepository};
use ersatzrs_domain::{Block, Template};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn template_repository_loads_items_from_child_rows_and_lists_empty_navigation_fields() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.execute(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'blocks')"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (1, 'group')"#)
        .await
        .unwrap();
    repos
        .block
        .upsert(&Block {
            id: 10,
            block_group_id: Some(1),
            name: "block".to_owned(),
            updated_ticks: 100,
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
            updated_ticks: 84,
            item_ids: Vec::new(),
        })
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "TemplateItem" ("Id", "TemplateId", "BlockId", "StartTime") VALUES (20, 1, 10, '00:00:00')"#,
    )
    .await
    .unwrap();

    assert_eq!(
        repos.template.get_with_items(1).await.unwrap().item_ids,
        vec![20]
    );
    let listed = repos.template.list_for_group(1).await.unwrap();
    assert_eq!(listed[0].id, 1);
    assert_eq!(listed[0].updated_ticks, 84);
    assert!(listed[0].item_ids.is_empty());
}

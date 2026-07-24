#![cfg(not(feature = "mysql"))]
mod common;

use chrono::Duration;
use ersatzrs_core::repositories::{BlockRepository, TemplateItemAdmin, TemplateRepository};
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

#[tokio::test]
async fn template_admin_repository_round_trips_atomic_item_graph_and_copy() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    let group_id = repos
        .template
        .create_group_admin("templates")
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "BlockGroup" ("Id", "Name") VALUES (1, 'blocks')"#)
        .await
        .unwrap();
    repos
        .block
        .upsert(&Block {
            id: 10,
            block_group_id: Some(1),
            name: "half hour".to_owned(),
            updated_ticks: 100,
            duration: Duration::minutes(30),
            ..Block::default()
        })
        .await
        .unwrap();

    let template_id = repos
        .template
        .create_admin(group_id, "weekday")
        .await
        .unwrap();
    repos
        .template
        .save_admin(
            template_id,
            "weekday updated",
            &[TemplateItemAdmin {
                block_id: 10,
                block_name: "ignored by persistence".to_owned(),
                start_minutes: 8 * 60,
                duration_minutes: 30,
            }],
        )
        .await
        .unwrap();

    assert_eq!(
        repos
            .template
            .get_admin(template_id)
            .await
            .unwrap()
            .unwrap()
            .name,
        "weekday updated"
    );
    assert_eq!(
        repos.template.list_items_admin(template_id).await.unwrap()[0].start_minutes,
        8 * 60
    );
    assert_eq!(
        repos.template.list_groups_admin().await.unwrap()[0].template_count,
        1
    );
    assert!(repos
        .template
        .template_name_exists_admin(group_id, None, "weekday updated")
        .await
        .unwrap());

    let copy_id = repos
        .template
        .copy_admin(template_id, group_id, "weekday copy")
        .await
        .unwrap();
    assert_eq!(
        repos
            .template
            .list_items_admin(copy_id)
            .await
            .unwrap()
            .len(),
        1
    );
    repos.template.delete_admin(template_id).await.unwrap();
    assert!(repos.template.delete_admin(template_id).await.is_err());
}

#[tokio::test]
async fn template_admin_repository_propagates_closed_pool_failures() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;

    assert!(repos.template.list_groups_admin().await.is_err());
    assert!(repos.template.list_admin("").await.is_err());
    assert!(repos.template.group_exists_admin(1).await.is_err());
}

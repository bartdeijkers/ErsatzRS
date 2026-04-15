mod common;

use chrono::Duration;
use nastv_core::repositories::BlockRepository;
use nastv_domain::Block;
use nastv_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn block_repository_loads_items_and_lists_empty_navigation_fields() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.execute("INSERT INTO block_groups (id, name) VALUES (1, 'group')")
        .await
        .unwrap();

    repos
        .block
        .upsert(&Block {
            id: 1,
            block_group_id: Some(1),
            name: "block".to_owned(),
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
    assert!(listed[0].item_ids.is_empty());
}

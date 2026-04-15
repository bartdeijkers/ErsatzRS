mod common;

use nastv_core::repositories::CollectionRepository;
use nastv_domain::{Collection, CollectionType};
use nastv_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn collection_repository_loads_relation_ids_only_for_explicit_get() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_media_graph(&pool).await;

    repos
        .collection
        .upsert(&Collection {
            id: 1,
            name: "collection".to_owned(),
            collection_type: CollectionType::Collection,
            media_item_ids: vec![1],
            item_ids: vec![10],
        })
        .await
        .unwrap();

    let loaded = repos.collection.get_with_items(1).await.unwrap();
    assert_eq!(loaded.media_item_ids, vec![1]);
    assert_eq!(loaded.item_ids, vec![10]);

    let listed = repos.collection.list_for_media_item(1).await.unwrap();
    assert_eq!(listed[0].id, 1);
    assert!(listed[0].media_item_ids.is_empty());
    assert!(listed[0].item_ids.is_empty());
}

#[tokio::test]
async fn collection_repository_loads_smart_collections_and_playlists() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_media_graph(&pool).await;

    pool.execute(
        "INSERT INTO smart_collections (id, name, query) VALUES (2, 'smart', 'title = Movie')",
    )
    .await
    .unwrap();
    pool.execute("INSERT INTO playlists (id, name) VALUES (3, 'playlist')")
        .await
        .unwrap();
    pool.execute(
        "INSERT INTO playlist_groups (id, playlist_id, name, sort_order) VALUES (4, 3, 'group', 0)",
    )
    .await
    .unwrap();
    pool.execute(
        "INSERT INTO playlist_items (
            id, playlist_id, media_item_id, collection_type, playback_order, sort_order
         ) VALUES (5, 3, 1, 'Collection', 'None', 0)",
    )
    .await
    .unwrap();

    assert_eq!(
        repos.collection.get_smart(2).await.unwrap().query,
        "title = Movie"
    );
    let playlist = repos.collection.get_playlist_with_items(3).await.unwrap();
    assert_eq!(playlist.group_ids, vec![4]);
    assert_eq!(playlist.item_ids, vec![5]);
}

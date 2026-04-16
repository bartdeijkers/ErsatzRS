mod common;

use ersatzrs_core::repositories::CollectionRepository;
use ersatzrs_domain::{Collection, CollectionType};
use ersatzrs_infra::repositories::make_repositories;
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
            item_ids: Vec::new(),
        })
        .await
        .unwrap();

    let loaded = repos.collection.get_with_items(1).await.unwrap();
    assert_eq!(loaded.media_item_ids, vec![1]);
    // ErsatzTV CollectionItem has no separate item id — item_ids == media_item_ids
    assert_eq!(loaded.item_ids, vec![1]);

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
        r#"INSERT INTO "SmartCollection" ("Id", "Name", "Query") VALUES (2, 'smart', 'title = Movie')"#,
    )
    .await
    .unwrap();
    // PlaylistGroup is the parent; Playlist has PlaylistGroupId FK (hierarchy reversed from old schema)
    pool.execute(r#"INSERT INTO "PlaylistGroup" ("Id", "Name") VALUES (4, 'group')"#)
        .await
        .unwrap();
    pool.execute(
        r#"INSERT INTO "Playlist" ("Id", "Name", "PlaylistGroupId", "IsSystem") VALUES (3, 'playlist', 4, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "PlaylistItem" (
            "Id", "Index", "PlaylistId", "CollectionType", "MediaItemId", "PlaybackOrder"
         ) VALUES (5, 0, 3, 'Collection', 1, 'None')"#,
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

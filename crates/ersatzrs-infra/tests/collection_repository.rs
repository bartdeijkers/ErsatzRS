#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{
    CollectionRepository, MultiCollectionItem, PlaylistEditorAdmin, PlaylistItemAdmin,
    RerunCollectionAdmin,
};
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

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn collection_admin_repository_preserves_crud_and_default_item_idempotence() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_media_graph(&pool).await;

    let id = repos
        .collection
        .save_admin(0, "Manual Collection")
        .await
        .expect("create collection");
    assert!(id > 0);
    assert!(repos
        .collection
        .admin_name_exists(0, "manual collection")
        .await
        .expect("check case-insensitive duplicate"));

    let (rows, total) = repos
        .collection
        .list_admin("Manual", 0, 10)
        .await
        .expect("list collections");
    assert_eq!(total, 1);
    assert_eq!(rows[0].name, "Manual Collection");

    repos
        .collection
        .add_default_item(id, 1)
        .await
        .expect("add default collection item");
    repos
        .collection
        .add_default_item(id, 1)
        .await
        .expect("repeat default collection item");
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "CollectionItem" WHERE "CollectionId" = ?"#)
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("count collection items");
    assert_eq!(count, 1);

    let smart_id = repos
        .collection
        .save_smart_admin(0, "Dynamic", "title contains movie")
        .await
        .expect("create smart collection");
    let (smart_rows, smart_total) = repos
        .collection
        .list_smart_admin("Dyn", 0, 10)
        .await
        .expect("list smart collections");
    assert_eq!(smart_total, 1);
    assert_eq!(smart_rows[0].id, smart_id);
    assert_eq!(smart_rows[0].query, "title contains movie");

    let multi_id = repos
        .collection
        .save_multi_admin(
            0,
            "Mixed",
            &[
                MultiCollectionItem {
                    collection_id: Some(id),
                    smart_collection_id: None,
                    name: String::new(),
                    collection_type: 0,
                    use_custom_playback_order: false,
                    schedule_as_group: true,
                    playback_order: 1,
                },
                MultiCollectionItem {
                    collection_id: None,
                    smart_collection_id: Some(smart_id),
                    name: String::new(),
                    collection_type: 5,
                    use_custom_playback_order: false,
                    schedule_as_group: false,
                    playback_order: 1,
                },
            ],
            true,
        )
        .await
        .expect("create multi collection");
    let multi = repos
        .collection
        .get_multi_admin(multi_id)
        .await
        .expect("load multi collection")
        .expect("multi collection exists");
    assert_eq!(multi.items.len(), 2);
    assert_eq!(multi.items[0].name, "Dynamic");
    assert_eq!(multi.items[1].name, "Manual Collection");

    let rerun_id = repos
        .collection
        .save_rerun_admin(&RerunCollectionAdmin {
            id: 0,
            name: "Reruns".to_owned(),
            collection_type: 0,
            collection_id: Some(id),
            media_item_id: None,
            multi_collection_id: None,
            smart_collection_id: None,
            first_run_playback_order: 1,
            rerun_playback_order: 1,
        })
        .await
        .expect("create rerun collection");
    let rerun = repos
        .collection
        .get_rerun_admin(rerun_id)
        .await
        .expect("load rerun collection")
        .expect("rerun collection exists");
    assert_eq!(rerun.collection_id, Some(id));
    assert_eq!(rerun.name, "Reruns");

    let group = repos
        .collection
        .create_playlist_group_admin("Favorites")
        .await
        .expect("create playlist group");
    let playlist = repos
        .collection
        .create_playlist_admin(group.id, "Movie Night")
        .await
        .expect("create playlist");
    repos
        .collection
        .save_playlist_admin(&PlaylistEditorAdmin {
            id: playlist.id,
            playlist_group_id: group.id,
            name: "Movie Night Updated".to_owned(),
            is_system: false,
            items: vec![PlaylistItemAdmin {
                id: 0,
                index: 0,
                collection_type: 10,
                collection_id: None,
                multi_collection_id: None,
                smart_collection_id: None,
                media_item_id: Some(1),
                item_name: String::new(),
                playback_order: 1,
                count: Some(2),
                play_all: false,
                include_in_program_guide: true,
                preferred_audio_language_code: Some("eng".to_owned()),
                preferred_audio_title: None,
                preferred_subtitle_language_code: Some("nld".to_owned()),
                subtitle_mode: Some(1),
                pinned_stream_selection: Some(ersatzrs_domain::PinnedStreamSelection {
                    media_version_id: 101,
                    video_stream_index: 0,
                    audio_stream_index: Some(1),
                    subtitle_stream_index: Some(2),
                }),
            }],
        })
        .await
        .expect("save playlist");
    let editor = repos
        .collection
        .get_playlist_editor_admin(playlist.id)
        .await
        .expect("load playlist")
        .expect("playlist exists");
    assert_eq!(editor.name, "Movie Night Updated");
    assert_eq!(editor.items.len(), 1);
    assert_eq!(editor.items[0].media_item_id, Some(1));
    assert_eq!(
        editor.items[0].preferred_audio_language_code.as_deref(),
        Some("eng")
    );
    assert_eq!(
        editor.items[0].pinned_stream_selection,
        Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 101,
            video_stream_index: 0,
            audio_stream_index: Some(1),
            subtitle_stream_index: Some(2),
        })
    );

    let groups = repos
        .collection
        .list_playlist_groups_admin()
        .await
        .expect("list playlist groups");
    let favorites = groups
        .iter()
        .find(|candidate| candidate.id == group.id)
        .expect("created group");
    assert_eq!(favorites.playlists[0].name, "Movie Night Updated");
}

#[tokio::test]
async fn collection_creation_with_first_item_is_atomic() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_media_graph(&pool).await;

    let collection_id = repos
        .collection
        .create_with_default_item_admin("Atomic", 1)
        .await
        .expect("create collection and first item");
    let item_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "CollectionItem"
           WHERE "CollectionId" = ? AND "MediaItemId" = 1"#,
    )
    .bind(collection_id)
    .fetch_one(&pool)
    .await
    .expect("count first collection item");
    assert_eq!(item_count, 1);

    let missing = repos
        .collection
        .create_with_default_item_admin("Missing media", 99_999)
        .await;
    assert!(matches!(
        missing,
        Err(ersatzrs_core::repositories::RepositoryError::NotFound {
            kind: "media item",
            id: 99_999
        })
    ));

    pool.execute(
        r#"CREATE TRIGGER "TR_CollectionItem_RejectTestInsert"
           BEFORE INSERT ON "CollectionItem"
           BEGIN
               SELECT RAISE(ABORT, 'reject collection item insert');
           END"#,
    )
    .await
    .expect("create rollback test trigger");
    assert!(repos
        .collection
        .create_with_default_item_admin("Rollback", 1)
        .await
        .is_err());

    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "Collection"
           WHERE "Name" IN ('Missing media', 'Rollback')"#,
    )
    .fetch_one(&pool)
    .await
    .expect("count rolled-back collections");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn collection_admin_repository_reports_closed_pool() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    pool.close().await;

    let error = repos
        .collection
        .list_admin("", 0, 10)
        .await
        .expect_err("closed pool must fail");
    assert!(matches!(
        error,
        ersatzrs_core::repositories::RepositoryError::Unavailable { .. }
    ));
}

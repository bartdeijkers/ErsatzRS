mod common;

use ersatzrs_core::repositories::{LibraryRepository, MediaSourceRepository};
use ersatzrs_domain::{LibraryMediaKind, MediaSource, MediaSourceKind, PlexMediaSource};
use ersatzrs_infra::repositories::make_repositories;
use sqlx::Executor;

#[tokio::test]
async fn library_repository_round_trips_paths_and_lists_by_source() {
    let repos = make_repositories(common::test_db().await);
    repos
        .media_source
        .upsert_plex(&PlexMediaSource {
            media_source: MediaSource {
                id: 1,
                name: "plex".to_owned(),
                kind: MediaSourceKind::Plex,
                last_scan: None,
            },
            ..PlexMediaSource::default()
        })
        .await
        .unwrap();

    repos
        .library
        .upsert(&common::library(2, 1, 3))
        .await
        .unwrap();

    assert_eq!(repos.library.list_for_source(1).await.unwrap()[0].id, 2);
    assert!(repos.library.list_for_source(1).await.unwrap()[0]
        .path_ids
        .is_empty());
    assert_eq!(
        repos.library.get_with_paths(2).await.unwrap().path_ids,
        vec![3]
    );
}

#[tokio::test]
async fn scan_target_uses_persisted_provider_sync_flag_and_show_membership() {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    repos
        .media_source
        .upsert_plex(&PlexMediaSource {
            media_source: MediaSource {
                id: 10,
                name: "plex".to_owned(),
                kind: MediaSourceKind::Plex,
                last_scan: None,
            },
            ..PlexMediaSource::default()
        })
        .await
        .unwrap();

    let mut library = common::library(20, 10, 30);
    library.media_kind = LibraryMediaKind::Shows;
    repos.library.upsert(&library).await.unwrap();
    pool.execute(
        r#"INSERT INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems")
           VALUES (20, 'shows', 0)"#,
    )
    .await
    .unwrap();
    pool.execute(
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State")
           VALUES (40, 30, 0)"#,
    )
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "Show" ("Id") VALUES (40)"#)
        .await
        .unwrap();

    let target = repos.library.get_scan_target(20).await.unwrap();
    assert_eq!(target.library_id, 20);
    assert_eq!(target.provider, MediaSourceKind::Plex);
    assert_eq!(target.media_kind, LibraryMediaKind::Shows);
    assert!(!target.should_sync_items);
    assert!(repos.library.show_belongs_to_library(20, 40).await.unwrap());
    assert!(!repos.library.show_belongs_to_library(20, 41).await.unwrap());
}

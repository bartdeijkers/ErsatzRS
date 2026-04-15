mod common;

use nastv_core::repositories::{LibraryRepository, MediaSourceRepository};
use nastv_domain::{MediaSource, MediaSourceKind, PlexMediaSource};
use nastv_infra::repositories::make_repositories;

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

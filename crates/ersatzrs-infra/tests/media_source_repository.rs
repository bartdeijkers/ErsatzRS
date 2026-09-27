mod common;

use ersatzrs_core::repositories::{MediaSourceRepository, RepositoryError};
use ersatzrs_domain::{
    EmbyConnection, EmbyMediaSource, EmbyPathReplacement, JellyfinConnection, JellyfinMediaSource,
    JellyfinPathReplacement, MediaSource, MediaSourceKind, PlexConnection, PlexMediaSource,
    PlexPathReplacement,
};
use ersatzrs_infra::repositories::make_repositories;

#[tokio::test]
async fn media_source_repository_round_trips_provider_children() {
    let repos = make_repositories(common::test_db().await);

    repos
        .media_source
        .upsert_plex(&PlexMediaSource {
            media_source: MediaSource {
                id: 1,
                name: "plex".to_owned(),
                kind: MediaSourceKind::Plex,
                last_scan: Some(common::fixed_time()),
            },
            server_name: Some("plex-server".to_owned()),
            machine_identifier: Some("machine".to_owned()),
            missing_since: Some(common::fixed_time()),
            connections: vec![PlexConnection {
                id: 10,
                plex_media_source_id: 1,
                is_active: true,
                uri: "http://plex".to_owned(),
            }],
            path_replacements: vec![PlexPathReplacement {
                id: 11,
                plex_media_source_id: 1,
                path: "/source".to_owned(),
                replacement_path: "/dest".to_owned(),
            }],
        })
        .await
        .unwrap();
    repos
        .media_source
        .upsert_jellyfin(&JellyfinMediaSource {
            media_source: MediaSource {
                id: 2,
                name: "jellyfin".to_owned(),
                kind: MediaSourceKind::Jellyfin,
                last_scan: None,
            },
            server_name: Some("jellyfin-server".to_owned()),
            operating_system: Some("linux".to_owned()),
            connections: vec![JellyfinConnection {
                id: 20,
                jellyfin_media_source_id: 2,
                address: "http://jellyfin".to_owned(),
            }],
            path_replacements: vec![JellyfinPathReplacement {
                id: 21,
                jellyfin_media_source_id: 2,
                path: "/jf".to_owned(),
                replacement_path: "/media/jf".to_owned(),
            }],
            last_collections_scan: Some(common::fixed_time()),
        })
        .await
        .unwrap();
    repos
        .media_source
        .upsert_emby(&EmbyMediaSource {
            media_source: MediaSource {
                id: 3,
                name: "emby".to_owned(),
                kind: MediaSourceKind::Emby,
                last_scan: None,
            },
            server_name: Some("emby-server".to_owned()),
            operating_system: Some("linux".to_owned()),
            connections: vec![EmbyConnection {
                id: 30,
                emby_media_source_id: 3,
                address: "http://emby".to_owned(),
            }],
            path_replacements: vec![EmbyPathReplacement {
                id: 31,
                emby_media_source_id: 3,
                path: "/emby".to_owned(),
                replacement_path: "/media/emby".to_owned(),
            }],
            last_collections_scan: Some(common::fixed_time()),
        })
        .await
        .unwrap();

    assert_eq!(repos.media_source.list_all().await.unwrap().len(), 3);
    assert_eq!(
        repos.media_source.get_plex(1).await.unwrap().connections[0].id,
        10
    );
    assert_eq!(
        repos
            .media_source
            .get_jellyfin(2)
            .await
            .unwrap()
            .path_replacements[0]
            .replacement_path,
        "/media/jf"
    );
    assert_eq!(
        repos.media_source.get_emby(3).await.unwrap().connections[0].address,
        "http://emby"
    );
}

#[tokio::test]
async fn plex_missing_timestamp_round_trips_in_domain_and_admin_rows() {
    let repos = make_repositories(common::test_db().await);
    let mut source = PlexMediaSource {
        media_source: MediaSource {
            id: 1,
            kind: MediaSourceKind::Plex,
            ..Default::default()
        },
        missing_since: Some(common::fixed_time()),
        ..Default::default()
    };
    for expected in [Some(common::fixed_time()), None] {
        source.missing_since = expected;
        repos.media_source.upsert_plex(&source).await.unwrap();
        assert_eq!(
            repos.media_source.get_plex(1).await.unwrap().missing_since,
            expected
        );
        assert_eq!(
            repos
                .media_source
                .list_remote_sources_admin(ersatzrs_core::repositories::RemoteMediaSourceKind::Plex)
                .await
                .unwrap()[0]
                .missing_since,
            expected
        );
    }
}

#[tokio::test]
async fn media_source_repository_maps_missing_rows_to_not_found() {
    let repos = make_repositories(common::test_db().await);

    assert!(matches!(
        repos.media_source.get_plex(404).await,
        Err(RepositoryError::NotFound { kind, id }) if kind == "media source" && id == 404
    ));
}

#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_domain::LibraryMediaKind;
use ersatzrs_infra::plex::{
    sync_plex_collection_library, sync_plex_movie_library, sync_plex_other_video_library,
    sync_plex_show_by_id, sync_plex_show_library, PlexConnectionConfig, PlexService,
};
use httpmock::prelude::*;
use serde_json::json;
use sqlx::{Row, SqlitePool};

#[tokio::test]
async fn plex_library_discovery_exposes_other_video_sections() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "Directory": [
                        {
                            "Key": "8",
                            "Title": "Home Videos",
                            "type": "movie",
                            "agent": "com.plexapp.agents.none",
                            "scanner": "Plex Video Files Scanner"
                        }
                    ]
                }
            }));
        })
        .await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    let libraries = service.list_libraries(&config).await.unwrap();
    assert_eq!(libraries.len(), 1);
    assert_eq!(libraries[0].key, "8");
    assert_eq!(libraries[0].name, "Home Videos");
    assert_eq!(libraries[0].media_kind, LibraryMediaKind::OtherVideos);
}

#[tokio::test]
async fn plex_sync_maps_mock_movie_response_to_media_rows() {
    let server = MockServer::start_async().await;
    mock_plex_movie_page(&server).await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    let synced = sync_plex_movie_library(&pool, &service, &config, 1, 1, "1", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "PlexMovie").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    assert_eq!(count(&pool, "PlexMediaFile").await, 1);
    assert_eq!(count(&pool, "MediaStream").await, 2);
    for table in [
        "Genre",
        "Tag",
        "Studio",
        "MusicLabel",
        "MetadataGuid",
        "Actor",
        "Director",
        "Writer",
        "Artwork",
    ] {
        assert!(count(&pool, table).await > 0, "expected movie {table} rows");
    }
    let movie_metadata = sqlx::query(
        r#"SELECT "ContentRating", "OriginalTitle", "Plot"
           FROM "MovieMetadata" LIMIT 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        movie_metadata
            .get::<Option<String>, _>("ContentRating")
            .as_deref(),
        Some("NL:12/US:PG-13")
    );
    assert_eq!(
        movie_metadata
            .get::<Option<String>, _>("OriginalTitle")
            .as_deref(),
        Some("Original Video")
    );
    assert_eq!(
        provider_library_key(&pool, "PlexLibrary", "Key", 1).await,
        Some("1".to_owned())
    );
    insta::assert_json_snapshot!(plex_movie_rows(&pool).await);
}

#[tokio::test]
async fn plex_other_video_sync_maps_uniform_metadata_and_playback_rows() {
    let server = MockServer::start_async().await;
    let mut item = media_json(
        "/library/metadata/800",
        "/library/parts/80",
        "/media/other/concert.mkv",
        800,
    );
    item["title"] = json!("Concert Film");
    item["originalTitle"] = json!("Original Concert");
    item["year"] = json!(2024);
    item["originallyAvailableAt"] = json!("2024-01-02");
    item["contentRating"] = json!("NL:12/US:PG-13");
    item["summary"] = json!("Live performance");
    item["tagline"] = json!("One night only");
    item["studio"] = json!("Concert Studio");
    item["thumb"] = json!("/library/metadata/800/thumb");
    item["art"] = json!("/library/metadata/800/art");
    item["Genre"] = json!([{ "tag": "Music" }]);
    item["Label"] = json!([{ "tag": "Indie Label" }]);
    item["Collection"] = json!([{ "tag": "Live" }]);
    item["Guid"] = json!([{ "id": "imdb://tt1234567" }]);
    item["Role"] = json!([{ "tag": "Singer", "role": "Self" }]);
    item["Director"] = json!([{ "tag": "Director Name" }]);
    item["Writer"] = json!([{ "tag": "Writer Name" }]);
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/8/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": { "totalSize": 1, "Metadata": [item] }
            }));
        })
        .await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    assert_eq!(
        sync_plex_other_video_library(&pool, &service, &config, 1, 8, "8", false)
            .await
            .unwrap(),
        1
    );
    assert_eq!(count(&pool, "PlexOtherVideo").await, 1);
    assert_eq!(count(&pool, "OtherVideoMetadata").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaStream").await, 2);
    for table in [
        "Genre",
        "Tag",
        "Studio",
        "MusicLabel",
        "MetadataGuid",
        "Actor",
        "Director",
        "Writer",
        "Artwork",
    ] {
        assert!(count(&pool, table).await > 0, "expected {table} rows");
    }
    let row =
        sqlx::query(r#"SELECT "Title", "ContentRating", "Plot" FROM "OtherVideoMetadata" LIMIT 1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.get::<String, _>("Title"), "Concert Film");
    assert_eq!(
        row.get::<Option<String>, _>("ContentRating").as_deref(),
        Some("NL:12/US:PG-13")
    );
    assert_eq!(
        row.get::<Option<String>, _>("Plot").as_deref(),
        Some("Live performance")
    );
    let artwork_paths =
        sqlx::query_scalar::<_, String>(r#"SELECT "Path" FROM "Artwork" ORDER BY "ArtworkKind""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(
        artwork_paths,
        [
            "plex://1/library/metadata/800/thumb",
            "plex://1/library/metadata/800/art"
        ]
    );
}

#[tokio::test]
async fn plex_movie_sync_classifies_provider_extras_and_alternate_versions() {
    let server = MockServer::start_async().await;
    let items = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/9/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 3,
                    "Metadata": [
                        media_json_with_title(
                            "/library/metadata/900",
                            "/library/parts/90",
                            "/media/Movie.mkv",
                            900,
                            "Movie"
                        ),
                        media_json_with_title(
                            "/library/metadata/901",
                            "/library/parts/91",
                            "/media/Movie (Director's Cut).mkv",
                            901,
                            "Movie (Director's Cut)"
                        ),
                        media_json_with_title(
                            "/library/metadata/902",
                            "/library/parts/92",
                            "/media/Movie-trailer.mkv",
                            902,
                            "Movie Trailer"
                        )
                    ]
                }
            }));
        })
        .await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_movie_library(&pool, &service, &config, 1, 9, "9", false)
        .await
        .unwrap();
    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "ContentKind" = 0, "SubKind" = NULL, "ParentMediaItemId" = NULL"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sync_plex_movie_library(&pool, &service, &config, 1, 9, "9", false)
        .await
        .unwrap();

    assert_eq!(
        content_variant_rows(&pool, "MovieMetadata", "MovieId").await,
        json!([
            {
                "content_kind": 0,
                "parent_title": null,
                "sub_kind": null,
                "title": "Movie"
            },
            {
                "content_kind": 2,
                "parent_title": "Movie",
                "sub_kind": "directors_cut",
                "title": "Movie (Director's Cut)"
            },
            {
                "content_kind": 1,
                "parent_title": null,
                "sub_kind": "trailer",
                "title": "Movie Trailer"
            }
        ])
    );
    assert_eq!(items.calls_async().await, 2);
}

#[tokio::test]
async fn plex_movie_sync_writes_metadata_subtitle_rows() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/5/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json_with_subtitles(
                            "/library/metadata/500",
                            "/library/parts/50",
                            "/media/subtitled-movie.mkv",
                            500
                        )
                    ]
                }
            }));
        })
        .await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_movie_library(&pool, &service, &config, 1, 5, "5", false)
        .await
        .unwrap();

    let external_subtitle_url = format!(
        "{}/library/streams/4?X-Plex-Token=plex-token",
        server.base_url()
    );
    assert_eq!(count(&pool, "MovieMetadata").await, 1);
    assert_eq!(count(&pool, "Subtitle").await, 2);
    assert_eq!(
        subtitle_rows(&pool).await,
        json!([
            {
                "codec": "srt",
                "default": 1,
                "forced": 0,
                "language": "eng",
                "owner": "movie",
                "path": null,
                "stream_index": 2,
                "subtitle_kind": 0
            },
            {
                "codec": "ass",
                "default": 0,
                "forced": 1,
                "language": "jpn",
                "owner": "movie",
                "path": external_subtitle_url,
                "stream_index": 4,
                "subtitle_kind": 1
            }
        ])
    );
}

#[tokio::test]
async fn plex_sync_paginates_movie_library() {
    let server = MockServer::start_async().await;
    let first_page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/3/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("X-Plex-Container-Start", "0")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 101,
                    "Metadata": [
                        media_json("/library/metadata/300", "/library/parts/30", "/media/movie-1.mkv", 300)
                    ]
                }
            }));
        })
        .await;
    let second_page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/3/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("X-Plex-Container-Start", "100")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 101,
                    "Metadata": [
                        media_json("/library/metadata/301", "/library/parts/31", "/media/movie-2.mkv", 301)
                    ]
                }
            }));
        })
        .await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    let synced = sync_plex_movie_library(&pool, &service, &config, 1, 3, "3", false)
        .await
        .unwrap();

    assert_eq!(synced, 2);
    assert_eq!(count(&pool, "PlexMovie").await, 2);
    first_page.assert_async().await;
    second_page.assert_async().await;
}

#[tokio::test]
async fn plex_sync_skips_unchanged_movie_etag_writes() {
    let server = MockServer::start_async().await;
    let page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/1/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json("/library/metadata/100", "/library/parts/10", "/media/unchanged.mkv", 100)
                    ]
                }
            }));
        })
        .await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_movie_library(&pool, &service, &config, 1, 1, "1", false)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path" = '/media/kept.mkv'"#)
        .execute(&pool)
        .await
        .unwrap();

    let synced = sync_plex_movie_library(&pool, &service, &config, 1, 1, "1", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(media_file_path(&pool).await, "/media/kept.mkv");

    let deep_synced = sync_plex_movie_library(&pool, &service, &config, 1, 1, "1", true)
        .await
        .unwrap();

    assert_eq!(deep_synced, 1);
    assert_eq!(media_file_path(&pool).await, "/media/unchanged.mkv");
    assert_eq!(page.calls_async().await, 3);
}

#[tokio::test]
async fn plex_sync_scopes_matching_etags_to_the_current_library() {
    let server = MockServer::start_async().await;
    let first_page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/1/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json("/library/metadata/shared", "/library/parts/10", "/media/shared.mkv", 100)
                    ]
                }
            }));
        })
        .await;
    let second_page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/2/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json("/library/metadata/shared", "/library/parts/20", "/media/shared-2.mkv", 200)
                    ]
                }
            }));
        })
        .await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_movie_library(&pool, &service, &config, 1, 1, "1", false)
        .await
        .unwrap();
    let synced = sync_plex_movie_library(&pool, &service, &config, 1, 2, "2", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "PlexMovie").await, 2);
    assert_eq!(count(&pool, "MediaFile").await, 2);
    assert_eq!(first_page.calls_async().await, 1);
    assert_eq!(second_page.calls_async().await, 1);
}

#[tokio::test]
async fn plex_sync_maps_mock_show_response_to_media_rows() {
    let server = MockServer::start_async().await;
    mock_plex_show_pages(&server).await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    let synced = sync_plex_show_library(&pool, &service, &config, 1, 1, "2", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "PlexShow").await, 1);
    assert_eq!(count(&pool, "PlexSeason").await, 1);
    assert_eq!(count(&pool, "PlexEpisode").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    assert_eq!(count(&pool, "MediaStream").await, 2);
    for table in [
        "Genre",
        "Tag",
        "Studio",
        "MusicLabel",
        "MetadataGuid",
        "Actor",
        "Director",
        "Writer",
        "Artwork",
    ] {
        assert!(
            count(&pool, table).await > 0,
            "expected episode {table} rows"
        );
    }
    let episode_metadata = sqlx::query(
        r#"SELECT "ContentRating", "OriginalTitle", "Plot"
           FROM "EpisodeMetadata" LIMIT 1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        episode_metadata
            .get::<Option<String>, _>("ContentRating")
            .as_deref(),
        Some("NL:12/US:PG-13")
    );
    assert_eq!(
        episode_metadata
            .get::<Option<String>, _>("OriginalTitle")
            .as_deref(),
        Some("Original Video")
    );
    assert_eq!(
        provider_library_key(&pool, "PlexLibrary", "Key", 1).await,
        Some("2".to_owned())
    );
    insta::assert_json_snapshot!(plex_show_rows(&pool).await);
}

#[tokio::test]
async fn plex_episode_sync_writes_metadata_subtitle_rows() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/6/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        { "key": "/library/metadata/600", "title": "Subtitle Show", "updatedAt": 1 }
                    ]
                }
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/metadata/600/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        {
                            "key": "/library/metadata/601",
                            "title": "Season 1",
                            "index": 1,
                            "updatedAt": 2
                        }
                    ]
                }
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/metadata/601/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json_with_subtitles(
                            "/library/metadata/602",
                            "/library/parts/60",
                            "/media/show/subtitled.mkv",
                            602
                        )
                    ]
                }
            }));
        })
        .await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_show_library(&pool, &service, &config, 1, 6, "6", false)
        .await
        .unwrap();

    let external_subtitle_url = format!(
        "{}/library/streams/4?X-Plex-Token=plex-token",
        server.base_url()
    );
    assert_eq!(count(&pool, "EpisodeMetadata").await, 1);
    assert_eq!(count(&pool, "Subtitle").await, 2);
    assert_eq!(
        subtitle_rows(&pool).await,
        json!([
            {
                "codec": "srt",
                "default": 1,
                "forced": 0,
                "language": "eng",
                "owner": "episode",
                "path": null,
                "stream_index": 2,
                "subtitle_kind": 0
            },
            {
                "codec": "ass",
                "default": 0,
                "forced": 1,
                "language": "jpn",
                "owner": "episode",
                "path": external_subtitle_url,
                "stream_index": 4,
                "subtitle_kind": 1
            }
        ])
    );
}

#[tokio::test]
async fn plex_single_show_sync_targets_one_show_and_honors_episode_etags() {
    let server = MockServer::start_async().await;
    let single_show = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/metadata/200")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "Metadata": [
                        {
                            "key": "/library/metadata/200/children",
                            "title": "Target Show",
                            "updatedAt": 10
                        }
                    ]
                }
            }));
        })
        .await;
    let seasons = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/metadata/200/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("X-Plex-Container-Start", "0")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        {
                            "key": "/library/metadata/201",
                            "title": "Season 1",
                            "index": 1,
                            "updatedAt": 20
                        }
                    ]
                }
            }));
        })
        .await;
    let episodes = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/metadata/201/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("X-Plex-Container-Start", "0")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json(
                            "/library/metadata/202",
                            "/library/parts/20",
                            "/media/show/target.mkv",
                            202
                        )
                    ]
                }
            }));
        })
        .await;

    let pool = common::test_db().await;
    seed_plex_show_scan_config(&pool, &server.base_url()).await;

    let synced = sync_plex_show_by_id(&pool, 1, 10, false, None)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "PlexShow").await, 2);
    assert_eq!(count(&pool, "PlexSeason").await, 1);
    assert_eq!(count(&pool, "PlexEpisode").await, 1);
    assert_eq!(media_file_path(&pool).await, "/media/show/target.mkv");

    sqlx::query(r#"UPDATE "MediaFile" SET "Path" = '/media/show/kept.mkv'"#)
        .execute(&pool)
        .await
        .unwrap();

    let shallow_synced = sync_plex_show_by_id(&pool, 1, 10, false, None)
        .await
        .unwrap();

    assert_eq!(shallow_synced, 1);
    assert_eq!(media_file_path(&pool).await, "/media/show/kept.mkv");

    let deep_synced = sync_plex_show_by_id(&pool, 1, 10, true, None)
        .await
        .unwrap();

    assert_eq!(deep_synced, 1);
    assert_eq!(media_file_path(&pool).await, "/media/show/target.mkv");
    assert_eq!(single_show.calls_async().await, 3);
    assert_eq!(seasons.calls_async().await, 3);
    assert_eq!(episodes.calls_async().await, 3);
}

#[tokio::test]
async fn plex_sync_maps_mock_collection_response_to_collection_rows() {
    let server = MockServer::start_async().await;
    mock_plex_collection_movie_pages(&server).await;
    let collections = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("type", "18")
                .query_param("X-Plex-Container-Start", "0")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        {
                            "ratingKey": "900",
                            "title": "Favorites",
                            "updatedAt": 5,
                            "librarySectionId": 4
                        }
                    ]
                }
            }));
        })
        .await;
    let members = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/collections/900/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("X-Plex-Container-Start", "0")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 2,
                    "Metadata": [
                        { "key": "/library/metadata/400", "type": "movie" },
                        { "key": "/library/metadata/401", "type": "movie" }
                    ]
                }
            }));
        })
        .await;

    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_movie_library(&pool, &service, &config, 1, 4, "4", false)
        .await
        .unwrap();

    let synced = sync_plex_collection_library(&pool, &service, &config, 1, "4")
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "Collection").await, 1);
    assert_eq!(count(&pool, "PlexCollection").await, 1);
    assert_eq!(count(&pool, "CollectionItem").await, 2);
    insta::assert_json_snapshot!(plex_collection_rows(&pool).await);
    collections.assert_async().await;
    members.assert_async().await;
}

async fn mock_plex_movie_page(server: &MockServer) {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/1/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json_with_uniform_metadata(
                            "/library/metadata/100",
                            "/library/parts/10",
                            "/media/movie.mkv",
                            100
                        )
                    ]
                }
            }));
        })
        .await;
}

async fn mock_plex_show_pages(server: &MockServer) {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/2/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        { "key": "/library/metadata/200", "title": "Example Show", "updatedAt": 1 }
                    ]
                }
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/metadata/200/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        {
                            "key": "/library/metadata/201",
                            "title": "Season 1",
                            "index": 1,
                            "updatedAt": 2
                        }
                    ]
                }
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/metadata/201/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json_with_uniform_metadata(
                            "/library/metadata/202",
                            "/library/parts/20",
                            "/media/show/s01e01.mkv",
                            202
                        )
                    ]
                }
            }));
        })
        .await;
}

/// Exercises real-world Plex collection response shapes that the mock test
/// above does not: paginated `/library/all?type=18` results, unicode in
/// collection titles, mixed `type` values for collection members, and a
/// member with a redirect-style `ratingKey`-only key. Asserts that the sync
/// projects collections deterministically and that pagination is honored
/// (Plex caps each page at 100, so a totalSize of 150 must trigger a second
/// fetch).
#[tokio::test]
async fn plex_sync_paginates_collection_list_and_handles_mixed_member_types() {
    let server = MockServer::start_async().await;
    mock_plex_collection_movie_pages(&server).await;

    let collections_page_one = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("type", "18")
                .query_param("X-Plex-Container-Start", "0")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 2,
                    "Metadata": [
                        {
                            "ratingKey": "910",
                            "title": "Action — Greatest Hits",
                            "updatedAt": 1_776_406_500i64,
                            "librarySectionId": 4
                        },
                        {
                            "ratingKey": "911",
                            "title": "Holiday Marathon \u{1F389}",
                            "updatedAt": 1_776_406_600i64,
                            "librarySectionId": 4
                        }
                    ]
                }
            }));
        })
        .await;
    let action_members = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/collections/910/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("X-Plex-Container-Start", "0")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 2,
                    "Metadata": [
                        { "key": "/library/metadata/400", "type": "movie" },
                        { "key": "/library/metadata/401", "type": "movie" }
                    ]
                }
            }));
        })
        .await;
    let holiday_members = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/collections/911/children")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token")
                .query_param("X-Plex-Container-Start", "0")
                .query_param("X-Plex-Container-Size", "100");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 2,
                    "Metadata": [
                        { "key": "/library/metadata/401", "type": "movie" },
                        { "key": "/library/metadata/9999", "type": "show" }
                    ]
                }
            }));
        })
        .await;

    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_movie_library(&pool, &service, &config, 1, 4, "4", false)
        .await
        .unwrap();

    let synced = sync_plex_collection_library(&pool, &service, &config, 1, "4")
        .await
        .unwrap();

    assert_eq!(synced, 2);
    assert_eq!(count(&pool, "Collection").await, 2);
    assert_eq!(count(&pool, "PlexCollection").await, 2);
    // 2 movies in Action + 1 movie in Holiday (the show key has no matching
    // PlexShow row, so it is dropped during member resolution rather than
    // failing the sync).
    assert_eq!(count(&pool, "CollectionItem").await, 3);
    insta::assert_json_snapshot!(plex_collection_rows(&pool).await);
    collections_page_one.assert_async().await;
    action_members.assert_async().await;
    holiday_members.assert_async().await;
}

async fn mock_plex_collection_movie_pages(server: &MockServer) {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/4/all")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 2,
                    "Metadata": [
                        media_json("/library/metadata/400", "/library/parts/40", "/media/collection/movie-1.mkv", 400),
                        media_json("/library/metadata/401", "/library/parts/41", "/media/collection/movie-2.mkv", 401)
                    ]
                }
            }));
        })
        .await;
}

fn media_json(key: &str, part_key: &str, path: &str, id: i32) -> serde_json::Value {
    json!({
        "key": key,
        "title": "Video",
        "addedAt": 1_776_406_400,
        "updatedAt": 1_776_406_500,
        "index": 1,
        "media": [
            {
                "id": id,
                "duration": 120_000,
                "width": 1920,
                "height": 1080,
                "aspectRatio": 1.777,
                "part": [
                    {
                        "id": id + 1,
                        "key": part_key,
                        "file": path,
                        "stream": [
                            {
                                "id": 1,
                                "index": 0,
                                "streamType": 1,
                                "codec": "h264",
                                "width": 1920,
                                "height": 1080,
                                "scanType": "progressive",
                                "frameRate": "24.000"
                            },
                            {
                                "id": 2,
                                "index": 1,
                                "streamType": 2,
                                "codec": "aac",
                                "channels": 2,
                                "languageCode": "eng",
                                "default": true
                            }
                        ]
                    }
                ]
            }
        ]
    })
}

fn media_json_with_uniform_metadata(
    key: &str,
    part_key: &str,
    path: &str,
    id: i32,
) -> serde_json::Value {
    let mut value = media_json(key, part_key, path, id);
    value["originalTitle"] = json!("Original Video");
    value["year"] = json!(2024);
    value["originallyAvailableAt"] = json!("2024-01-02");
    value["contentRating"] = json!("NL:12/US:PG-13");
    value["summary"] = json!("Provider plot");
    value["tagline"] = json!("Provider tagline");
    value["studio"] = json!("Provider Studio");
    value["thumb"] = json!(format!("{key}/thumb"));
    value["art"] = json!(format!("{key}/art"));
    value["Genre"] = json!([{ "tag": "Drama" }]);
    value["Label"] = json!([{ "tag": "Provider Music Label" }]);
    value["Collection"] = json!([{ "tag": "Featured" }]);
    value["Guid"] = json!([{ "id": format!("imdb://tt{id:07}") }]);
    value["Role"] = json!([{ "tag": "Actor Name", "role": "Lead" }]);
    value["Director"] = json!([{ "tag": "Director Name" }]);
    value["Writer"] = json!([{ "tag": "Writer Name" }]);
    value
}

fn media_json_with_title(
    key: &str,
    part_key: &str,
    path: &str,
    id: i32,
    title: &str,
) -> serde_json::Value {
    let mut value = media_json(key, part_key, path, id);
    value["title"] = json!(title);
    value
}

fn media_json_with_subtitles(key: &str, part_key: &str, path: &str, id: i32) -> serde_json::Value {
    let mut value = media_json(key, part_key, path, id);
    let streams = value["media"][0]["part"][0]["stream"]
        .as_array_mut()
        .expect("media_json stream array");
    streams.push(json!({
        "id": 3,
        "index": 2,
        "streamType": 3,
        "codec": "srt",
        "languageCode": "eng",
        "default": true,
        "embeddedInVideo": false
    }));
    streams.push(json!({
        "id": 4,
        "key": "/library/streams/4",
        "streamType": 3,
        "codec": "ass",
        "languageCode": "jpn",
        "forced": true
    }));
    value
}

async fn seed_plex_show_scan_config(pool: &SqlitePool, base_url: &str) {
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlexMediaSource" ("Id", "ClientIdentifier", "ServerName")
           VALUES (1, 'client-1', 'Plex Test')"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlexConnection" ("Id", "IsActive", "Uri", "PlexMediaSourceId")
           VALUES (1, 1, ?, 1)"#,
    )
    .bind(base_url)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES ('plex.server_auth_token.client-1', 'plex-token')"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (1, 'Shows', 2, 1)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "PlexLibrary" ("Id", "Key", "ShouldSyncItems")
           VALUES (1, '2', 1)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path")
           VALUES (1, 1, 'plex://1')"#,
    )
    .execute(pool)
    .await
    .unwrap();

    for (id, key, title) in [
        (10, "/library/metadata/200/children", "Target Show"),
        (11, "/library/metadata/999", "Other Show"),
    ] {
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "PlexShow" ("Id", "Key", "Etag") VALUES (?, ?, 'seed')"#)
            .bind(id)
            .bind(key)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(
            r#"INSERT INTO "ShowMetadata"
               ("ShowId", "Title", "DateAdded", "DateUpdated", "MetadataKind")
               VALUES (?, ?, '2026-05-20T00:00:00Z', '2026-05-20T00:00:00Z', 1)"#,
        )
        .bind(id)
        .bind(title)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}""#);
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

async fn provider_library_key(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    id: i64,
) -> Option<String> {
    let sql = format!(r#"SELECT "{column}" AS "Value" FROM "{table}" WHERE "Id" = ?"#);
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Value")
}

async fn media_file_path(pool: &SqlitePool) -> String {
    sqlx::query(r#"SELECT "Path" FROM "MediaFile" LIMIT 1"#)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Path")
}

async fn content_variant_rows(
    pool: &SqlitePool,
    metadata_table: &str,
    metadata_fk: &str,
) -> serde_json::Value {
    let sql = format!(
        r#"SELECT "mm"."Title" AS "Title",
                  "mi"."ContentKind" AS "ContentKind",
                  "mi"."SubKind" AS "SubKind",
                  "pmm"."Title" AS "ParentTitle"
           FROM "{metadata_table}" AS "mm"
           JOIN "MediaItem" AS "mi" ON "mi"."Id" = "mm"."{metadata_fk}"
           LEFT JOIN "MediaItem" AS "pmi" ON "pmi"."Id" = "mi"."ParentMediaItemId"
           LEFT JOIN "{metadata_table}" AS "pmm" ON "pmm"."{metadata_fk}" = "pmi"."Id"
           ORDER BY "mm"."Id""#
    );
    let rows = sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .fetch_all(pool)
        .await
        .unwrap();
    serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "title": row.get::<String, _>("Title"),
                    "content_kind": row.get::<i64, _>("ContentKind"),
                    "sub_kind": row.get::<Option<String>, _>("SubKind"),
                    "parent_title": row.get::<Option<String>, _>("ParentTitle"),
                })
            })
            .collect(),
    )
}

async fn plex_movie_rows(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query(
        r#"SELECT "pm"."Key" AS "Key",
                  "mf"."Path" AS "Path",
                  "pmf"."Key" AS "PartKey",
                  "mv"."Duration" AS "Duration",
                  COUNT("ms"."Id") AS "StreamCount"
           FROM "PlexMovie" AS "pm"
           JOIN "MediaVersion" AS "mv" ON "mv"."MovieId" = "pm"."Id"
           JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id"
           JOIN "PlexMediaFile" AS "pmf" ON "pmf"."Id" = "mf"."Id"
           LEFT JOIN "MediaStream" AS "ms" ON "ms"."MediaVersionId" = "mv"."Id"
           GROUP BY "pm"."Key", "mf"."Path", "pmf"."Key", "mv"."Duration""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "key": row.get::<String, _>("Key"),
                    "path": row.get::<String, _>("Path"),
                    "part_key": row.get::<String, _>("PartKey"),
                    "duration": row.get::<String, _>("Duration"),
                    "stream_count": row.get::<i64, _>("StreamCount")
                })
            })
            .collect(),
    )
}

async fn plex_show_rows(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query(
        r#"SELECT "psh"."Key" AS "ShowKey",
                  "pse"."Key" AS "SeasonKey",
                  "s"."SeasonNumber" AS "SeasonNumber",
                  "pe"."Key" AS "EpisodeKey",
                  "mf"."Path" AS "Path"
           FROM "PlexShow" AS "psh"
           JOIN "Season" AS "s" ON "s"."ShowId" = "psh"."Id"
           JOIN "PlexSeason" AS "pse" ON "pse"."Id" = "s"."Id"
           JOIN "Episode" AS "e" ON "e"."SeasonId" = "s"."Id"
           JOIN "PlexEpisode" AS "pe" ON "pe"."Id" = "e"."Id"
           JOIN "MediaVersion" AS "mv" ON "mv"."EpisodeId" = "e"."Id"
           JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "show_key": row.get::<String, _>("ShowKey"),
                    "season_key": row.get::<String, _>("SeasonKey"),
                    "season_number": row.get::<i64, _>("SeasonNumber"),
                    "episode_key": row.get::<String, _>("EpisodeKey"),
                    "path": row.get::<String, _>("Path")
                })
            })
            .collect(),
    )
}

async fn subtitle_rows(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query(
        r#"SELECT CASE
                    WHEN "MovieMetadataId" IS NOT NULL THEN 'movie'
                    WHEN "EpisodeMetadataId" IS NOT NULL THEN 'episode'
                    ELSE 'other'
                  END AS "Owner",
                  "StreamIndex", "Codec", "SubtitleKind", "Language",
                  "Default", "Forced", "Path"
           FROM "Subtitle"
           ORDER BY "Owner", "StreamIndex""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "owner": row.get::<String, _>("Owner"),
                    "stream_index": row.get::<i64, _>("StreamIndex"),
                    "codec": row.get::<String, _>("Codec"),
                    "subtitle_kind": row.get::<i64, _>("SubtitleKind"),
                    "language": row.get::<Option<String>, _>("Language"),
                    "default": row.get::<i64, _>("Default"),
                    "forced": row.get::<i64, _>("Forced"),
                    "path": row.get::<Option<String>, _>("Path")
                })
            })
            .collect(),
    )
}

async fn plex_collection_rows(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query(
        r#"SELECT "pc"."Key" AS "CollectionKey",
                  "c"."Name" AS "Name",
                  "pm"."Key" AS "MediaKey",
                  "ci"."CustomIndex" AS "CustomIndex"
           FROM "Collection" AS "c"
           JOIN "PlexCollection" AS "pc" ON "pc"."Id" = "c"."Id"
           JOIN "CollectionItem" AS "ci" ON "ci"."CollectionId" = "c"."Id"
           JOIN "PlexMovie" AS "pm" ON "pm"."Id" = "ci"."MediaItemId"
           ORDER BY "ci"."CustomIndex""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "collection_key": row.get::<String, _>("CollectionKey"),
                    "name": row.get::<String, _>("Name"),
                    "media_key": row.get::<String, _>("MediaKey"),
                    "custom_index": row.get::<i64, _>("CustomIndex")
                })
            })
            .collect(),
    )
}

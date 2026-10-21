mod common;

#[macro_use]
#[path = "../../../tests/support/manifest_dir.rs"]
mod test_manifest;

use ersatzrs_core::repositories::{EmbyConnectionConfig, EmbyService as _};
use ersatzrs_infra::emby::{
    sync_emby_collection_library, sync_emby_movie_library, sync_emby_show_by_id,
    sync_emby_show_library, EmbyService,
};
use httpmock::prelude::*;
use httpmock::Mock;
use serde_json::json;
use sqlx::{Row, SqlitePool};

#[tokio::test]
async fn emby_episode_page_rejects_other_seasons_but_accepts_missing_season_id() {
    let server = MockServer::start_async().await;
    let response = server.mock_async(|when, then| {
        when.method(GET).path("/Shows/show/Episodes").query_param("seasonId", "season");
        then.status(200).json_body(json!({
            "TotalRecordCount": 3,
            "Items": [
                { "Id": "special", "Name": "Special", "Path": "/fixture/special.mkv", "SeasonId": "specials" },
                { "Id": "episode", "Name": "Episode", "Path": "/fixture/episode.mkv", "SeasonId": "season" },
                { "Id": "legacy", "Name": "Legacy", "Path": "/fixture/legacy.mkv" }
            ]
        }));
    }).await;
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "fixture".to_owned(),
    };
    let page = EmbyService::new()
        .fetch_episode_page(&config, "show", "season", 0, 100)
        .await
        .unwrap();
    assert_eq!(
        page.total_record_count, 3,
        "pagination retains the provider count"
    );
    assert_eq!(
        page.items
            .iter()
            .map(|item| item.item_id.as_str())
            .collect::<Vec<_>>(),
        vec!["episode", "legacy"]
    );
    response.assert_async().await;
}

#[tokio::test]
async fn emby_sync_maps_mock_movie_response_to_media_rows() {
    let server = MockServer::start_async().await;
    let libraries = mock_emby_libraries(&server).await;
    let items = mock_detailed_movie_items(&server).await;
    let playback = mock_detailed_movie_playback(&server).await;

    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "MediaBrowser Token=\"test\"".to_owned(),
    };

    let libraries_result = service.list_libraries(&config).await.unwrap();
    assert_eq!(libraries_result[0].item_id, "library-1");

    let synced = sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "MediaItem").await, 1);
    assert_eq!(count(&pool, "Movie").await, 1);
    assert_eq!(count(&pool, "EmbyMovie").await, 1);
    assert_eq!(count(&pool, "MovieMetadata").await, 1);
    assert_eq!(movie_metadata_title(&pool, 1).await, "Big Buck Bunny");
    assert_eq!(count(&pool, "MetadataGuid").await, 3);
    assert_eq!(count(&pool, "Artwork").await, 2);
    assert_eq!(count(&pool, "Genre").await, 2);
    assert_eq!(count(&pool, "Tag").await, 1);
    assert_eq!(count(&pool, "Actor").await, 1);
    assert_eq!(count(&pool, "Director").await, 1);
    assert_eq!(count(&pool, "Writer").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    let selected: Option<String> = sqlx::query_scalar(
        r#"SELECT f."ProviderMediaSourceId" FROM "MediaFile" f JOIN "MediaVersion" v ON v."Id"=f."MediaVersionId" WHERE v."MovieId"=1"#,
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(selected.as_deref(), Some("selected-movie-source"));
    assert_eq!(count(&pool, "MediaStream").await, 4);
    assert_eq!(count(&pool, "MediaChapter").await, 2);
    assert_eq!(count(&pool, "Subtitle").await, 2);
    let subtitles = subtitle_rows(&pool).await;
    assert_eq!(
        subtitles,
        vec![
            (2, Some("subrip".to_owned()), 0, 0),
            (3, Some("pgssub".to_owned()), 0, 0),
        ]
    );
    assert_eq!(
        provider_library_item_id(&pool, "EmbyLibrary", 1).await,
        Some("library-1".to_owned())
    );
    libraries.assert_async().await;
    items.assert_async().await;
    playback.assert_async().await;
}

#[tokio::test]
async fn emby_movie_sync_classifies_provider_extras_and_alternate_versions() {
    let server = MockServer::start_async().await;
    let items = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-variants")
                .query_param("includeItemTypes", "Movie");
            then.status(200).json_body(json!({
                "TotalRecordCount": 3,
                "Items": [
                    {
                        "Name": "Movie",
                        "Id": "movie-primary",
                        "Etag": "movie-primary-etag",
                        "Path": "/srv/media/Movie.mkv",
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "RunTimeTicks": 120_000_000,
                        "LocationType": "FileSystem"
                    },
                    {
                        "Name": "Movie (Director's Cut)",
                        "Id": "movie-directors-cut",
                        "Etag": "movie-directors-cut-etag",
                        "Path": "/srv/media/Movie (Director's Cut).mkv",
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "RunTimeTicks": 120_000_000,
                        "LocationType": "FileSystem"
                    },
                    {
                        "Name": "Making Of",
                        "Id": "movie-featurette",
                        "Etag": "movie-featurette-etag",
                        "Path": "/srv/media/Movie/Featurettes/Making Of.mkv",
                        "TagItems": [{ "Name": "Featurette" }],
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "RunTimeTicks": 120_000_000,
                        "LocationType": "FileSystem"
                    }
                ]
            }));
        })
        .await;
    let primary_playback = mock_emby_playback(&server, "movie-primary").await;
    let alternate_playback = mock_emby_playback(&server, "movie-directors-cut").await;
    let extra_playback = mock_emby_playback(&server, "movie-featurette").await;
    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "test".to_owned(),
    };

    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-variants", false)
        .await
        .unwrap();
    sqlx::query(
        r#"UPDATE "MediaItem"
           SET "ContentKind" = 0, "SubKind" = NULL, "ParentMediaItemId" = NULL"#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-variants", false)
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
                "sub_kind": "featurette",
                "title": "Making Of"
            }
        ])
    );
    assert_eq!(items.calls_async().await, 2);
    assert_eq!(primary_playback.calls_async().await, 1);
    assert_eq!(alternate_playback.calls_async().await, 1);
    assert_eq!(extra_playback.calls_async().await, 1);
}

#[tokio::test]
async fn emby_sync_pages_movie_library_beyond_first_result_window() {
    let server = MockServer::start_async().await;
    let first_page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-1")
                .query_param("includeItemTypes", "Movie")
                .query_param("startIndex", "0")
                .query_param("limit", "100");
            then.status(200).json_body(movie_page_json(
                101,
                "movie-page-1",
                "/srv/media/page-1.mkv",
            ));
        })
        .await;
    let second_page = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-1")
                .query_param("includeItemTypes", "Movie")
                .query_param("startIndex", "100")
                .query_param("limit", "100");
            then.status(200).json_body(movie_page_json(
                101,
                "movie-page-2",
                "/srv/media/page-2.mkv",
            ));
        })
        .await;
    let first_playback = mock_emby_playback(&server, "movie-page-1").await;
    let second_playback = mock_emby_playback(&server, "movie-page-2").await;

    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "test".to_owned(),
    };

    let synced = sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    assert_eq!(synced, 2);
    assert_eq!(count(&pool, "EmbyMovie").await, 2);
    first_page.assert_async().await;
    second_page.assert_async().await;
    first_playback.assert_async().await;
    second_playback.assert_async().await;
}

#[tokio::test]
async fn emby_sync_skips_unchanged_movie_etag_before_playback_lookup() {
    let server = MockServer::start_async().await;
    let items = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-1")
                .query_param("includeItemTypes", "Movie");
            then.status(200).json_body(movie_page_json(
                1,
                "movie-skip",
                "/srv/media/unchanged.mkv",
            ));
        })
        .await;
    let playback = mock_emby_playback(&server, "movie-skip").await;

    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "test".to_owned(),
    };

    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path" = '/srv/media/kept.mkv'"#)
        .execute(&pool)
        .await
        .unwrap();

    let synced = sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(media_file_path(&pool).await, "/srv/media/kept.mkv");
    assert_eq!(playback.calls_async().await, 1);
    assert_eq!(items.calls_async().await, 2);
}

#[tokio::test]
async fn emby_sync_restores_non_normal_movie_with_matching_etag() {
    let server = MockServer::start_async().await;
    let _items = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-1")
                .query_param("includeItemTypes", "Movie");
            then.status(200).json_body(movie_page_json(
                1,
                "movie-restore",
                &test_manifest_dir!().join("Cargo.toml").to_string_lossy(),
            ));
        })
        .await;
    let playback = mock_emby_playback(&server, "movie-restore").await;

    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "test".to_owned(),
    };

    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaItem" SET "State" = 2"#)
        .execute(&pool)
        .await
        .unwrap();

    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    assert_eq!(media_item_state(&pool).await, 0);
    assert_eq!(
        media_file_path(&pool).await,
        test_manifest_dir!()
            .join("Cargo.toml")
            .to_string_lossy()
            .into_owned()
    );
    assert_eq!(playback.calls_async().await, 2);
}

#[tokio::test]
async fn emby_sync_maps_mock_collection_response_to_collection_rows() {
    let server = MockServer::start_async().await;
    mock_movie(&server).await;
    let (collections, members) = mock_emby_collection_pages(&server).await;

    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "MediaBrowser Token=\"test\"".to_owned(),
    };
    sync_emby_movie_library(&pool, &service, &config, 2, 99, "library-1", false)
        .await
        .unwrap();
    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    let synced = sync_emby_collection_library(&pool, &service, &config, 1, "collections-root")
        .await
        .unwrap();

    assert_eq!(synced, 1);
    common::assert_collection_member_source(&pool, 1).await;
    assert_eq!(count(&pool, "Collection").await, 1);
    assert_eq!(count(&pool, "EmbyCollection").await, 1);
    assert_eq!(count(&pool, "CollectionItem").await, 1);
    insta::assert_json_snapshot!(emby_collection_rows(&pool).await);
    collections.assert_async().await;
    members.assert_async().await;
    let owned = common::collection_tag_rows(&pool, "emby/collection").await;
    assert_eq!(owned.len(), 1);
    assert!(owned
        .iter()
        .all(|(_, name, key)| name == "Favorites" && key == "collection-1"));
    sqlx::query(
        r#"INSERT INTO "Tag" ("Name", "ExternalCollectionId", "ExternalTypeId", "MovieMetadataId")
        SELECT 'Favorites', ?, '319', "Id" FROM "MovieMetadata" WHERE "MovieId" IN (SELECT "MediaItemId" FROM "CollectionItem") ORDER BY "Id" LIMIT 1"#,
    )
    .bind("collection-1")
    .execute(&pool)
    .await
    .unwrap();
    let protected = common::collection_tag_rows(&pool, "319").await;
    sync_emby_collection_library(&pool, &service, &config, 1, "collections-root")
        .await
        .unwrap();
    assert_eq!(
        common::collection_tag_rows(&pool, "emby/collection").await,
        owned
    );
    members.delete_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "collection-1");
            then.status(200)
                .json_body(json!({"TotalRecordCount": 0, "Items": []}));
        })
        .await;
    sync_emby_collection_library(&pool, &service, &config, 1, "collections-root")
        .await
        .unwrap();
    assert!(common::collection_tag_rows(&pool, "emby/collection")
        .await
        .is_empty());
    assert_eq!(common::collection_tag_rows(&pool, "319").await, protected);
    assert_eq!(count(&pool, "CollectionItem").await, 0);
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn emby_sync_maps_mock_show_response_to_media_rows() {
    let server = MockServer::start_async().await;
    let shows = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-shows")
                .query_param("includeItemTypes", "Series");
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Name": "Example Show",
                        "Id": "show-1",
                        "Etag": "show-etag",
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "Overview": "An example series.",
                        "ProductionYear": 2026,
                        "PremiereDate": "2026-04-01T00:00:00Z",
                        "OfficialRating": "TV-PG",
                        "Genres": ["Drama", "Mystery"],
                        "TagItems": [{ "Name": "serialized" }],
                        "Studios": [{ "Name": "Example Studio" }],
                        "People": [
                            { "Name": "Show Actor", "Type": "Actor", "Role": "Lead" }
                        ],
                        "ProviderIds": {
                            "Imdb": "ttshow",
                            "Tmdb": "101",
                            "Tvdb": "202"
                        },
                        "ImageTags": { "Primary": "show-poster", "Thumb": "show-thumb" },
                        "BackdropImageTags": ["show-fanart"]
                    }
                ]
            }));
        })
        .await;
    let seasons = server
        .mock_async(|when, then| {
            when.method(GET).path("/Shows/show-1/Seasons").query_param(
                "fields",
                "Path,DateCreated,Etag,ProviderIds,PremiereDate,ImageTags",
            );
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Name": "Season 1",
                        "Id": "season-1",
                        "Etag": "season-etag",
                        "IndexNumber": 1,
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "ProductionYear": 2026,
                        "PremiereDate": "2026-04-01T00:00:00Z",
                        "ProviderIds": {
                            "Tvdb": "303"
                        },
                        "ImageTags": { "Primary": "season-poster" },
                        "BackdropImageTags": ["season-fanart"]
                    }
                ]
            }));
        })
        .await;
    let episodes = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Shows/show-1/Episodes")
                .query_param("seasonId", "season-1")
                .query_param(
                    "fields",
                    "Path,Genres,Tags,DateCreated,Etag,Overview,ProductionYear,PremiereDate,MediaSources,LocationType,ProviderIds,People,ImageTags,Chapters",
                );
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Name": "Pilot",
                        "Id": "episode-1",
                        "Etag": "episode-etag",
                        "Path": "/srv/media/show/s01e01.mkv",
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "RunTimeTicks": 1_800_000_000,
                        "LocationType": "FileSystem",
                        "Overview": "The story starts.",
                        "ProductionYear": 2026,
                        "PremiereDate": "2026-04-02T00:00:00Z",
                        "IndexNumber": 1,
                        "Genres": ["Drama", "Sci-Fi"],
                        "TagItems": [{ "Name": "pilot" }],
                        "People": [
                            { "Name": "Episode Actor", "Type": "Actor", "Role": "Guest" },
                            { "Name": "Episode Director", "Type": "Director" },
                            { "Name": "Episode Writer", "Type": "Writer" }
                        ],
                        "ProviderIds": {
                            "Imdb": "ttepisode",
                            "Tmdb": "404",
                            "Tvdb": "505"
                        },
                        "ImageTags": { "Primary": "episode-thumb" },
                        "Chapters": [
                            { "StartPositionTicks": 0, "Name": "Cold Open" }
                        ]
                    }
                ]
            }));
        })
        .await;
    let playback = server
        .mock_async(|when, then| {
            when.method(GET).path("/Items/episode-1/PlaybackInfo");
            then.status(200).json_body(json!({
                "MediaSources": [
                    {
                        "Id": "selected-episode-source",
                        "Path": "/srv/media/show/s01e01.mkv",
                        "RunTimeTicks": 1_800_000_000,
                        "MediaStreams": [
                            {
                                "Type": "Video",
                                "Codec": "hevc",
                                "Width": 1280,
                                "Height": 720,
                                "Index": 0,
                                "IsDefault": true,
                                "AspectRatio": "16:9",
                                "RealFrameRate": 25.0
                            },
                            {
                                "Type": "Audio",
                                "Codec": "aac",
                                "Language": "eng",
                                "Channels": 2,
                                "Index": 1,
                                "IsDefault": true
                            },
                            {
                                "Type": "Subtitle",
                                "Codec": "subrip",
                                "Language": "eng",
                                "Index": 2
                            },
                            {
                                "Type": "Subtitle",
                                "Codec": "pgssub",
                                "Language": "eng",
                                "Index": 3
                            }
                        ]
                    }
                ]
            }));
        })
        .await;

    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "MediaBrowser Token=\"test\"".to_owned(),
    };

    let synced = sync_emby_show_library(&pool, &service, &config, 1, 1, "library-shows", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "EmbyShow").await, 1);
    assert_eq!(count(&pool, "ShowMetadata").await, 1);
    assert_eq!(count(&pool, "EmbySeason").await, 1);
    assert_eq!(count(&pool, "SeasonMetadata").await, 1);
    assert_eq!(count(&pool, "EmbyEpisode").await, 1);
    assert_eq!(count(&pool, "EpisodeMetadata").await, 1);
    assert_eq!(count(&pool, "MetadataGuid").await, 7);
    assert_eq!(count(&pool, "Artwork").await, 6);
    assert_eq!(count(&pool, "Genre").await, 4);
    assert_eq!(count(&pool, "Tag").await, 2);
    assert_eq!(count(&pool, "Studio").await, 1);
    assert_eq!(count(&pool, "Actor").await, 2);
    assert_eq!(count(&pool, "Director").await, 1);
    assert_eq!(count(&pool, "Writer").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    let selected: Option<String> = sqlx::query_scalar(
        r#"SELECT f."ProviderMediaSourceId" FROM "MediaFile" f JOIN "MediaVersion" v ON v."Id"=f."MediaVersionId" WHERE v."EpisodeId" IS NOT NULL"#,
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(selected.as_deref(), Some("selected-episode-source"));
    assert_eq!(count(&pool, "MediaStream").await, 4);
    assert_eq!(count(&pool, "MediaChapter").await, 1);
    // Both subtitle streams (text + image) are persisted as Subtitle rows with
    // IsExtracted=0. The image-codec filter only applies at *extraction* time.
    assert_eq!(count(&pool, "Subtitle").await, 2);
    assert_eq!(
        subtitle_rows(&pool).await,
        vec![
            (2, Some("subrip".to_owned()), 0, 0),
            (3, Some("pgssub".to_owned()), 0, 0),
        ]
    );
    assert_eq!(
        provider_library_item_id(&pool, "EmbyLibrary", 1).await,
        Some("library-shows".to_owned())
    );
    insta::assert_json_snapshot!(emby_projected_rows(&pool).await);
    shows.assert_async().await;
    seasons.assert_async().await;
    episodes.assert_async().await;
    playback.assert_async().await;
}

#[tokio::test]
async fn emby_single_show_sync_targets_one_show_and_honors_episode_etags() {
    let server = MockServer::start_async().await;
    // Single-show fetch: /Items?parentId=library-shows&ids=show-target&includeItemTypes=Series&recursive=false&limit=1
    let single_show = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-shows")
                .query_param("ids", "show-target")
                .query_param("includeItemTypes", "Series")
                .query_param("recursive", "false")
                .query_param("limit", "1");
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Name": "Target Show",
                        "Id": "show-target",
                        "Etag": "show-target-etag",
                        "DateCreated": "2026-04-17T10:00:00Z"
                    }
                ]
            }));
        })
        .await;
    let seasons = server
        .mock_async(|when, then| {
            when.method(GET).path("/Shows/show-target/Seasons");
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Name": "Season 1",
                        "Id": "season-target",
                        "Etag": "season-target-etag",
                        "IndexNumber": 1,
                        "DateCreated": "2026-04-17T10:00:00Z"
                    }
                ]
            }));
        })
        .await;
    let filtered_page = mock_filtered_specials_page(&server).await;
    let episodes = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Shows/show-target/Episodes")
                .query_param("seasonId", "season-target")
                .query_param("startIndex", "100");
            then.status(200).json_body(json!({
                "TotalRecordCount": 101,
                "Items": [
                    {
                        "Name": "Pilot",
                        "Id": "episode-target",
                        "SeasonId": "season-target",
                        "Etag": "episode-target-etag",
                        "Path": "/srv/media/show/target.mkv",
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "RunTimeTicks": 1_800_000_000,
                        "LocationType": "FileSystem",
                        "IndexNumber": 1
                    }
                ]
            }));
        })
        .await;
    let playback = mock_emby_playback(&server, "episode-target").await;

    let pool = common::test_db().await;
    seed_emby_show_scan_config(&pool, &server.base_url()).await;

    let synced = sync_emby_show_by_id(&pool, 1, 10, false, None)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    // Only the target show was synced; the other seeded show row is untouched.
    assert_eq!(count(&pool, "EmbyShow").await, 2);
    assert_eq!(count(&pool, "EmbySeason").await, 1);
    assert_eq!(count(&pool, "EmbyEpisode").await, 1);
    assert_eq!(media_file_path(&pool).await, "/srv/media/show/target.mkv");

    // Non-deep re-run should etag-skip and not re-upsert the file path.
    sqlx::query(r#"UPDATE "MediaFile" SET "Path" = '/srv/media/show/kept.mkv'"#)
        .execute(&pool)
        .await
        .unwrap();

    let original_episode = common::displace_episode_season(&pool).await;

    let shallow_synced = sync_emby_show_by_id(&pool, 1, 10, false, None)
        .await
        .unwrap();
    assert_eq!(shallow_synced, 1);
    common::assert_episode_parent(&pool, original_episode).await;
    assert_eq!(media_file_path(&pool).await, "/srv/media/show/kept.mkv");
    // Shallow re-run should NOT hit the per-episode playback endpoint.
    assert_eq!(playback.calls_async().await, 1);

    // Deep re-run should bypass the etag skip and re-upsert the file path.
    let deep_synced = sync_emby_show_by_id(&pool, 1, 10, true, None)
        .await
        .unwrap();
    assert_eq!(deep_synced, 1);
    assert_eq!(media_file_path(&pool).await, "/srv/media/show/target.mkv");

    assert_eq!(single_show.calls_async().await, 3);
    assert_eq!(seasons.calls_async().await, 3);
    assert_eq!(episodes.calls_async().await, 3);
    assert_eq!(filtered_page.calls_async().await, 3);
    // Playback was hit on the first sync and the deep re-run, not the shallow.
    assert_eq!(playback.calls_async().await, 2);
}

async fn mock_filtered_specials_page(server: &MockServer) -> Mock<'_> {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Shows/show-target/Episodes")
                .query_param("seasonId", "season-target")
                .query_param("startIndex", "0");
            then.status(200).json_body(json!({
                "TotalRecordCount": 101,
                "Items": (0..100).map(|index| json!({
                    "Id": format!("special-{index}"), "Name": "Special",
                    "Path": format!("/fixture/special-{index}.mkv"), "SeasonId": "specials"
                })).collect::<Vec<_>>()
            }));
        })
        .await
}

async fn seed_emby_show_scan_config(pool: &SqlitePool, base_url: &str) {
    sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "EmbyMediaSource" ("Id", "ServerName")
           VALUES (1, 'Emby Test')"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "EmbyConnection" ("Id", "Address", "EmbyMediaSourceId")
           VALUES (1, ?, 1)"#,
    )
    .bind(base_url)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key", "Value")
           VALUES (?, 'emby-api-key')"#,
    )
    .bind(format!("emby.api_key.{base_url}"))
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
        r#"INSERT INTO "EmbyLibrary" ("Id", "ItemId", "ShouldSyncItems")
           VALUES (1, 'library-shows', 1)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path")
           VALUES (1, 1, 'emby://1')"#,
    )
    .execute(pool)
    .await
    .unwrap();

    for (id, item_id, title) in [
        (10, "show-target", "Target Show"),
        (11, "show-other", "Other Show"),
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
        sqlx::query(r#"INSERT INTO "EmbyShow" ("Id", "ItemId", "Etag") VALUES (?, ?, 'seed')"#)
            .bind(id)
            .bind(item_id)
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

async fn mock_movie(server: &MockServer) {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-1")
                .query_param("includeItemTypes", "Movie");
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Name": "Big Buck Bunny",
                        "Id": "movie-1",
                        "Etag": "etag-1",
                        "Path": "/srv/media/bbb.mp4",
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "RunTimeTicks": 120_000_000,
                        "LocationType": "FileSystem"
                    }
                ]
            }));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/Items/movie-1/PlaybackInfo");
            then.status(200).json_body(json!({
                "MediaSources": [
                    {
                        "RunTimeTicks": 120_000_000,
                        "MediaStreams": [
                            {
                                "Type": "Video",
                                "Codec": "h264",
                                "Width": 1920,
                                "Height": 1080,
                                "Index": 0,
                                "IsDefault": true
                            }
                        ]
                    }
                ]
            }));
        })
        .await;
}

async fn mock_emby_libraries(server: &MockServer) -> Mock<'_> {
    server
        .mock_async(|when, then| {
            when.method(GET).path("/Library/VirtualFolders");
            then.status(200).json_body(json!([
                { "Name": "Movies", "CollectionType": "movies", "ItemId": "library-1" }
            ]));
        })
        .await
}

async fn mock_detailed_movie_items(server: &MockServer) -> Mock<'_> {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "library-1")
                .query_param(
                    "fields",
                    "Path,Genres,Tags,DateCreated,Etag,Overview,Taglines,Studios,People,ProductionYear,PremiereDate,MediaSources,OfficialRating,ProviderIds,Chapters",
                )
                .query_param("includeItemTypes", "Movie");
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Name": "Big Buck Bunny",
                        "Id": "movie-1",
                        "Etag": "etag-1",
                        "Path": "/srv/media/bbb.mp4",
                        "DateCreated": "2026-04-17T10:00:00Z",
                        "RunTimeTicks": 120_000_000,
                        "Overview": "A large rabbit has a larger problem.",
                        "ProductionYear": 2008,
                        "PremiereDate": "2008-04-10T00:00:00Z",
                        "OfficialRating": "G",
                        "Taglines": ["A short movie"],
                        "ProviderIds": {
                            "Imdb": "tt1254207",
                            "Tmdb": "10378",
                            "Tvdb": "98765"
                        },
                        "Genres": ["Animation", "Short"],
                        "TagItems": [{ "Name": "cgi" }],
                        "People": [
                            { "Name": "Sacha Goedegebure", "Type": "Actor", "Role": "Bunny" },
                            { "Name": "Sacha Goedegebure", "Type": "Director" },
                            { "Name": "William Reynish", "Type": "Writer" }
                        ],
                        "ImageTags": { "Primary": "poster-tag" },
                        "BackdropImageTags": ["fanart-tag"],
                        "LocationType": "FileSystem",
                        "Chapters": [
                            { "StartPositionTicks": 0, "Name": "Intro" },
                            { "StartPositionTicks": 50_000_000, "Name": "Main" }
                        ]
                    }
                ]
            }));
        })
        .await
}

async fn mock_detailed_movie_playback(server: &MockServer) -> Mock<'_> {
    server
        .mock_async(|when, then| {
            when.method(GET).path("/Items/movie-1/PlaybackInfo");
            then.status(200).json_body(json!({
                "MediaSources": [
                    {
                        "Id": "selected-movie-source",
                        "Path": "/srv/media/bbb.mp4",
                        "RunTimeTicks": 120_000_000,
                        "MediaStreams": [
                            {
                                "Type": "Video",
                                "Codec": "h264",
                                "Width": 1920,
                                "Height": 1080,
                                "Index": 0,
                                "IsDefault": true,
                                "AspectRatio": "16:9",
                                "RealFrameRate": 23.976
                            },
                            {
                                "Type": "Audio",
                                "Codec": "aac",
                                "Language": "eng",
                                "Channels": 2,
                                "Index": 1,
                                "IsDefault": true,
                                "Title": "Stereo"
                            },
                            {
                                "Type": "Subtitle",
                                "Codec": "subrip",
                                "Language": "eng",
                                "Index": 2,
                                "IsDefault": true
                            },
                            {
                                "Type": "Subtitle",
                                "Codec": "pgssub",
                                "Language": "eng",
                                "Index": 3
                            }
                        ]
                    }
                ]
            }));
        })
        .await
}

async fn mock_emby_playback<'a>(server: &'a MockServer, item_id: &str) -> Mock<'a> {
    let path = format!("/Items/{item_id}/PlaybackInfo");
    server
        .mock_async(|when, then| {
            when.method(GET).path(path.as_str());
            then.status(200).json_body(json!({
                "MediaSources": [
                    {
                        "RunTimeTicks": 120_000_000,
                        "MediaStreams": [
                            {
                                "Type": "Video",
                                "Codec": "h264",
                                "Width": 1920,
                                "Height": 1080,
                                "Index": 0,
                                "IsDefault": true
                            }
                        ]
                    }
                ]
            }));
        })
        .await
}

fn movie_page_json(total_record_count: i32, item_id: &str, path: &str) -> serde_json::Value {
    json!({
        "TotalRecordCount": total_record_count,
        "Items": [
            {
                "Name": item_id,
                "Id": item_id,
                "Etag": format!("{item_id}-etag"),
                "Path": path,
                "DateCreated": "2026-04-17T10:00:00Z",
                "RunTimeTicks": 120_000_000,
                "LocationType": "FileSystem"
            }
        ]
    })
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}""#);
    sqlx::query(ersatzrs_infra::db::assert_sql_safe(&sql))
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

async fn movie_metadata_title(pool: &SqlitePool, movie_id: i64) -> String {
    sqlx::query(r#"SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = ?"#)
        .bind(movie_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Title")
}

async fn provider_library_item_id(pool: &SqlitePool, table: &str, id: i64) -> Option<String> {
    let sql = format!(r#"SELECT "ItemId" AS "Value" FROM "{table}" WHERE "Id" = ?"#);
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

async fn media_item_state(pool: &SqlitePool) -> i64 {
    sqlx::query(r#"SELECT "State" FROM "MediaItem" LIMIT 1"#)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("State")
}

/// Returns `(StreamIndex, Codec, SubtitleKind, IsExtracted)` for every Subtitle
/// row, ordered by stream index.
async fn subtitle_rows(pool: &SqlitePool) -> Vec<(i64, Option<String>, i64, i64)> {
    sqlx::query(
        r#"SELECT "StreamIndex", "Codec", "SubtitleKind", "IsExtracted"
           FROM "Subtitle" ORDER BY "StreamIndex""#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
    .into_iter()
    .map(|row| {
        (
            row.get::<i64, _>("StreamIndex"),
            row.get::<Option<String>, _>("Codec"),
            row.get::<i64, _>("SubtitleKind"),
            row.get::<i64, _>("IsExtracted"),
        )
    })
    .collect()
}

async fn emby_projected_rows(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query(
        r#"SELECT "jfs"."ItemId" AS "ShowItemId",
                  "jfse"."ItemId" AS "SeasonItemId",
                  "s"."SeasonNumber" AS "SeasonNumber",
                  "sm"."Title" AS "SeasonTitle",
                  "jfe"."ItemId" AS "EpisodeItemId",
                  "em"."Title" AS "EpisodeTitle",
                  "em"."EpisodeNumber" AS "EpisodeNumber",
                  "mf"."Path" AS "Path",
                  "mv"."Duration" AS "Duration",
                  "mv"."Width" AS "Width",
                  "mv"."Height" AS "Height",
                  COUNT(DISTINCT "ms"."Id") AS "StreamCount",
                  COUNT(DISTINCT "mc"."Id") AS "ChapterCount"
           FROM "EmbyShow" AS "jfs"
           JOIN "Season" AS "s" ON "s"."ShowId" = "jfs"."Id"
           JOIN "EmbySeason" AS "jfse" ON "jfse"."Id" = "s"."Id"
           JOIN "SeasonMetadata" AS "sm" ON "sm"."SeasonId" = "s"."Id"
           JOIN "Episode" AS "e" ON "e"."SeasonId" = "s"."Id"
           JOIN "EmbyEpisode" AS "jfe" ON "jfe"."Id" = "e"."Id"
           JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "e"."Id"
           JOIN "MediaVersion" AS "mv" ON "mv"."EpisodeId" = "e"."Id"
           JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id"
           LEFT JOIN "MediaStream" AS "ms" ON "ms"."MediaVersionId" = "mv"."Id"
           LEFT JOIN "MediaChapter" AS "mc" ON "mc"."MediaVersionId" = "mv"."Id"
           GROUP BY "jfs"."ItemId", "jfse"."ItemId", "s"."SeasonNumber",
                    "sm"."Title", "jfe"."ItemId", "em"."Title", "em"."EpisodeNumber",
                    "mf"."Path", "mv"."Duration", "mv"."Width", "mv"."Height""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();

    serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "show_item_id": row.get::<String, _>("ShowItemId"),
                    "season_item_id": row.get::<String, _>("SeasonItemId"),
                    "season_number": row.get::<i64, _>("SeasonNumber"),
                    "season_title": row.get::<String, _>("SeasonTitle"),
                    "episode_item_id": row.get::<String, _>("EpisodeItemId"),
                    "episode_title": row.get::<String, _>("EpisodeTitle"),
                    "episode_number": row.get::<i64, _>("EpisodeNumber"),
                    "path": row.get::<String, _>("Path"),
                    "duration": row.get::<String, _>("Duration"),
                    "width": row.get::<i64, _>("Width"),
                    "height": row.get::<i64, _>("Height"),
                    "stream_count": row.get::<i64, _>("StreamCount"),
                    "chapter_count": row.get::<i64, _>("ChapterCount")
                })
            })
            .collect(),
    )
}

async fn emby_collection_rows(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query(
        r#"SELECT "c"."Name" AS "Name",
                  "jfc"."ItemId" AS "CollectionItemId",
                  "jfm"."ItemId" AS "MovieItemId",
                  "ci"."CustomIndex" AS "CustomIndex"
           FROM "Collection" AS "c"
           JOIN "EmbyCollection" AS "jfc" ON "jfc"."Id" = "c"."Id"
           JOIN "CollectionItem" AS "ci" ON "ci"."CollectionId" = "c"."Id"
           JOIN "EmbyMovie" AS "jfm" ON "jfm"."Id" = "ci"."MediaItemId""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();

    serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                json!({
                    "name": row.get::<String, _>("Name"),
                    "collection_item_id": row.get::<String, _>("CollectionItemId"),
                    "movie_item_id": row.get::<String, _>("MovieItemId"),
                    "custom_index": row.get::<i64, _>("CustomIndex")
                })
            })
            .collect(),
    )
}

#[tokio::test]
async fn emby_external_subtitles_recover_on_ordinary_scan_after_repair() {
    let server = MockServer::start_async().await;
    let _items = mock_detailed_movie_items(&server).await;
    let playback = server
        .mock_async(|when, then| {
            when.method(GET).path("/Items/movie-1/PlaybackInfo");
            then.status(200).json_body(json!({"MediaSources": [{
            "Id": "shared-provider-source", "RunTimeTicks": 120_000_000,
            "MediaStreams": [
                {"Type":"Video","Index":0,"Codec":"h264","Width":320,"Height":180},
                {"Type":"Audio","Index":1,"Codec":"aac","Language":"eng","Channels":2},
                {"Type":"Subtitle","Index":2,"Codec":"subrip","Language":"eng","IsExternal":true},
                {"Type":"Subtitle","Index":3,"Codec":"subrip","Language":"nld","IsExternal":true}
            ]
        }]}));
        })
        .await;
    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "test".to_owned(),
    };
    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "Subtitle" WHERE "SubtitleKind" = 1"#)
        .execute(&pool)
        .await
        .unwrap();
    // The unchanged provider Etag reproduces missing tracks staying missing.
    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();
    assert_eq!(playback.calls_async().await, 1);
    assert_eq!(count(&pool, "Subtitle").await, 0);
    sqlx::query(r#"INSERT INTO "EmbyConnection" ("Address","EmbyMediaSourceId") VALUES (?,1)"#)
        .bind(server.base_url())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?, 'synthetic-subtitle-key')"#,
    )
    .bind(format!("emby.api_key.{}", server.base_url()))
    .execute(&pool)
    .await
    .unwrap();
    let subtitles = server
        .mock_async(|when, then| {
            when.method(GET)
                .path_includes("/Subtitles/")
                .header("X-Emby-Token", "synthetic-subtitle-key");
            then.status(200)
                .body("1\n00:00:00,000 --> 00:00:01,000\nSynthetic captions\n");
        })
        .await;
    let artifact_cache = tempfile::tempdir().unwrap();
    // Apply the exact data repair to a populated fixture, then use a normal scan.
    sqlx::raw_sql(include_str!(
        "../../../migrations/202609260808_provider_external_subtitle_rescan.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();
    assert_eq!(playback.calls_async().await, 2);
    for _ in 0..2 {
        assert_eq!(
            ersatzrs_infra::subtitle_cleanup::clean_orphaned_subtitles(&pool, 100_000)
                .await
                .unwrap(),
            0
        );
        let tracks: Vec<(i64, String, Option<String>)> = sqlx::query_as(
            r#"SELECT "StreamIndex","Language","Path" FROM "Subtitle" WHERE "SubtitleKind" = 1 ORDER BY "StreamIndex""#,
        ).fetch_all(&pool).await.unwrap();
        assert_eq!(
            tracks,
            vec![
                (2, "eng".into(), Some("shared-provider-source".into())),
                (3, "nld".into(), Some("shared-provider-source".into()))
            ]
        );
        assert_restored_provider_inputs(&pool, &artifact_cache).await;
        sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
            .await
            .unwrap();
    }
    assert_eq!(
        playback.calls_async().await,
        2,
        "subsequent ordinary scans retain the refreshed Etag"
    );
    assert_eq!(subtitles.calls_async().await, 8);
    subtitles.delete_async().await;
    assert_provider_retrieval_fails_closed(&pool, &server).await;
}

async fn assert_restored_provider_inputs(pool: &SqlitePool, artifact_cache: &tempfile::TempDir) {
    let item_id: i64 = sqlx::query_scalar(r#"SELECT "Id" FROM "Movie""#)
        .fetch_one(pool)
        .await
        .unwrap();
    for (language, index) in [("eng", 2), ("nld", 3)] {
        let selected = ersatzrs_infra::playback::select_playback_subtitle(
            pool,
            item_id,
            &[language.into()],
            ersatzrs_domain::ChannelSubtitleMode::Any,
            ersatzrs_domain::StreamingMode::TransportStream,
        )
        .await
        .unwrap()
        .expect("restored provider track is selectable");
        assert_eq!(selected.stream_index, index);
        assert_eq!(selected.language.as_deref(), Some(language));
        let binding = ersatzrs_infra::provider_subtitles::binding(pool, selected.id)
            .await
            .unwrap()
            .expect("provider subtitle binding");
        assert_eq!(binding.media_source_id, 1);
        let content = ersatzrs_infra::provider_subtitles::fetch(pool, selected.id)
            .await
            .unwrap()
            .expect("authenticated provider subtitle content");
        assert_eq!(content.extension, "srt");
        assert!(content.bytes.ends_with(b"Synthetic captions\n"));
        let prepared =
            ersatzrs_infra::provider_subtitles::prepare(pool, &selected, artifact_cache.path())
                .await
                .unwrap();
        assert_eq!(prepared.id, selected.id);
        assert_eq!(prepared.stream_index, selected.stream_index);
        let path = prepared.path.as_ref().expect("complete provider artifact");
        assert_eq!(tokio::fs::read(path).await.unwrap(), content.bytes);
        let input = ersatzrs_infra::playback::subtitle_input_for_stream(
            &prepared,
            "synthetic.mkv",
            ersatzrs_ffmpeg::OutputFormat::MpegTs,
            ersatzrs_domain::StreamingMode::TransportStream,
        )
        .expect("restored provider subtitle produces a native playback input");
        assert_eq!(&input.path, path);
        let mut stale_selection = selected.clone();
        stale_selection.stream_index += 1;
        assert!(
            ersatzrs_infra::provider_subtitles::prepare(
                pool,
                &stale_selection,
                artifact_cache.path(),
            )
            .await
            .is_err(),
            "a changed selected provider index must not silently fetch another track"
        );

        let endpoint = binding.endpoint("https://example.test/server/").unwrap();
        assert_eq!(endpoint.as_str(), format!("https://example.test/server/Videos/movie-1/shared-provider-source/Subtitles/{index}/Stream.srt"));
    }
}

async fn assert_provider_retrieval_fails_closed(pool: &SqlitePool, server: &MockServer) {
    let subtitle_id: i64 =
        sqlx::query_scalar(r#"SELECT "Id" FROM "Subtitle" WHERE "StreamIndex" = 2"#)
            .fetch_one(pool)
            .await
            .unwrap();
    let unexpected = server
        .mock_async(|when, then| {
            when.method(GET).path("/unexpected-subtitle-redirect");
            then.status(200).body("must not be requested");
        })
        .await;
    for status in [401, 302] {
        let rejected = server
            .mock_async(|when, then| {
                when.method(GET).path_includes("/Subtitles/");
                then.status(status).header(
                    "Location",
                    format!("{}/unexpected-subtitle-redirect", server.base_url()),
                );
            })
            .await;
        let error = ersatzrs_infra::provider_subtitles::fetch(pool, subtitle_id)
            .await
            .err()
            .expect("authentication and redirects must fail closed");
        let message = error.to_string();
        assert!(!message.contains("synthetic-subtitle-key"));
        assert!(!message.contains(&server.base_url()));
        rejected.delete_async().await;
    }
    assert_eq!(unexpected.calls_async().await, 0);
}

#[tokio::test]
async fn emby_collection_identity_isolates_servers_with_the_same_key_and_name() {
    let server = MockServer::start_async().await;
    mock_movie(&server).await;
    let (collections, members) = mock_emby_collection_pages(&server).await;

    let pool = common::test_db().await;
    let service = EmbyService::new();
    let config = EmbyConnectionConfig {
        address: server.base_url(),
        api_key: "MediaBrowser Token=\"test\"".to_owned(),
    };
    sync_emby_movie_library(&pool, &service, &config, 2, 99, "library-1", false)
        .await
        .unwrap();
    sync_emby_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    for source in [1, 2, 1, 2] {
        sync_emby_collection_library(&pool, &service, &config, source, "collections-root")
            .await
            .unwrap();
    }
    common::assert_source_owned_collections(&pool, "EmbyCollection", "emby/collection", 1).await;
    let tags = common::collection_tag_rows(&pool, "emby/collection").await;
    common::add_foreign_collection_member(&pool, "EmbyCollection").await;
    sync_emby_collection_library(&pool, &service, &config, 1, "collections-root")
        .await
        .unwrap();
    assert_eq!(
        common::collection_tag_rows(&pool, "emby/collection").await,
        tags
    );
    collections.assert_calls_async(5).await;
    members.assert_calls_async(5).await;
}

async fn mock_emby_collection_pages(
    server: &MockServer,
) -> (httpmock::Mock<'_>, httpmock::Mock<'_>) {
    let collections = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "collections-root")
                .query_param("includeItemTypes", "BoxSet");
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    { "Name": "Favorites", "Id": "collection-1", "Etag": "collection-etag" }
                ]
            }));
        })
        .await;
    let members = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "collection-1")
                .query_param("includeItemTypes", "Movie,Series,Season,Episode");
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Id": "movie-1",
                        "Type": "Movie",
                        "LocationType": "FileSystem"
                    }
                ]
            }));
        })
        .await;

    (collections, members)
}

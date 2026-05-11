#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{JellyfinConnectionConfig, JellyfinService as _};
use ersatzrs_infra::jellyfin::{
    sync_jellyfin_collection_library, sync_jellyfin_movie_library, sync_jellyfin_show_library,
    JellyfinService,
};
use httpmock::prelude::*;
use httpmock::Mock;
use serde_json::json;
use sqlx::{Row, SqlitePool};

#[tokio::test]
async fn jellyfin_sync_maps_mock_movie_response_to_media_rows() {
    let server = MockServer::start_async().await;
    let libraries = server
        .mock_async(|when, then| {
            when.method(GET).path("/Library/VirtualFolders");
            then.status(200).json_body(json!([
                { "Name": "Movies", "CollectionType": "movies", "ItemId": "library-1" }
            ]));
        })
        .await;
    let items = server
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
        .await;
    let playback = server
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
                            }
                        ]
                    }
                ]
            }));
        })
        .await;

    let pool = common::test_db().await;
    let service = JellyfinService::new();
    let config = JellyfinConnectionConfig {
        address: server.base_url(),
        authorization_header: "MediaBrowser Token=\"test\"".to_owned(),
    };

    let libraries_result = service.list_libraries(&config).await.unwrap();
    assert_eq!(libraries_result[0].item_id, "library-1");

    let synced = sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "MediaItem").await, 1);
    assert_eq!(count(&pool, "Movie").await, 1);
    assert_eq!(count(&pool, "JellyfinMovie").await, 1);
    assert_eq!(count(&pool, "MovieMetadata").await, 1);
    assert_eq!(movie_metadata_title(&pool, 1).await, "Big Buck Bunny");
    assert_eq!(count(&pool, "MetadataGuid").await, 3);
    assert_eq!(count(&pool, "Artwork").await, 2);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    assert_eq!(count(&pool, "MediaStream").await, 2);
    assert_eq!(count(&pool, "MediaChapter").await, 2);
    assert_eq!(
        provider_library_item_id(&pool, "JellyfinLibrary", 1).await,
        Some("library-1".to_owned())
    );
    libraries.assert_async().await;
    items.assert_async().await;
    playback.assert_async().await;
}

#[tokio::test]
async fn jellyfin_sync_pages_movie_library_beyond_first_result_window() {
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
    let first_playback = mock_jellyfin_playback(&server, "movie-page-1").await;
    let second_playback = mock_jellyfin_playback(&server, "movie-page-2").await;

    let pool = common::test_db().await;
    let service = JellyfinService::new();
    let config = JellyfinConnectionConfig {
        address: server.base_url(),
        authorization_header: "MediaBrowser Token=\"test\"".to_owned(),
    };

    let synced = sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    assert_eq!(synced, 2);
    assert_eq!(count(&pool, "JellyfinMovie").await, 2);
    first_page.assert_async().await;
    second_page.assert_async().await;
    first_playback.assert_async().await;
    second_playback.assert_async().await;
}

#[tokio::test]
async fn jellyfin_sync_skips_unchanged_movie_etag_before_playback_lookup() {
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
    let playback = mock_jellyfin_playback(&server, "movie-skip").await;

    let pool = common::test_db().await;
    let service = JellyfinService::new();
    let config = JellyfinConnectionConfig {
        address: server.base_url(),
        authorization_header: "MediaBrowser Token=\"test\"".to_owned(),
    };

    sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path" = '/srv/media/kept.mkv'"#)
        .execute(&pool)
        .await
        .unwrap();

    let synced = sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(media_file_path(&pool).await, "/srv/media/kept.mkv");
    assert_eq!(playback.calls_async().await, 1);
    assert_eq!(items.calls_async().await, 2);
}

#[tokio::test]
async fn jellyfin_sync_restores_non_normal_movie_with_matching_etag() {
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
                "/srv/media/restored.mkv",
            ));
        })
        .await;
    let playback = mock_jellyfin_playback(&server, "movie-restore").await;

    let pool = common::test_db().await;
    let service = JellyfinService::new();
    let config = JellyfinConnectionConfig {
        address: server.base_url(),
        authorization_header: "MediaBrowser Token=\"test\"".to_owned(),
    };

    sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaItem" SET "State" = 2"#)
        .execute(&pool)
        .await
        .unwrap();

    sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    assert_eq!(media_item_state(&pool).await, 0);
    assert_eq!(media_file_path(&pool).await, "/srv/media/restored.mkv");
    assert_eq!(playback.calls_async().await, 2);
}

#[tokio::test]
async fn jellyfin_sync_maps_mock_collection_response_to_collection_rows() {
    let server = MockServer::start_async().await;
    mock_movie(&server).await;
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

    let pool = common::test_db().await;
    let service = JellyfinService::new();
    let config = JellyfinConnectionConfig {
        address: server.base_url(),
        authorization_header: "MediaBrowser Token=\"test\"".to_owned(),
    };
    sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1", false)
        .await
        .unwrap();

    let synced = sync_jellyfin_collection_library(&pool, &service, &config, 1, "collections-root")
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "Collection").await, 1);
    assert_eq!(count(&pool, "JellyfinCollection").await, 1);
    assert_eq!(count(&pool, "CollectionItem").await, 1);
    insta::assert_json_snapshot!(jellyfin_collection_rows(&pool).await);
    collections.assert_async().await;
    members.assert_async().await;
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn jellyfin_sync_maps_mock_show_response_to_media_rows() {
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
                        "DateCreated": "2026-04-17T10:00:00Z"
                    }
                ]
            }));
        })
        .await;
    let seasons = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "show-1")
                .query_param("includeItemTypes", "Season");
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [
                    {
                        "Name": "Season 1",
                        "Id": "season-1",
                        "Etag": "season-etag",
                        "IndexNumber": 1,
                        "DateCreated": "2026-04-17T10:00:00Z"
                    }
                ]
            }));
        })
        .await;
    let episodes = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("parentId", "season-1")
                .query_param("includeItemTypes", "Episode");
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
                            }
                        ]
                    }
                ]
            }));
        })
        .await;

    let pool = common::test_db().await;
    let service = JellyfinService::new();
    let config = JellyfinConnectionConfig {
        address: server.base_url(),
        authorization_header: "MediaBrowser Token=\"test\"".to_owned(),
    };

    let synced = sync_jellyfin_show_library(&pool, &service, &config, 1, 1, "library-shows", false)
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "JellyfinShow").await, 1);
    assert_eq!(count(&pool, "JellyfinSeason").await, 1);
    assert_eq!(count(&pool, "JellyfinEpisode").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    assert_eq!(count(&pool, "MediaStream").await, 2);
    assert_eq!(count(&pool, "MediaChapter").await, 1);
    assert_eq!(
        provider_library_item_id(&pool, "JellyfinLibrary", 1).await,
        Some("library-shows".to_owned())
    );
    insta::assert_json_snapshot!(jellyfin_projected_rows(&pool).await);
    shows.assert_async().await;
    seasons.assert_async().await;
    episodes.assert_async().await;
    playback.assert_async().await;
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

async fn mock_jellyfin_playback<'a>(server: &'a MockServer, item_id: &str) -> Mock<'a> {
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
    sqlx::query(&sql)
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
    sqlx::query(&sql)
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

async fn media_item_state(pool: &SqlitePool) -> i64 {
    sqlx::query(r#"SELECT "State" FROM "MediaItem" LIMIT 1"#)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("State")
}

async fn jellyfin_projected_rows(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query(
        r#"SELECT "jfs"."ItemId" AS "ShowItemId",
                  "jfse"."ItemId" AS "SeasonItemId",
                  "s"."SeasonNumber" AS "SeasonNumber",
                  "jfe"."ItemId" AS "EpisodeItemId",
                  "mf"."Path" AS "Path",
                  "mv"."Duration" AS "Duration",
                  "mv"."Width" AS "Width",
                  "mv"."Height" AS "Height",
                  COUNT(DISTINCT "ms"."Id") AS "StreamCount",
                  COUNT(DISTINCT "mc"."Id") AS "ChapterCount"
           FROM "JellyfinShow" AS "jfs"
           JOIN "Season" AS "s" ON "s"."ShowId" = "jfs"."Id"
           JOIN "JellyfinSeason" AS "jfse" ON "jfse"."Id" = "s"."Id"
           JOIN "Episode" AS "e" ON "e"."SeasonId" = "s"."Id"
           JOIN "JellyfinEpisode" AS "jfe" ON "jfe"."Id" = "e"."Id"
           JOIN "MediaVersion" AS "mv" ON "mv"."EpisodeId" = "e"."Id"
           JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id"
           LEFT JOIN "MediaStream" AS "ms" ON "ms"."MediaVersionId" = "mv"."Id"
           LEFT JOIN "MediaChapter" AS "mc" ON "mc"."MediaVersionId" = "mv"."Id"
           GROUP BY "jfs"."ItemId", "jfse"."ItemId", "s"."SeasonNumber",
                    "jfe"."ItemId", "mf"."Path", "mv"."Duration", "mv"."Width", "mv"."Height""#,
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
                    "episode_item_id": row.get::<String, _>("EpisodeItemId"),
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

async fn jellyfin_collection_rows(pool: &SqlitePool) -> serde_json::Value {
    let rows = sqlx::query(
        r#"SELECT "c"."Name" AS "Name",
                  "jfc"."ItemId" AS "CollectionItemId",
                  "jfm"."ItemId" AS "MovieItemId",
                  "ci"."CustomIndex" AS "CustomIndex"
           FROM "Collection" AS "c"
           JOIN "JellyfinCollection" AS "jfc" ON "jfc"."Id" = "c"."Id"
           JOIN "CollectionItem" AS "ci" ON "ci"."CollectionId" = "c"."Id"
           JOIN "JellyfinMovie" AS "jfm" ON "jfm"."Id" = "ci"."MediaItemId""#,
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

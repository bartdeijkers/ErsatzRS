#![cfg(not(feature = "mysql"))]
mod common;

use ersatzrs_core::repositories::{JellyfinConnectionConfig, JellyfinService as _};
use ersatzrs_infra::jellyfin::{
    sync_jellyfin_collection_library, sync_jellyfin_movie_library, sync_jellyfin_show_library,
    JellyfinService,
};
use httpmock::prelude::*;
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

    let synced = sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1")
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "MediaItem").await, 1);
    assert_eq!(count(&pool, "Movie").await, 1);
    assert_eq!(count(&pool, "JellyfinMovie").await, 1);
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
    sync_jellyfin_movie_library(&pool, &service, &config, 1, 1, "library-1")
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

    let synced = sync_jellyfin_show_library(&pool, &service, &config, 1, 1, "library-shows")
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

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}""#);
    sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
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

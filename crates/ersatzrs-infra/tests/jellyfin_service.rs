mod common;

use ersatzrs_core::repositories::{JellyfinConnectionConfig, JellyfinService as _};
use ersatzrs_infra::jellyfin::{sync_jellyfin_movie_library, JellyfinService};
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
    libraries.assert_async().await;
    items.assert_async().await;
    playback.assert_async().await;
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}""#);
    sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

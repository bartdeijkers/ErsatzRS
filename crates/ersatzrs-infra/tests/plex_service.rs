mod common;

use ersatzrs_infra::plex::{
    sync_plex_movie_library, sync_plex_show_library, PlexConnectionConfig, PlexService,
};
use httpmock::prelude::*;
use serde_json::json;
use sqlx::{Row, SqlitePool};

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

    let synced = sync_plex_movie_library(&pool, &service, &config, 1, 1, "1")
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "PlexMovie").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    assert_eq!(count(&pool, "PlexMediaFile").await, 1);
    assert_eq!(count(&pool, "MediaStream").await, 2);
    insta::assert_json_snapshot!(plex_movie_rows(&pool).await);
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

    let synced = sync_plex_show_library(&pool, &service, &config, 1, 1, "2")
        .await
        .unwrap();

    assert_eq!(synced, 1);
    assert_eq!(count(&pool, "PlexShow").await, 1);
    assert_eq!(count(&pool, "PlexSeason").await, 1);
    assert_eq!(count(&pool, "PlexEpisode").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    assert_eq!(count(&pool, "MediaStream").await, 2);
    insta::assert_json_snapshot!(plex_show_rows(&pool).await);
}

async fn mock_plex_movie_page(server: &MockServer) {
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/sections/1/all")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json("/library/metadata/100", "/library/parts/10", "/media/movie.mkv", 100)
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
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        media_json("/library/metadata/202", "/library/parts/20", "/media/show/s01e01.mkv", 202)
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

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}""#);
    sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
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

mod common;
#[path = "plex_service/networks.rs"]
mod networks;

use ersatzrs_domain::LibraryMediaKind;
use ersatzrs_infra::plex::{
    sync_plex_collection_library, sync_plex_movie_library, sync_plex_other_video_library,
    sync_plex_show_by_id, sync_plex_show_library, PlexConnectionConfig, PlexService,
};
use httpmock::prelude::*;
use serde_json::json;
use sqlx::{Row, SqlitePool};

#[tokio::test]
async fn plex_label_refresh_preserves_other_owners_and_survives_restart() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}?mode=rwc",
        directory.path().join("tags.db").display()
    );
    let mut pool = SqlitePool::connect(&url).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let server = MockServer::start_async().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "synthetic-plex-label-test".to_owned(),
    };
    let mut protected = Vec::<(i64, String, Option<String>, Option<String>)>::new();
    for (phase, name) in [Some("old"), Some("renamed"), None].into_iter().enumerate() {
        let mut item = media_json(
            "/library/metadata/1",
            "/library/parts/1",
            "/fixture/movie.mkv",
            1,
        );
        item["Label"] = name.map_or_else(
            || json!([]),
            |name| {
                json!([
                    { "id": 5, "tag": name }, { "id": "5", "tag": name }
                ])
            },
        );
        let response = server
            .mock_async(|when, then| {
                when.method(GET).path("/library/sections/1/all");
                then.status(200).json_body(json!({
                    "MediaContainer": { "totalSize": 1, "Metadata": [item] }
                }));
            })
            .await;
        let mut previous = None;
        for _ in 0..2 {
            sync_plex_movie_library(&pool, &service, &config, 1, 1, "1", true)
                .await
                .unwrap();
            let labels: Vec<(i64, String, Option<String>)> = sqlx::query_as(
                r#"SELECT "Id", "Name", "ExternalCollectionId" FROM "Tag"
                   WHERE "ExternalTypeId" = 'plex/label' ORDER BY "Id""#,
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(labels.len(), usize::from(name.is_some()));
            if let Some(name) = name {
                assert_eq!(labels[0].1, name);
                assert_eq!(labels[0].2.as_deref(), Some("5"));
            }
            if let Some(previous) = previous {
                assert_eq!(
                    labels, previous,
                    "identical scan retains label row identity"
                );
            }
            previous = Some(labels);
            let others: Vec<(i64, String, Option<String>, Option<String>)> = sqlx::query_as(
                r#"SELECT "Id", "Name", "ExternalCollectionId", "ExternalTypeId" FROM "Tag"
                   WHERE "ExternalTypeId" != 'plex/label' ORDER BY "Id""#,
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            assert_eq!(others, protected);
            assert_eq!(count(&pool, "MediaStream").await, 2);
        }
        response.delete_async().await;
        if phase == 0 {
            for owner in [
                "319",
                "nfo/country",
                "plex/collection",
                "jellyfin/collection",
                "emby/collection",
            ] {
                sqlx::query(
                    r#"INSERT INTO "Tag" ("Name", "ExternalCollectionId", "ExternalTypeId", "MovieMetadataId")
                       SELECT 'renamed', '5', ?, "Id" FROM "MovieMetadata""#,
                ).bind(owner).execute(&pool).await.unwrap();
            }
            protected = sqlx::query_as(
                r#"SELECT "Id", "Name", "ExternalCollectionId", "ExternalTypeId" FROM "Tag"
                   WHERE "ExternalTypeId" != 'plex/label' ORDER BY "Id""#,
            )
            .fetch_all(&pool)
            .await
            .unwrap();
            pool.close().await;
            pool = SqlitePool::connect(&url).await.unwrap();
        }
    }
}

#[tokio::test]
async fn plex_container_labels_reconcile_without_removing_networks_or_collections() {
    let server = MockServer::start_async().await;
    mock_plex_show_pages(&server).await;
    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };
    sync_plex_show_library(&pool, &service, &config, 1, 1, "2", true)
        .await
        .unwrap();
    let original: Vec<(i64, String, String)> = sqlx::query_as(
        r#"SELECT "Id", "Name", "ExternalCollectionId" FROM "Tag"
           WHERE "ShowMetadataId" IS NOT NULL OR "SeasonMetadataId" IS NOT NULL ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(original.len(), 2);
    assert_eq!(
        (&original[0].1, &original[0].2),
        (&"Show label".to_owned(), &"7".to_owned())
    );
    assert_eq!(
        (&original[1].1, &original[1].2),
        (&"Season label".to_owned(), &"8".to_owned())
    );
    for kind in ["Show", "Season"] {
        for owner in ["319", "plex/collection"] {
            sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
                r#"INSERT INTO "Tag" ("Name", "ExternalTypeId", "{kind}MetadataId")
                   SELECT 'Protected', ?, "Id" FROM "{kind}Metadata""#,
            )))
            .bind(owner)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(ersatzrs_infra::db::assert_sql_safe(format!(
            r#"INSERT INTO "Tag" ("Name", "{kind}MetadataId") SELECT 'Legacy stale', "Id" FROM "{kind}Metadata""#,
        ))).execute(&pool).await.unwrap();
    }
    for _ in 0..2 {
        sync_plex_show_library(&pool, &service, &config, 1, 1, "2", true)
            .await
            .unwrap();
        let labels: Vec<(i64, String, String)> = sqlx::query_as(
            r#"SELECT "Id", "Name", "ExternalCollectionId" FROM "Tag"
               WHERE ("ShowMetadataId" IS NOT NULL OR "SeasonMetadataId" IS NOT NULL)
                 AND "ExternalTypeId" = 'plex/label' ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(labels, original);
        let protected: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Tag" WHERE "Name" = 'Protected'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(protected, 4);
        let stale: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Tag" WHERE "Name" = 'Legacy stale'"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(stale, 0);
        assert_eq!(count(&pool, "ShowMetadata").await, 1);
        assert_eq!(count(&pool, "SeasonMetadata").await, 1);
    }
}

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
    assert_plex_subtitle_titles(&pool, "movie", &external_subtitle_url).await;
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
async fn plex_title_only_change_refreshes_movie_and_other_video_metadata() {
    for (library_id, provider_table, metadata_table, metadata_sql, etag_sql) in [
        (
            1,
            "PlexMovie",
            "MovieMetadata",
            r#"SELECT "Title", "SortTitle" FROM "MovieMetadata" LIMIT 1"#,
            r#"SELECT "Etag" FROM "PlexMovie" LIMIT 1"#,
        ),
        (
            8,
            "PlexOtherVideo",
            "OtherVideoMetadata",
            r#"SELECT "Title", "SortTitle" FROM "OtherVideoMetadata" LIMIT 1"#,
            r#"SELECT "Etag" FROM "PlexOtherVideo" LIMIT 1"#,
        ),
    ] {
        let original_server = MockServer::start_async().await;
        mock_plex_title_page(&original_server, library_id, "Original Plex Title").await;
        let renamed_server = MockServer::start_async().await;
        mock_plex_title_page(&renamed_server, library_id, "Renamed Plex Title").await;
        let pool = common::test_db().await;
        let service = PlexService::new();
        let mut config = PlexConnectionConfig {
            uri: original_server.base_url(),
            token: "plex-token".to_owned(),
        };

        if library_id == 1 {
            sync_plex_movie_library(&pool, &service, &config, 1, library_id, "1", false)
                .await
                .unwrap();
        } else {
            sync_plex_other_video_library(&pool, &service, &config, 1, library_id, "8", false)
                .await
                .unwrap();
        }
        let original = sqlx::query(metadata_sql).fetch_one(&pool).await.unwrap();
        assert_eq!(original.get::<String, _>("Title"), "Original Plex Title");
        assert_eq!(
            original.get::<String, _>("SortTitle"),
            "Original Plex Title"
        );
        let original_etag: String = sqlx::query_scalar(etag_sql).fetch_one(&pool).await.unwrap();
        assert!(original_etag.starts_with("title-v1:"));

        // Existing installations store only updatedAt. The first ordinary
        // scan after this change must replace that old fingerprint.
        if library_id == 1 {
            sqlx::query(r#"UPDATE "PlexMovie" SET "Etag" = '1776406500'"#)
                .execute(&pool)
                .await
                .unwrap();
            sync_plex_movie_library(&pool, &service, &config, 1, library_id, "1", false)
                .await
                .unwrap();
        } else {
            sqlx::query(r#"UPDATE "PlexOtherVideo" SET "Etag" = '1776406500'"#)
                .execute(&pool)
                .await
                .unwrap();
            sync_plex_other_video_library(&pool, &service, &config, 1, library_id, "8", false)
                .await
                .unwrap();
        }
        let migrated_etag: String = sqlx::query_scalar(etag_sql).fetch_one(&pool).await.unwrap();
        assert_eq!(migrated_etag, original_etag);

        config.uri = renamed_server.base_url();
        if library_id == 1 {
            sync_plex_movie_library(&pool, &service, &config, 1, library_id, "1", false)
                .await
                .unwrap();
        } else {
            sync_plex_other_video_library(&pool, &service, &config, 1, library_id, "8", false)
                .await
                .unwrap();
        }
        let renamed = sqlx::query(metadata_sql).fetch_one(&pool).await.unwrap();
        assert_eq!(renamed.get::<String, _>("Title"), "Renamed Plex Title");
        assert_eq!(renamed.get::<String, _>("SortTitle"), "Renamed Plex Title");
        let renamed_etag: String = sqlx::query_scalar(etag_sql).fetch_one(&pool).await.unwrap();
        assert_ne!(original_etag, renamed_etag);
        assert_eq!(count(&pool, provider_table).await, 1);
        assert_eq!(count(&pool, metadata_table).await, 1);
    }
}

async fn mock_plex_title_page(server: &MockServer, library_id: i64, title: &str) {
    let mut item = media_json(
        "/library/metadata/100",
        "/library/parts/10",
        "/media/title-change.mkv",
        100,
    );
    item["title"] = json!(title);
    server
        .mock_async(|when, then| {
            when.method(GET)
                .path(format!("/library/sections/{library_id}/all"))
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": { "totalSize": 1, "Metadata": [item] }
            }));
        })
        .await;
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
    assert_plex_subtitle_titles(&pool, "episode", &external_subtitle_url).await;
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

    let original_episode = common::displace_episode_season(&pool).await;

    let shallow_synced = sync_plex_show_by_id(&pool, 1, 10, false, None)
        .await
        .unwrap();

    assert_eq!(shallow_synced, 1);
    common::assert_episode_parent(&pool, original_episode).await;
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
    let (collections, members) = mock_plex_collection_pages(&server).await;

    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_movie_library(&pool, &service, &config, 2, 99, "4", false)
        .await
        .unwrap();
    sync_plex_movie_library(&pool, &service, &config, 1, 4, "4", false)
        .await
        .unwrap();

    let synced = sync_plex_collection_library(&pool, &service, &config, 1, "4")
        .await
        .unwrap();

    assert_eq!(synced, 1);
    common::assert_collection_member_source(&pool, 1).await;
    assert_eq!(count(&pool, "Collection").await, 1);
    assert_eq!(count(&pool, "PlexCollection").await, 1);
    assert_eq!(count(&pool, "CollectionItem").await, 2);
    insta::assert_json_snapshot!(plex_collection_rows(&pool).await);
    collections.assert_async().await;
    members.assert_async().await;
    assert_plex_collection_tag_updates(&pool, &service, &config, &server, &members).await;
}

async fn assert_plex_collection_tag_updates(
    pool: &SqlitePool,
    service: &PlexService,
    config: &PlexConnectionConfig,
    server: &MockServer,
    members: &httpmock::Mock<'_>,
) {
    let owned = common::collection_tag_rows(pool, "plex/collection").await;
    assert_eq!(owned.len(), 2);
    assert!(owned
        .iter()
        .all(|(_, name, key)| name == "Favorites" && key == "900"));
    sqlx::query(
        r#"INSERT INTO "Tag" ("Name", "ExternalCollectionId", "ExternalTypeId", "MovieMetadataId")
        SELECT 'Favorites', ?, '319', "Id" FROM "MovieMetadata" WHERE "MovieId" IN (SELECT "MediaItemId" FROM "CollectionItem") ORDER BY "Id" LIMIT 1"#,
    )
    .bind("900")
    .execute(pool)
    .await
    .unwrap();
    let protected = common::collection_tag_rows(pool, "319").await;
    sync_plex_collection_library(pool, service, config, 1, "4")
        .await
        .unwrap();
    assert_eq!(
        common::collection_tag_rows(pool, "plex/collection").await,
        owned
    );
    members.delete_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/library/collections/900/children");
            then.status(200)
                .json_body(json!({"MediaContainer": {"totalSize": 0, "Metadata": []}}));
        })
        .await;
    sync_plex_collection_library(pool, service, config, 1, "4")
        .await
        .unwrap();
    assert!(common::collection_tag_rows(pool, "plex/collection")
        .await
        .is_empty());
    assert_eq!(common::collection_tag_rows(pool, "319").await, protected);
    assert_eq!(count(pool, "CollectionItem").await, 0);
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
                .query_param_missing("network")
                .header("Accept", "application/json")
                .header("X-Plex-Token", "plex-token");
            then.status(200).json_body(json!({
                "MediaContainer": {
                    "totalSize": 1,
                    "Metadata": [
                        { "key": "/library/metadata/200", "title": "Example Show", "updatedAt": 1,
                          "Label": [{ "id": 7, "tag": "Show label" }] }
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
                            "updatedAt": 2,
                            "Label": [{ "id": "8", "tag": "Season label" }]
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

/// Covers Unicode collection names and mixed movie/show member types. Unknown
/// local member IDs are skipped without preventing known members from syncing.
#[tokio::test]
async fn plex_sync_handles_unicode_collection_names_and_mixed_member_types() {
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
        "title": "English captions",
        "default": true,
        "embeddedInVideo": false
    }));
    streams.push(json!({
        "id": 4,
        "key": "/library/streams/4",
        "streamType": 3,
        "codec": "ass",
        "languageCode": "jpn",
        "title": "Japanese signs",
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
                  "StreamIndex", "Codec", "SubtitleKind", "Language", "Title",
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
                    "title": row.get::<Option<String>, _>("Title"),
                    "default": row.get::<i64, _>("Default"),
                    "forced": row.get::<i64, _>("Forced"),
                    "path": row.get::<Option<String>, _>("Path")
                })
            })
            .collect(),
    )
}

async fn subtitle_stream_titles(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        r#"SELECT "Title" FROM "MediaStream"
           WHERE "MediaStreamKind" IN (3, 5) ORDER BY "Index""#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn assert_plex_subtitle_titles(pool: &SqlitePool, owner: &str, external_subtitle_url: &str) {
    assert_eq!(
        subtitle_stream_titles(pool).await,
        vec!["English captions", "Japanese signs"]
    );
    assert_eq!(
        subtitle_rows(pool).await,
        json!([
            {
                "codec": "srt",
                "default": 1,
                "forced": 0,
                "language": "eng",
                "owner": owner,
                "path": null,
                "stream_index": 2,
                "subtitle_kind": 0,
                "title": "English captions"
            },
            {
                "codec": "ass",
                "default": 0,
                "forced": 1,
                "language": "jpn",
                "owner": owner,
                "path": external_subtitle_url,
                "stream_index": 4,
                "subtitle_kind": 1,
                "title": "Japanese signs"
            }
        ])
    );
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

#[tokio::test]
async fn plex_collection_identity_isolates_servers_with_the_same_key_and_name() {
    let server = MockServer::start_async().await;
    mock_plex_collection_movie_pages(&server).await;
    let (collections, members) = mock_plex_collection_pages(&server).await;

    let pool = common::test_db().await;
    let service = PlexService::new();
    let config = PlexConnectionConfig {
        uri: server.base_url(),
        token: "plex-token".to_owned(),
    };

    sync_plex_movie_library(&pool, &service, &config, 2, 99, "4", false)
        .await
        .unwrap();
    sync_plex_movie_library(&pool, &service, &config, 1, 4, "4", false)
        .await
        .unwrap();

    for source in [1, 2, 1, 2] {
        sync_plex_collection_library(&pool, &service, &config, source, "4")
            .await
            .unwrap();
    }
    common::assert_source_owned_collections(&pool, "PlexCollection", "plex/collection", 2).await;
    let tags = common::collection_tag_rows(&pool, "plex/collection").await;
    common::add_foreign_collection_member(&pool, "PlexCollection").await;
    sync_plex_collection_library(&pool, &service, &config, 1, "4")
        .await
        .unwrap();
    assert_eq!(
        common::collection_tag_rows(&pool, "plex/collection").await,
        tags
    );
    collections.assert_calls_async(5).await;
    members.assert_calls_async(5).await;
}

async fn mock_plex_collection_pages(
    server: &MockServer,
) -> (httpmock::Mock<'_>, httpmock::Mock<'_>) {
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

    (collections, members)
}

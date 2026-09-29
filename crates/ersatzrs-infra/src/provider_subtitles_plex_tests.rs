use super::*;

async fn fixture() -> DbPool {
    let pool = crate::db::init("sqlite::memory:").await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "PlexMediaSource" ("Id","ClientIdentifier") VALUES (1,'synthetic-server');
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Synthetic',1,1);
        INSERT INTO "PlexLibrary" ("Id","ShouldSyncItems") VALUES (1,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,'synthetic');
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (1,1,0),(2,1,0),(3,1,0),(4,1,0),(5,1,0);
        INSERT INTO "Movie" ("Id") VALUES (1);
        INSERT INTO "PlexMovie" ("Id") VALUES (1);
        INSERT INTO "Show" ("Id") VALUES (4);
        INSERT INTO "Season" ("Id","ShowId") VALUES (5,4);
        INSERT INTO "Episode" ("Id","SeasonId") VALUES (2,5);
        INSERT INTO "PlexEpisode" ("Id") VALUES (2);
        INSERT INTO "OtherVideo" ("Id") VALUES (3);
        INSERT INTO "PlexOtherVideo" ("Id") VALUES (3);
        INSERT INTO "MovieMetadata" ("Id","MovieId","MetadataKind","DateAdded","DateUpdated") VALUES (1,1,0,'2026-09-29','2026-09-29');
        INSERT INTO "EpisodeMetadata" ("Id","EpisodeId","MetadataKind","DateAdded","DateUpdated") VALUES (2,2,0,'2026-09-29','2026-09-29');
        INSERT INTO "OtherVideoMetadata" ("Id","OtherVideoId","MetadataKind","DateAdded","DateUpdated") VALUES (3,3,0,'2026-09-29','2026-09-29');
    "#).execute(&pool).await.unwrap();
    for (id, owner) in [
        (11, "MovieMetadataId"),
        (12, "EpisodeMetadataId"),
        (13, "OtherVideoMetadataId"),
    ] {
        let query = format!(
            r#"INSERT INTO "Subtitle" ("Id","{owner}","SubtitleKind","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated") VALUES (?,?,1,2,'subrip',1,0,0,'nld',0,?,'2026-09-29','2026-09-29')"#
        );
        sqlx::query(crate::db::assert_sql_safe(&query))
            .bind(id)
            .bind(id - 10)
            .bind("https://old.example.test/library/streams/123?X-Plex-Token=synthetic-old")
            .execute(&pool)
            .await
            .unwrap();
    }
    pool
}

#[tokio::test]
async fn plex_sidecars_use_current_owner_credentials_for_all_metadata_kinds() {
    let pool = fixture().await;
    let server = httpmock::MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.path("/");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="synthetic-server" />"#);
        })
        .await;
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id","PlexMediaSourceId","IsActive","Uri") VALUES (1,1,1,?)"#)
        .bind(server.base_url()).execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES ('plex.server_auth_token.synthetic-server','synthetic-current')"#)
        .execute(&pool).await.unwrap();
    let body = "1\n00:00:00,000 --> 00:00:02,000\nOndertitels\n";
    let current = server
        .mock_async(|when, then| {
            when.path("/library/streams/123")
                .header("X-Plex-Token", "synthetic-current");
            then.status(200).body(body);
        })
        .await;
    for id in [11, 12, 13] {
        let binding = super::binding(&pool, id).await.unwrap().unwrap();
        assert_eq!(binding.kind, RemoteMediaSourceKind::Plex);
        assert_eq!(binding.media_source_id, 1);
        assert_eq!(binding.source_id.as_deref(), Some("/library/streams/123"));
        let content = super::super::fetch(&pool, id).await.unwrap().unwrap();
        assert_eq!(content.bytes, body.as_bytes());
    }
    let selected = crate::playback::load_subtitles_for_media_item(&pool, 3)
        .await
        .unwrap()
        .remove(0);
    let cache = tempfile::tempdir().unwrap();
    let prepared = super::super::prepare(&pool, &selected, cache.path())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(prepared.path.unwrap()).unwrap(),
        body
    );
    assert_eq!(
        crate::playback::load_subtitles_for_media_item(&pool, 3)
            .await
            .unwrap()[0]
            .path,
        selected.path
    );
    current.assert_calls_async(4).await;
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='synthetic-rotated' WHERE "Key"='plex.server_auth_token.synthetic-server'"#)
        .execute(&pool).await.unwrap();
    let rotated = server
        .mock_async(|when, then| {
            when.path("/library/streams/123")
                .header("X-Plex-Token", "synthetic-rotated");
            then.status(200).body(body);
        })
        .await;
    assert!(super::super::fetch(&pool, 13).await.unwrap().is_some());
    rotated.assert_calls_async(1).await;
    current.assert_calls_async(4).await;
}

#[tokio::test]
async fn plex_sidecar_binding_rejects_invalid_or_ambiguous_ownership() {
    let pool = fixture().await;
    for path in [
        "",
        " ",
        "/library/parts/123/file",
        "file:///library/streams/123",
    ] {
        sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=13"#)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
        assert!(super::binding(&pool, 13).await.is_err());
    }
    sqlx::query(
        r#"UPDATE "Subtitle" SET "Path"='/library/streams/123', "MovieMetadataId"=1 WHERE "Id"=13"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(super::binding(&pool, 13).await.is_err());
}

#[tokio::test]
async fn plex_sidecar_rejects_ambiguous_global_credentials_before_network_access() {
    let pool = fixture().await;
    let server = httpmock::MockServer::start_async().await;
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id","PlexMediaSourceId","IsActive","Uri") VALUES (1,1,1,?)"#)
        .bind(server.base_url()).execute(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (2);
        INSERT INTO "PlexMediaSource" ("Id","ClientIdentifier") VALUES (2,'different-server');
        INSERT INTO "ConfigElement" ("Key","Value") VALUES ('plex.current_server_auth_token','synthetic-unscoped');
    "#).execute(&pool).await.unwrap();
    let unexpected = server
        .mock_async(|when, then| {
            when.any_request();
            then.status(200);
        })
        .await;
    let error = super::super::fetch(&pool, 13).await.err().unwrap();
    assert_eq!(error.to_string(), unavailable().to_string());
    unexpected.assert_calls_async(0).await;
}

//! Stable exact pins survive an ordinary repair of physical source routing.
use super::*;
use ersatzrs_domain::StreamingMode;
use ersatzrs_infra::playback::select_pinned_playback_subtitle;

async fn playback(server: &MockServer, episode: bool) -> Mock<'_> {
    let (id, path) = if episode {
        ("episode-target", "/srv/media/show/target.mkv")
    } else {
        ("movie-1", "/srv/media/bbb.mp4")
    };
    server.mock_async(|when,then| {
        when.method(GET).path(format!("/Items/{id}/PlaybackInfo"));
        then.status(200).json_body(json!({"MediaSources":[{"Id":"selected","Path":path,"RunTimeTicks":120_000_000,"MediaStreams":[
            {"Type":"Video","Index":0,"Codec":"h264","Width":320,"Height":180},
            {"Type":"Audio","Index":1,"Codec":"aac","Language":"nld","Channels":2},
            {"Type":"Subtitle","Index":2,"Codec":"subrip","IsExternal":true,"Language":"nld","Title":"Dutch sidecar","IsForced":true},
            {"Type":"Subtitle","Index":3,"Codec":"subrip","Language":"eng","Title":"English embedded","IsDefault":true}
        ]}]}));
    }).await
}

async fn scan(
    pool: &SqlitePool,
    server: &MockServer,
    episode: bool,
) -> ersatzrs_core::repositories::Result<usize> {
    if episode {
        sync_jellyfin_show_by_id(pool, 1, 10, false, None).await
    } else {
        sync_jellyfin_movie_library(
            pool,
            &JellyfinService::new(),
            &JellyfinConnectionConfig {
                address: server.base_url(),
                authorization_header: "MediaBrowser Token=\"test\"".into(),
            },
            1,
            1,
            "library-1",
            false,
        )
        .await
    }
}

#[tokio::test]
async fn row295_mapping_repair_preserves_pins_on_unchanged_movie_and_episode_scans() {
    for episode in [false, true] {
        let server = MockServer::start_async().await;
        let _show = if episode {
            Some(mock_target_show_pages(&server).await)
        } else {
            None
        };
        let _movies = if episode {
            None
        } else {
            Some(mock_detailed_movie_items(&server).await)
        };
        let response = playback(&server, episode).await;
        let pool = common::test_db().await;
        if episode {
            seed_jellyfin_show_scan_config(&pool, &server.base_url()).await;
        }
        assert_eq!(scan(&pool, &server, episode).await.unwrap(), 1);
        let (version, owner): (i64, i64) = sqlx::query_as(
            r#"SELECT "Id",COALESCE("MovieId","EpisodeId") FROM "MediaVersion" LIMIT 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let selected = select_pinned_playback_subtitle(
            &pool,
            owner,
            version,
            2,
            StreamingMode::HttpLiveStreamingDirect,
        )
        .await
        .unwrap();
        assert_eq!(selected.language.as_deref(), Some("nld"));
        assert_eq!(selected.subtitle_kind, 1);
        let map:Vec<(i32,i32)>=sqlx::query_as(r#"SELECT "StreamIndex","SourceStreamIndex" FROM "ProviderStreamIndex" WHERE "MediaVersionId"=? ORDER BY "StreamIndex""#).bind(version).fetch_all(&pool).await.unwrap();
        assert_eq!(map, [(0, 0), (1, 1), (3, 2)]);
        scan(&pool, &server, episode).await.unwrap();
        assert_eq!(
            response.calls_async().await,
            1,
            "qualified unchanged ETag skips playback refresh"
        );
        sqlx::query(r#"UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=0 WHERE "Id"=?"#)
            .bind(version)
            .execute(&pool)
            .await
            .unwrap();
        response.delete_async().await;
        let failed = server
            .mock_async(|when, then| {
                when.method(GET).path(if episode {
                    "/Items/episode-target/PlaybackInfo"
                } else {
                    "/Items/movie-1/PlaybackInfo"
                });
                then.status(503);
            })
            .await;
        assert!(scan(&pool, &server, episode).await.is_err());
        assert_pending_pin(&pool, owner, version, selected.id).await;
        failed.delete_async().await;
        let repaired = playback(&server, episode).await;
        sqlx::raw_sql(r#"CREATE TRIGGER "FailIndexRepair" BEFORE INSERT ON "ProviderStreamIndex" WHEN NEW."SourceStreamIndex"=1 BEGIN SELECT RAISE(ABORT,'synthetic map failure'); END;"#).execute(&pool).await.unwrap();
        assert!(scan(&pool, &server, episode).await.is_err());
        assert_pending_pin(&pool, owner, version, selected.id).await;
        sqlx::query(r#"DROP TRIGGER "FailIndexRepair""#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(scan(&pool, &server, episode).await.unwrap(), 1);
        assert_eq!(
            repaired.calls_async().await,
            2,
            "ordinary scan repairs despite unchanged ETag"
        );
        assert_eq!(
            select_pinned_playback_subtitle(
                &pool,
                owner,
                version,
                2,
                StreamingMode::HttpLiveStreamingDirect
            )
            .await
            .unwrap()
            .id,
            selected.id
        );
        scan(&pool, &server, episode).await.unwrap();
        assert_eq!(
            repaired.calls_async().await,
            2,
            "successful repair is idempotent"
        );
    }
}

#[tokio::test]
async fn row295_absent_etag_repairs_pending_map_and_skips_qualified_items() {
    let server = MockServer::start_async().await;
    let _items=server.mock_async(|when,then| {
        when.method(GET).path("/Items").query_param("includeItemTypes","Movie");
        then.status(200).json_body(json!({"TotalRecordCount":1,"Items":[{"Id":"movie-1","Type":"Movie","Name":"Synthetic","Path":"/srv/media/bbb.mp4","RunTimeTicks":120_000_000,"LocationType":"FileSystem"}]}));
    }).await;
    let response = playback(&server, false).await;
    let pool = common::test_db().await;
    scan(&pool, &server, false).await.unwrap();
    scan(&pool, &server, false).await.unwrap();
    assert_eq!(response.calls_async().await, 1);
    sqlx::query(r#"UPDATE "MediaVersion" SET "ProviderStreamIndexVersion"=0"#)
        .execute(&pool)
        .await
        .unwrap();
    scan(&pool, &server, false).await.unwrap();
    assert_eq!(
        response.calls_async().await,
        2,
        "NULL ETag equality must not hide pending repair"
    );
    scan(&pool, &server, false).await.unwrap();
    assert_eq!(response.calls_async().await, 2);
}

#[tokio::test]
async fn row295_ordinary_scan_repairs_missing_map_with_current_revision() {
    for episode in [false, true] {
        let server = MockServer::start_async().await;
        let _show = if episode {
            Some(mock_target_show_pages(&server).await)
        } else {
            None
        };
        let _movies = if episode {
            None
        } else {
            Some(mock_detailed_movie_items(&server).await)
        };
        let response = playback(&server, episode).await;
        let pool = common::test_db().await;
        if episode {
            seed_jellyfin_show_scan_config(&pool, &server.base_url()).await;
        }
        scan(&pool, &server, episode).await.unwrap();
        sqlx::query(r#"DELETE FROM "ProviderStreamIndex" WHERE "StreamIndex"=3"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(scan(&pool, &server, episode).await.unwrap(), 1);
        assert_eq!(response.calls_async().await, 2);
        let physical: i32 = sqlx::query_scalar(
            r#"SELECT "SourceStreamIndex" FROM "ProviderStreamIndex" WHERE "StreamIndex"=3"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(physical, 2);
        scan(&pool, &server, episode).await.unwrap();
        assert_eq!(response.calls_async().await, 2);
    }
}

async fn assert_pending_pin(pool: &SqlitePool, owner: i64, version: i64, caption: i64) {
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "ProviderStreamIndexVersion" FROM "MediaVersion" WHERE "Id"=?"#
        )
        .bind(version)
        .fetch_one(pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        select_pinned_playback_subtitle(
            pool,
            owner,
            version,
            2,
            StreamingMode::HttpLiveStreamingDirect
        )
        .await
        .unwrap()
        .id,
        caption,
        "failed refresh preserves the original caption"
    );
    let map:Vec<(i32,i32)>=sqlx::query_as(r#"SELECT "StreamIndex","SourceStreamIndex" FROM "ProviderStreamIndex" WHERE "MediaVersionId"=? ORDER BY "StreamIndex""#).bind(version).fetch_all(pool).await.unwrap();
    assert_eq!(
        map,
        [(0, 0), (1, 1), (3, 2)],
        "failed map replacement rolls back the original complete map"
    );
}

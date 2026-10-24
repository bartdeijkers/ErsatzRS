use super::*;

const REPAIR: &str =
    include_str!("../../../../migrations/202610240341_jellyfin_external_audio_rescan.sql");

async fn playback_with_external_audio(server: &MockServer, episode: bool) -> Mock<'_> {
    let (id, path) = if episode {
        ("episode-target", "/srv/media/show/target.mkv")
    } else {
        ("movie-1", "/srv/media/bbb.mp4")
    };
    server.mock_async(|when,then| {
        when.method(GET).path(format!("/Items/{id}/PlaybackInfo"));
        then.status(200).json_body(json!({"MediaSources":[{
            "Id":"selected-provider-source","Path":path,"RunTimeTicks":120_000_000,
            "MediaStreams":[
                {"Type":"Video","Index":0,"Codec":"h264","Width":320,"Height":180},
                {"Type":"Audio","Index":1,"Codec":"aac","Language":"eng","IsDefault":true,"Channels":2},
                {"Type":"Audio","Index":2,"Codec":"aac","Language":"nld","Title":"Dutch","IsForced":true,"Channels":6},
                {"Type":"Audio","Index":-1,"Codec":"aac","IsExternal":true},
                {"Type":"Audio","Index":7,"Codec":"aac","IsExternal":true},
                {"Type":"Subtitle","Index":3,"Codec":"subrip","Language":"nld","IsExternal":true,"IsForced":true}
            ]
        }]}));
    }).await
}

async fn scan(pool: &SqlitePool, server: &MockServer, episode: bool) {
    if episode {
        assert_eq!(
            sync_jellyfin_show_by_id(pool, 1, 10, false, None)
                .await
                .unwrap(),
            1
        );
    } else {
        let service = JellyfinService::new();
        let config = JellyfinConnectionConfig {
            address: server.base_url(),
            authorization_header: "MediaBrowser Token=\"test\"".into(),
        };
        assert_eq!(
            sync_jellyfin_movie_library(pool, &service, &config, 1, 1, "library-1", false)
                .await
                .unwrap(),
            1
        );
    }
}

async fn assert_valid_tracks(pool: &SqlitePool) {
    let audio:Vec<(i64,String,Option<String>,i64,i64,i64)>=sqlx::query_as(r#"SELECT "Index","Language","Title","Default","Forced","Channels" FROM "MediaStream" WHERE "MediaStreamKind"=2 ORDER BY "Index""#).fetch_all(pool).await.unwrap();
    assert_eq!(
        audio,
        [
            (1, "eng".into(), None, 1, 0, 2),
            (2, "nld".into(), Some("Dutch".into()), 0, 1, 6)
        ]
    );
    let subtitles: Vec<(i64, String, i64)> = sqlx::query_as(
        r#"SELECT "StreamIndex","Language","Forced" FROM "Subtitle" WHERE "SubtitleKind"=1"#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(subtitles, [(3, "nld".into(), 1)]);
    assert_eq!(count(pool, "MediaVersion").await, 1);
}

#[tokio::test]
async fn jellyfin_negative_audio_repair_rescans_unchanged_movies_and_episodes() {
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
        let playback = playback_with_external_audio(&server, episode).await;
        let pool = common::test_db().await;
        if episode {
            seed_jellyfin_show_scan_config(&pool, &server.base_url()).await;
        }
        scan(&pool, &server, episode).await;
        assert_valid_tracks(&pool).await;
        // Reproduce the persisted pre-fix audio row without changing the Etag.
        sqlx::query(r#"INSERT INTO "MediaStream" ("MediaVersionId","Index","Codec","MediaStreamKind","Channels","Default","Forced") SELECT "Id",-1,'aac',2,2,0,0 FROM "MediaVersion" LIMIT 1"#).execute(&pool).await.unwrap();
        scan(&pool, &server, episode).await;
        assert_eq!(
            playback.calls_async().await,
            1,
            "unchanged Etag reproduces the stale row"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "MediaStream" WHERE "Index"<0"#)
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        sqlx::raw_sql(REPAIR).execute(&pool).await.unwrap();
        scan(&pool, &server, episode).await;
        assert_valid_tracks(&pool).await;
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "MediaStream" WHERE "Index"<0"#)
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            playback.calls_async().await,
            2,
            "repair forces ordinary replacement playback lookup"
        );
        let streams: Vec<(i64, String)> =
            sqlx::query_as(r#"SELECT "Id","Codec" FROM "MediaStream" ORDER BY "Id""#)
                .fetch_all(&pool)
                .await
                .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        scan(&pool, &server, episode).await;
        assert_eq!(
            playback.calls_async().await,
            2,
            "restarted/unchanged scans must not repeat repair"
        );
        assert_eq!(
            sqlx::query_as::<_, (i64, String)>(
                r#"SELECT "Id","Codec" FROM "MediaStream" ORDER BY "Id""#
            )
            .fetch_all(&pool)
            .await
            .unwrap(),
            streams
        );
        assert_valid_tracks(&pool).await;
    }
}

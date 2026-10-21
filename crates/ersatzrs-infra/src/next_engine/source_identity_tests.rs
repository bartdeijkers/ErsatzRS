//! Exact persisted provider variant selection, separate from caption readiness.

use super::*;
use crate::next_engine::media::MediaRelay;
use httpmock::prelude::*;

async fn selected_id(pool: &DbPool, file: i64, id: &str) {
    sqlx::query(r#"UPDATE "MediaFile" SET "ProviderMediaSourceId"=? WHERE "Id"=?"#)
        .bind(id)
        .bind(file)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn selected_provider_source_is_bound_to_exact_file_owner_and_encoded_as_query_data() {
    for kind in [RemoteMediaSourceKind::Jellyfin, RemoteMediaSourceKind::Emby] {
        let (pool, media) = tests::fixture(kind).await;
        let selected = "selected/source ?&={{variable}}#é";
        selected_id(&pool, 1, selected).await;
        selected_id(&pool, 2, "different-source").await;
        let Source::Http { uri, .. } = resolve(&pool, &media, &[]).await.unwrap() else {
            panic!("remote-only provider source");
        };
        let uri = Url::parse(&uri).unwrap();
        assert_eq!(
            uri.origin().ascii_serialization(),
            "https://media.example.test"
        );
        assert_eq!(uri.path(), "/base/Videos/fixture-item/stream");
        assert_eq!(
            uri.query_pairs().collect::<Vec<_>>(),
            [
                ("static".into(), "true".into()),
                ("MediaSourceId".into(), selected.into())
            ]
        );
        assert!(uri.fragment().is_none());
        for (file, version, path) in [
            (2, 100, "media/first.mkv"),
            (1, 101, "media/first.mkv"),
            (1, 100, "MEDIA/first.mkv"),
        ] {
            let mut stale = media.clone();
            stale.media_file_id = file;
            stale.media_version_id = version;
            stale.path = path.into();
            assert!(resolve(&pool, &stale, &[]).await.is_err());
        }
        assert!(selected_video_url(
            &pool,
            &media,
            2,
            kind,
            "https://media.example.test",
            "fixture-item"
        )
        .await
        .is_err());
        selected_id(&pool, 1, " ").await;
        let error = resolve(&pool, &media, &[]).await.err().unwrap();
        assert!(!format!("{error:?}").contains("<api-key>"));
        pool.close().await;
    }
}

async fn connection(pool: &DbPool, kind: RemoteMediaSourceKind, address: &str) {
    // Closed provider enum, matching the existing source fixture's credentials.
    let provider = kind.provider_name();
    sqlx::query(crate::db::assert_sql_safe(format!(
        r#"UPDATE "{provider}Connection" SET "Address"=?"#
    )))
    .bind(address)
    .execute(pool)
    .await
    .unwrap();
    let prefix = provider.to_lowercase();
    sqlx::query(r#"UPDATE "ConfigElement" SET "Key"=? WHERE "Key" LIKE ?"#)
        .bind(format!("{prefix}.api_key.{address}"))
        .bind(format!("{prefix}.api_key.%"))
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn actual_authenticated_relay_opens_the_selected_variant_instead_of_provider_default() {
    for kind in [RemoteMediaSourceKind::Jellyfin, RemoteMediaSourceKind::Emby] {
        let server = MockServer::start_async().await;
        let (pool, media) = tests::fixture(kind).await;
        connection(&pool, kind, &server.base_url()).await;
        selected_id(&pool, 1, "selected-first").await;
        selected_id(&pool, 2, "selected-second").await;
        let mocks = variant_mocks(&server, kind).await;
        let relay = MediaRelay::new();
        relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
        let (published, original) = grant(&pool, &media, &relay).await;
        assert!(!published.to_string().contains("selected-first"));
        assert!(!published.to_string().contains("<api-key>"));
        for (file, expected) in [(1, "first-variant"), (2, "second-variant")] {
            let mut selected = media.clone();
            selected.media_file_id = file;
            if file == 2 {
                selected.path = "media/second.mkv".into();
            }
            let (_, ticket) = grant(&pool, &selected, &relay).await;
            let response = relay
                .open_bound(&pool, (10, 100, file), Some("bytes=0-13"), Some(&ticket))
                .await
                .unwrap();
            assert_eq!(response.status(), reqwest::StatusCode::PARTIAL_CONTENT);
            assert_eq!(
                response.bytes().await.unwrap().as_ref(),
                expected.as_bytes()
            );
        }
        // Changing selection cannot pair an old prepared caption with new AV.
        selected_id(&pool, 1, "selected-second").await;
        for stale in [None, Some(original.as_str()), Some("invalid")] {
            assert!(relay
                .open_bound(&pool, (10, 100, 1), Some("bytes=0-13"), stale)
                .await
                .is_err());
        }
        mocks[0].assert_calls_async(1).await;
        mocks[1].assert_calls_async(1).await;
        let (updated, fresh) = grant(&pool, &media, &relay).await;
        assert_eq!(updated["uri"], published["uri"]);
        assert_ne!(fresh, original);
        let response = relay
            .open_bound(&pool, (10, 100, 1), Some("bytes=0-13"), Some(&fresh))
            .await
            .unwrap();
        assert_eq!(response.bytes().await.unwrap().as_ref(), b"second-variant");
        assert!(relay.open(&pool, (11, 100, 1), None).await.is_err());
        assert!(relay.open(&pool, (10, 101, 1), None).await.is_err());
        // Losing the declaration also invalidates a previously issued grant.
        sqlx::query(r#"UPDATE "MediaFile" SET "ProviderMediaSourceId"=NULL WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
        assert!(relay
            .open_bound(&pool, (10, 100, 1), None, Some(&fresh))
            .await
            .is_err());
        mocks[0].assert_calls_async(1).await;
        mocks[1].assert_calls_async(2).await;
        pool.close().await;
    }
}

async fn grant(
    pool: &DbPool,
    media: &ScheduledMedia,
    relay: &MediaRelay,
) -> (serde_json::Value, String) {
    let source = resolve_published(pool, media, &[], Some(relay))
        .await
        .unwrap();
    let Source::Http { headers, .. } = &source else {
        panic!("private relay source");
    };
    let prefix = format!("{}: ", crate::next_engine::media::SOURCE_BINDING_HEADER);
    let ticket = headers
        .iter()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap()
        .to_owned();
    assert_eq!(ticket.len(), 64);
    (serde_json::to_value(source).unwrap(), ticket)
}

async fn variant_mocks(
    server: &MockServer,
    kind: RemoteMediaSourceKind,
) -> Vec<httpmock::Mock<'_>> {
    let (header, credential) = match kind {
        RemoteMediaSourceKind::Jellyfin => ("Authorization", "MediaBrowser Token=\"<api-key>\""),
        RemoteMediaSourceKind::Emby => ("X-Emby-Token", "<api-key>"),
        RemoteMediaSourceKind::Plex => unreachable!(),
    };
    let mut mocks = Vec::new();
    for (id, body) in [
        ("selected-first", "first-variant"),
        ("selected-second", "second-variant"),
    ] {
        mocks.push(
            server
                .mock_async(|when, then| {
                    when.method(GET)
                        .path("/Videos/fixture-item/stream")
                        .query_param("static", "true")
                        .query_param("MediaSourceId", id)
                        .header(header, credential)
                        .header("Range", "bytes=0-13");
                    then.status(206)
                        .header(
                            "Content-Range",
                            format!("bytes 0-{}/{}", body.len() - 1, body.len()),
                        )
                        .body(body);
                })
                .await,
        );
    }
    mocks
}

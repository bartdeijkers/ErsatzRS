use super::*;

#[tokio::test]
async fn publication_uses_listener_and_capability_without_provider_credentials() {
    for kind in RemoteMediaSourceKind::ALL {
        let (pool, media) = source::tests::fixture(kind).await;
        let relay = MediaRelay::new();
        assert!(relay.source(&media).is_err());
        relay.configure("0.0.0.0:8409".parse().unwrap()).unwrap();
        let published = source::resolve_published(&pool, &media, &[], Some(&relay))
            .await
            .unwrap();
        let Source::Http {
            uri,
            headers,
            keep_alive,
            reconnect,
            ..
        } = published
        else {
            panic!("expected HTTP");
        };
        assert_eq!(keep_alive, Some(false));
        assert_eq!(reconnect, Some(true));
        assert_eq!(uri, "http://127.0.0.1:8409/ffmpeg/next-media/10/100/1");
        let (name, value) = headers[0].split_once(": ").unwrap();
        assert_eq!(name, ACCESS_HEADER);
        assert!(relay.authorized(value));
        assert!(!MediaRelay::new().authorized(value));
        assert!(!relay.authorized("invalid"));
        assert!(!headers[0].contains("<token>"));
        assert!(!headers[0].contains("<api-key>"));
        assert!(relay.configure("127.0.0.1:8409".parse().unwrap()).is_ok());
        assert!(relay.configure("127.0.0.1:8410".parse().unwrap()).is_err());
    }
}

#[tokio::test]
async fn media_open_reads_rotated_credentials_and_preserves_ranges() {
    use httpmock::prelude::*;
    let server = MockServer::start_async().await;
    let (pool, media) = source::tests::fixture(RemoteMediaSourceKind::Emby).await;
    let address = server.base_url();
    sqlx::query(r#"UPDATE "EmbyConnection" SET "Address"=?"#)
        .bind(&address)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "ConfigElement" SET "Key"=? WHERE "Key" LIKE 'emby.api_key.%'"#)
        .bind(format!("emby.api_key.{address}"))
        .execute(&pool)
        .await
        .unwrap();
    let relay = MediaRelay::new();
    relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
    let _published = relay.source(&media).unwrap();
    sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='<rotated-placeholder>' WHERE "Key" LIKE 'emby.api_key.%'"#)
        .execute(&pool).await.unwrap();
    let mock = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Videos/fixture-item/stream")
                .query_param("static", "true")
                .header("X-Emby-Token", "<rotated-placeholder>")
                .header("Range", "bytes=2-4");
            then.status(206)
                .header("Content-Range", "bytes 2-4/6")
                .body("cde");
        })
        .await;
    let response = relay
        .open(&pool, (10, 100, 1), Some("bytes=2-4"))
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()["Content-Range"], "bytes 2-4/6");
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"cde");
    assert!(relay.open(&pool, (11, 100, 1), None).await.is_err());
    assert!(relay.open(&pool, (10, 101, 1), None).await.is_err());
    mock.assert_calls_async(1).await;
}

#[tokio::test]
async fn media_open_fails_over_to_the_verified_plex_server_after_publication() {
    use httpmock::prelude::*;
    let wrong = MockServer::start_async().await;
    let correct = MockServer::start_async().await;
    let (pool, media) = source::tests::fixture(RemoteMediaSourceKind::Plex).await;
    let relay = MediaRelay::new();
    relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
    let published = serde_json::to_value(relay.source(&media).unwrap()).unwrap();
    wrong
        .mock_async(|when, then| {
            when.method(GET).path("/");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="wrong-server" />"#);
        })
        .await;
    let identity = correct
        .mock_async(|when, then| {
            when.method(GET).path("/").header("X-Plex-Token", "<token>");
            then.status(200)
                .body(r#"<MediaContainer machineIdentifier="fixture-server" />"#);
        })
        .await;
    let stream = correct
        .mock_async(|when, then| {
            when.method(GET)
                .path("/library/parts/11/file.mkv")
                .header("X-Plex-Token", "<token>")
                .header("Range", "bytes=2-4");
            then.status(206)
                .header("Content-Range", "bytes 2-4/6")
                .body("cde");
        })
        .await;
    sqlx::query(r#"UPDATE "PlexConnection" SET "Uri"=? WHERE "Id"=1"#)
        .bind(wrong.base_url())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (2,1,0,?)"#)
        .bind(correct.base_url()).execute(&pool).await.unwrap();
    let response = relay
        .open(&pool, (10, 100, 1), Some("bytes=2-4"))
        .await
        .unwrap();
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"cde");
    let active: Vec<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "PlexConnection" WHERE "IsActive"=1"#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(active, vec![2]);
    assert_eq!(
        serde_json::to_value(relay.source(&media).unwrap()).unwrap(),
        published
    );
    identity.assert_calls_async(1).await;
    stream.assert_calls_async(1).await;
}

#[tokio::test]
async fn redirects_preserve_ranges_but_never_forward_credentials_across_origins() {
    use httpmock::prelude::*;
    let first = MockServer::start_async().await;
    let second = MockServer::start_async().await;
    let start = first
        .mock_async(|when, then| {
            when.path("/start")
                .header("X-Plex-Token", "<token>")
                .header("Range", "bytes=2-4");
            then.status(302).header("Location", "/same-origin");
        })
        .await;
    let same = first
        .mock_async(|when, then| {
            when.path("/same-origin").header("X-Plex-Token", "<token>");
            then.status(307).header("Location", second.url("/external"));
        })
        .await;
    let external = second
        .mock_async(|when, then| {
            when.path("/external")
                .header_missing("X-Plex-Token")
                .header("Range", "bytes=2-4");
            then.status(308).header("Location", first.url("/return"));
        })
        .await;
    let returned = first
        .mock_async(|when, then| {
            when.path("/return")
                .header_missing("X-Plex-Token")
                .header("Range", "bytes=2-4");
            then.status(206)
                .header("Content-Range", "bytes 2-4/6")
                .body("cde");
        })
        .await;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = open_http(
        &client,
        &first.url("/start"),
        &["X-Plex-Token: <token>".into()],
        Some("bytes=2-4"),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), 206);
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"cde");
    for mock in [start, same, external, returned] {
        mock.assert_calls_async(1).await;
    }
    let looped = first
        .mock_async(|when, then| {
            when.path("/loop");
            then.status(302).header("Location", "/loop");
        })
        .await;
    assert!(open_http(&client, &first.url("/loop"), &[], None)
        .await
        .is_err());
    looped.assert_calls_async(6).await;
}

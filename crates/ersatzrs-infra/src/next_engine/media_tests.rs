use super::*;

#[test]
fn canvas_source_authenticates_occurrence_expiry_and_host_without_exposing_paths() {
    use super::super::{canvas::CanvasBinding, protocol::ProbeHint};
    let relay = MediaRelay::new();
    relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
    let start = chrono::DateTime::from_timestamp(1_000, 0).unwrap();
    let binding = CanvasBinding {
        channel: 1,
        item: 2,
        media: 3,
        version: 4,
        file: 5,
        start,
        finish: start + chrono::TimeDelta::minutes(5),
        in_point_ms: 10_000,
        out_point_ms: 0,
        expires: start + chrono::TimeDelta::minutes(6),
    };
    let hint = ProbeHint {
        video: Vec::new(),
        audio: Vec::new(),
        subtitle: Vec::new(),
        format_name: Some("nut".into()),
        duration_ms: None,
    };
    let Source::Http {
        uri,
        headers,
        probe_hint,
        reconnect,
        in_point_ms,
        out_point_ms,
        ..
    } = relay.canvas_source(&binding, hint).unwrap()
    else {
        panic!("HTTP canvas source");
    };
    let uri = url::Url::parse(&uri).unwrap();
    assert_eq!(uri.path(), "/ffmpeg/next-canvas");
    let query = uri
        .query_pairs()
        .find(|(key, _)| key == "binding")
        .unwrap()
        .1;
    assert_eq!(
        serde_json::from_str::<CanvasBinding>(&query).unwrap(),
        binding
    );
    assert!(headers
        .iter()
        .any(|header| header == &format!("{SESSION_HEADER}: {{{{{SESSION_ENV}}}}}")));
    assert!(probe_hint.is_some());
    assert_eq!(reconnect, Some(false));
    assert_eq!((in_point_ms, out_point_ms), (None, None));
    let ticket = headers
        .iter()
        .find_map(|header| header.strip_prefix(&format!("{CANVAS_HEADER}: ")))
        .unwrap();
    assert!(relay.authorized_canvas(&binding, ticket, start));
    assert!(!relay.authorized_canvas(&binding, ticket, binding.expires));
    assert!(!MediaRelay::new().authorized_canvas(&binding, ticket, start));
    assert!(!relay.authorized_canvas(&binding, "invalid", start));
    for field in [
        "channel", "item", "media", "version", "file", "start", "finish", "in", "out", "expires",
    ] {
        let mut changed = binding.clone();
        match field {
            "channel" => changed.channel += 1,
            "item" => changed.item += 1,
            "media" => changed.media += 1,
            "version" => changed.version += 1,
            "file" => changed.file += 1,
            "start" => changed.start += chrono::TimeDelta::seconds(1),
            "finish" => changed.finish += chrono::TimeDelta::seconds(1),
            "in" => changed.in_point_ms += 1,
            "out" => changed.out_point_ms += 1,
            _ => changed.expires += chrono::TimeDelta::seconds(1),
        }
        assert!(!relay.authorized_canvas(&changed, ticket, start), "{field}");
    }
}

#[test]
fn callback_credentials_reject_tampering_and_previous_host_instances() {
    let relay = MediaRelay::new();
    let restarted = MediaRelay::new();
    let capability = relay.capability.to_string();
    assert!(relay.authorized(&capability));
    assert!(!restarted.authorized(&capability));
    for invalid in ["", "invalid", "00000000-0000-0000-0000-000000000000"] {
        assert!(!relay.authorized(invalid));
    }

    let binding = (1, 2, 3, 4, 5);
    let ticket = relay.credits_ticket(1, 2, 3, 4, 5);
    assert!(relay.authorized_credits(binding, &ticket));
    assert!(!restarted.authorized_credits(binding, &ticket));
    for changed in [
        (9, 2, 3, 4, 5),
        (1, 9, 3, 4, 5),
        (1, 2, 9, 4, 5),
        (1, 2, 3, 9, 5),
        (1, 2, 3, 4, 9),
    ] {
        assert!(!relay.authorized_credits(changed, &ticket));
    }
    assert!(!relay.authorized_credits(binding, ""));
    assert!(!relay.authorized_credits(binding, &ticket[..63]));
    assert!(!relay.authorized_credits(binding, &format!("{ticket}0")));
    for index in 0..ticket.len() {
        let mut changed = ticket.as_bytes().to_vec();
        changed[index] = if changed[index] == b'0' { b'1' } else { b'0' };
        assert!(!relay.authorized_credits(binding, std::str::from_utf8(&changed).unwrap()));
    }
}

#[test]
fn prepared_source_binds_single_use_session_transport_without_seeking() {
    let relay = MediaRelay::new();
    relay.configure("0.0.0.0:8409".parse().unwrap()).unwrap();
    let instance = Uuid::new_v4();
    let transfer = Uuid::new_v4();
    let hint = super::super::protocol::ProbeHint {
        video: Vec::new(),
        audio: Vec::new(),
        subtitle: Vec::new(),
        format_name: Some("nut".into()),
        duration_ms: Some(12_000),
    };
    let Source::Http {
        uri,
        headers,
        reconnect,
        keep_alive,
        in_point_ms,
        out_point_ms,
        probe_hint,
        ..
    } = relay
        .prepared_source(instance, transfer, hint, false)
        .unwrap()
    else {
        panic!("expected HTTP");
    };
    assert_eq!(
        uri,
        format!("http://127.0.0.1:8409/ffmpeg/next-prepared/{instance}/{transfer}")
    );
    assert_eq!(reconnect, Some(false));
    assert_eq!(keep_alive, Some(true));
    assert_eq!((in_point_ms, out_point_ms), (None, None));
    assert_eq!(probe_hint.unwrap().format_name.as_deref(), Some("nut"));
    let (name, capability) = headers[0].split_once(": ").unwrap();
    assert_eq!(name, ACCESS_HEADER);
    assert!(relay.authorized(capability));
}

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

#[test]
fn dynamic_publication_uses_a_shared_process_expanded_session_header() {
    let relay = MediaRelay::new();
    relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
    let Source::Dynamic {
        headers: Some(headers),
        ..
    } = relay.dynamic_source(1, 2, "fixture", None).unwrap()
    else {
        panic!("expected dynamic callback");
    };
    assert!(headers
        .iter()
        .any(|header| header == "x-ersatzrs-next-session: {{ERSATZRS_NEXT_SESSION}}"));
    assert!(headers.iter().any(|header| header
        .split_once(": ")
        .is_some_and(|(name, value)| name == ACCESS_HEADER && relay.authorized(value))));
}

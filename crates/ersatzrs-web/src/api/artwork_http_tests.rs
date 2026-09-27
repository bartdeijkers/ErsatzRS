use super::*;
use crate::host::state::{IptvAccessTokenVerifier, SystemStartup};
use axum::{http::Request, routing::get, Router};
use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
use tower::ServiceExt as _;

struct ProviderFixture {
    routes: Router,
    observed: tokio::sync::mpsc::UnboundedReceiver<(axum::http::Uri, axum::http::HeaderMap)>,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for ProviderFixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[cfg(test)]
async fn seed_provider_artwork(pool: &ersatzrs_infra::db::DbPool, address: &str) {
    for statement in [
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1), (2), (3), (10)"#,
        r#"INSERT INTO "JellyfinMediaSource" ("Id") VALUES (1)"#,
        r#"INSERT INTO "EmbyMediaSource" ("Id") VALUES (2)"#,
        r#"INSERT INTO "PlexMediaSource" ("Id", "ClientIdentifier") VALUES (3, 'decoy'), (10, 'fixture')"#,
        r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES
          ('jellyfin.current_server_api_key', 'fixture-jellyfin'),
          ('emby.current_server_api_key', 'fixture-emby'),
          ('plex.server_auth_token.fixture', 'fixture-plex'),
          ('plex.server_auth_token.decoy', 'fixture-decoy')"#,
        r#"INSERT INTO "Artwork" ("Id", "ArtworkKind", "Path", "DateAdded", "DateUpdated") VALUES
          (1, 0, 'plex://10/library/metadata/7/thumb/9?tag=stored', '2026-09-27', '2026-09-27'),
          (2, 0, 'jellyfin://Items/fixture/Images/Primary?tag=stored', '2026-09-27', '2026-09-27'),
          (3, 1, 'emby://Items/fixture/Images/Primary?tag=stored', '2026-09-27', '2026-09-27'),
          (4, 0, 'https://images.example.test/poster.jpg', '2026-09-27', '2026-09-27'),
          (5, 0, 'abcdef012345', '2026-09-27', '2026-09-27'),
          (6, 1, 'abcdef012345', '2026-09-27', '2026-09-27'),
          (7, 2, 'abcdef012345', '2026-09-27', '2026-09-27'),
          (8, 3, 'abcdef012345', '2026-09-27', '2026-09-27'),
          (9, 4, 'abcdef012345', '2026-09-27', '2026-09-27')"#,
    ] {
        sqlx::query(statement).execute(pool).await.unwrap();
    }
    for sql in [
        r#"INSERT INTO "JellyfinConnection" ("JellyfinMediaSourceId", "Address") VALUES (1, ?)"#,
        r#"INSERT INTO "EmbyConnection" ("EmbyMediaSourceId", "Address") VALUES (2, ?)"#,
        r#"INSERT INTO "PlexConnection" ("PlexMediaSourceId", "IsActive", "Uri") VALUES (3, 1, ?)"#,
        r#"INSERT INTO "PlexConnection" ("PlexMediaSourceId", "IsActive", "Uri") VALUES (10, 1, ?)"#,
    ] {
        sqlx::query(sql).bind(address).execute(pool).await.unwrap();
    }
}

#[cfg(test)]
async fn provider_fixture() -> ProviderFixture {
    let (observed_tx, observed_rx) = tokio::sync::mpsc::unbounded_channel();
    let provider = Router::new().fallback(
        move |uri: axum::http::Uri, headers: axum::http::HeaderMap| {
            let observed_tx = observed_tx.clone();
            async move {
                observed_tx.send((uri, headers)).unwrap();
                ([(header::CONTENT_TYPE, "image/jpeg")], "fixture-image")
            }
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let pool = ersatzrs_infra::db::init("sqlite::memory:").await.unwrap();
    seed_provider_artwork(&pool, &address).await;
    let (build_tx, _build_rx) = tokio::sync::mpsc::unbounded_channel();
    let state = AppState::new(
        pool,
        DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        1,
        IptvAccessTokenVerifier::disabled(),
        build_tx,
        ersatzrs_core::locking::EntityLocker::default(),
    );
    let routes = Router::new()
        .route("/artwork/{id}", get(redirect_artwork))
        .route("/artwork/posters/{*path}", get(serve_poster))
        .with_state(state);
    ProviderFixture {
        routes,
        observed: observed_rx,
        server,
    }
}

#[tokio::test]
async fn provider_routes_resolve_stored_ids_and_ignore_request_query_overrides() {
    let mut fixture = provider_fixture().await;
    for (provider, id) in [("plex", 1), ("jellyfin", 2), ("emby", 3)] {
        let response = fixture.routes.clone().oneshot(Request::builder()
            .uri(format!("/artwork/posters/{provider}/{id}?tag=caller&fillHeight=999&maxHeight=999&url=caller"))
            .body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap(),
            "fixture-image"
        );
        let (uri, headers) = fixture.observed.try_recv().unwrap();
        let params: HashMap<String, String> =
            serde_urlencoded::from_str(uri.query().unwrap()).unwrap();
        match provider {
            "plex" => {
                assert_eq!(uri.path(), "/photo/:/transcode");
                assert_eq!(params["url"], "/library/metadata/7/thumb/9?tag=stored");
                assert_eq!(params["height"], "440");
                assert_eq!(params["width"], "304");
                assert_eq!(headers["x-plex-token"], "fixture-plex");
            }
            "jellyfin" => {
                assert_eq!(params["tag"], "stored");
                assert_eq!(params["fillHeight"], "440");
                assert!(!params.contains_key("maxHeight"));
                assert!(headers["authorization"]
                    .to_str()
                    .unwrap()
                    .contains("fixture-jellyfin"));
            }
            _ => {
                assert_eq!(params["tag"], "stored");
                assert_eq!(params["maxHeight"], "220");
                assert!(!params.contains_key("fillHeight"));
                assert_eq!(headers["x-emby-token"], "fixture-emby");
            }
        }
        assert!(!headers.contains_key("x-ers-api-key"));
    }
    for path in [
        "jellyfin/999",
        "jellyfin/1",
        "plex/2",
        "emby/4",
        "jellyfin/Items/fixture/Images/Primary",
    ] {
        let response = fixture
            .routes
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/artwork/posters/{path}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
    assert!(
        fixture.observed.try_recv().is_err(),
        "invalid IDs must not reach a provider"
    );
}

#[tokio::test]
async fn artwork_redirects_only_persisted_hex_cache_names() {
    let mut fixture = provider_fixture().await;
    for (id, prefix) in [
        (5, "/artwork/posters"),
        (6, "/artwork/thumbnails"),
        (7, "/iptv/logos"),
        (8, "/artwork/fanart"),
        (9, "/artwork/watermarks"),
    ] {
        let response = fixture
            .routes
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/artwork/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FOUND);
        assert_eq!(
            response.headers()[header::LOCATION],
            format!("{prefix}/abcdef012345")
        );
    }
    for id in ["1", "4", "999", "invalid"] {
        let response = fixture
            .routes
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/artwork/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    assert!(fixture.observed.try_recv().is_err());
}

#[tokio::test]
async fn provider_redirects_preserve_images_but_never_restore_cross_origin_credentials() {
    // Abort these disposable servers even if an assertion fails.
    struct Servers(Vec<tokio::task::JoinHandle<()>>);
    impl Drop for Servers {
        fn drop(&mut self) {
            for server in &self.0 {
                server.abort();
            }
        }
    }
    let first = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_url = format!("http://{}", first.local_addr().unwrap());
    let second_url = format!("http://{}", second.local_addr().unwrap());
    let (observed_tx, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let tx = observed_tx.clone();
    let first_routes = Router::new().fallback(
        move |uri: axum::http::Uri, headers: axum::http::HeaderMap| {
            let tx = tx.clone();
            let second_url = second_url.clone();
            async move {
                tx.send((uri.clone(), headers)).unwrap();
                let location = match uri.path() {
                    "/start" => "/same".to_string(),
                    "/same" => format!("{second_url}/external"),
                    "/loop" => "/loop".to_string(),
                    "/invalid" => "file:///artwork.jpg".to_string(),
                    _ => {
                        return ([(header::CONTENT_TYPE, "image/png")], "fixture-image")
                            .into_response()
                    }
                };
                (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
            }
        },
    );
    let return_url = format!("{first_url}/final");
    let second_routes = Router::new().fallback(
        move |uri: axum::http::Uri, headers: axum::http::HeaderMap| {
            let tx = observed_tx.clone();
            let return_url = return_url.clone();
            async move {
                tx.send((uri, headers)).unwrap();
                (
                    StatusCode::TEMPORARY_REDIRECT,
                    [(header::LOCATION, return_url)],
                )
            }
        },
    );
    let _servers = Servers(vec![
        tokio::spawn(async move { axum::serve(first, first_routes).await.unwrap() }),
        tokio::spawn(async move { axum::serve(second, second_routes).await.unwrap() }),
    ]);
    let response = proxy_request(
        &format!("{first_url}/start"),
        &[("X-Plex-Token", "fixture-redirect")],
        &HashMap::from([("tag".into(), "stored".into())]),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    assert_eq!(
        axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap(),
        "fixture-image"
    );
    for (path, credentials) in [
        ("/start", true),
        ("/same", true),
        ("/external", false),
        ("/final", false),
    ] {
        let (uri, headers) = observed.try_recv().unwrap();
        assert_eq!(uri.path(), path);
        assert_eq!(
            headers
                .get("X-Plex-Token")
                .map(|value| value.to_str().unwrap()),
            credentials.then_some("fixture-redirect")
        );
        assert_eq!(uri.query(), (path == "/start").then_some("tag=stored"));
    }
    for (path, requests) in [("loop", 6), ("invalid", 1)] {
        let response = proxy_request(&format!("{first_url}/{path}"), &[], &HashMap::new()).await;
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        for _ in 0..requests {
            assert!(observed.try_recv().is_ok());
        }
        assert!(observed.try_recv().is_err());
    }
}

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use std::net::SocketAddr;
use tower::ServiceExt;

use crate::host::state::{AppState, IptvAccessTokenVerifier};

#[tokio::test]
async fn hdhr_routes_are_absent_with_jwt_even_when_credentials_are_supplied() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("api-secrets.json"),
        br#"{"ApiKey":"synthetic-key"}"#,
    )
    .unwrap();
    let mut state = AppState::for_tests(IptvAccessTokenVerifier::enabled("synthetic-iptv-secret"));
    state.api_key = Some(ersatzrs_infra::api_security::load_or_create(root.path()).unwrap());
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &serde_json::json!({ "exp": chrono::Utc::now().timestamp() + 60 }),
        &jsonwebtoken::EncodingKey::from_secret(b"synthetic-iptv-secret"),
    )
    .unwrap();
    for app in [
        super::build_app(state.clone()),
        super::build_streaming_app(state.clone()),
    ] {
        for path in [
            "/discover.json",
            "/device.xml",
            "/lineup_status.json",
            "/lineup.json",
        ] {
            for method in ["GET", "HEAD"] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(format!("{path}?access_token={token}"))
                            .header("X-Ers-Api-Key", "synthetic-key")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {path}");
            }
        }
    }
    let result = crate::admin::server_fns::settings::save_hdhr_settings_with_state(
        &state,
        crate::admin::server_fns::settings::HdhrSettingsForm {
            uuid: String::new(),
            tuner_count: 8,
        },
    )
    .await;
    assert!(result.is_err());
    assert_eq!(state.tuner_count(), 2);
    assert_eq!(
        state.pool.size(),
        0,
        "disabled HDHR never accesses the database"
    );
}

#[tokio::test]
async fn hdhr_routes_remain_available_without_jwt() {
    use crate::host::state::SystemStartup;
    use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
    let pool = ersatzrs_infra::db::init("sqlite::memory:").await.unwrap();
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
        2,
        IptvAccessTokenVerifier::disabled(),
        build_tx,
        ersatzrs_core::locking::EntityLocker::default(),
    );
    for app in [
        super::build_app(state.clone()),
        super::build_streaming_app(state.clone()),
    ] {
        for path in [
            "/discover.json",
            "/device.xml",
            "/lineup_status.json",
            "/lineup.json",
        ] {
            for method in ["GET", "HEAD"] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .header("host", "fixture.example:8409")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK, "{method} {path}");
                let body = axum::body::to_bytes(response.into_body(), 16384)
                    .await
                    .unwrap();
                if method == "HEAD" {
                    assert!(body.is_empty());
                } else if path == "/discover.json" {
                    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(value["TunerCount"], 2);
                    assert_eq!(
                        value["LineupURL"],
                        "http://fixture.example:8409/lineup.json"
                    );
                } else if path == "/lineup.json" {
                    assert_eq!(body, "[]");
                }
            }
        }
    }
}

#[tokio::test]
async fn public_iptv_routes_never_dispatch_internal_callbacks_for_forged_local_headers() {
    let state = AppState::for_tests(IptvAccessTokenVerifier::enabled("synthetic-iptv-secret"));
    for app in [
        super::build_iptv_router(&state).with_state(state.clone()),
        super::build_streaming_app(state.clone()),
        super::build_app(state.clone()),
    ] {
        for path in [
            "/ffmpeg/external-media/1/2",
            "/ffmpeg/concat/900",
            "/ffmpeg/stream/900",
            "/ffmpeg/concat/900?wrapper_id=00000000000000000000000000001234",
            "/ffmpeg/stream/900?wrapper_id=00000000000000000000000000001234",
            "/ffmpeg/concat/900?wrapper_id=invalid",
            "/ffmpeg/stream/900?wrapper_id=invalid",
            "/ffmpeg/next-media/1/2/3",
        "/ffmpeg/next-source/00000000-0000-0000-0000-000000000001/00000000-0000-0000-0000-000000000002",
            "/ffmpeg/next-item/1/2/digest/3",
            "/ffmpeg/next-prepared/instance/transfer",
            "/ffmpeg/next-canvas?binding=invalid",
            "/ffmpeg/next-song-background?binding=invalid",
            "/ffmpeg/next-fallback/1/2/3",
            "/ffmpeg/next-credits/1/2/3/4/5",
        ] {
            for host in ["localhost", "localhost.evil.test", "127.0.0.1", "[::1]"] {
                for method in ["GET", "HEAD", "POST"] {
                    let response = app
                        .clone()
                        .oneshot(
                            Request::builder()
                                .method(method)
                                .uri(path)
                                .header("host", host)
                                .header("forwarded", "for=127.0.0.1;host=localhost;proto=http")
                                .header("x-forwarded-for", "127.0.0.1")
                                .header("x-forwarded-host", "localhost")
                                .header("x-forwarded-proto", "http")
                                .extension(ConnectInfo(
                                    "192.0.2.17:42000".parse::<SocketAddr>().unwrap(),
                                ))
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .unwrap();
                    assert_eq!(
                        response.status(),
                        StatusCode::NOT_FOUND,
                        "{method} {path}, {host}"
                    );
                    assert_eq!(
                        state.pool.size(),
                        0,
                        "public callback reached the database: {path}"
                    );
                }
            }
        }
    }
}

#[test]
fn public_ffmpeg_profile_shapes_keep_both_existing_admin_aliases() {
    for path in [
        "/ffmpeg",
        "/ffmpeg/",
        "/ffmpeg/add",
        "/ffmpeg/profiles",
        "/ffmpeg/profiles/add",
        "/ffmpeg/42",
        "/ffmpeg/profiles/42",
        "/ffmpeg/profiles/42/",
    ] {
        assert!(super::is_public_ffmpeg_profile_path(path), "{path}");
    }
    for path in [
        "/ffmpeg/next-song-background",
        "/ffmpeg/next-canvas",
        "/ffmpeg/next-media/1/2/3",
        "/ffmpeg/next-source/00000000-0000-0000-0000-000000000001/00000000-0000-0000-0000-000000000002",
        "/ffmpeg/external-media/1/2",
        "/ffmpeg/unknown-callback",
        "/ffmpeg/profiles/42/callback",
        "/api/ffmpeg/profiles",
        "/admin-api/ffmpeg/save",
    ] {
        assert!(!super::is_public_ffmpeg_profile_path(path), "{path}");
    }
}

#[tokio::test]
async fn internal_routes_require_actual_loopback_peer_and_keep_capability_checks() {
    let state = AppState::for_tests(IptvAccessTokenVerifier::enabled("synthetic-iptv-secret"));
    let app = super::build_internal_app(state.clone());
    for (peer, expected) in [
        (None, StatusCode::NOT_FOUND),
        (Some("192.0.2.17:42000"), StatusCode::NOT_FOUND),
        (Some("[2001:db8::17]:42000"), StatusCode::NOT_FOUND),
        (Some("127.0.0.1:42000"), StatusCode::UNAUTHORIZED),
        (Some("[::1]:42000"), StatusCode::UNAUTHORIZED),
        (Some("[::ffff:127.0.0.1]:42000"), StatusCode::UNAUTHORIZED),
    ] {
        for path in [
            "/ffmpeg/next-media/1/2/3",
        "/ffmpeg/next-source/00000000-0000-0000-0000-000000000001/00000000-0000-0000-0000-000000000002",
            "/ffmpeg/next-song-background?binding=invalid",
        ] {
            let mut request = Request::builder()
                .uri(path)
                .header("host", "localhost")
                .header("forwarded", "for=127.0.0.1;host=localhost")
                .header("x-forwarded-for", "127.0.0.1");
            if let Some(peer) = peer {
                request = request.extension(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "peer: {peer:?}");
            assert_eq!(state.pool.size(), 0);
        }
    }
}

#[tokio::test]
async fn streaming_listener_omits_management_and_keeps_public_iptv_authentication() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("api-secrets.json"),
        br#"{"ApiKey":"synthetic-fixture-key"}"#,
    )
    .unwrap();
    let mut state = AppState::for_tests(IptvAccessTokenVerifier::enabled("synthetic-iptv-secret"));
    state.api_key = Some(ersatzrs_infra::api_security::load_or_create(root.path()).unwrap());
    let app = super::build_streaming_app(state.clone());
    for path in [
        "/",
        "/settings/ffmpeg",
        "/api/maintenance/clean_artwork",
        "/api/openapi.json",
        "/api/addons/package",
        "/api/serverfn",
        "/admin-api/addons/package",
        "/admin-api/serverfn",
        "/public/channel-preview.js",
        "/pkg/ersatzrs_web.wasm",
        "/healthz",
        "/artwork/1",
        "/artwork/posters/abcdef",
        "/artwork/thumbnails/abcdef",
        "/artwork/fanart/abcdef",
        "/artwork/watermarks/abcdef",
        "/artwork/posters/plex/1",
        "/artwork/posters/jellyfin/1",
        "/artwork/thumbnails/emby/1",
        "/dev",
        "/%73ettings/ffmpeg",
        "//settings/ffmpeg",
    ] {
        for method in ["GET", "HEAD", "POST"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("host", "localhost")
                        .header("forwarded", "for=127.0.0.1;host=localhost")
                        .header("x-forwarded-host", "localhost")
                        .header("x-ers-api-key", "synthetic-fixture-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {path}");
        }
    }
    for suffix in [
        "ts",
        "ts?mode=ts-legacy",
        "ts?mode=mpegts",
        "ts?mode=mpeg-ts",
        "m3u8?mode=hls-direct",
        "m3u8?mode=hls-direct-raw",
        "m3u8?mode=ts-legacy",
    ] {
        for method in ["GET", "HEAD"] {
            for credential in [None, Some("invalid")] {
                let mut uri = format!("/iptv/channel/7.{suffix}");
                if let Some(credential) = credential {
                    let separator = if uri.contains('?') { '&' } else { '?' };
                    uri.push(separator);
                    uri.push_str("access_token=");
                    uri.push_str(credential);
                }
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(uri)
                            .header("X-Ers-Api-Key", "synthetic-fixture-key")
                            .header("cookie", "management=synthetic-session")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    StatusCode::UNAUTHORIZED,
                    "{method} {suffix}"
                );
                assert_eq!(
                    state.pool.size(),
                    0,
                    "cached URL authentication must precede channel discovery"
                );
                assert!(state.hls_sessions.active_channels().await.is_empty());
            }
        }
    }
}

#[tokio::test]
async fn artwork_routes_keep_management_and_iptv_admission_separate() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("api-secrets.json"),
        br#"{"ApiKey":"synthetic-fixture-key"}"#,
    )
    .unwrap();
    let mut state = AppState::for_tests(IptvAccessTokenVerifier::enabled("synthetic-iptv-secret"));
    state.api_key = Some(ersatzrs_infra::api_security::load_or_create(root.path()).unwrap());
    let management = super::build_app(state.clone());
    let streaming = super::build_streaming_app(state.clone());
    let token = jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &serde_json::json!({ "exp": chrono::Utc::now().timestamp() + 60 }),
        &jsonwebtoken::EncodingKey::from_secret(b"synthetic-iptv-secret"),
    )
    .unwrap();
    // Invalid IDs/cache names make admission observable without touching a
    // database or an operator's image files. Row 124 covers successful serving.
    for suffix in [
        "posters/invalid-cache",
        "thumbnails/invalid-cache",
        "posters/plex/not-an-id",
        "posters/jellyfin/not-an-id",
        "thumbnails/emby/not-an-id",
    ] {
        for method in ["GET", "HEAD"] {
            let response = management
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(format!("/artwork/{suffix}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            // ADR 0103: management authentication belongs to the deployment's
            // trusted-network/authenticated-proxy boundary, not the machine key.
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            for app in [&management, &streaming] {
                for credential in [None, Some("invalid"), Some(token.as_str())] {
                    let uri = match credential {
                        Some(value) => format!("/iptv/artwork/{suffix}?access_token={value}"),
                        None => format!("/iptv/artwork/{suffix}"),
                    };
                    let response = app
                        .clone()
                        .oneshot(
                            Request::builder()
                                .method(method)
                                .uri(uri)
                                .header("X-Ers-Api-Key", "synthetic-fixture-key")
                                .header("cookie", "management=synthetic-session")
                                .body(Body::empty())
                                .unwrap(),
                        )
                        .await
                        .unwrap();
                    assert_eq!(
                        response.status(),
                        if credential == Some(token.as_str()) {
                            StatusCode::NOT_FOUND
                        } else {
                            StatusCode::UNAUTHORIZED
                        },
                        "{method} {suffix}"
                    );
                }
            }
        }
    }
    let open = super::build_streaming_app(AppState::for_tests(IptvAccessTokenVerifier::disabled()));
    let response = open
        .oneshot(
            Request::get("/iptv/artwork/posters/invalid-cache")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        state.pool.size(),
        0,
        "authorization must precede repository access"
    );
}

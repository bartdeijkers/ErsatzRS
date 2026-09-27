use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use std::net::SocketAddr;
use tower::ServiceExt;

use crate::host::state::{AppState, IptvAccessTokenVerifier};

#[tokio::test]
async fn public_iptv_routes_never_dispatch_internal_callbacks_for_forged_local_headers() {
    let state = AppState::for_tests(IptvAccessTokenVerifier::enabled("synthetic-iptv-secret"));
    let app = super::build_iptv_router(&state).with_state(state.clone());
    for path in [
        "/ffmpeg/concat/900",
        "/ffmpeg/stream/900",
        "/ffmpeg/next-media/1/2/3",
        "/ffmpeg/next-item/1/2/digest/3",
        "/ffmpeg/next-prepared/instance/transfer",
        "/ffmpeg/next-fallback/1/2/3",
        "/ffmpeg/next-credits/1/2/3/4/5",
    ] {
        for host in ["localhost", "localhost.evil.test", "127.0.0.1", "[::1]"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
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
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}, {host}");
            assert_eq!(
                state.pool.size(),
                0,
                "public callback reached the database: {path}"
            );
        }
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
        let mut request = Request::builder()
            .uri("/ffmpeg/next-media/1/2/3")
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

#[tokio::test]
async fn streaming_listener_omits_management_and_keeps_public_iptv_authentication() {
    let state = AppState::for_tests(IptvAccessTokenVerifier::enabled("synthetic-iptv-secret"));
    let app = super::build_streaming_app(state.clone());
    for path in [
        "/",
        "/settings/ffmpeg",
        "/api/maintenance/clean_artwork",
        "/api/addons/package",
        "/api/serverfn",
        "/public/channel-preview.js",
        "/pkg/ersatzrs_web.wasm",
        "/healthz",
        "/dev",
        "/%73ettings/ffmpeg",
        "//settings/ffmpeg",
    ] {
        for method in ["GET", "POST"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(path)
                        .header("host", "localhost")
                        .header("forwarded", "for=127.0.0.1;host=localhost")
                        .header("x-forwarded-host", "localhost")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{method} {path}");
        }
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri("/iptv/channel/7.ts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(state.pool.size(), 0);
}

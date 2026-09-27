//! Machine API policy. Browser management and streaming policies are separate
//! boundaries; see ADR 0103 and upstream `Startup.IsApiPath`.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse as _, Response},
};
use ersatzrs_infra::api_security::ApiKey;

const API_KEY_HEADER: &str = "x-ers-api-key";

pub(crate) async fn openapi() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "openapi": "3.1.0",
        "info": {"title": "ErsatzRS machine API", "version": env!("CARGO_PKG_VERSION")},
        "components": {"securitySchemes": {"ApiKey": {
            "type": "apiKey", "in": "header", "name": "X-Ers-Api-Key",
            "description": "API key from api-secrets.json in the application state directory"
        }}},
        "security": [{"ApiKey": []}],
        "paths": {
            "/api/maintenance/clean_artwork": {"post": {
                "operationId": "cleanArtwork",
                "summary": "Queue orphan artwork cleanup",
                "parameters": [{"name": "limit", "in": "query", "required": false,
                    "schema": {"type": "integer", "format": "int32", "default": 100_000}}],
                "responses": {
                    "200": {"description": "Cleanup accepted for background execution"},
                    "400": {"description": "Invalid limit"},
                    "401": {"description": "Missing or invalid API key"},
                    "503": {"description": "Cleanup queue unavailable"}
                }
            }},
            "/api/openapi.json": {"get": {
                "operationId": "getMachineApiDescription",
                "summary": "Read this API description",
                "responses": {
                    "200": {"description": "OpenAPI document"},
                    "401": {"description": "Missing or invalid API key"}
                }
            }}
        }
    }))
}

pub(crate) async fn authenticate(
    State(key): State<Option<ApiKey>>,
    mut request: Request,
    next: Next,
) -> Response {
    for (name, value) in request.headers_mut().iter_mut() {
        if name == API_KEY_HEADER {
            value.set_sensitive(true);
        }
    }
    let path = request.uri().path();
    if is_segment(path, "/api") && !is_segment(path, "/api/troubleshoot") {
        let mut values = request.headers().get_all(API_KEY_HEADER).iter();
        let supplied = values.next();
        let authorized = values.next().is_none()
            && supplied.is_some_and(|value| {
                key.as_ref()
                    .is_some_and(|key| key.verifies(value.as_bytes()))
            });
        if !authorized {
            return StatusCode::UNAUTHORIZED.into_response();
        }
    }
    next.run(request).await
}

fn is_segment(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|tail| tail.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, middleware, routing::any, Router};
    use tower::ServiceExt as _;

    #[test]
    fn generated_browser_functions_are_registered_outside_machine_namespace() {
        let paths: Vec<_> = leptos::server_fn::axum::server_fn_paths().collect();
        assert!(
            !paths.is_empty(),
            "server-function inventory must be linked"
        );
        for (path, _) in paths {
            assert!(
                path.starts_with("/admin-api/"),
                "unexpected browser function path: {path}"
            );
        }
    }

    fn router(key: Option<ApiKey>) -> Router {
        Router::new()
            .route("/{*path}", any(|| async { StatusCode::NO_CONTENT }))
            .layer(middleware::from_fn_with_state(key, authenticate))
    }

    #[tokio::test]
    async fn machine_namespace_fails_closed_without_configured_key() {
        for path in [
            "/api",
            "/api/maintenance/clean_artwork",
            "/api/unknown",
            "/api/troubleshoot-extra",
        ] {
            for method in ["GET", "POST", "OPTIONS", "DELETE"] {
                let response = router(None)
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(path)
                            .header(API_KEY_HEADER, "synthetic-fixture-key")
                            .body(Body::empty())
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            }
        }
    }

    #[tokio::test]
    async fn only_one_exact_header_authorizes_machine_requests() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("api-secrets.json"),
            br#"{"ApiKey":"synthetic-fixture-key"}"#,
        )
        .unwrap();
        let key = ersatzrs_infra::api_security::load_or_create(root.path()).unwrap();
        for (headers, expected) in [
            (vec![], StatusCode::UNAUTHORIZED),
            (vec![""], StatusCode::UNAUTHORIZED),
            (vec!["synthetic-wrong-key"], StatusCode::UNAUTHORIZED),
            (vec!["synthetic-fixture-key"], StatusCode::NO_CONTENT),
            (
                vec!["synthetic-fixture-key", "synthetic-fixture-key"],
                StatusCode::UNAUTHORIZED,
            ),
            (
                vec!["synthetic-fixture-key, synthetic-fixture-key"],
                StatusCode::UNAUTHORIZED,
            ),
        ] {
            let mut request =
                Request::post("/api/maintenance/clean_artwork?api_key=synthetic-fixture-key")
                    .header("cookie", "ApiKey=synthetic-fixture-key")
                    .header("host", "localhost")
                    .header("x-forwarded-for", "127.0.0.1");
            for value in headers {
                request = request.header(API_KEY_HEADER, value);
            }
            let response = router(Some(key.clone()))
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    }

    #[tokio::test]
    async fn management_exception_is_delimited_and_other_surfaces_are_independent() {
        for path in [
            "/api/troubleshoot",
            "/api/troubleshoot/playback/log",
            "/admin-api/save",
            "/iptv/channels.m3u",
            "/apiculture",
        ] {
            let response = router(None)
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
        }
    }

    #[tokio::test]
    async fn production_router_authenticates_before_maintenance_admission() {
        use crate::host::{
            artwork_cleanup::TokioArtworkCleanupQueue,
            state::{AppState, IptvAccessTokenVerifier},
        };
        use std::sync::Arc;

        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("api-secrets.json"),
            br#"{"ApiKey":"synthetic-fixture-key"}"#,
        )
        .unwrap();
        let mut state = AppState::for_tests(IptvAccessTokenVerifier::disabled());
        state.api_key = Some(ersatzrs_infra::api_security::load_or_create(root.path()).unwrap());
        let (queue, mut receiver) = TokioArtworkCleanupQueue::channel();
        state.artwork_cleanup = Arc::new(queue);
        let app = crate::host::router::build_app(state);
        for (header, supplied) in [
            (API_KEY_HEADER, None),
            (API_KEY_HEADER, Some("synthetic-wrong-key")),
            ("x-etv-api-key", Some("synthetic-fixture-key")),
        ] {
            let mut request = Request::post("/api/maintenance/clean_artwork?limit=7");
            if let Some(value) = supplied {
                request = request.header(header, value);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            assert!(receiver.try_recv().is_err());
        }
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/maintenance/clean_artwork?limit=7")
                    .header(API_KEY_HEADER, "synthetic-fixture-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(receiver.try_recv().unwrap().limit, 7);
        let response = app
            .clone()
            .oneshot(
                Request::get("/api/openapi.json")
                    .header(API_KEY_HEADER, "synthetic-fixture-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 16_384)
            .await
            .unwrap();
        let description: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            description["components"]["securitySchemes"]["ApiKey"]["name"],
            "X-Ers-Api-Key"
        );
        assert_eq!(description["security"], serde_json::json!([{"ApiKey": []}]));
        for path in [
            "/api/settings/ui/save",
            "/api/unknown",
            "/api/troubleshoot-extra",
        ] {
            let response = app
                .clone()
                .oneshot(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        let response = app
            .oneshot(
                Request::get("/admin-api/settings/ui/save")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }
}

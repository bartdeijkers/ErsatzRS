//! Read-only machine projection of the normal playlist-worker registry.

use crate::host::state::AppState;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};

pub async fn list_sessions(State(state): State<AppState>) -> Response {
    match state.application.list_streaming_sessions().await {
        Ok(sessions) => ([(header::CACHE_CONTROL, "no-store")], Json(sessions)).into_response(),
        Err(error) => {
            tracing::warn!(%error, "streaming session report unavailable");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::state::IptvAccessTokenVerifier;
    use axum::{body::Body, http::Request};
    use ersatzrs_core::hls::HlsSessionHandle;
    use ersatzrs_infra::{hls_worker::HlsSessionWorker, next_engine::session::NextSession};
    use std::sync::Arc;
    use tower::ServiceExt as _;

    async fn report(app: axum::Router) -> serde_json::Value {
        let response = app
            .oneshot(
                Request::get("/api/sessions")
                    .header("X-Ers-Api-Key", "synthetic-session-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 16_384)
                .await
                .unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn sessions_api_reports_mixed_workers_without_touching_and_excludes_previews_and_direct()
    {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("api-secrets.json"),
            br#"{"ApiKey":"synthetic-session-key"}"#,
        )
        .unwrap();
        let mut state = AppState::for_tests(IptvAccessTokenVerifier::from_optional_secret(None));
        state.api_key = Some(ersatzrs_infra::api_security::load_or_create(root.path()).unwrap());
        let app = crate::host::router::build_app(state.clone());
        assert_eq!(report(app.clone()).await, serde_json::json!([]));
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        let next: Arc<dyn HlsSessionHandle> = Arc::new(NextSession::new(
            "2".into(),
            root.path().join("next"),
            deadline,
        ));
        let native: Arc<dyn HlsSessionHandle> = HlsSessionWorker::new(
            "1".into(),
            1,
            root.path().join("native"),
            ersatzrs_ffmpeg::OutputFormat::Hls,
            Vec::new(),
            state.ffmpeg_runtime.clone(),
        );
        let preview: Arc<dyn HlsSessionHandle> = Arc::new(NextSession::new(
            "preview".into(),
            root.path().join("preview"),
            deadline,
        ));
        let direct: Arc<dyn HlsSessionHandle> = Arc::new(NextSession::new(
            "direct".into(),
            root.path().join("direct"),
            deadline,
        ));
        state.hls_sessions.insert(next.clone()).await;
        state.hls_sessions.insert(native.clone()).await;
        state.preview_hls_sessions.insert(preview).await;
        state.hls_sessions.register_direct(direct.clone()).await;
        native.touch();
        next.touch();
        let before = next.session_model().unwrap();
        let rows = report(app.clone()).await;
        assert_eq!(rows.as_array().unwrap().len(), 2);
        assert_eq!(rows[0]["channelNumber"], "1");
        assert_eq!(rows[0]["state"], "SeekAndRealtime");
        assert_eq!(rows[1], serde_json::to_value(&before).unwrap());
        assert!(rows[1]["transcodedUntil"].is_null());
        assert_eq!(rows[1].as_object().unwrap().len(), 4);
        assert_eq!(
            next.session_model().unwrap(),
            before,
            "listing must not touch demand"
        );
        state.hls_sessions.remove_if_current(&next).await;
        assert_eq!(report(app.clone()).await.as_array().unwrap().len(), 1);
        state.hls_sessions.remove_if_current(&native).await;
        assert_eq!(report(app).await, serde_json::json!([]));
        assert!(state.hls_sessions.is_active("direct").await);
    }

    #[tokio::test]
    async fn sessions_api_retains_machine_auth_listener_boundary_and_nullable_schema() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("api-secrets.json"),
            br#"{"ApiKey":"synthetic-session-key"}"#,
        )
        .unwrap();
        let mut state = AppState::for_tests(IptvAccessTokenVerifier::from_optional_secret(None));
        state.api_key = Some(ersatzrs_infra::api_security::load_or_create(root.path()).unwrap());
        let app = crate::host::router::build_app(state.clone());
        for header in [
            None,
            Some(("X-Ers-Api-Key", "synthetic-wrong")),
            Some(("X-Etv-Api-Key", "synthetic-session-key")),
        ] {
            let mut request = Request::get("/api/sessions");
            if let Some((name, value)) = header {
                request = request.header(name, value);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        for isolated in [
            crate::host::router::build_streaming_app(state.clone()),
            crate::host::router::build_internal_app(state),
        ] {
            let response = isolated
                .oneshot(
                    Request::get("/api/sessions")
                        .header("X-Ers-Api-Key", "synthetic-session-key")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }
        let response = app
            .oneshot(
                Request::get("/api/openapi.json")
                    .header("X-Ers-Api-Key", "synthetic-session-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let schema: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 16_384)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            schema["paths"]["/api/sessions"]["get"]["responses"]["200"]["content"]
                ["application/json"]["schema"]["items"]["$ref"],
            "#/components/schemas/HlsSessionModel"
        );
        assert_eq!(
            schema["components"]["schemas"]["HlsSessionModel"]["properties"]["transcodedUntil"]
                ["type"],
            serde_json::json!(["null", "string"])
        );
    }
}

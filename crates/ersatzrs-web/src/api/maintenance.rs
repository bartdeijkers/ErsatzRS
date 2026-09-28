use axum::{
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;

use crate::host::state::AppState;

#[derive(Deserialize)]
pub struct CleanArtworkQuery {
    #[serde(default = "default_limit")]
    limit: i32,
}

const fn default_limit() -> i32 {
    100_000
}

pub async fn clean_artwork(
    State(state): State<AppState>,
    Query(query): Query<CleanArtworkQuery>,
) -> StatusCode {
    match ersatzrs_app::artwork_cleanup::request_artwork_cleanup(
        state.artwork_cleanup.as_ref(),
        query.limit,
    )
    .await
    {
        Ok(()) => StatusCode::OK,
        Err(error) => {
            tracing::warn!(%error, "failed to admit artwork cleanup");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// Native form fallback for the admin health refresh action.
pub async fn refresh_health(State(state): State<AppState>) -> axum::response::Response {
    use axum::response::{IntoResponse, Redirect};
    match state.application.run_admin_health_checks(true).await {
        Ok(_) => Redirect::to("/system/health").into_response(),
        Err(error) => {
            tracing::warn!(%error, "admin health refresh failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::{artwork_cleanup::TokioArtworkCleanupQueue, state::IptvAccessTokenVerifier};
    use axum::{body::Body, http::Request, routing::post, Router};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn artwork_cleanup_http_admission_is_consumed_by_worker() {
        use crate::host::artwork_cleanup::run_artwork_cleanup_worker;
        use sqlx::Executor;
        use std::time::Duration;
        use tokio_util::sync::CancellationToken;

        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        pool.execute(
            r#"INSERT INTO "Artwork" ("Id", "ArtworkKind", "DateAdded", "DateUpdated")
            VALUES (1, 0, '2026-01-01', '2026-01-01');
            INSERT INTO "Actor" ("ArtworkId", "Name") VALUES (1, 'synthetic orphan');"#,
        )
        .await
        .unwrap();
        let (queue, receiver) = TokioArtworkCleanupQueue::channel();
        let mut state = AppState::for_tests(IptvAccessTokenVerifier::from_optional_secret(None));
        state.artwork_cleanup = Arc::new(queue);
        let app = Router::new()
            .route("/api/maintenance/clean_artwork", post(clean_artwork))
            .with_state(state);
        let response = app
            .oneshot(
                Request::post("/api/maintenance/clean_artwork?limit=1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        // Admission returns before the worker starts; HTTP does not execute SQL.
        let before: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Artwork""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(before, 1);
        let shutdown = CancellationToken::new();
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("artwork/posters");
        tokio::fs::create_dir_all(&cache).await.unwrap();
        let stale_file = cache.join("DEADBEEF");
        tokio::fs::write(&stale_file, b"orphaned-cache")
            .await
            .unwrap();
        let maintenance = ersatzrs_infra::artwork_cache_maintenance::ArtworkCacheMaintenance::new(
            pool.clone(),
            root.path().to_owned(),
        );
        let worker = tokio::spawn(run_artwork_cleanup_worker(
            maintenance,
            receiver,
            shutdown.clone(),
        ));
        let completed =
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let remaining: i64 = sqlx::query_scalar(
                    r#"SELECT (SELECT COUNT(*) FROM "Actor") + (SELECT COUNT(*) FROM "Artwork")"#,
                ).fetch_one(&pool).await.unwrap();
                    if remaining == 0 && !stale_file.exists() {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await;
        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .unwrap()
            .unwrap();
        completed.expect("queued HTTP cleanup removes orphan actor and artwork");
    }

    #[tokio::test]
    async fn artwork_cleanup_route_preserves_limits_and_rejects_invalid_input() {
        let (queue, mut receiver) = TokioArtworkCleanupQueue::channel();
        let mut state = AppState::for_tests(IptvAccessTokenVerifier::from_optional_secret(None));
        state.artwork_cleanup = Arc::new(queue);
        let app = Router::new()
            .route("/api/maintenance/clean_artwork", post(clean_artwork))
            .with_state(state);
        for (query, expected) in [
            ("", 100_000),
            ("?limit=10", 10),
            ("?limit=0", 0),
            ("?limit=-1", -1),
            ("?limit=2147483647", i32::MAX),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::post(format!("/api/maintenance/clean_artwork{query}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(receiver.recv().await.unwrap().limit, expected);
        }
        for query in ["?limit=no", "?limit=2147483648", "?limit="] {
            let response = app
                .clone()
                .oneshot(
                    Request::post(format!("/api/maintenance/clean_artwork{query}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert!(receiver.try_recv().is_err());
        }
        drop(receiver);
        let response = app
            .oneshot(
                Request::post("/api/maintenance/clean_artwork")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}

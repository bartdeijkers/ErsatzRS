//! Reuses the disposable origin/private-router pattern from the existing Next preview tests.
use super::*;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware,
};

#[derive(Clone)]
pub(super) struct Captured {
    pub path: String,
    headers: HeaderMap,
    pub status: u16,
    pub item: Option<serde_json::Value>,
}

pub(super) struct Origin {
    pub base: String,
    captured: Arc<Mutex<Vec<Captured>>>,
    cancel: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Origin {
    pub async fn start(fixture: &Fixture) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        fixture.state.next_engine.media.configure(address).unwrap();
        let captured = Arc::new(Mutex::new(Vec::new()));
        let app = crate::host::router::build_internal_app(app_state(fixture))
            .route_service(
                "/source.mkv",
                tower_http::services::ServeFile::new(fixture.root.path().join("fixture.mkv")),
            )
            .layer(middleware::from_fn_with_state(captured.clone(), capture));
        let (cancel, cancelled) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = cancelled.await;
            })
            .await
            .unwrap();
        });
        Self {
            base: format!("http://{address}"),
            captured,
            cancel: Some(cancel),
            task,
        }
    }

    pub async fn captured(&self) -> Vec<Captured> {
        self.captured.lock().await.clone()
    }

    pub async fn assert_revoked(&self) {
        let captured = self.captured().await;
        assert!(
            captured.iter().any(|value| value.status == 200),
            "real Next must request its callback"
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        for value in captured.into_iter().filter(|value| value.status == 200) {
            let response = client
                .get(format!("{}{}", self.base, value.path))
                .headers(value.headers)
                .send()
                .await
                .unwrap();
            assert!(
                matches!(
                    response.status(),
                    StatusCode::GONE | StatusCode::SERVICE_UNAVAILABLE
                ),
                "an owned callback survived session revocation: {}",
                response.status()
            );
        }
    }

    pub async fn stop(mut self) {
        let _ = self.cancel.take().unwrap().send(());
        timeout(Duration::from_secs(10), &mut self.task)
            .await
            .unwrap()
            .unwrap();
    }
}

impl Drop for Origin {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        self.task.abort();
    }
}

async fn capture(
    State(captured): State<Arc<Mutex<Vec<Captured>>>>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path_and_query().unwrap().as_str().to_owned();
    let headers = request.headers().clone();
    let response = next.run(request).await;
    if !path.starts_with("/ffmpeg/") {
        return response;
    }
    let status = response.status().as_u16();
    let (response, item) =
        if path.starts_with("/ffmpeg/next-item/") || path.starts_with("/ffmpeg/next-fallback/") {
            let (parts, body) = response.into_parts();
            let bytes = to_bytes(body, 256 * 1024).await.unwrap();
            let item = serde_json::from_slice(&bytes).ok();
            (
                axum::response::Response::from_parts(parts, Body::from(bytes)),
                item,
            )
        } else {
            (response, None)
        };
    let mut values = captured.lock().await;
    assert!(
        values.len() < 256,
        "synthetic callback capture must remain bounded"
    );
    values.push(Captured {
        path,
        headers,
        status,
        item,
    });
    response
}

fn app_state(fixture: &Fixture) -> crate::host::state::AppState {
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let mut state = AppState::new(
        fixture.state.pool.clone(),
        DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        1,
        IptvAccessTokenVerifier::disabled(),
        tx,
        ersatzrs_core::locking::EntityLocker::default(),
    );
    state.next_engine = fixture.state.next_engine.clone();
    state
}

//! Disposable HTTP origin and the production Next callback route.
use super::*;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware,
    routing::get,
    Router,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Local,
    Direct,
    Prepared,
    #[cfg(unix)]
    Producer,
}

struct Captured {
    path: String,
    headers: HeaderMap,
}

pub(super) struct Origin {
    base: String,
    captured: Arc<tokio::sync::Mutex<Option<Captured>>>,
    cancel: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
    _artifact: Option<tempfile::TempDir>,
}

impl Origin {
    pub async fn start(fixture: &Fixture, mode: Mode) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        fixture.state.next_engine.media.configure(address).unwrap();
        let base = format!("http://{address}");
        install_remote(&fixture.state.pool, &format!("{base}/source.mkv")).await;
        let artifact = match mode {
            Mode::Prepared => Some(prepared::install(fixture).await),
            #[cfg(unix)]
            Mode::Producer => {
                install_producer(fixture).await;
                None
            }
            _ => None,
        };
        let captured = Arc::new(tokio::sync::Mutex::new(None));
        let app = Router::new()
            .route(
                "/ffmpeg/next-item/{channel}/{item}/{digest}/{audio}",
                get(crate::transport::iptv::next_media::get_item),
            )
            .route(
                "/ffmpeg/next-prepared/{instance}/{id}",
                get(crate::transport::iptv::next_media::get_prepared),
            )
            .route_service(
                "/source.mkv",
                tower_http::services::ServeFile::new(fixture.root.path().join("fixture.mkv")),
            )
            .layer(middleware::from_fn_with_state(captured.clone(), capture))
            .with_state(app_state(fixture));
        let (cancel, cancelled) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = cancelled.await;
                })
                .await
                .unwrap();
        });
        Self {
            base,
            captured,
            cancel: Some(cancel),
            task,
            _artifact: artifact,
        }
    }

    pub async fn verify_revoked_and_stop(mut self) {
        let captured = self
            .captured
            .lock()
            .await
            .take()
            .expect("real Next must resolve the preview callback");
        let response = reqwest::Client::new()
            .get(format!("{}{}", self.base, captured.path))
            .headers(captured.headers)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let _ = self.cancel.take().unwrap().send(());
        tokio::time::timeout(Duration::from_secs(5), &mut self.task)
            .await
            .unwrap()
            .unwrap();
    }
}

pub(super) async fn isolated(test: &str) {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            &format!("host::troubleshooting::next::tests::{test}"),
            "--ignored",
            "--nocapture",
        ])
        .env(
            ersatzrs_infra::file_system_layout::APP_DATA_DIR_ENV_VAR,
            root.path(),
        )
        .env(
            ersatzrs_infra::file_system_layout::APP_STATE_DIR_ENV_VAR,
            root.path().join("state"),
        );
    let output = output(command).await;
    assert!(String::from_utf8_lossy(&output).contains("1 passed"));
}

#[cfg(unix)]
async fn install_producer(fixture: &Fixture) {
    crate::transport::iptv::next_media::routed_runtime_tests::fixture::install(
        &fixture.state.pool,
        fixture.root.path(),
    )
    .await;
    tokio::fs::copy(
        fixture.root.path().join("fixture.mkv"),
        fixture.root.path().join("source.mkv"),
    )
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "RemoteStream" SET "AddonId"='org.ersatzrs.addon.fixture',"ProviderId"='item',"Url"='https://example.test/item' WHERE "Id"=10"#)
        .execute(&fixture.state.pool).await.unwrap();
}

pub(super) async fn verify_producer(mode: Mode, root: &FsPath) {
    #[cfg(unix)]
    if mode == Mode::Producer {
        let starts = tokio::fs::read_to_string(root.join("producer-starts"))
            .await
            .unwrap();
        assert_eq!(
            starts.lines().count(),
            1,
            "preview must open its producer once"
        );
        let seek = tokio::fs::read_to_string(root.join("producer-seek"))
            .await
            .unwrap();
        assert_eq!(seek.trim(), "00:00:01.0000000");
        let pid = tokio::fs::read_to_string(root.join("producer-pid"))
            .await
            .unwrap();
        let status = Command::new("kill")
            .args(["-0", pid.trim()])
            .status()
            .await
            .unwrap();
        assert!(!status.success(), "preview producer survived cleanup");
    }
    #[cfg(not(unix))]
    let _ = (mode, root);
}

#[path = "troubleshooting_prepared_runtime_tests.rs"]
mod prepared;

impl Drop for Origin {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        self.task.abort();
    }
}

async fn capture(
    State(captured): State<Arc<tokio::sync::Mutex<Option<Captured>>>>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    if request.uri().path().starts_with("/ffmpeg/next-item/") {
        *captured.lock().await = Some(Captured {
            path: request.uri().path().into(),
            headers: request.headers().clone(),
        });
    }
    next.run(request).await
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

async fn install_remote(pool: &DbPool, uri: &str) {
    for sql in [
        r#"INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (10,'https://example.test/unused',1,0)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=10 WHERE "MovieId"=10"#,
        r#"INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (30,10,0,'Synthetic','2026-09-26','2026-09-26')"#,
        r#"UPDATE "Subtitle" SET "MovieMetadataId"=NULL,"RemoteStreamMetadataId"=30"#,
        r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=10"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"=? WHERE "Id"=10"#)
        .bind(uri)
        .execute(pool)
        .await
        .unwrap();
}

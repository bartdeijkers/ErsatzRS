use axum::http::StatusCode;
use ersatzrs_core::repositories::{ConfigRepository, RepositoryError};
use ersatzrs_infra::db::{DatabaseBackend, DbPool};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::OnceLock,
};
use tracing_subscriber::registry::Registry;
use tracing_subscriber::{layer::SubscriberExt, reload, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};

const LISTEN_ADDR: &str = "0.0.0.0:8409";
const LISTEN_PORT: u16 = 8409;
const CONFIG_KEY_HDHR_UUID: &str = "hdhr.uuid";
const CONFIG_KEY_HDHR_TUNER_COUNT: &str = "hdhr.tuner_count";
const LEGACY_CONFIG_KEY_HDHR_UUID: &str = "HDHR.DeviceUuid";
const LEGACY_CONFIG_KEY_HDHR_TUNER_COUNT: &str = "HDHR.TunerCount";
const DEFAULT_TUNER_COUNT: i32 = 2;
const PLAYOUT_BUILD_QUEUE_CAPACITY: usize = 256;
static LOG_FILTER_RELOAD: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

pub async fn run() {
    let startup = ersatzrs_infra::db::bootstrap_media_server_database()
        .await
        .expect("database initialisation failed");

    init_logging(&startup.pool).await;

    tracing::info!(
        database_url = %startup.database_url,
        ?startup.database_backend,
        "database initialised"
    );

    let ffmpeg_runtime = check_startup_ffmpeg_runtime().await;
    let (playout_build_tx, playout_build_rx) =
        tokio::sync::mpsc::channel(PLAYOUT_BUILD_QUEUE_CAPACITY);
    // Single shared playout lock registry: the web host clones it into AppState
    // (so the playouts page can poll lock state) and hands it to the scheduler,
    // matching ErsatzTV's single-process `IEntityLocker`.
    let playout_locker = ersatzrs_core::locking::EntityLocker::new();

    if let Some(version) = startup.schema_status.dirty_version {
        tracing::error!(
            version,
            "database schema contains a partially applied migration"
        );
    }
    if !startup.schema_status.unknown_migrations.is_empty() {
        tracing::error!(
            unknown_migrations = ?startup.schema_status.unknown_migrations,
            "database schema contains migrations unknown to this build"
        );
    }
    if !startup.schema_status.mismatched_migrations.is_empty() {
        tracing::error!(
            mismatched_migrations = ?startup.schema_status.mismatched_migrations,
            "database schema contains migrations whose checksums differ from this build"
        );
    }

    let app_state = build_app_state(
        startup.pool,
        startup.database_backend,
        startup.schema_status,
        ffmpeg_runtime,
        playout_build_tx,
        playout_locker.clone(),
    )
    .await
    .expect("failed to build application state");

    super::tasks::spawn_background_tasks(
        app_state.pool.clone(),
        app_state.device_uuid,
        app_state.tuner_count.clone(),
        LISTEN_PORT,
        playout_build_rx,
        playout_locker,
    );

    let app = super::router::build_app(app_state);

    tracing::info!("ersatzrs-web listening on {LISTEN_ADDR}");

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR)
        .await
        .unwrap_or_else(|e| panic!("failed to bind to {LISTEN_ADDR}: {e}"));

    axum::serve(listener, app.into_make_service())
        .await
        .expect("server error");
}

async fn init_logging(pool: &DbPool) {
    let filter = logging_filter_from_database(pool)
        .await
        .unwrap_or_else(|_| ersatzrs_infra::logging::default_filter().to_owned());

    let writer = FileAndStdoutWriter::new(ersatzrs_infra::logging::default_log_file_path());
    let env_filter = EnvFilter::try_new(filter)
        .unwrap_or_else(|_| EnvFilter::new(ersatzrs_infra::logging::default_filter()));
    let (filter_layer, reload_handle) = reload::Layer::new(env_filter);

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer),
        )
        .init();

    let _ = LOG_FILTER_RELOAD.set(reload_handle);
}

pub async fn reload_logging_from_database(pool: &DbPool) -> Result<(), String> {
    let filter = logging_filter_from_database(pool)
        .await
        .map_err(|error| error.to_string())?;
    let env_filter = EnvFilter::try_new(filter).map_err(|error| error.to_string())?;

    if let Some(handle) = LOG_FILTER_RELOAD.get() {
        handle
            .reload(env_filter)
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

async fn logging_filter_from_database(pool: &DbPool) -> Result<String, sqlx::Error> {
    #[allow(clippy::disallowed_methods)]
    let base_filter = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| ersatzrs_infra::logging::default_filter().to_owned());
    ersatzrs_infra::logging::logging_filter_from_database(pool, base_filter).await
}

#[derive(Clone)]
struct FileAndStdoutWriter {
    path: PathBuf,
}

impl FileAndStdoutWriter {
    fn new(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        Self { path }
    }

    fn open_file(&self) -> io::Result<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
    }
}

impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for FileAndStdoutWriter {
    type Writer = FileAndStdout;

    fn make_writer(&'writer self) -> Self::Writer {
        FileAndStdout {
            file: self.open_file().ok(),
            stdout: io::stdout(),
        }
    }
}

struct FileAndStdout {
    file: Option<File>,
    stdout: io::Stdout,
}

impl Write for FileAndStdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdout.write_all(buf)?;
        if let Some(file) = self.file.as_mut() {
            let _ = file.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()?;
        if let Some(file) = self.file.as_mut() {
            let _ = file.flush();
        }
        Ok(())
    }
}

async fn check_startup_ffmpeg_runtime() -> ersatzrs_ffmpeg::FfmpegRuntimeReport {
    match ersatzrs_ffmpeg::check_ffmpeg_runtime().await {
        Ok(report) => {
            tracing::info!(
                ffmpeg = %report.ffmpeg.version,
                ffprobe = %report.ffprobe.version,
                ffmpeg_path = %report.ffmpeg.path,
                ffprobe_path = %report.ffprobe.path,
                "FFmpeg 8.1+ runtime detected"
            );
            for issue in report.issues.iter().filter(|issue| !issue.is_error()) {
                tracing::warn!(?issue, "FFmpeg runtime capability warning");
            }
            report
        }
        Err(error) => {
            tracing::error!(error = %error, "FFmpeg runtime check failed");
            std::process::exit(1);
        }
    }
}

async fn build_app_state(
    pool: DbPool,
    database_backend: DatabaseBackend,
    schema_status: ersatzrs_infra::db::DatabaseSchemaStatus,
    ffmpeg_runtime: ersatzrs_ffmpeg::FfmpegRuntimeReport,
    playout_build_tx: tokio::sync::mpsc::Sender<ersatzrs_core::scheduling::PlayoutBuildRequest>,
    playout_locker: ersatzrs_core::locking::EntityLocker,
) -> Result<AppState, sqlx::Error> {
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let iptv_access_token_verifier =
        IptvAccessTokenVerifier::from_optional_secret(load_iptv_jwt_signing_key());

    let device_uuid = if let Some(value) = load_config_value(
        &repositories.config,
        CONFIG_KEY_HDHR_UUID,
        Some(LEGACY_CONFIG_KEY_HDHR_UUID),
    )
    .await?
    {
        Uuid::parse_str(value.trim()).unwrap_or_else(|_| Uuid::new_v4())
    } else {
        let new_uuid = Uuid::new_v4();
        repositories
            .config
            .set(CONFIG_KEY_HDHR_UUID, &new_uuid.to_string())
            .await
            .map_err(|error| sqlx_repo_error(&error))?;
        tracing::info!(device_uuid = %new_uuid, "seeded HDHomeRun device UUID");
        new_uuid
    };

    let tuner_count = load_config_value(
        &repositories.config,
        CONFIG_KEY_HDHR_TUNER_COUNT,
        Some(LEGACY_CONFIG_KEY_HDHR_TUNER_COUNT),
    )
    .await?
    .and_then(|value| value.trim().parse::<i32>().ok())
    .unwrap_or(DEFAULT_TUNER_COUNT);

    Ok(AppState::new(
        pool,
        database_backend,
        SystemStartup {
            database_ready: true,
            schema_status,
        },
        ffmpeg_runtime,
        device_uuid,
        tuner_count,
        iptv_access_token_verifier,
        playout_build_tx,
        playout_locker,
    ))
}

fn load_iptv_jwt_signing_key() -> Option<String> {
    #[allow(clippy::disallowed_methods)]
    std::env::var("JWT__IssuerSigningKey")
        .ok()
        .or_else(|| std::env::var("JWT:IssuerSigningKey").ok())
}

async fn load_config_value(
    repository: &dyn ConfigRepository,
    key: &str,
    legacy_key: Option<&str>,
) -> Result<Option<String>, sqlx::Error> {
    if let Some(value) = repository
        .get(key)
        .await
        .map_err(|error| sqlx_repo_error(&error))?
    {
        return Ok(Some(value));
    }

    let Some(legacy_key) = legacy_key else {
        return Ok(None);
    };

    let legacy_value = repository
        .get(legacy_key)
        .await
        .map_err(|error| sqlx_repo_error(&error))?;

    if let Some(value) = legacy_value {
        repository
            .set(key, &value)
            .await
            .map_err(|error| sqlx_repo_error(&error))?;
        return Ok(Some(value));
    }

    Ok(None)
}

fn sqlx_repo_error(error: &RepositoryError) -> sqlx::Error {
    sqlx::Error::Protocol(error.to_string())
}

pub async fn healthz(axum::extract::State(state): axum::extract::State<AppState>) -> StatusCode {
    if state.system_startup.is_ready() && !state.ffmpeg_runtime.read().await.has_errors() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

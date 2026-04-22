use axum::http::StatusCode;
use ersatzrs_core::repositories::{ConfigRepository, RepositoryError};
use ersatzrs_infra::db::{DatabaseBackend, DbPool};
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};

const LISTEN_ADDR: &str = "0.0.0.0:8409";
const LISTEN_PORT: u16 = 8409;
const CONFIG_KEY_HDHR_UUID: &str = "hdhr.uuid";
const CONFIG_KEY_HDHR_TUNER_COUNT: &str = "hdhr.tuner_count";
const LEGACY_CONFIG_KEY_HDHR_UUID: &str = "HDHR.DeviceUuid";
const LEGACY_CONFIG_KEY_HDHR_TUNER_COUNT: &str = "HDHR.TunerCount";
const DEFAULT_TUNER_COUNT: i32 = 2;

pub async fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let ffmpeg_runtime = check_startup_ffmpeg_runtime().await;

    let startup = ersatzrs_infra::db::bootstrap_media_server_database()
        .await
        .expect("database initialisation failed");
    tracing::info!(
        database_url = %startup.database_url,
        ?startup.database_backend,
        "database initialised"
    );
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
    )
    .await
    .expect("failed to build application state");

    super::tasks::spawn_background_tasks(
        app_state.pool.clone(),
        app_state.device_uuid,
        app_state.tuner_count,
        LISTEN_PORT,
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
    if state.system_startup.is_ready() && !state.ffmpeg_runtime.has_errors() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

use axum::http::StatusCode;
use ersatzrs_core::repositories::{ConfigRepository, FfmpegProfileRepository, RepositoryError};
use ersatzrs_domain::{HardwareAccelerationKind, VaapiDriver};
use ersatzrs_infra::{
    db::{DatabaseBackend, DbPool},
    file_system_layout,
};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    sync::OnceLock,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::{layer::SubscriberExt, reload, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};

const LISTEN_ADDR: &str = "0.0.0.0:8409";
const LISTEN_PORT: u16 = 8409;
/// Upper bound on how long shutdown waits for background tasks to drain before
/// forcing exit, so a stuck task cannot hang the tray "Exit" / `docker stop`.
const SHUTDOWN_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const CONFIG_KEY_HDHR_UUID: &str = "hdhr.uuid";
const CONFIG_KEY_HDHR_TUNER_COUNT: &str = "hdhr.tuner_count";
const LEGACY_CONFIG_KEY_HDHR_UUID: &str = "HDHR.DeviceUuid";
const LEGACY_CONFIG_KEY_HDHR_TUNER_COUNT: &str = "HDHR.TunerCount";
const DEFAULT_TUNER_COUNT: i32 = 2;
const PLAYOUT_BUILD_QUEUE_CAPACITY: usize = 256;
const DEFAULT_MPEGTS_SCRIPT_FILES: &[(&str, &[u8])] = &[
    (
        "mpegts.yml",
        include_bytes!("../../../../assets/Scripts/MpegTs/mpegts.yml"),
    ),
    (
        "run.sh",
        include_bytes!("../../../../assets/Scripts/MpegTs/run.sh"),
    ),
    (
        "run.bat",
        include_bytes!("../../../../assets/Scripts/MpegTs/run.bat"),
    ),
];
// Bundled streaming resources extracted into `<AppData>/cache/resources` at
// startup, mirroring ErsatzTV's `ResourceExtractorService`. Extraction
// overwrites (ErsatzTV uses `File.Create`) so shipped resources stay current;
// `ChannelWatermarkImageSource::Resource` watermarks (the song progress
// overlay) resolve against this folder.
const STREAMING_RESOURCE_FILES: &[(&str, &[u8])] = &[
    (
        "_background.png",
        include_bytes!("../../../../assets/_background.png"),
    ),
    (
        "_song_album_cover_512.png",
        include_bytes!("../../../../assets/_song_album_cover_512.png"),
    ),
    (
        "_song_background_1.png",
        include_bytes!("../../../../assets/_song_background_1.png"),
    ),
    (
        "_song_background_2.png",
        include_bytes!("../../../../assets/_song_background_2.png"),
    ),
    (
        "_song_background_3.png",
        include_bytes!("../../../../assets/_song_background_3.png"),
    ),
    (
        "song_progress_overlay.png",
        include_bytes!("../../../../assets/song_progress_overlay.png"),
    ),
    (
        "song_progress_overlay_43.png",
        include_bytes!("../../../../assets/song_progress_overlay_43.png"),
    ),
];
const STREAMING_FONT_FILES: &[(&str, &[u8])] = &[
    (
        "Sen.ttf",
        include_bytes!("../../../../assets/fonts/Sen.ttf"),
    ),
    (
        "Roboto-Regular.ttf",
        include_bytes!("../../../../assets/fonts/Roboto-Regular.ttf"),
    ),
    (
        "OPTIKabel-Heavy.otf",
        include_bytes!("../../../../assets/fonts/OPTIKabel-Heavy.otf"),
    ),
];
static LOG_FILTER_RELOAD: OnceLock<reload::Handle<EnvFilter, Registry>> = OnceLock::new();

pub async fn run() {
    let startup = ersatzrs_infra::db::bootstrap_media_server_database()
        .await
        .expect("database initialisation failed");

    init_logging(&startup.pool).await;
    if let Err(error) = seed_default_mpegts_script().await {
        tracing::warn!(%error, "failed to seed default MPEG-TS script resources");
    }
    if let Err(error) = extract_streaming_resources().await {
        tracing::warn!(%error, "failed to extract bundled streaming resources");
    }
    if let Err(error) = extract_streaming_fonts().await {
        tracing::warn!(%error, "failed to extract bundled streaming fonts");
    }

    tracing::info!(
        database_url = %startup.database_url,
        ?startup.database_backend,
        "database initialised"
    );

    let (ffmpeg_path, ffprobe_path) = resolve_and_persist_ffmpeg_paths(&startup.pool).await;
    let ffmpeg_runtime =
        check_startup_ffmpeg_runtime(&startup.pool, &ffmpeg_path, &ffprobe_path).await;
    let (playout_build_tx, playout_build_rx) =
        tokio::sync::mpsc::channel(PLAYOUT_BUILD_QUEUE_CAPACITY);
    // Single playout lock registry shared with the scheduler: the web host
    // clones it into AppState (so the playouts page can poll which playouts are
    // building) and hands the same instance to the scheduler (the sole builder).
    // Library/collection scan locks use the separate web-host `EntityLocker` in
    // AppState; together they cover ErsatzTV's single `IEntityLocker` surface.
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

    // Coordinated shutdown: the signal handler cancels this token, the axum
    // server drains in-flight requests, and every background task observes the
    // same token so the whole process exits cleanly (tray "Exit", `docker stop`,
    // or console Ctrl+C).
    let shutdown = CancellationToken::new();

    let task_handles = super::tasks::spawn_background_tasks(
        super::tasks::HdhrTaskInputs {
            device_uuid: app_state.device_uuid,
            tuner_count: app_state.tuner_count.clone(),
            listen_port: LISTEN_PORT,
        },
        super::tasks::MaintenanceTaskInputs {
            pool: app_state.pool.clone(),
            playout_build_rx,
            playout_locker,
            hls_sessions: app_state.hls_sessions.clone(),
        },
        shutdown.clone(),
    );

    // Keep a pool handle for the post-serve cleanup; `build_app` consumes state.
    let pool = app_state.pool.clone();
    let app = super::router::build_app(app_state);

    tracing::info!("ersatzrs-web listening on {LISTEN_ADDR}");

    let listener = tokio::net::TcpListener::bind(LISTEN_ADDR)
        .await
        .unwrap_or_else(|e| panic!("failed to bind to {LISTEN_ADDR}: {e}"));

    tokio::spawn(shutdown_signal(shutdown.clone()));

    if let Err(error) = axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown.clone().cancelled_owned())
        .await
    {
        tracing::error!(%error, "server error");
    }

    // Ensure tasks observe shutdown even if serve returned for another reason.
    shutdown.cancel();

    // Drain background tasks within a bounded window, then close the pool and
    // flush the final log line.
    let drain = async {
        for handle in task_handles {
            if let Err(error) = handle.await {
                if !error.is_cancelled() {
                    tracing::warn!(%error, "background task join failed");
                }
            }
        }
    };
    if tokio::time::timeout(SHUTDOWN_DRAIN_TIMEOUT, drain)
        .await
        .is_err()
    {
        tracing::warn!("timed out draining background tasks during shutdown");
    }

    pool.close().await;
    tracing::info!("shutdown complete");
}

/// Await the first OS shutdown signal and cancel the shared token.
///
/// Listens for `Ctrl+C` (which is also the Windows `CTRL_C_EVENT` that the tray
/// launcher sends to the child) and, on Unix, `SIGTERM` (so `docker stop` drains
/// cleanly rather than being killed after its grace period).
async fn shutdown_signal(token: CancellationToken) {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::error!(%error, "failed to install Ctrl+C handler");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                tracing::error!(%error, "failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("received Ctrl+C; initiating graceful shutdown"),
        () = terminate => tracing::info!("received SIGTERM; initiating graceful shutdown"),
    }

    token.cancel();
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

async fn extract_streaming_resources() -> io::Result<()> {
    extract_streaming_resources_in(&file_system_layout::resources_cache_folder()).await
}

async fn extract_streaming_resources_in(folder: &std::path::Path) -> io::Result<()> {
    tokio::fs::create_dir_all(folder).await?;
    for (name, contents) in STREAMING_RESOURCE_FILES {
        tokio::fs::write(folder.join(name), contents).await?;
    }
    Ok(())
}

async fn extract_streaming_fonts() -> io::Result<()> {
    extract_streaming_fonts_in(&file_system_layout::fonts_cache_folder()).await
}

async fn extract_streaming_fonts_in(folder: &std::path::Path) -> io::Result<()> {
    tokio::fs::create_dir_all(folder).await?;
    for (name, contents) in STREAMING_FONT_FILES {
        tokio::fs::write(folder.join(name), contents).await?;
    }
    Ok(())
}

async fn seed_default_mpegts_script() -> io::Result<()> {
    seed_default_mpegts_script_in(&file_system_layout::default_mpegts_script_folder()).await
}

async fn seed_default_mpegts_script_in(folder: &std::path::Path) -> io::Result<()> {
    tokio::fs::create_dir_all(folder).await?;
    for (name, contents) in DEFAULT_MPEGTS_SCRIPT_FILES {
        let path = folder.join(name);
        if tokio::fs::try_exists(&path).await? {
            continue;
        }
        tokio::fs::write(path, contents).await?;
    }
    Ok(())
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

/// Resolve the `ffmpeg`/`ffprobe` paths the way ErsatzTV's `FFmpegLocator` does
/// — DB config (if it still exists) → next to the running executable → `PATH` —
/// persist a freshly located path back into DB config, and publish the resolved
/// paths into the process-wide holder every spawn site reads.
async fn resolve_and_persist_ffmpeg_paths(pool: &DbPool) -> (String, String) {
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let ffmpeg = resolve_and_persist_tool(
        &repositories.config,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH,
        "ffmpeg",
        ersatzrs_ffmpeg::resolve_ffmpeg_path,
    )
    .await;
    let ffprobe = resolve_and_persist_tool(
        &repositories.config,
        ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH,
        "ffprobe",
        ersatzrs_ffmpeg::resolve_ffprobe_path,
    )
    .await;

    ersatzrs_ffmpeg::set_resolved_ffmpeg_path(ffmpeg.clone());
    ersatzrs_ffmpeg::set_resolved_ffprobe_path(ffprobe.clone());
    (ffmpeg, ffprobe)
}

async fn resolve_and_persist_tool<R: ConfigRepository>(
    config: &R,
    key: &str,
    tool: &str,
    resolve: impl Fn(Option<&str>) -> ersatzrs_ffmpeg::LocatedTool,
) -> String {
    let configured = config.get(key).await.unwrap_or_default();
    let located = resolve(configured.as_deref());

    if located.should_persist() {
        if let Err(error) = config.set(key, &located.path).await {
            tracing::warn!(%error, key, "failed to persist resolved FFmpeg tool path");
        } else {
            tracing::info!(tool, path = %located.path, "located and persisted FFmpeg tool path");
        }
    } else if located.db_was_stale {
        // Configured path no longer exists and nothing else was located; clear
        // the stale value (ErsatzTV deletes it).
        let _ = config.set(key, "").await;
        tracing::warn!(
            tool,
            key,
            "configured FFmpeg tool path no longer exists; cleared"
        );
    }

    located.path
}

async fn check_startup_ffmpeg_runtime(
    pool: &DbPool,
    ffmpeg_path: &str,
    ffprobe_path: &str,
) -> ersatzrs_ffmpeg::FfmpegRuntimeReport {
    let config = ffmpeg_runtime_config_with_profile_probes(pool, ffmpeg_path, ffprobe_path).await;
    match ersatzrs_ffmpeg::check_ffmpeg_runtime_with_config(&config).await {
        Ok(mut report) => {
            // Startup is not an explicit capability refresh (ADR 0014):
            // denials learned against this same ffmpeg binary are reapplied,
            // but a path change since the last run clears them instead.
            if let Err(error) = ersatzrs_infra::ffmpeg_capabilities::apply_hardware_decode_denials(
                pool,
                &mut report.capabilities,
                ffmpeg_path,
            )
            .await
            {
                tracing::warn!(%error, "failed to apply runtime-learned hardware decode denials");
            }
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

pub(crate) async fn ffmpeg_runtime_config_with_profile_probes(
    pool: &DbPool,
    ffmpeg_path: &str,
    ffprobe_path: &str,
) -> ersatzrs_ffmpeg::FfmpegRuntimeCheckConfig {
    let mut config =
        ersatzrs_ffmpeg::FfmpegRuntimeCheckConfig::with_paths(ffmpeg_path, ffprobe_path);
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    match repositories.ffmpeg_profile.list().await {
        Ok(profiles) => {
            let devices = profiles
                .into_iter()
                .filter(|profile| {
                    matches!(
                        profile.hardware_acceleration,
                        HardwareAccelerationKind::Vaapi | HardwareAccelerationKind::Qsv
                    )
                })
                .filter_map(|profile| {
                    let display = non_empty_profile_value(profile.vaapi_display);
                    let device = non_empty_profile_value(profile.vaapi_device);
                    let driver = vaapi_driver_name(profile.vaapi_driver).map(str::to_owned);
                    (display.is_some() || device.is_some() || driver.is_some()).then(|| {
                        ersatzrs_ffmpeg::FfmpegHardwareProbeDevice::new(display, device, driver)
                    })
                })
                .fold(Vec::new(), |mut devices, device| {
                    if !devices.contains(&device) {
                        devices.push(device);
                    }
                    devices
                });
            config = config.with_hardware_probe_devices(devices);
        }
        Err(error) => {
            tracing::warn!(
                %error,
                "failed to load FFmpeg profiles for hardware capability probes"
            );
        }
    }
    config
}

fn non_empty_profile_value(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

const fn vaapi_driver_name(driver: VaapiDriver) -> Option<&'static str> {
    match driver {
        VaapiDriver::Auto => None,
        VaapiDriver::IHD => Some("iHD"),
        VaapiDriver::I965 => Some("i965"),
        VaapiDriver::RadeonSI => Some("radeonsi"),
        VaapiDriver::Nouveau => Some("nouveau"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn extract_streaming_resources_overwrites_stale_files() {
        let root = tempfile::tempdir().expect("temp resources root");
        let folder = root.path().join("resources");
        tokio::fs::create_dir_all(&folder)
            .await
            .expect("create resources folder");
        tokio::fs::write(folder.join("song_progress_overlay.png"), b"stale")
            .await
            .expect("write stale resource");

        extract_streaming_resources_in(&folder)
            .await
            .expect("extract resources");

        for (name, contents) in STREAMING_RESOURCE_FILES {
            let extracted = tokio::fs::read(folder.join(name))
                .await
                .unwrap_or_else(|error| panic!("read extracted resource {name}: {error}"));
            assert_eq!(&extracted, contents, "resource {name} content mismatch");
        }
    }

    #[tokio::test]
    async fn extract_streaming_fonts_overwrites_stale_files() {
        let root = tempfile::tempdir().expect("temp fonts root");
        let folder = root.path().join("fonts");
        tokio::fs::create_dir_all(&folder)
            .await
            .expect("create fonts folder");
        tokio::fs::write(folder.join("OPTIKabel-Heavy.otf"), b"stale")
            .await
            .expect("write stale font");

        extract_streaming_fonts_in(&folder)
            .await
            .expect("extract fonts");

        for (name, contents) in STREAMING_FONT_FILES {
            let extracted = tokio::fs::read(folder.join(name))
                .await
                .unwrap_or_else(|error| panic!("read extracted font {name}: {error}"));
            assert_eq!(&extracted, contents, "font {name} content mismatch");
        }
    }

    #[tokio::test]
    async fn seed_default_mpegts_script_creates_missing_files_without_overwriting() {
        let root = tempfile::tempdir().expect("temp script root");
        let folder = root.path().join("default");
        tokio::fs::create_dir_all(&folder)
            .await
            .expect("create script folder");
        tokio::fs::write(folder.join("run.sh"), b"custom")
            .await
            .expect("write custom script");

        seed_default_mpegts_script_in(&folder)
            .await
            .expect("seed scripts");

        let yaml = tokio::fs::read_to_string(folder.join("mpegts.yml"))
            .await
            .expect("read mpegts yaml");
        let run_sh = tokio::fs::read_to_string(folder.join("run.sh"))
            .await
            .expect("read shell script");
        let run_bat = tokio::fs::read(folder.join("run.bat"))
            .await
            .expect("read batch script");

        assert!(yaml.contains("linux_script"));
        assert_eq!(run_sh, "custom");
        assert!(String::from_utf8_lossy(&run_bat).contains("FFmpegPath"));
    }
}

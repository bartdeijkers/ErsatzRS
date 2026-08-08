use axum::http::StatusCode;
use ersatzrs_core::repositories::{ConfigRepository, FfmpegProfileRepository};
use ersatzrs_domain::{HardwareAccelerationKind, VaapiDriver};
use ersatzrs_infra::{
    db::{DatabaseBackend, DbPool},
    file_system_layout,
};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::OnceLock,
};
use tokio_util::sync::CancellationToken;
use tracing_subscriber::registry::Registry;
use tracing_subscriber::{layer::SubscriberExt, reload, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};

/// Upper bound on how long shutdown waits for background tasks to drain before
/// forcing exit, so a stuck task cannot hang the tray "Exit" / `docker stop`.
const SHUTDOWN_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const CONFIG_KEY_HDHR_UUID: &str = "hdhr.uuid";
const CONFIG_KEY_HDHR_TUNER_COUNT: &str = "hdhr.tuner_count";
const LEGACY_CONFIG_KEY_HDHR_UUID: &str = "HDHR.DeviceUuid";
const LEGACY_CONFIG_KEY_HDHR_TUNER_COUNT: &str = "HDHR.TunerCount";
const DEFAULT_TUNER_COUNT: i32 = 2;
const LEGACY_DEFAULT_MPEGTS_RUN_SH: &[u8] = b"#! /bin/sh\n\n\"{{ FFmpegPath }}\" -nostdin -threads 1 -hide_banner -loglevel error -nostats -fflags +genpts+discardcorrupt+igndts -readrate 1.0 -i \"{{ HlsUrl }}\" -map 0 -c copy -metadata service_provider=\"ErsatzTV\" -metadata service_name=\"{{ ChannelName }}\" -f mpegts pipe:1\n";
const LEGACY_DEFAULT_MPEGTS_RUN_BAT: &[u8] = b"\xEF\xBB\xBF@echo off\n\n\"{{ FFmpegPath }}\" -nostdin -threads 1 -hide_banner -loglevel error -nostats -fflags +genpts+discardcorrupt+igndts -readrate 1.0 -i \"{{ HlsUrl }}\" -map 0 -c copy -metadata service_provider=\"ErsatzTV\" -metadata service_name=\"{{ ChannelName }}\" -f mpegts pipe:1\n";
const MUXRATE_DEFAULT_MPEGTS_RUN_SH: &[u8] = b"#! /bin/sh\n\n\"{{ FFmpegPath }}\" -nostdin -threads 1 -hide_banner -loglevel error -nostats -fflags +genpts+discardcorrupt+igndts -readrate 1.0 -i \"{{ HlsUrl }}\" -map 0 -c copy -muxrate {{ MuxRate }} -metadata service_provider=\"ErsatzTV\" -metadata service_name=\"{{ ChannelName }}\" -f mpegts pipe:1\n";
const MUXRATE_DEFAULT_MPEGTS_RUN_BAT: &[u8] = b"\xEF\xBB\xBF@echo off\n\n\"{{ FFmpegPath }}\" -nostdin -threads 1 -hide_banner -loglevel error -nostats -fflags +genpts+discardcorrupt+igndts -readrate 1.0 -i \"{{ HlsUrl }}\" -map 0 -c copy -muxrate {{ MuxRate }} -metadata service_provider=\"ErsatzTV\" -metadata service_name=\"{{ ChannelName }}\" -f mpegts pipe:1\n";
const DEFAULT_MPEGTS_SCRIPT_FILES: &[DefaultMpegTsScriptFile] = &[
    DefaultMpegTsScriptFile {
        name: "mpegts.yml",
        contents: include_bytes!("../../../../assets/Scripts/MpegTs/mpegts.yml"),
        legacy_contents: &[],
    },
    DefaultMpegTsScriptFile {
        name: "run.sh",
        contents: include_bytes!("../../../../assets/Scripts/MpegTs/run.sh"),
        legacy_contents: &[LEGACY_DEFAULT_MPEGTS_RUN_SH, MUXRATE_DEFAULT_MPEGTS_RUN_SH],
    },
    DefaultMpegTsScriptFile {
        name: "run.bat",
        contents: include_bytes!("../../../../assets/Scripts/MpegTs/run.bat"),
        legacy_contents: &[
            LEGACY_DEFAULT_MPEGTS_RUN_BAT,
            MUXRATE_DEFAULT_MPEGTS_RUN_BAT,
        ],
    },
];

struct DefaultMpegTsScriptFile {
    name: &'static str,
    contents: &'static [u8],
    legacy_contents: &'static [&'static [u8]],
}
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
    let listen_addr = configured_listen_addr();

    prepare_streaming_resources().await;

    tracing::info!(
        database_url = %startup.database_url,
        ?startup.database_backend,
        "database initialised"
    );

    let ffmpeg_runtime = resolve_and_check_startup_ffmpeg_runtime(&startup.pool).await;
    apply_seeded_default_profile_acceleration(&startup, &ffmpeg_runtime).await;
    // Match ErsatzTV's unbounded single-reader worker channel. A manual reset
    // must be accepted or return an error; bounded `try_send` silently lost
    // requests during Reset All operations.
    let (playout_build_tx, playout_build_rx) = tokio::sync::mpsc::unbounded_channel();
    let (scan_work_tx, scan_work_rx) = tokio::sync::mpsc::unbounded_channel();
    // Single playout lock registry shared with the scheduler. Library/show scan
    // admission and lifecycle live in ApplicationServices and are consumed by
    // the separate single-reader scan channel above.
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
        scan_work_tx,
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
            listen_port: listen_addr.port(),
        },
        super::tasks::MaintenanceTaskInputs {
            pool: app_state.pool.clone(),
            playout_build_rx,
            scan_work_rx,
            application: app_state.application.clone(),
            hls_sessions: app_state.hls_sessions.clone(),
        },
        shutdown.clone(),
    );

    // Keep a pool handle for the post-serve cleanup; `build_app` consumes state.
    let pool = app_state.pool.clone();
    let app = super::router::build_app(app_state);

    tracing::info!(%listen_addr, "ersatzrs-web listening");

    let listener = tokio::net::TcpListener::bind(listen_addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind to {listen_addr}: {e}"));

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

async fn prepare_streaming_resources() {
    if let Err(error) = seed_default_mpegts_script().await {
        tracing::warn!(%error, "failed to seed default MPEG-TS script resources");
    }
    if let Err(error) = extract_streaming_resources().await {
        tracing::warn!(%error, "failed to extract bundled streaming resources");
    }
    if let Err(error) = extract_streaming_fonts().await {
        tracing::warn!(%error, "failed to extract bundled streaming fonts");
    }
    if let Err(error) = create_user_content_folders().await {
        tracing::warn!(%error, "failed to create user content folders");
    }
}

/// Create the folders users drop their own content into, mirroring ErsatzTV's
/// `directoriesToCreate` list in `Startup.cs`. Readers already treat an absent
/// folder as empty, so this only makes the drop locations discoverable.
async fn create_user_content_folders() -> io::Result<()> {
    tokio::fs::create_dir_all(file_system_layout::channel_stream_selectors_folder()).await
}

fn configured_listen_addr() -> std::net::SocketAddr {
    super::config::leptos_configuration()
        .leptos_options
        .site_addr
}

async fn resolve_and_check_startup_ffmpeg_runtime(
    pool: &DbPool,
) -> ersatzrs_ffmpeg::FfmpegRuntimeReport {
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    let ffmpeg_config = repositories
        .config
        .get(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH)
        .await
        .unwrap_or_default();
    let ffprobe_config = repositories
        .config
        .get(ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH)
        .await
        .unwrap_or_default();
    let paths_managed = repositories
        .config
        .get(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED)
        .await
        .unwrap_or_default()
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let manager = ersatzrs_ffmpeg::FfmpegRuntimeManagerConfig::from_env(
        file_system_layout::ffmpeg_runtime_folder(),
    );
    let legacy_managed_paths = persisted_paths_are_managed(
        ffmpeg_config.as_deref(),
        ffprobe_config.as_deref(),
        &manager.managed_root,
    );
    let ffmpeg_config = sanitize_startup_ffmpeg_config(
        &repositories.config,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH,
        ffmpeg_config,
        "ffmpeg.exe",
    )
    .await;
    let ffprobe_config = sanitize_startup_ffmpeg_config(
        &repositories.config,
        ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH,
        ffprobe_config,
        "ffprobe.exe",
    )
    .await;
    let (ffmpeg_config, ffprobe_config) = if paths_managed || legacy_managed_paths {
        (None, None)
    } else {
        (ffmpeg_config, ffprobe_config)
    };
    let client = ersatzrs_ffmpeg::GitHubFfmpegReleaseClient::default_repository();
    let validator = StartupFfmpegRuntimeValidator { pool: pool.clone() };

    match ersatzrs_ffmpeg::resolve_or_acquire_ffmpeg_runtime_with(
        &manager,
        ffmpeg_config.as_deref(),
        ffprobe_config.as_deref(),
        &client,
        &validator,
    )
    .await
    {
        Ok(report) => finish_startup_ffmpeg_runtime(pool, report).await,
        Err(error) => {
            tracing::error!(error = %error, "FFmpeg runtime acquisition failed");
            std::process::exit(1);
        }
    }
}

fn persisted_paths_are_managed(
    ffmpeg: Option<&str>,
    ffprobe: Option<&str>,
    managed_root: &Path,
) -> bool {
    let (Some(ffmpeg), Some(ffprobe)) = (ffmpeg, ffprobe) else {
        return false;
    };
    let ffmpeg = Path::new(ffmpeg.trim());
    let ffprobe = Path::new(ffprobe.trim());
    if ffmpeg.starts_with(managed_root) && ffprobe.starts_with(managed_root) {
        return true;
    }

    #[cfg(windows)]
    {
        let executable_dir = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf));
        executable_dir.is_some_and(|directory| {
            ffmpeg.parent() == Some(directory.as_path())
                && ffprobe.parent() == Some(directory.as_path())
                && ffmpeg
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("ffmpeg.exe"))
                && ffprobe
                    .file_name()
                    .is_some_and(|name| name.eq_ignore_ascii_case("ffprobe.exe"))
        })
    }
    #[cfg(not(windows))]
    {
        false
    }
}

async fn sanitize_startup_ffmpeg_config(
    repository: &dyn ConfigRepository,
    key: &str,
    value: Option<String>,
    tool_name: &str,
) -> Option<String> {
    let value = value?;

    if !legacy_missing_release_bundle_tool(&value, tool_name) {
        return Some(value);
    }

    tracing::warn!(
        key = %key,
        path = %value,
        "clearing stale bundled FFmpeg path so managed runtime acquisition can run"
    );
    if let Err(error) = repository.set(key, "").await {
        tracing::warn!(
            key = %key,
            path = %value,
            %error,
            "failed to clear stale bundled FFmpeg path"
        );
    }

    None
}

fn legacy_missing_release_bundle_tool(value: &str, tool_name: &str) -> bool {
    let path = Path::new(value.trim());
    if path.as_os_str().is_empty() || path.exists() {
        return false;
    }

    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if !file_name.eq_ignore_ascii_case(tool_name) {
        return false;
    }

    let Some(parent) = path.parent() else {
        return false;
    };

    parent.join("ersatzrs-web.exe").is_file() || parent.join("ErsatzRS.exe").is_file()
}

async fn apply_seeded_default_profile_acceleration(
    startup: &ersatzrs_infra::db::MediaServerStartupDatabase,
    ffmpeg_runtime: &ersatzrs_ffmpeg::FfmpegRuntimeReport,
) {
    if !startup.seeded_default_ffmpeg_profiles {
        return;
    }

    let acceleration =
        ersatzrs_ffmpeg::preferred_default_profile_acceleration(&ffmpeg_runtime.capabilities);
    if let Err(error) = ersatzrs_infra::db::apply_default_ffmpeg_profile_hardware_acceleration(
        &startup.pool,
        acceleration,
    )
    .await
    {
        tracing::warn!(
            %error,
            ?acceleration,
            "failed to apply startup FFmpeg hardware acceleration to seeded profiles"
        );
    } else {
        tracing::info!(
            ?acceleration,
            "applied startup FFmpeg hardware acceleration to seeded profiles"
        );
    }
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
    for script_file in DEFAULT_MPEGTS_SCRIPT_FILES {
        let path = folder.join(script_file.name);
        if tokio::fs::try_exists(&path).await? {
            if !script_file.legacy_contents.is_empty() {
                let existing = tokio::fs::read(&path).await?;
                if script_file
                    .legacy_contents
                    .iter()
                    .any(|legacy| existing == *legacy)
                {
                    tokio::fs::write(path, script_file.contents).await?;
                }
            }
            continue;
        }
        tokio::fs::write(path, script_file.contents).await?;
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

struct StartupFfmpegRuntimeValidator {
    pool: DbPool,
}

#[async_trait::async_trait]
impl ersatzrs_ffmpeg::FfmpegRuntimeValidator for StartupFfmpegRuntimeValidator {
    async fn validate(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
    ) -> Result<ersatzrs_ffmpeg::FfmpegRuntimeReport, ersatzrs_ffmpeg::FfmpegCheckError> {
        let config =
            ffmpeg_runtime_config_with_profile_probes(&self.pool, ffmpeg_path, ffprobe_path).await;
        ersatzrs_ffmpeg::check_ffmpeg_runtime_with_config(&config).await
    }
}

fn managed_runtime_source(report: &ersatzrs_ffmpeg::FfmpegRuntimeReport) -> bool {
    matches!(
        report.acquisition.source,
        ersatzrs_ffmpeg::FfmpegRuntimeSource::Managed
            | ersatzrs_ffmpeg::FfmpegRuntimeSource::Downloaded
    )
}

#[cfg(windows)]
fn colocate_managed_ffmpeg_runtime(
    report: &mut ersatzrs_ffmpeg::FfmpegRuntimeReport,
) -> io::Result<()> {
    if !managed_runtime_source(report) {
        return Ok(());
    }

    let Some(destination) = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
    else {
        return Ok(());
    };

    if let Some((ffmpeg_path, ffprobe_path)) = copy_runtime_tools_to_directory(
        Path::new(&report.ffmpeg.path),
        Path::new(&report.ffprobe.path),
        &destination,
    )? {
        report.ffmpeg.path = ffmpeg_path.to_string_lossy().into_owned();
        report.ffprobe.path = ffprobe_path.to_string_lossy().into_owned();
    }

    Ok(())
}

#[cfg(any(windows, test))]
fn copy_runtime_tools_to_directory(
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
    destination: &Path,
) -> io::Result<Option<(PathBuf, PathBuf)>> {
    let Some(source_dir) = ffmpeg_path.parent() else {
        return Ok(None);
    };
    let (Some(ffmpeg_file), Some(ffprobe_file)) =
        (ffmpeg_path.file_name(), ffprobe_path.file_name())
    else {
        return Ok(None);
    };

    std::fs::create_dir_all(destination)?;
    if source_dir != destination {
        for entry in std::fs::read_dir(source_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                std::fs::copy(entry.path(), destination.join(entry.file_name()))?;
            }
        }
    }

    let copied_ffmpeg = destination.join(ffmpeg_file);
    let copied_ffprobe = destination.join(ffprobe_file);
    if copied_ffmpeg.is_file() && copied_ffprobe.is_file() {
        Ok(Some((copied_ffmpeg, copied_ffprobe)))
    } else {
        Ok(None)
    }
}

async fn persist_managed_ffmpeg_runtime_paths(
    pool: &DbPool,
    report: &ersatzrs_ffmpeg::FfmpegRuntimeReport,
) -> ersatzrs_core::repositories::Result<()> {
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    persist_managed_ffmpeg_runtime_paths_to_repo(&repositories.config, report).await
}

async fn persist_managed_ffmpeg_runtime_paths_to_repo(
    repository: &dyn ConfigRepository,
    report: &ersatzrs_ffmpeg::FfmpegRuntimeReport,
) -> ersatzrs_core::repositories::Result<()> {
    if !managed_runtime_source(report) {
        return Ok(());
    }

    repository
        .set(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH, &report.ffmpeg.path)
        .await?;
    repository
        .set(
            ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH,
            &report.ffprobe.path,
        )
        .await?;
    repository
        .set(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED, "true")
        .await?;

    Ok(())
}

async fn finish_startup_ffmpeg_runtime(
    pool: &DbPool,
    mut report: ersatzrs_ffmpeg::FfmpegRuntimeReport,
) -> ersatzrs_ffmpeg::FfmpegRuntimeReport {
    #[cfg(not(windows))]
    let _ = &mut report;
    #[cfg(windows)]
    if let Err(error) = colocate_managed_ffmpeg_runtime(&mut report) {
        tracing::warn!(%error, "failed to copy managed FFmpeg runtime next to executable");
    }
    if let Err(error) = persist_managed_ffmpeg_runtime_paths(pool, &report).await {
        tracing::warn!(%error, "failed to persist managed FFmpeg runtime paths");
    }

    ersatzrs_ffmpeg::set_resolved_ffmpeg_path(report.ffmpeg.path.clone());
    ersatzrs_ffmpeg::set_resolved_ffprobe_path(report.ffprobe.path.clone());

    // Startup is not an explicit capability refresh (ADR 0014): denials learned
    // against this same ffmpeg binary are reapplied, but a path change since the
    // last run clears them instead.
    if let Err(error) = ersatzrs_infra::ffmpeg_capabilities::apply_hardware_decode_denials(
        pool,
        &mut report.capabilities,
        &report.ffmpeg.path,
    )
    .await
    {
        tracing::warn!(%error, "failed to apply runtime-learned hardware decode denials");
    }

    // Operation-scoped graph capability (ADR 0014, milestone p8b-01). Persisted
    // denials are reapplied first so a machine whose graphs were already probed
    // does not pay for the probes again; a fresh install (or one whose ffmpeg
    // path changed, which clears the rows above) probes once and persists the
    // result. An advertised hwaccel is not evidence the graph works.
    // A reusable record means every applicable graph already has terminal
    // evidence for this binary and device. Anything else — stale identity,
    // a partial record from an unclassified probe failure, or a legacy row —
    // re-probes. An empty denial list is not proof that every graph works.
    let reused = ersatzrs_infra::hardware_graph::apply_hardware_graph_denials(
        pool,
        &report.ffmpeg.path,
        &mut report.capabilities,
    )
    .await
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "failed to apply runtime-learned hardware graph denials");
        false
    });
    if !reused {
        ersatzrs_infra::hardware_graph::refresh_hardware_graph_capabilities(
            std::path::Path::new(&report.ffmpeg.path),
            &mut report.capabilities,
        )
        .await;
        if let Err(error) = ersatzrs_infra::hardware_graph::persist_hardware_graph_denials(
            pool,
            &report.ffmpeg.path,
            &report.capabilities,
        )
        .await
        {
            tracing::warn!(%error, "failed to persist runtime-learned hardware graph denials");
        }
    }
    for denial in &report.capabilities.hardware_graph_denials {
        tracing::warn!(evidence = %denial.evidence(), "hardware graph unavailable; using fallback");
    }
    tracing::info!(
        ffmpeg = %report.ffmpeg.version,
        ffprobe = %report.ffprobe.version,
        ffmpeg_path = %report.ffmpeg.path,
        ffprobe_path = %report.ffprobe.path,
        source = %report.acquisition.source,
        installed_tag = ?report.acquisition.installed_tag,
        latest_tag = ?report.acquisition.latest_tag,
        previous_tag = ?report.acquisition.previous_tag,
        downloaded_update = report.acquisition.downloaded_update,
        update_available = report.acquisition.update_available,
        update_error = ?report.acquisition.update_check_error,
        "FFmpeg 9 runtime detected"
    );
    for issue in report.issues.iter().filter(|issue| !issue.is_error()) {
        tracing::warn!(?issue, "FFmpeg runtime capability warning");
    }
    report
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
    playout_build_tx: tokio::sync::mpsc::UnboundedSender<
        ersatzrs_core::scheduling::PlayoutBuildRequest,
    >,
    scan_work_tx: tokio::sync::mpsc::UnboundedSender<ersatzrs_app::commands::ScanWorkRequest>,
    playout_locker: ersatzrs_core::locking::EntityLocker,
) -> ersatzrs_app::Result<AppState> {
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
            .await?;
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

    Ok(AppState::new_with_scan_queue(
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
        scan_work_tx,
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
) -> ersatzrs_app::Result<Option<String>> {
    if let Some(value) = repository.get(key).await? {
        return Ok(Some(value));
    }

    let Some(legacy_key) = legacy_key else {
        return Ok(None);
    };

    let legacy_value = repository.get(legacy_key).await?;

    if let Some(value) = legacy_value {
        repository.set(key, &value).await?;
        return Ok(Some(value));
    }

    Ok(None)
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
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct TestConfigRepository {
        values: Mutex<BTreeMap<String, String>>,
    }

    #[async_trait::async_trait]
    impl ConfigRepository for TestConfigRepository {
        async fn get(&self, key: &str) -> ersatzrs_core::repositories::Result<Option<String>> {
            Ok(self.values.lock().expect("values").get(key).cloned())
        }

        async fn set(&self, key: &str, value: &str) -> ersatzrs_core::repositories::Result<()> {
            self.values
                .lock()
                .expect("values")
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }
    }

    #[test]
    fn copy_runtime_tools_to_directory_copies_companion_files() {
        let source = tempfile::tempdir().expect("source runtime");
        let destination = tempfile::tempdir().expect("release dir");
        std::fs::write(source.path().join("ffmpeg.exe"), b"ffmpeg").expect("write ffmpeg");
        std::fs::write(source.path().join("ffprobe.exe"), b"ffprobe").expect("write ffprobe");
        std::fs::write(source.path().join("avcodec.dll"), b"dll").expect("write dll");

        let copied = copy_runtime_tools_to_directory(
            &source.path().join("ffmpeg.exe"),
            &source.path().join("ffprobe.exe"),
            destination.path(),
        )
        .expect("copy runtime")
        .expect("copied paths");

        assert_eq!(copied.0, destination.path().join("ffmpeg.exe"));
        assert_eq!(copied.1, destination.path().join("ffprobe.exe"));
        assert_eq!(
            std::fs::read(destination.path().join("avcodec.dll")).expect("read dll"),
            b"dll"
        );
    }

    #[tokio::test]
    async fn managed_ffmpeg_runtime_paths_are_persisted_to_config() {
        let repo = TestConfigRepository::default();
        let mut report = ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests();
        report.acquisition.source = ersatzrs_ffmpeg::FfmpegRuntimeSource::Downloaded;
        report.ffmpeg.path = r"C:\ErsatzRS\ffmpeg\current\ffmpeg.exe".to_owned();
        report.ffprobe.path = r"C:\ErsatzRS\ffmpeg\current\ffprobe.exe".to_owned();

        persist_managed_ffmpeg_runtime_paths_to_repo(&repo, &report)
            .await
            .expect("persist paths");

        assert_eq!(
            repo.get(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH)
                .await
                .expect("ffmpeg path")
                .as_deref(),
            Some(r"C:\ErsatzRS\ffmpeg\current\ffmpeg.exe")
        );
        assert_eq!(
            repo.get(ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH)
                .await
                .expect("ffprobe path")
                .as_deref(),
            Some(r"C:\ErsatzRS\ffmpeg\current\ffprobe.exe")
        );
        assert_eq!(
            repo.get(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATHS_MANAGED)
                .await
                .expect("managed marker")
                .as_deref(),
            Some("true")
        );
    }

    #[test]
    fn persisted_paths_under_managed_root_are_managed() {
        let root = Path::new("/var/lib/ersatzrs/ffmpeg");

        assert!(persisted_paths_are_managed(
            Some("/var/lib/ersatzrs/ffmpeg/current/ffmpeg"),
            Some("/var/lib/ersatzrs/ffmpeg/current/ffprobe"),
            root,
        ));
        assert!(!persisted_paths_are_managed(
            Some("/opt/operator/ffmpeg"),
            Some("/opt/operator/ffprobe"),
            root,
        ));
    }

    #[tokio::test]
    async fn path_ffmpeg_runtime_paths_do_not_override_config() {
        let repo = TestConfigRepository::default();
        repo.set(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH, "custom-ffmpeg")
            .await
            .expect("seed ffmpeg path");
        let mut report = ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests();
        report.acquisition.source = ersatzrs_ffmpeg::FfmpegRuntimeSource::Path;
        report.ffmpeg.path = "ffmpeg-from-path".to_owned();
        report.ffprobe.path = "ffprobe-from-path".to_owned();

        persist_managed_ffmpeg_runtime_paths_to_repo(&repo, &report)
            .await
            .expect("persist paths");

        assert_eq!(
            repo.get(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH)
                .await
                .expect("ffmpeg path")
                .as_deref(),
            Some("custom-ffmpeg")
        );
    }

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
        let run_bat = String::from_utf8_lossy(&run_bat);
        assert!(run_bat.contains("FFmpegPath"));
        assert!(run_bat.contains("MuxRate"));
        assert!(run_bat.contains("PcrPeriod"));
    }

    #[tokio::test]
    async fn seed_default_mpegts_script_upgrades_unchanged_legacy_defaults() {
        let root = tempfile::tempdir().expect("temp script root");
        let folder = root.path().join("default");
        tokio::fs::create_dir_all(&folder)
            .await
            .expect("create script folder");
        tokio::fs::write(folder.join("run.sh"), LEGACY_DEFAULT_MPEGTS_RUN_SH)
            .await
            .expect("write old shell script");
        tokio::fs::write(folder.join("run.bat"), LEGACY_DEFAULT_MPEGTS_RUN_BAT)
            .await
            .expect("write old batch script");

        seed_default_mpegts_script_in(&folder)
            .await
            .expect("seed scripts");

        let run_sh = tokio::fs::read_to_string(folder.join("run.sh"))
            .await
            .expect("read shell script");
        let run_bat = tokio::fs::read_to_string(folder.join("run.bat"))
            .await
            .expect("read batch script");

        assert!(run_sh.contains("MuxRate"));
        assert!(run_sh.contains("PcrPeriod"));
        assert!(run_bat.contains("MuxRate"));
        assert!(run_bat.contains("PcrPeriod"));
    }

    #[tokio::test]
    async fn seed_default_mpegts_script_upgrades_muxrate_only_defaults() {
        let root = tempfile::tempdir().expect("temp script root");
        let folder = root.path().join("default");
        tokio::fs::create_dir_all(&folder)
            .await
            .expect("create script folder");
        tokio::fs::write(folder.join("run.sh"), MUXRATE_DEFAULT_MPEGTS_RUN_SH)
            .await
            .expect("write muxrate shell script");
        tokio::fs::write(folder.join("run.bat"), MUXRATE_DEFAULT_MPEGTS_RUN_BAT)
            .await
            .expect("write muxrate batch script");

        seed_default_mpegts_script_in(&folder)
            .await
            .expect("seed scripts");

        let run_sh = tokio::fs::read_to_string(folder.join("run.sh"))
            .await
            .expect("read shell script");
        let run_bat = tokio::fs::read_to_string(folder.join("run.bat"))
            .await
            .expect("read batch script");

        assert!(run_sh.contains("MuxRate"));
        assert!(run_sh.contains("PcrPeriod"));
        assert!(run_bat.contains("MuxRate"));
        assert!(run_bat.contains("PcrPeriod"));
    }

    #[test]
    fn legacy_missing_release_bundle_tool_detects_old_windows_bundle_paths() {
        let root = tempfile::tempdir().expect("temp release root");
        std::fs::write(root.path().join("ersatzrs-web.exe"), b"server")
            .expect("write server exe marker");
        let legacy_ffmpeg = root.path().join("ffmpeg.exe");

        assert!(legacy_missing_release_bundle_tool(
            &legacy_ffmpeg.to_string_lossy(),
            "ffmpeg.exe"
        ));
    }

    #[test]
    fn legacy_missing_release_bundle_tool_keeps_real_overrides_explicit() {
        let root = tempfile::tempdir().expect("temp release root");
        std::fs::write(root.path().join("ersatzrs-web.exe"), b"server")
            .expect("write server exe marker");
        std::fs::write(root.path().join("ffmpeg.exe"), b"ffmpeg").expect("write ffmpeg");
        let existing_ffmpeg = root.path().join("ffmpeg.exe");
        let missing_custom = root.path().join("tools").join("ffmpeg.exe");
        let wrong_tool = root.path().join("ffprobe.exe");

        assert!(!legacy_missing_release_bundle_tool(
            &existing_ffmpeg.to_string_lossy(),
            "ffmpeg.exe"
        ));
        assert!(!legacy_missing_release_bundle_tool(
            &missing_custom.to_string_lossy(),
            "ffmpeg.exe"
        ));
        assert!(!legacy_missing_release_bundle_tool(
            &wrong_tool.to_string_lossy(),
            "ffmpeg.exe"
        ));
    }

    #[tokio::test]
    async fn sanitize_startup_ffmpeg_config_clears_legacy_missing_bundle_path() {
        let root = tempfile::tempdir().expect("temp release root");
        std::fs::write(root.path().join("ersatzrs-web.exe"), b"server")
            .expect("write server exe marker");
        let legacy_ffmpeg = root
            .path()
            .join("ffmpeg.exe")
            .to_string_lossy()
            .into_owned();
        let repo = TestConfigRepository::default();

        let sanitized = sanitize_startup_ffmpeg_config(
            &repo,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH,
            Some(legacy_ffmpeg),
            "ffmpeg.exe",
        )
        .await;

        assert!(sanitized.is_none());
        assert_eq!(
            repo.get(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH)
                .await
                .expect("config")
                .as_deref(),
            Some("")
        );
    }
}

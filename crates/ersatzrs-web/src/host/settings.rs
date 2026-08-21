use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};

use async_trait::async_trait;
use tokio::sync::RwLock;

use ersatzrs_app::{
    SettingsFfmpegPaths, SettingsRuntime, SettingsRuntimeError, WindowsStartupStatus,
};
use ersatzrs_core::repositories::{AdminSettingsRepository, SettingsMutation};
use ersatzrs_domain::EntityId;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::DbPool;
use ersatzrs_platform::windows_startup;

#[derive(Clone)]
pub struct NativeSettingsRuntime {
    pool: DbPool,
    ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    tuner_count: Arc<AtomicI32>,
}

impl NativeSettingsRuntime {
    #[must_use]
    pub fn new(
        pool: DbPool,
        ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
        tuner_count: Arc<AtomicI32>,
    ) -> Self {
        Self {
            pool,
            ffmpeg_runtime,
            tuner_count,
        }
    }
}

#[async_trait]
impl SettingsRuntime for NativeSettingsRuntime {
    fn windows_startup_status(&self) -> WindowsStartupStatus {
        windows_startup_status()
    }

    fn set_windows_startup(&self, enabled: bool) -> Result<(), SettingsRuntimeError> {
        set_windows_startup(enabled)
    }

    async fn reload_logging(&self) -> Result<(), SettingsRuntimeError> {
        crate::host::bootstrap::reload_logging_from_database(&self.pool)
            .await
            .map_err(SettingsRuntimeError::unavailable)
    }

    async fn validate_ffmpeg_tools(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
    ) -> Result<SettingsFfmpegPaths, SettingsRuntimeError> {
        let ffmpeg = ersatzrs_ffmpeg::resolve_ffmpeg_path(Some(ffmpeg_path)).path;
        let ffprobe = ersatzrs_ffmpeg::resolve_ffprobe_path(Some(ffprobe_path)).path;
        validate_tool(&ffmpeg, "ffmpeg").await?;
        validate_tool(&ffprobe, "ffprobe").await?;
        Ok(SettingsFfmpegPaths { ffmpeg, ffprobe })
    }

    async fn commit_ffmpeg_settings(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
        mut mutations: Vec<SettingsMutation>,
    ) -> Result<(), SettingsRuntimeError> {
        let config = crate::host::bootstrap::ffmpeg_runtime_config_with_profile_probes(
            &self.pool,
            ffmpeg_path,
            ffprobe_path,
        )
        .await;
        let mut report = ersatzrs_ffmpeg::check_ffmpeg_runtime_with_config(&config)
            .await
            .map_err(|error| SettingsRuntimeError::unavailable(error.to_string()))?;

        // Saving FFmpeg settings is an explicit capability refresh (ADR 0014),
        // so learned graph denials are discarded and re-probed against the
        // runtime the operator just selected.
        ersatzrs_infra::hardware_graph::refresh_hardware_graph_capabilities(
            std::path::Path::new(ffmpeg_path),
            &mut report.capabilities,
        )
        .await;

        // Hold publication closed while the database transaction commits. A
        // failure leaves both the old settings and old runtime active.
        let mut active = self.ffmpeg_runtime.write().await;
        report.set_generation(active.generation.saturating_add(1));
        let graph_state = ersatzrs_infra::hardware_graph::encode_hardware_graph_state(
            ffmpeg_path,
            &report.capabilities,
        )
        .map_err(|error| SettingsRuntimeError::unavailable(error.to_string()))?;
        mutations.push(SettingsMutation::Delete {
            key: ersatzrs_infra::ffmpeg_capabilities::CONFIG_KEY_HARDWARE_DECODE_DENIALS.to_owned(),
        });
        mutations.push(SettingsMutation::Set {
            key: ersatzrs_infra::hardware_graph::CONFIG_KEY_HARDWARE_GRAPH_DENIALS.to_owned(),
            value: graph_state,
        });
        let repository =
            ersatzrs_infra::repositories::SqliteAdminSettingsRepository(self.pool.clone());
        repository
            .apply_mutations(mutations)
            .await
            .map_err(|error| SettingsRuntimeError::unavailable(error.to_string()))?;

        *active = report;
        ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg_path, ffprobe_path);
        Ok(())
    }

    fn request_embedded_subtitle_extraction(&self, playout_ids: Vec<EntityId>) {
        let pool = self.pool.clone();
        tokio::spawn(async move {
            for playout_id in playout_ids {
                if let Err(error) =
                    ersatzrs_infra::subtitles::extract_embedded_playout_subtitles(&pool, playout_id)
                        .await
                {
                    tracing::warn!(
                        playout_id,
                        %error,
                        "embedded subtitle extraction failed after ffmpeg settings save"
                    );
                }
            }
        });
    }

    fn ensure_ffmpeg_report_directory(&self) {
        let path = ersatzrs_infra::logging::default_log_dir().join("ffmpeg");
        if let Err(error) = std::fs::create_dir_all(&path) {
            tracing::warn!(%error, path = %path.display(), "failed to create ffmpeg report folder");
        }
    }

    fn set_tuner_count(&self, tuner_count: i32) {
        self.tuner_count.store(tuner_count, Ordering::Relaxed);
    }
}

fn windows_startup_status() -> WindowsStartupStatus {
    match windows_startup::status() {
        Ok(windows_startup::Status::Unsupported) => WindowsStartupStatus::Unsupported,
        Ok(windows_startup::Status::Disabled) => WindowsStartupStatus::Disabled,
        Ok(windows_startup::Status::Enabled) => WindowsStartupStatus::Enabled,
        Ok(windows_startup::Status::NeedsRepair) => WindowsStartupStatus::NeedsRepair,
        Ok(windows_startup::Status::Unavailable) => WindowsStartupStatus::Unavailable,
        Err(error) => {
            tracing::warn!(%error, "failed to inspect Windows startup registration");
            WindowsStartupStatus::Unavailable
        }
    }
}

fn set_windows_startup(enabled: bool) -> Result<(), SettingsRuntimeError> {
    windows_startup::set_enabled(enabled).map_err(|error| {
        tracing::warn!(%error, enabled, "failed to update Windows startup registration");
        match error {
            windows_startup::Error::Unsupported => SettingsRuntimeError::unavailable(
                "Windows startup registration is not supported on this platform",
            ),
            windows_startup::Error::ResolveExecutable(_)
            | windows_startup::Error::LauncherUnavailable => SettingsRuntimeError::unavailable(
                "ErsatzRS.exe was not found next to ersatzrs-web.exe",
            ),
            windows_startup::Error::Registry(_) => {
                SettingsRuntimeError::unavailable("Unable to update Windows startup registration")
            }
        }
    })
}

async fn validate_tool(path: &str, name: &str) -> Result<(), SettingsRuntimeError> {
    if path.trim().is_empty() {
        return Err(SettingsRuntimeError::unavailable(format!(
            "{name} path does not exist"
        )));
    }
    let output = tokio::process::Command::new(path)
        .arg("-version")
        .output()
        .await
        .map_err(|_| {
            SettingsRuntimeError::unavailable(format!("Unable to verify {name} version"))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout.contains(&format!("{name} version")) {
        Ok(())
    } else {
        Err(SettingsRuntimeError::unavailable(format!(
            "Unable to verify {name} version"
        )))
    }
}

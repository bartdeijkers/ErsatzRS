use std::sync::{
    atomic::{AtomicI32, Ordering},
    Arc,
};

use async_trait::async_trait;
use tokio::sync::RwLock;

use ersatzrs_app::{SettingsFfmpegPaths, SettingsRuntime, SettingsRuntimeError};
use ersatzrs_domain::EntityId;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::DbPool;

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

    async fn refresh_ffmpeg_runtime(
        &self,
        ffmpeg_path: &str,
        ffprobe_path: &str,
    ) -> Result<(), SettingsRuntimeError> {
        ersatzrs_ffmpeg::set_resolved_ffmpeg_path(ffmpeg_path.to_owned());
        ersatzrs_ffmpeg::set_resolved_ffprobe_path(ffprobe_path.to_owned());
        let config = crate::host::bootstrap::ffmpeg_runtime_config_with_profile_probes(
            &self.pool,
            ffmpeg_path,
            ffprobe_path,
        )
        .await;
        let report = ersatzrs_ffmpeg::check_ffmpeg_runtime_with_config(&config)
            .await
            .map_err(|error| SettingsRuntimeError::unavailable(error.to_string()))?;
        ersatzrs_infra::ffmpeg_capabilities::clear_hardware_decode_denials(&self.pool)
            .await
            .map_err(|error| SettingsRuntimeError::unavailable(error.to_string()))?;
        *self.ffmpeg_runtime.write().await = report;
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

use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_app::{
    AdminFfmpegRuntimeRead, AdminHealthRead, AdminHealthRuntime, AdminHealthRuntimeError,
};
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::health::HealthCheckCoordinator;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct NativeAdminHealthRuntime {
    pool: DbPool,
    ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    health: HealthCheckCoordinator,
    startup_errors: u32,
    startup_warnings: u32,
}

impl NativeAdminHealthRuntime {
    #[must_use]
    pub fn new(
        pool: DbPool,
        ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
        health: HealthCheckCoordinator,
        startup_errors: u32,
        startup_warnings: u32,
    ) -> Self {
        Self {
            pool,
            ffmpeg_runtime,
            health,
            startup_errors,
            startup_warnings,
        }
    }

    async fn runtime_read(&self) -> AdminFfmpegRuntimeRead {
        let runtime = self.ffmpeg_runtime.read().await;
        AdminFfmpegRuntimeRead {
            source: runtime.acquisition.source.to_string(),
            ffmpeg_version: runtime.ffmpeg.version.to_string(),
            ffprobe_version: runtime.ffprobe.version.to_string(),
            installed_tag: runtime.acquisition.installed_tag.clone(),
            latest_tag: runtime.acquisition.latest_tag.clone(),
            previous_tag: runtime.acquisition.previous_tag.clone(),
            asset_name: runtime.acquisition.asset_name.clone(),
            downloaded_update: runtime.acquisition.downloaded_update,
            update_available: runtime.acquisition.update_available,
            update_check_error: runtime.acquisition.update_check_error.clone(),
        }
    }
}

#[async_trait]
impl AdminHealthRuntime for NativeAdminHealthRuntime {
    async fn issue_counts(&self) -> Result<(u32, u32), AdminHealthRuntimeError> {
        use ersatzrs_infra::health::SqlHealthCheckStore;

        let runtime_counts = self.ffmpeg_runtime.read().await.issue_counts();
        let summary = SqlHealthCheckStore::new(self.pool.clone())
            .load_summary()
            .await
            .map_err(|error| AdminHealthRuntimeError::new(error.to_string()))?;
        Ok((
            self.startup_errors
                .saturating_add(runtime_counts.0)
                .saturating_add(summary.errors),
            self.startup_warnings
                .saturating_add(runtime_counts.1)
                .saturating_add(summary.warnings),
        ))
    }

    async fn run_health_checks(
        &self,
        refresh: bool,
    ) -> Result<AdminHealthRead, AdminHealthRuntimeError> {
        let run = self
            .health
            .run(refresh)
            .await
            .map_err(|error| AdminHealthRuntimeError::new(error.to_string()))?;
        Ok(AdminHealthRead {
            summary: run.summary,
            results: run.results,
            ffmpeg_runtime: self.runtime_read().await,
        })
    }

    async fn ffmpeg_runtime(&self) -> AdminFfmpegRuntimeRead {
        self.runtime_read().await
    }
}

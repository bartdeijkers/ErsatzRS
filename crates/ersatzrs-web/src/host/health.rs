use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_app::{
    operations::ScanOperationRegistry, AdminFfmpegRuntimeRead, AdminHealthRead, AdminHealthRuntime,
    AdminHealthRuntimeError,
};
use ersatzrs_core::health::{HealthCheckResult, HealthCheckStatus, HealthCheckSummary};
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::health::HealthCheckCoordinator;
use tokio::sync::RwLock;

const SCAN_WORKER_TITLE: &str = "Scanner worker";
const SCAN_WORKER_BRIEF: &str = "Scanner worker is unavailable";
const SCAN_WORKER_MESSAGE: &str = "The scan worker is unavailable. New scans cannot run. Check the logs and confirm that any remaining local probe processes have stopped before restarting ErsatzRS.";

#[derive(Clone)]
pub struct NativeAdminHealthRuntime {
    pool: DbPool,
    ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    health: HealthCheckCoordinator,
    startup_errors: u32,
    startup_warnings: u32,
    scan_activity: ScanOperationRegistry,
}

impl NativeAdminHealthRuntime {
    #[must_use]
    pub fn new(
        pool: DbPool,
        ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
        health: HealthCheckCoordinator,
        startup_errors: u32,
        startup_warnings: u32,
        scan_activity: ScanOperationRegistry,
    ) -> Self {
        Self {
            pool,
            ffmpeg_runtime,
            health,
            startup_errors,
            startup_warnings,
            scan_activity,
        }
    }

    fn current_results(&self, mut results: Vec<HealthCheckResult>) -> Vec<HealthCheckResult> {
        // This host-local state is read after the cached/persisted baseline.
        // Never persist it across host restarts or count an old copy twice.
        results.retain(|result| result.title != SCAN_WORKER_TITLE);
        if !self.scan_activity.is_worker_available() {
            results.push(
                HealthCheckResult::new(
                    SCAN_WORKER_TITLE,
                    HealthCheckStatus::Fail,
                    SCAN_WORKER_MESSAGE,
                    SCAN_WORKER_BRIEF,
                )
                .with_link("/libraries"),
            );
        }
        results
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
        let results = SqlHealthCheckStore::new(self.pool.clone())
            .load_results()
            .await
            .map_err(|error| AdminHealthRuntimeError::new(error.to_string()))?;
        let summary = HealthCheckSummary::from_results(&self.current_results(results));
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
        let results = self.current_results(run.results);
        Ok(AdminHealthRead {
            summary: HealthCheckSummary::from_results(&results),
            results,
            ffmpeg_runtime: self.runtime_read().await,
        })
    }

    async fn ffmpeg_runtime(&self) -> AdminFfmpegRuntimeRead {
        self.runtime_read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_infra::db::DatabaseSchemaStatus;
    use ersatzrs_infra::health::SqlHealthCheckStore;

    #[tokio::test]
    async fn stopped_scan_worker_is_visible_over_cached_health_and_clears_on_restart() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("health fixture database");
        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .expect("migrate health fixture");
        let ffmpeg = Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests()));
        let health = HealthCheckCoordinator::new(
            pool.clone(),
            ffmpeg.clone(),
            DatabaseSchemaStatus::default(),
        );
        let scan_activity = ScanOperationRegistry::default();
        let runtime = NativeAdminHealthRuntime::new(
            pool.clone(),
            ffmpeg.clone(),
            health.clone(),
            0,
            0,
            scan_activity.clone(),
        );
        let baseline = runtime
            .run_health_checks(false)
            .await
            .expect("initial health");
        let baseline_counts = runtime.issue_counts().await.expect("initial badge");
        assert!(!baseline
            .results
            .iter()
            .any(|row| row.title == SCAN_WORKER_TITLE));

        scan_activity.worker_stopped("local probe cleanup was not confirmed");
        let stopped = runtime
            .run_health_checks(false)
            .await
            .expect("cached baseline plus live failure");
        let row = stopped
            .results
            .iter()
            .find(|row| row.title == SCAN_WORKER_TITLE)
            .expect("visible worker failure detail");
        assert_eq!(row.status, HealthCheckStatus::Fail);
        assert_eq!(row.brief_message, SCAN_WORKER_BRIEF);
        assert_eq!(row.message, SCAN_WORKER_MESSAGE);
        assert_eq!(
            stopped.summary,
            HealthCheckSummary::from_results(&stopped.results)
        );
        assert_eq!(stopped.summary.errors, baseline.summary.errors + 1);
        assert_eq!(
            runtime.issue_counts().await.expect("live badge"),
            (baseline_counts.0 + 1, baseline_counts.1)
        );
        let store = SqlHealthCheckStore::new(pool.clone());
        assert_eq!(
            store.load_results().await.expect("persisted baseline"),
            baseline.results
        );
        assert_eq!(
            store.load_summary().await.expect("persisted summary"),
            baseline.summary
        );
        assert_eq!(
            runtime.current_results(stopped.results.clone()),
            stopped.results,
            "a pre-existing live row is not counted twice"
        );

        // A new host gets a fresh existing registry, never an old host failure.
        let restarted = NativeAdminHealthRuntime::new(
            pool,
            ffmpeg,
            health,
            0,
            0,
            ScanOperationRegistry::default(),
        );
        let fresh = restarted
            .run_health_checks(false)
            .await
            .expect("fresh host cached baseline");
        assert_eq!(fresh.results, baseline.results);
        assert_eq!(fresh.summary, baseline.summary);
        assert_eq!(
            restarted.issue_counts().await.expect("fresh host badge"),
            baseline_counts
        );
        assert!(
            !scan_activity.is_worker_available(),
            "old host remains fenced"
        );
    }
}

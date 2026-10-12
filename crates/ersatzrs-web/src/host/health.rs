use std::sync::Arc;

use async_trait::async_trait;
use ersatzrs_app::{
    operations::ScanOperationRegistry, AdminFfmpegRuntimeRead, AdminHealthRead, AdminHealthRuntime,
    AdminHealthRuntimeError,
};
use ersatzrs_core::health::{HealthCheckResult, HealthCheckStatus, HealthCheckSummary};
use ersatzrs_core::repositories::ConfigRepository;
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::health::HealthCheckCoordinator;
use ersatzrs_infra::repositories::SqliteConfigRepository;
use tokio::sync::RwLock;

const SCAN_WORKER_TITLE: &str = "Scanner worker";
const SCAN_WORKER_BRIEF: &str = "Scanner worker is unavailable";
const SCAN_WORKER_MESSAGE: &str = "The scan worker is unavailable. New scans cannot run. Check the logs and confirm that any remaining local probe processes have stopped before restarting ErsatzRS.";
const TELETEXT_OUTPUT_TITLE: &str = "Teletext output configuration";
const TELETEXT_OUTPUT_ABSENT_BRIEF: &str = "Teletext output is not configured";
const TELETEXT_OUTPUT_ABSENT_MESSAGE: &str = "No teletext output policy is configured. Channels use ordinary playback without adding teletext.";
const TELETEXT_OUTPUT_CONFIGURED_BRIEF: &str = "Teletext output settings are valid";
const TELETEXT_OUTPUT_CONFIGURED_MESSAGE: &str = "The teletext output settings are valid. Playback still checks each channel's accepted pages and stream compatibility. This check does not verify teletext delivery, full Level 2.5 support or memory limits.";
const TELETEXT_OUTPUT_INVALID_BRIEF: &str = "Teletext output settings are invalid";
const TELETEXT_OUTPUT_INVALID_MESSAGE: &str = "The teletext output settings are invalid. Ordinary playback continues without adding teletext. Correct the settings to enable teletext output.";
const TELETEXT_OUTPUT_UNREADABLE_BRIEF: &str = "Teletext output settings could not be read";
const TELETEXT_OUTPUT_UNREADABLE_MESSAGE: &str = "The teletext output settings could not be read. Ordinary playback continues without adding teletext. Check database availability and access.";

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

    async fn current_results(&self, mut results: Vec<HealthCheckResult>) -> Vec<HealthCheckResult> {
        // This host-local state is read after the cached/persisted baseline.
        // Never persist it across host restarts or count an old copy twice.
        results.retain(|result| {
            result.title != SCAN_WORKER_TITLE && result.title != TELETEXT_OUTPUT_TITLE
        });
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
        results.push(teletext_output_configuration(&self.pool).await);
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

async fn teletext_output_configuration(pool: &DbPool) -> HealthCheckResult {
    use super::teletext_runtime_policy::{
        parse_native_teletext_runtime_policy, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
    };

    let configured = SqliteConfigRepository(pool.clone())
        .get(NATIVE_TELETEXT_RUNTIME_POLICY_SETTING)
        .await;
    let (status, message, brief) = match configured {
        Ok(None) => (
            HealthCheckStatus::Info,
            TELETEXT_OUTPUT_ABSENT_MESSAGE,
            TELETEXT_OUTPUT_ABSENT_BRIEF,
        ),
        Ok(Some(raw)) if parse_native_teletext_runtime_policy(&raw).is_ok() => (
            HealthCheckStatus::Pass,
            TELETEXT_OUTPUT_CONFIGURED_MESSAGE,
            TELETEXT_OUTPUT_CONFIGURED_BRIEF,
        ),
        Ok(Some(_)) => (
            HealthCheckStatus::Warning,
            TELETEXT_OUTPUT_INVALID_MESSAGE,
            TELETEXT_OUTPUT_INVALID_BRIEF,
        ),
        Err(_) => (
            HealthCheckStatus::Warning,
            TELETEXT_OUTPUT_UNREADABLE_MESSAGE,
            TELETEXT_OUTPUT_UNREADABLE_BRIEF,
        ),
    };
    HealthCheckResult::new(TELETEXT_OUTPUT_TITLE, status, message, brief)
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
        let summary = HealthCheckSummary::from_results(&self.current_results(results).await);
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
        let results = self.current_results(run.results).await;
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

    async fn health_fixture() -> NativeAdminHealthRuntime {
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
        NativeAdminHealthRuntime::new(pool, ffmpeg, health, 0, 0, ScanOperationRegistry::default())
    }

    fn output_row(results: &[HealthCheckResult]) -> &HealthCheckResult {
        let rows = results
            .iter()
            .filter(|result| result.title == TELETEXT_OUTPUT_TITLE)
            .collect::<Vec<_>>();
        assert_eq!(
            rows.len(),
            1,
            "fresh configuration replaces rather than duplicates cached rows"
        );
        rows[0]
    }

    #[tokio::test]
    async fn stopped_scan_worker_is_visible_over_cached_health_and_clears_on_restart() {
        let runtime = health_fixture().await;
        let pool = runtime.pool.clone();
        let ffmpeg = runtime.ffmpeg_runtime.clone();
        let health = runtime.health.clone();
        let scan_activity = runtime.scan_activity.clone();
        let baseline = runtime
            .run_health_checks(false)
            .await
            .expect("initial health");
        let baseline_counts = runtime.issue_counts().await.expect("initial badge");
        let store = SqlHealthCheckStore::new(pool.clone());
        let persisted_baseline = store.load_results().await.expect("persisted baseline");
        let persisted_summary = store.load_summary().await.expect("persisted summary");
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
        assert_eq!(
            store.load_results().await.expect("persisted baseline"),
            persisted_baseline
        );
        assert_eq!(
            store.load_summary().await.expect("persisted summary"),
            persisted_summary
        );
        assert_eq!(
            runtime.current_results(stopped.results.clone()).await,
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

    #[tokio::test]
    async fn teletext_output_configuration_changes_are_fresh_over_cached_health_and_badges() {
        use super::super::teletext_runtime_policy::NATIVE_TELETEXT_RUNTIME_POLICY_SETTING as KEY;

        let runtime = health_fixture().await;
        let baseline = runtime.run_health_checks(false).await.unwrap();
        let baseline_counts = runtime.issue_counts().await.unwrap();
        assert_eq!(
            output_row(&baseline.results).status,
            HealthCheckStatus::Info
        );
        assert_eq!(
            output_row(&baseline.results).message,
            TELETEXT_OUTPUT_ABSENT_MESSAGE
        );
        let store = SqlHealthCheckStore::new(runtime.pool.clone());
        let persisted = store.load_results().await.unwrap();
        assert!(!persisted
            .iter()
            .any(|row| row.title == TELETEXT_OUTPUT_TITLE));
        let config = SqliteConfigRepository(runtime.pool.clone());
        config.set(KEY, "synthetic-private-policy").await.unwrap();
        let invalid = runtime.run_health_checks(false).await.unwrap();
        let row = output_row(&invalid.results);
        assert_eq!(row.status, HealthCheckStatus::Warning);
        assert_eq!(row.message, TELETEXT_OUTPUT_INVALID_MESSAGE);
        assert!(!format!("{row:?}").contains("synthetic-private"));
        assert_eq!(invalid.summary.warnings, baseline.summary.warnings + 1);
        assert_eq!(
            runtime.issue_counts().await.unwrap(),
            (baseline_counts.0, baseline_counts.1 + 1)
        );
        let fixture = include_str!("../../tests/fixtures/teletext-output-policy-v1.json");
        config.set(KEY, fixture).await.unwrap();
        let configured = runtime.run_health_checks(false).await.unwrap();
        let row = output_row(&configured.results);
        assert_eq!(row.status, HealthCheckStatus::Pass);
        assert_eq!(row.message, TELETEXT_OUTPUT_CONFIGURED_MESSAGE);
        assert_eq!(runtime.issue_counts().await.unwrap(), baseline_counts);
        assert_eq!(
            runtime.current_results(invalid.results).await,
            configured.results
        );
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(KEY)
            .execute(&runtime.pool)
            .await
            .unwrap();
        let absent = runtime.run_health_checks(false).await.unwrap();
        assert_eq!(absent.results, baseline.results);
        assert_eq!(absent.summary, baseline.summary);
        assert_eq!(store.load_results().await.unwrap(), persisted);
    }

    #[tokio::test]
    async fn teletext_output_health_uses_same_zero_credit_and_source_admission_rules() {
        use super::super::teletext_runtime_policy::NATIVE_TELETEXT_RUNTIME_POLICY_SETTING as KEY;

        let runtime = health_fixture().await;
        let config = SqliteConfigRepository(runtime.pool.clone());
        let original: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/teletext-output-policy-v1.json"
        ))
        .unwrap();
        let mut zero = original.clone();
        zero["encoding"]["max_attribute_cell_updates"] = serde_json::json!(0);
        zero["input"]["max_producers"] = serde_json::json!(0);
        config.set(KEY, &zero.to_string()).await.unwrap();
        let results = runtime.current_results(vec![]).await;
        let row = output_row(&results);
        assert_eq!(
            row.status,
            HealthCheckStatus::Pass,
            "zero means an explicit exhausted quota, not invalid schema or readiness"
        );
        assert_eq!(row.message, TELETEXT_OUTPUT_CONFIGURED_MESSAGE);
        let mut invalid = original;
        invalid["source"]["min_pcr_intervals"] = serde_json::json!(0);
        config.set(KEY, &invalid.to_string()).await.unwrap();
        let results = runtime.current_results(results).await;
        assert_eq!(output_row(&results).status, HealthCheckStatus::Warning);
        assert_eq!(
            output_row(&results).message,
            TELETEXT_OUTPUT_INVALID_MESSAGE
        );
        runtime.pool.close().await;
        let results = runtime.current_results(results).await;
        assert_eq!(output_row(&results).status, HealthCheckStatus::Warning);
        assert_eq!(
            output_row(&results).message,
            TELETEXT_OUTPUT_UNREADABLE_MESSAGE
        );
    }
}

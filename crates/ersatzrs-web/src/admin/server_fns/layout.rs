use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Counts playouts whose last build attempt failed (Success = 0).
#[server]
pub async fn count_playout_warnings() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutBuildStatus" WHERE "Success" = 0"#)
            .fetch_one(&state.pool)
            .await?;

    Ok(u32::try_from(count).unwrap_or(0))
}

/// Returns (error_count, warning_count) by combining the persisted parity
/// health summary with the startup/runtime counts wired into the web host.
#[server]
pub async fn count_health_issues() -> Result<(u32, u32), ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_infra::health::SqlHealthCheckStore;

    let state = expect_context::<AppState>();
    let (mut errors, mut warnings) = state.health_issue_counts().await;

    let store = SqlHealthCheckStore::new(state.pool.clone());
    if let Ok(summary) = store.load_summary().await {
        errors = errors.saturating_add(summary.errors);
        warnings = warnings.saturating_add(summary.warnings);
    }

    Ok((errors, warnings))
}

/// Persisted health row projected to a JSON-friendly shape for the admin page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminHealthRow {
    pub title: String,
    pub status: String,
    pub message: String,
    pub brief_message: String,
    pub link: Option<String>,
}

/// Snapshot rendered by the admin Health Checks page.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminHealthSnapshot {
    pub errors: u32,
    pub warnings: u32,
    pub ffmpeg_runtime: Option<AdminFfmpegRuntimeStatus>,
    pub rows: Vec<AdminHealthRow>,
}

/// Startup `FFmpeg` runtime status for localized health-page rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminFfmpegRuntimeStatus {
    pub source: String,
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
    pub installed_tag: Option<String>,
    pub latest_tag: Option<String>,
    pub previous_tag: Option<String>,
    pub downloaded_update: bool,
    pub update_available: bool,
    pub update_check_error: Option<String>,
}

/// One-shot startup `FFmpeg` runtime notification for hydrated admin sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminFfmpegRuntimeNotification {
    pub previous_tag: Option<String>,
    pub installed_tag: Option<String>,
    pub asset_name: Option<String>,
    pub update_failed: bool,
    pub error: Option<String>,
}

/// Loads the persisted parity health snapshot for the admin /system/health page.
#[server]
pub async fn load_health_snapshot() -> Result<AdminHealthSnapshot, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::health::HealthCheckStatus as S;
    use ersatzrs_infra::health::run_and_persist_health_checks;

    let state = expect_context::<AppState>();
    let runtime = state.ffmpeg_runtime.read().await.clone();
    let run = run_and_persist_health_checks(
        state.pool.clone(),
        runtime.clone(),
        state.system_startup.schema_status.clone(),
    )
    .await?;

    let rows = run
        .results
        .into_iter()
        .filter(|result| result.status != S::NotApplicable)
        .map(|result| {
            let status = match result.status {
                S::Pass => "Pass",
                S::Fail => "Fail",
                S::Warning => "Warning",
                S::Info => "Info",
                S::NotApplicable => "NotApplicable",
            };
            AdminHealthRow {
                title: result.title,
                status: status.to_owned(),
                message: result.message,
                brief_message: result.brief_message,
                link: result.link.map(|link| link.link),
            }
        })
        .collect();

    Ok(AdminHealthSnapshot {
        errors: run.summary.errors,
        warnings: run.summary.warnings,
        ffmpeg_runtime: Some(AdminFfmpegRuntimeStatus {
            source: runtime.acquisition.source.to_string(),
            ffmpeg_version: runtime.ffmpeg.version.to_string(),
            ffprobe_version: runtime.ffprobe.version.to_string(),
            installed_tag: runtime.acquisition.installed_tag,
            latest_tag: runtime.acquisition.latest_tag,
            previous_tag: runtime.acquisition.previous_tag,
            downloaded_update: runtime.acquisition.downloaded_update,
            update_available: runtime.acquisition.update_available,
            update_check_error: runtime.acquisition.update_check_error,
        }),
        rows,
    })
}

/// Returns a startup notification when this process downloaded a managed FFmpeg runtime.
#[server]
pub async fn load_ffmpeg_runtime_notification(
) -> Result<Option<AdminFfmpegRuntimeNotification>, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_ffmpeg::FfmpegRuntimeSource;

    if let Some(notification) = e2e_ffmpeg_runtime_notification_override() {
        return Ok(Some(notification));
    }

    let state = expect_context::<AppState>();
    let runtime = state.ffmpeg_runtime.read().await;
    if let Some(error) = runtime.acquisition.update_check_error.clone() {
        return Ok(Some(AdminFfmpegRuntimeNotification {
            previous_tag: runtime.acquisition.previous_tag.clone(),
            installed_tag: runtime.acquisition.installed_tag.clone(),
            asset_name: runtime.acquisition.asset_name.clone(),
            update_failed: true,
            error: Some(error),
        }));
    }

    if runtime.acquisition.source != FfmpegRuntimeSource::Downloaded {
        return Ok(None);
    }

    Ok(Some(AdminFfmpegRuntimeNotification {
        previous_tag: runtime.acquisition.previous_tag.clone(),
        installed_tag: runtime.acquisition.installed_tag.clone(),
        asset_name: runtime.acquisition.asset_name.clone(),
        update_failed: false,
        error: None,
    }))
}

#[cfg(debug_assertions)]
fn e2e_ffmpeg_runtime_notification_override() -> Option<AdminFfmpegRuntimeNotification> {
    #[allow(clippy::disallowed_methods)] // Debug-only Playwright hook, not runtime configuration.
    let installed_tag = std::env::var("ERSATZRS_E2E_FFMPEG_RUNTIME_NOTIFICATION").ok()?;
    if installed_tag.trim().is_empty() {
        return None;
    }

    Some(AdminFfmpegRuntimeNotification {
        previous_tag: None,
        asset_name: Some(format!("ersatzrs-ffmpeg-{installed_tag}-e2e.tar.gz")),
        installed_tag: Some(installed_tag),
        update_failed: false,
        error: None,
    })
}

#[cfg(not(debug_assertions))]
fn e2e_ffmpeg_runtime_notification_override() -> Option<AdminFfmpegRuntimeNotification> {
    None
}

/// Cheap revision token for admin media views that need to notice scanner
/// side effects. ErsatzTV's scanner calls back into the host with changed
/// media item ids so the Lucene index can reindex/remove them. ErsatzRS media
/// views query SQL directly, so the equivalent invalidation boundary is the
/// scan timestamp plus missing-state count.
#[server]
pub async fn media_state_revision() -> Result<String, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();

    let library_scan: Option<String> =
        sqlx::query_scalar(r#"SELECT MAX("LastScan") FROM "Library""#)
            .fetch_one(&state.pool)
            .await?;
    let path_scan: Option<String> =
        sqlx::query_scalar(r#"SELECT MAX("LastScan") FROM "LibraryPath""#)
            .fetch_one(&state.pool)
            .await?;
    let unavailable_count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaItem" WHERE "State" IN (1, 2, 3)"#)
            .fetch_one(&state.pool)
            .await?;

    Ok(format!(
        "{}|{}|{}",
        library_scan.unwrap_or_default(),
        path_scan.unwrap_or_default(),
        unavailable_count
    ))
}

#[cfg(test)]
mod tests {
    use super::{load_ffmpeg_runtime_notification, load_health_snapshot, media_state_revision};
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_ffmpeg::{FfmpegRuntimeReport, FfmpegRuntimeSource};
    use ersatzrs_infra::db::{self, DatabaseBackend, DatabaseSchemaStatus};
    use leptos::prelude::{provide_context, Owner};
    use sqlx::Executor;
    use tempfile::NamedTempFile;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    #[tokio::test(flavor = "current_thread")]
    async fn media_state_revision_changes_after_scan_marks_missing_item() {
        let (_db_file, pool) = test_pool().await;
        seed_library_fixture(&pool).await;
        let owner = provide_test_state(pool.clone());

        let before = media_state_revision().await.unwrap();

        sqlx::query(r#"UPDATE "Library" SET "LastScan" = '2026-06-18T16:56:46Z' WHERE "Id" = 1"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = 1 WHERE "Id" = 10"#)
            .execute(&pool)
            .await
            .unwrap();

        let after = media_state_revision().await.unwrap();

        assert_ne!(before, after);
        assert!(after.contains("2026-06-18T16:56:46Z"));
        assert!(after.ends_with("|1"));

        owner.cleanup();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ffmpeg_runtime_notification_reports_downloaded_update() {
        let (_db_file, pool) = test_pool().await;
        let mut runtime = FfmpegRuntimeReport::for_tests();
        runtime.acquisition.source = FfmpegRuntimeSource::Downloaded;
        runtime.acquisition.previous_tag = Some("v1.0.0".to_owned());
        runtime.acquisition.installed_tag = Some("v1.0.1".to_owned());
        runtime.acquisition.asset_name = Some("ersatzrs-ffmpeg-1.0.1-linux-x64.tar.gz".to_owned());
        let owner = provide_test_state_with_runtime(pool, runtime);

        let notification = load_ffmpeg_runtime_notification()
            .await
            .unwrap()
            .expect("notification");

        assert!(!notification.update_failed);
        assert_eq!(notification.previous_tag.as_deref(), Some("v1.0.0"));
        assert_eq!(notification.installed_tag.as_deref(), Some("v1.0.1"));

        owner.cleanup();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn ffmpeg_runtime_notification_reports_update_failure() {
        let (_db_file, pool) = test_pool().await;
        let mut runtime = FfmpegRuntimeReport::for_tests();
        runtime.acquisition.source = FfmpegRuntimeSource::Managed;
        runtime.acquisition.installed_tag = Some("v1.0.0".to_owned());
        runtime.acquisition.latest_tag = Some("v1.0.1".to_owned());
        runtime.acquisition.update_available = true;
        runtime.acquisition.update_check_error = Some("checksum mismatch".to_owned());
        let owner = provide_test_state_with_runtime(pool, runtime);

        let notification = load_ffmpeg_runtime_notification()
            .await
            .unwrap()
            .expect("notification");

        assert!(notification.update_failed);
        assert_eq!(notification.installed_tag.as_deref(), Some("v1.0.0"));
        assert_eq!(notification.error.as_deref(), Some("checksum mismatch"));

        owner.cleanup();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn health_snapshot_recomputes_file_not_found_from_current_media_state() {
        let (_db_file, pool) = test_pool().await;
        seed_library_fixture(&pool).await;
        let owner = provide_test_state(pool.clone());

        let before = load_health_snapshot().await.unwrap();
        let file_not_found_before = before
            .rows
            .iter()
            .find(|row| row.title == "File Not Found")
            .expect("file-not-found check should be present");
        assert_eq!(file_not_found_before.status, "Pass");

        sqlx::query(r#"UPDATE "MediaItem" SET "State" = 1 WHERE "Id" = 10"#)
            .execute(&pool)
            .await
            .unwrap();

        let after = load_health_snapshot().await.unwrap();
        let file_not_found_after = after
            .rows
            .iter()
            .find(|row| row.title == "File Not Found")
            .expect("file-not-found check should be present");

        assert_eq!(file_not_found_after.status, "Warning");
        assert!(file_not_found_after.message.contains("MediaItem 10"));
        assert!(after.warnings >= before.warnings.saturating_add(1));

        owner.cleanup();
    }

    async fn test_pool() -> (NamedTempFile, ersatzrs_infra::db::DbPool) {
        let file = NamedTempFile::new().unwrap();
        let database_url = format!("sqlite:{}", file.path().display());
        let pool = db::init(&database_url).await.unwrap();
        (file, pool)
    }

    async fn seed_library_fixture(pool: &ersatzrs_infra::db::DbPool) {
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'Movies', 1, 1)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryPath" ("Id", "Path", "LibraryId")
               VALUES (1, '/library', 1)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (10, 1, 0)"#,
        )
        .await
        .unwrap();
    }

    fn provide_test_state(pool: ersatzrs_infra::db::DbPool) -> Owner {
        provide_test_state_with_runtime(pool, FfmpegRuntimeReport::for_tests())
    }

    fn provide_test_state_with_runtime(
        pool: ersatzrs_infra::db::DbPool,
        runtime: FfmpegRuntimeReport,
    ) -> Owner {
        let (playout_build_tx, _playout_build_rx) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            runtime,
            Uuid::nil(),
            1,
            IptvAccessTokenVerifier::disabled(),
            playout_build_tx,
            ersatzrs_core::locking::EntityLocker::default(),
        );

        let owner = Owner::new();
        owner.with(|| provide_context(state));
        owner.set();
        owner
    }
}

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
    pub rows: Vec<AdminHealthRow>,
}

/// Loads the persisted parity health snapshot for the admin /system/health page.
#[server]
pub async fn load_health_snapshot() -> Result<AdminHealthSnapshot, ServerFnError> {
    use crate::host::state::AppState;
    use ersatzrs_core::health::HealthCheckStatus as S;
    use ersatzrs_infra::health::run_and_persist_health_checks;

    let state = expect_context::<AppState>();
    let run = run_and_persist_health_checks(
        state.pool.clone(),
        state.ffmpeg_runtime.read().await.clone(),
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
        rows,
    })
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
    use super::{load_health_snapshot, media_state_revision};
    use crate::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_ffmpeg::FfmpegRuntimeReport;
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
        let (playout_build_tx, _playout_build_rx) = mpsc::channel(4);
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
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

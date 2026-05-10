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
    let (mut errors, mut warnings) = state.health_issue_counts();

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
    use ersatzrs_core::health::{HealthCheckStatus as S, HealthCheckSummary};
    use ersatzrs_infra::health::{make_health_check_service, SqlHealthCheckStore};

    let state = expect_context::<AppState>();
    let store = SqlHealthCheckStore::new(state.pool.clone());

    let mut results = store.load_results().await.unwrap_or_default();
    let mut summary = store.load_summary().await.unwrap_or_default();
    if results.is_empty() {
        let service = make_health_check_service(
            state.pool.clone(),
            state.ffmpeg_runtime.as_ref().clone(),
            state.system_startup.schema_status.clone(),
        );
        results = service.perform_health_checks().await;
        summary = HealthCheckSummary::from_results(&results);
    }

    let rows = results
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
        errors: summary.errors,
        warnings: summary.warnings,
        rows,
    })
}

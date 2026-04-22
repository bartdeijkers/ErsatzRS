use leptos::prelude::*;

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

/// Returns (error_count, warning_count) from the current startup/runtime
/// health checks wired into the web host.
#[server]
#[allow(clippy::unused_async)]
pub async fn count_health_issues() -> Result<(u32, u32), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state.health_issue_counts())
}

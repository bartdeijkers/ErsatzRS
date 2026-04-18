use leptos::prelude::*;

/// Counts playouts whose last build attempt failed (Success = 0).
#[server]
pub async fn count_playout_warnings() -> Result<u32, ServerFnError> {
    use crate::state::AppState;

    let state = expect_context::<AppState>();
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutBuildStatus" WHERE "Success" = 0"#)
            .fetch_one(&state.pool)
            .await?;

    Ok(u32::try_from(count).unwrap_or(0))
}

/// Returns (error_count, warning_count) from system health checks.
/// Returns (0, 0) until health-check infrastructure is wired in Phase 6.x.
#[allow(clippy::unused_async)]
#[server]
pub async fn count_health_issues() -> Result<(u32, u32), ServerFnError> {
    Ok((0, 0))
}

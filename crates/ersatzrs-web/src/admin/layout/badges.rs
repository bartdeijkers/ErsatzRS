use leptos::prelude::*;

use super::style;

#[component]
pub(super) fn PlayoutWarningBadge() -> impl IntoView {
    let warnings = load_playout_warning_count_initial();
    let display = if warnings > 0 { "inline-flex" } else { "none" };
    let text = warnings.to_string();

    view! {
        <span
            class=style::nav_badge_warning
            data-playout-warning-badge
            style:display=display
        >
            {text}
        </span>
    }
}

#[component]
pub(super) fn HealthBadge() -> impl IntoView {
    let (errors, warnings) = load_health_counts_initial();
    let error_display = if errors > 0 { "inline-flex" } else { "none" };
    let warning_display = if errors == 0 && warnings > 0 {
        "inline-flex"
    } else {
        "none"
    };
    let error_text = errors.to_string();
    let warning_text = warnings.to_string();

    view! {
        <span style="display:contents" data-health-badges>
            <span
                class=style::nav_badge_error
                data-health-error-badge
                style:display=error_display
            >
                {error_text}
            </span>
            <span
                class=style::nav_badge_warning
                data-health-warning-badge
                style:display=warning_display
            >
                {warning_text}
            </span>
        </span>
    }
}

fn load_playout_warning_count_initial() -> u32 {
    #[cfg(feature = "ssr")]
    {
        use crate::host::state::AppState;

        let Some(state) = use_context::<AppState>() else {
            return 0;
        };
        let pool = state.pool.clone();
        let count = async move {
            sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "PlayoutBuildStatus" WHERE "Success" = 0"#,
            )
            .fetch_one(&pool)
            .await
            .ok()
            .and_then(|count| u32::try_from(count).ok())
            .unwrap_or(0)
        };

        return match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(count)),
            Err(_) => 0,
        };
    }

    #[cfg(not(feature = "ssr"))]
    {
        0
    }
}

fn load_health_counts_initial() -> (u32, u32) {
    #[cfg(feature = "ssr")]
    {
        use crate::host::state::AppState;
        use ersatzrs_infra::health::SqlHealthCheckStore;

        let Some(state) = use_context::<AppState>() else {
            return (0, 0);
        };
        let state_for_counts = state.clone();
        let state_for_store = state.clone();
        let counts = async move {
            let (mut errors, mut warnings) = state_for_counts.health_issue_counts().await;

            let store = SqlHealthCheckStore::new(state_for_store.pool.clone());
            if let Ok(summary) = store.load_summary().await {
                errors = errors.saturating_add(summary.errors);
                warnings = warnings.saturating_add(summary.warnings);
            }

            (errors, warnings)
        };

        return match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(counts)),
            Err(_) => (0, 0),
        };
    }

    #[cfg(not(feature = "ssr"))]
    {
        (0, 0)
    }
}

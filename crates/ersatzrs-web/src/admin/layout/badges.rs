use leptos::prelude::*;

use super::style;

#[component]
pub(super) fn PlayoutWarningBadge() -> impl IntoView {
    let (warnings, load_error) = match load_playout_warning_count_initial() {
        Ok(warnings) => (warnings, None),
        Err(error) => (0, Some(error)),
    };
    let no_warnings = warnings == 0;
    let text = warnings.to_string();
    let state = if load_error.is_some() {
        "error"
    } else {
        "ready"
    };

    view! {
        <span
            data-state=state title=load_error
            class=style::nav_badge_warning
            data-playout-warning-badge
            hidden=no_warnings
        >
            {text}
        </span>
    }
}

#[component]
pub(super) fn HealthBadge() -> impl IntoView {
    let (errors, warnings, load_error) = match load_health_counts_initial() {
        Ok((errors, warnings)) => (errors, warnings, None),
        Err(error) => (0, 0, Some(error)),
    };
    let hide_errors = errors == 0;
    let hide_warnings = errors > 0 || warnings == 0;
    let error_text = errors.to_string();
    let warning_text = warnings.to_string();
    let state = if load_error.is_some() {
        "error"
    } else {
        "ready"
    };

    view! {
        <span class=style::nav_badges data-health-badges data-state=state title=load_error>
            <span
                class=style::nav_badge_error
                data-health-error-badge
                hidden=hide_errors
            >
                {error_text}
            </span>
            <span
                class=style::nav_badge_warning
                data-health-warning-badge
                hidden=hide_warnings
            >
                {warning_text}
            </span>
        </span>
    }
}

fn load_playout_warning_count_initial() -> Result<u32, String> {
    #[cfg(feature = "ssr")]
    {
        use crate::host::state::AppState;

        let state = use_context::<AppState>()
            .ok_or_else(|| "admin application context is unavailable".to_owned())?;
        let application = state.application.clone();
        let count = async move { application.count_playout_warnings().await };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(count))
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        }
    }

    #[cfg(not(feature = "ssr"))]
    {
        Ok(0)
    }
}

fn load_health_counts_initial() -> Result<(u32, u32), String> {
    #[cfg(feature = "ssr")]
    {
        use crate::host::state::AppState;
        let state = use_context::<AppState>()
            .ok_or_else(|| "admin application context is unavailable".to_owned())?;
        let application = state.application.clone();
        let counts = async move { application.admin_health_issue_counts().await };

        match tokio::runtime::Handle::try_current() {
            Ok(handle) => tokio::task::block_in_place(|| handle.block_on(counts))
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        }
    }

    #[cfg(not(feature = "ssr"))]
    {
        Ok((0, 0))
    }
}

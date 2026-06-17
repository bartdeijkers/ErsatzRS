use leptos::prelude::*;

use crate::admin::server_fns::layout::{count_health_issues, count_playout_warnings};

use super::style;

#[island]
pub(super) fn PlayoutWarningBadge() -> impl IntoView {
    let count = LocalResource::new(|| async { count_playout_warnings().await.unwrap_or_default() });

    view! {
        <span
            class=style::nav_badge_warning
            data-playout-warning-badge
            style:display=move || {
                if count.get().unwrap_or_default() > 0 {
                    "inline-flex"
                } else {
                    "none"
                }
            }
        >
            {move || count.get().unwrap_or_default().to_string()}
        </span>
    }
}

#[island]
pub(super) fn HealthBadge() -> impl IntoView {
    let counts = LocalResource::new(|| async { count_health_issues().await.unwrap_or_default() });

    view! {
        <span
            class=style::nav_badge_error
            data-health-error-badge
            style:display=move || {
                let (errors, _) = counts.get().unwrap_or_default();
                if errors > 0 { "inline-flex" } else { "none" }
            }
        >
            {move || {
                let (errors, _) = counts.get().unwrap_or_default();
                errors.to_string()
            }}
        </span>
        <span
            class=style::nav_badge_warning
            data-health-warning-badge
            style:display=move || {
                let (errors, warnings) = counts.get().unwrap_or_default();
                if errors == 0 && warnings > 0 { "inline-flex" } else { "none" }
            }
        >
            {move || {
                let (_, warnings) = counts.get().unwrap_or_default();
                warnings.to_string()
            }}
        </span>
    }
}

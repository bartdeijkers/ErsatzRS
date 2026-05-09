use leptos::prelude::*;

use super::style;

#[component]
pub(super) fn PlayoutWarningBadge() -> impl IntoView {
    view! {
        <span class=style::nav_badge_warning style="display:none"></span>
    }
}

#[component]
pub(super) fn HealthBadge() -> impl IntoView {
    view! {
        <span class=style::nav_badge_error style="display:none"></span>
        <span class=style::nav_badge_warning style="display:none"></span>
    }
}

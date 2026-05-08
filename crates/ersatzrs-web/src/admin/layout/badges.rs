use leptos::prelude::*;

use super::style;

#[island]
pub(super) fn PlayoutWarningBadge() -> impl IntoView {
    use crate::admin::server_fns::layout::count_playout_warnings;

    let count = LocalResource::new(|| async move { count_playout_warnings().await.unwrap_or(0) });

    // Always render the span; hide it when count == 0 to keep a stable DOM shape.
    view! {
        <span
            class=style::nav_badge_warning
            style:display=move || {
                match count.get() {
                    Some(n) if n > 0 => "",
                    _ => "none",
                }
            }
        >
            {move || match count.get() {
                Some(n) if n > 99 => "99+".to_string(),
                Some(n) if n > 0 => n.to_string(),
                _ => String::new(),
            }}
        </span>
    }
}

#[island]
pub(super) fn HealthBadge() -> impl IntoView {
    use crate::admin::server_fns::layout::count_health_issues;

    let issues =
        LocalResource::new(|| async move { count_health_issues().await.unwrap_or((0, 0)) });

    // Always render both spans; show at most one at a time via style:display.
    // Error span takes priority over warnings span when both are non-zero.
    view! {
        <span
            class=style::nav_badge_error
            style:display=move || {
                match issues.get() {
                    Some((errors, _)) if errors > 0 => "",
                    _ => "none",
                }
            }
        >
            {move || match issues.get() {
                Some((errors, _)) if errors > 99 => "99+".to_string(),
                Some((errors, _)) if errors > 0 => errors.to_string(),
                _ => String::new(),
            }}
        </span>
        <span
            class=style::nav_badge_warning
            style:display=move || {
                match issues.get() {
                    Some((errors, warnings)) if errors == 0 && warnings > 0 => "",
                    _ => "none",
                }
            }
        >
            {move || match issues.get() {
                Some((errors, warnings)) if errors == 0 && warnings > 99 => "99+".to_string(),
                Some((errors, warnings)) if errors == 0 && warnings > 0 => warnings.to_string(),
                _ => String::new(),
            }}
        </span>
    }
}

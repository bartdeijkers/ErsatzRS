use leptos::prelude::*;

use super::style;

#[island]
pub(super) fn PlayoutWarningBadge() -> impl IntoView {
    use crate::admin::server_fns::layout::count_playout_warnings;

    let count = Resource::new(
        || (),
        |()| async move { count_playout_warnings().await.unwrap_or(0) },
    );

    view! {
        <Suspense fallback=|| ()>
            {move || {
                count.get().map(|value| {
                    if value == 0 {
                        ().into_any()
                    } else {
                        let label = if value > 99 {
                            "99+".to_string()
                        } else {
                            value.to_string()
                        };

                        view! {
                            <span class=style::nav_badge_warning>{label}</span>
                        }
                            .into_any()
                    }
                })
            }}
        </Suspense>
    }
}

#[island]
pub(super) fn HealthBadge() -> impl IntoView {
    use crate::admin::server_fns::layout::count_health_issues;

    let issues = Resource::new(
        || (),
        |()| async move { count_health_issues().await.unwrap_or((0, 0)) },
    );

    view! {
        <Suspense fallback=|| ()>
            {move || {
                issues.get().map(|(errors, warnings)| {
                    if errors != 0 {
                        let label = if errors > 99 {
                            "99+".to_string()
                        } else {
                            errors.to_string()
                        };

                        view! {
                            <span class=style::nav_badge_error>{label}</span>
                        }
                            .into_any()
                    } else if warnings != 0 {
                        let label = if warnings > 99 {
                            "99+".to_string()
                        } else {
                            warnings.to_string()
                        };

                        view! {
                            <span class=style::nav_badge_warning>{label}</span>
                        }
                            .into_any()
                    } else {
                        ().into_any()
                    }
                })
            }}
        </Suspense>
    }
}

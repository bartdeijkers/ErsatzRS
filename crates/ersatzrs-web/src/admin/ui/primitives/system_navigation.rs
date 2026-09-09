use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::admin::i18n::{tr, TextKey};

stylance::import_crate_style!(
    style,
    "src/admin/ui/primitives/system_navigation.module.css"
);

const SYSTEM_ROUTES: [(&str, TextKey); 3] = [
    ("/system/health", TextKey::HealthChecks),
    ("/system/logs", TextKey::Logs),
    ("/system/troubleshooting", TextKey::Troubleshooting),
];

fn is_system_route_active(pathname: &str, href: &str) -> bool {
    pathname == href
        || (href == "/system/troubleshooting" && pathname.starts_with("/system/troubleshooting/"))
}

/// Support route navigation that keeps individual pages out of the global drawer.
#[component]
pub fn SystemRouteTabs() -> impl IntoView {
    let pathname = use_location().pathname;

    view! {
        <nav class=style::tabs aria-label=tr(TextKey::Support) data-system-route-tabs>
            {SYSTEM_ROUTES.into_iter().map(|(href, label)| {
                let is_active = move || is_system_route_active(&pathname.get(), href);
                view! {
                    <a
                        href=href
                        class=move || if is_active() { style::tab_active } else { style::tab }
                        aria-current=move || is_active().then_some("page")
                    >
                        {tr(label)}
                    </a>
                }
            }).collect_view()}
        </nav>
    }
}

/// Stable support frame shared by every top-level system page.
#[component]
pub fn SystemPageLayout(children: Children) -> impl IntoView {
    view! {
        <div class=style::layout data-system-page-layout>
            <SystemRouteTabs/>
            <div class=style::content>{children()}</div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{is_system_route_active, SYSTEM_ROUTES};

    #[test]
    fn system_navigation_lists_every_support_page_the_drawer_dropped() {
        let paths = SYSTEM_ROUTES.map(|(path, _)| path);
        assert_eq!(
            paths,
            ["/system/health", "/system/logs", "/system/troubleshooting"]
        );
    }

    #[test]
    fn troubleshooting_detail_keeps_the_troubleshooting_tab_selected() {
        assert!(is_system_route_active(
            "/system/troubleshooting/playback",
            "/system/troubleshooting"
        ));
        assert!(!is_system_route_active(
            "/system/troubleshooting/playback",
            "/system/logs"
        ));
    }

    #[test]
    fn health_does_not_match_sibling_system_routes() {
        assert!(is_system_route_active("/system/health", "/system/health"));
        assert!(!is_system_route_active("/system/logs", "/system/health"));
    }
}

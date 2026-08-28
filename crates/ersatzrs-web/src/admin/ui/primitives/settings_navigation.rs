use leptos::prelude::*;
use leptos_router::hooks::use_location;

use crate::admin::i18n::{tr, TextKey};

stylance::import_crate_style!(
    style,
    "src/admin/ui/primitives/settings_navigation.module.css"
);

const SETTINGS_ROUTES: [(&str, TextKey); 8] = [
    ("/settings/addons", TextKey::Addons),
    ("/settings/ffmpeg", TextKey::Ffmpeg),
    ("/settings/logging", TextKey::Logging),
    ("/settings/hdhr", TextKey::HdHomeRun),
    ("/settings/scanner", TextKey::Scanner),
    ("/settings/playout", TextKey::Playout),
    ("/settings/ui", TextKey::Ui),
    ("/settings/xmltv", TextKey::Xmltv),
];

fn is_settings_route_active(pathname: &str, href: &str) -> bool {
    pathname == href || (href == "/settings/addons" && pathname.starts_with("/settings/addons/"))
}

/// Settings route navigation that keeps individual pages out of the global drawer.
#[component]
pub fn SettingsRouteTabs() -> impl IntoView {
    let pathname = use_location().pathname;

    view! {
        <nav class=style::tabs aria-label=tr(TextKey::Settings) data-settings-route-tabs>
            {SETTINGS_ROUTES.into_iter().map(|(href, label)| {
                let is_active = move || is_settings_route_active(&pathname.get(), href);
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

/// Stable settings frame shared by regular forms and add-on manager pages.
#[component]
pub fn SettingsPageLayout(children: Children) -> impl IntoView {
    view! {
        <div class=style::layout data-settings-page-layout>
            <SettingsRouteTabs/>
            <div class=style::content>{children()}</div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{is_settings_route_active, SETTINGS_ROUTES};

    #[test]
    fn settings_navigation_lists_every_top_level_settings_page() {
        let paths = SETTINGS_ROUTES.map(|(path, _)| path);
        assert_eq!(
            paths,
            [
                "/settings/addons",
                "/settings/ffmpeg",
                "/settings/logging",
                "/settings/hdhr",
                "/settings/scanner",
                "/settings/playout",
                "/settings/ui",
                "/settings/xmltv",
            ]
        );
    }

    #[test]
    fn addon_detail_keeps_the_addons_tab_selected() {
        assert!(is_settings_route_active(
            "/settings/addons/example.addon",
            "/settings/addons"
        ));
        assert!(!is_settings_route_active(
            "/settings/addons/example.addon",
            "/settings/ffmpeg"
        ));
    }
}

#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::components::Router;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

use crate::admin::i18n::{provide_i18n_context, UiLanguage};
use crate::admin::layout::AppShell;
#[cfg(not(target_arch = "wasm32"))]
use crate::admin::layout::NavStateHeadScript;
use crate::admin::routes::AdminRoutes;
use crate::admin::ui::primitives::toast::provide_toast_store;
#[cfg(not(target_arch = "wasm32"))]
use crate::host::state::AppState;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_toast_store();
    provide_i18n_context();

    view! {
        <Router>
            <AppShell>
                <AdminRoutes/>
            </AppShell>
        </Router>
    }
}

#[allow(clippy::needless_pass_by_value)] // signature dictated by leptos_axum
#[cfg(not(target_arch = "wasm32"))]
pub fn shell(options: LeptosOptions, app_state: AppState) -> impl IntoView {
    let hydration_assets_available = resolve_hydration_assets(&options).is_some();
    let language = crate::admin::i18n::load_language_initial_for_state(&app_state);

    view! {
        <!DOCTYPE html>
        <html lang=language.code()>
            <head>
                <meta charset="UTF-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
                <meta name="color-scheme" content="dark light"/>
                <title>"ErsatzRS"</title>
                <NavStateHeadScript/>
                <link rel="stylesheet" href="/public/bundle.css"/>
                {hydration_assets_available.then(|| {
                    view! { <HydrationScripts options=options.clone() islands=true/> }
                })}
            </head>
            <body>
                <AppWithState app_state=app_state/>
            </body>
        </html>
    }
}

#[component]
#[cfg(not(target_arch = "wasm32"))]
fn AppWithState(app_state: AppState) -> impl IntoView {
    let language = crate::admin::i18n::load_language_initial_for_state(&app_state);
    provide_context(app_state);
    provide_context::<UiLanguage>(language);
    view! { <App/> }
}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_hydration_assets(options: &LeptosOptions) -> Option<PathBuf> {
    let js_name = format!("{}.js", options.output_name);
    let wasm_names = [
        format!("{}_bg.wasm", options.output_name),
        format!("{}.wasm", options.output_name),
    ];
    let site_root = options.site_root.to_string();
    let site_pkg_dir = options.site_pkg_dir.to_string();

    candidate_site_roots(site_root)
        .into_iter()
        .map(|root| root.join(&site_pkg_dir))
        .find(|pkg_dir| {
            pkg_dir.join(&js_name).is_file()
                && wasm_names.iter().any(|name| pkg_dir.join(name).is_file())
        })
}

#[cfg(not(target_arch = "wasm32"))]
fn candidate_site_roots(site_root: impl AsRef<Path>) -> Vec<PathBuf> {
    let site_root = site_root.as_ref();
    let configured = PathBuf::from(site_root);
    if configured.is_absolute() {
        return vec![configured];
    }

    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    vec![
        crate_dir.join(site_root),
        crate_dir.join("..").join("..").join(site_root),
        configured,
    ]
}

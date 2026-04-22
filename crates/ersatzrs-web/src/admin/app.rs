#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::components::Router;
use std::path::{Path, PathBuf};

use crate::admin::layout::AppShell;
use crate::admin::routes::AdminRoutes;
use crate::admin::ui::primitives::toast::provide_toast_store;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_toast_store();

    view! {
        <Router>
            <AppShell>
                <AdminRoutes/>
            </AppShell>
        </Router>
    }
}

#[allow(clippy::needless_pass_by_value)] // signature dictated by leptos_axum
pub fn shell(options: LeptosOptions) -> impl IntoView {
    let hydration_assets_available = resolve_hydration_assets(&options).is_some();

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="UTF-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1.0"/>
                <meta name="color-scheme" content="dark"/>
                <title>"ErsatzRS"</title>
                <link rel="stylesheet" href="/public/bundle.css"/>
                {hydration_assets_available.then(|| {
                    view! { <HydrationScripts options=options.clone() islands=true/> }
                })}
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

fn resolve_hydration_assets(options: &LeptosOptions) -> Option<PathBuf> {
    let js_name = format!("{}.js", options.output_name);
    let wasm_name = format!("{}_bg.wasm", options.output_name);
    let site_root = options.site_root.to_string();
    let site_pkg_dir = options.site_pkg_dir.to_string();

    candidate_site_roots(site_root)
        .into_iter()
        .map(|root| root.join(&site_pkg_dir))
        .find(|pkg_dir| pkg_dir.join(&js_name).is_file() && pkg_dir.join(&wasm_name).is_file())
}

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

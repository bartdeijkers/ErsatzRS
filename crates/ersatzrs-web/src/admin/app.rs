#![allow(clippy::too_many_lines)]
use leptos::prelude::*;
use leptos_meta::provide_meta_context;
use leptos_router::components::Router;

use crate::admin::i18n::provide_i18n_context;
#[cfg(not(target_arch = "wasm32"))]
use crate::admin::i18n::UiLanguage;
use crate::admin::layout::AppShell;
#[cfg(not(target_arch = "wasm32"))]
use crate::admin::layout::NavStateHeadScript;
use crate::admin::routes::AdminRoutes;
use crate::admin::ui::primitives::toast::provide_toast_store;
#[cfg(not(target_arch = "wasm32"))]
use crate::host::assets::resolve_hydration_pkg_dir;
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
    let hydration_assets_available = resolve_hydration_pkg_dir(&options).is_some();
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

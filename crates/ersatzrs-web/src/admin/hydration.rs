//! Admin controls are required work: starting them must not depend on browser idle time.
//!
//! Leptos 0.8.20's `HydrationScripts` has no startup-scheduling hook. Keep this
//! adapter aligned with its islands bootstrap when upgrading Leptos. ErsatzRS
//! pins file hashing off and does not use WASM splitting or the islands router.

use leptos::prelude::*;

#[component]
pub fn AdminHydrationScripts(options: LeptosOptions) -> impl IntoView {
    // Islands establish their own hydration boundaries. This is the same SSR
    // context switch performed by Leptos's HydrationScripts(islands=true).
    if let Some(context) = Owner::current_shared_context() {
        context.set_is_hydrating(false);
    }

    let nonce = leptos::nonce::use_nonce();
    let name = options.output_name;
    let pkg = options.site_pkg_dir;
    let wasm_suffix = if option_env!("LEPTOS_OUTPUT_NAME").is_some() {
        ""
    } else {
        "_bg"
    };
    let js = format!("/{pkg}/{name}.js");
    let wasm = format!("/{pkg}/{name}{wasm_suffix}.wasm");

    view! {
        <link rel="modulepreload" href=js.clone() crossorigin="anonymous"/>
        <link rel="preload" href=wasm.clone() r#as="fetch" r#type="application/wasm" crossorigin="anonymous"/>
        <script
            id="ersatzrs-hydration"
            type="module"
            nonce=nonce
            data-js=js
            data-wasm=wasm
        >
            {include_str!("hydration.js")}
        </script>
    }
}

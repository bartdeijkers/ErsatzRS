#![recursion_limit = "512"]
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]
#![allow(
    clippy::implicit_hasher,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default
)]

pub mod admin;

#[cfg(not(target_arch = "wasm32"))]
pub mod api;
#[cfg(not(target_arch = "wasm32"))]
pub mod host;
#[cfg(not(target_arch = "wasm32"))]
pub mod transport;

#[cfg(not(target_arch = "wasm32"))]
pub use host::bootstrap::run;

#[cfg(all(target_arch = "wasm32", feature = "hydrate"))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    leptos::mount::hydrate_islands();

    // Positive, machine-checkable signal that island hydration ran to
    // completion. A marker-mismatch panic in any island (e.g. a shell-level
    // island streaming a resource that desyncs sibling islands) aborts
    // `hydrate_islands()` before this line, so the flag stays unset. The
    // release hydration smoke (`tests/e2e/island-hydration.spec.ts`) waits for
    // `window.__ersatzrs_hydrated === true` on every island route, turning the
    // whole-page hydration aborts we keep regressing into a hard test failure.
    if let Some(window) = web_sys::window() {
        let _ = js_sys::Reflect::set(
            &window,
            &wasm_bindgen::JsValue::from_str("__ersatzrs_hydrated"),
            &wasm_bindgen::JsValue::TRUE,
        );
    }
}

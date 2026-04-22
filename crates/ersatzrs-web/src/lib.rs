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
}

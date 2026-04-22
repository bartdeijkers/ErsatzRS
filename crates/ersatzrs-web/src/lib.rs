#![recursion_limit = "512"]
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]
#![allow(
    clippy::implicit_hasher,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    clippy::new_without_default
)]

pub mod app;
pub mod pages;
pub mod server;
pub mod ui;

#[cfg(not(target_arch = "wasm32"))]
pub mod hdhr;
#[cfg(not(target_arch = "wasm32"))]
pub mod iptv;
#[cfg(not(target_arch = "wasm32"))]
pub mod state;
#[cfg(not(target_arch = "wasm32"))]
pub mod streaming;
#[cfg(not(target_arch = "wasm32"))]
pub mod watermark_upload;

#[cfg(all(target_arch = "wasm32", feature = "hydrate"))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    leptos::mount::hydrate_islands();
}

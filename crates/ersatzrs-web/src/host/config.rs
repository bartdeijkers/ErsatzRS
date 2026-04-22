use std::path::Path;

use leptos::config::{get_configuration, ConfFile};

const WEB_CRATE_CARGO_TOML: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

pub fn leptos_configuration() -> ConfFile {
    let manifest = Path::new(WEB_CRATE_CARGO_TOML);
    let config = if manifest.is_file() {
        get_configuration(Some(WEB_CRATE_CARGO_TOML))
    } else {
        get_configuration(None)
    };

    config.expect("valid leptos config")
}

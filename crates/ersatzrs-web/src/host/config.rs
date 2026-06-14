use std::path::Path;
use std::sync::Arc;

use leptos::config::{get_configuration, ConfFile, LeptosOptions};

use crate::host::assets::{site_pkg_dir, DEFAULT_OUTPUT_NAME};

const WEB_CRATE_CARGO_TOML: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");

pub fn leptos_configuration() -> ConfFile {
    let manifest = Path::new(WEB_CRATE_CARGO_TOML);
    let mut config = if manifest.is_file() {
        get_configuration(Some(WEB_CRATE_CARGO_TOML))
    } else {
        get_configuration(None)
    };

    let config = config.as_mut().expect("valid leptos config");
    normalize_release_options(&mut config.leptos_options);
    config.clone()
}

fn normalize_release_options(options: &mut LeptosOptions) {
    if options.output_name.is_empty() {
        tracing::warn!(
            output_name = DEFAULT_OUTPUT_NAME,
            "Leptos output name was empty; using release asset basename"
        );
        options.output_name = Arc::from(DEFAULT_OUTPUT_NAME);
    }

    if options.site_pkg_dir.is_empty() {
        options.site_pkg_dir = Arc::from(site_pkg_dir(options));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_release_options_fills_empty_output_name() {
        let mut options = LeptosOptions::builder().output_name("").build();

        normalize_release_options(&mut options);

        assert_eq!(options.output_name.as_ref(), DEFAULT_OUTPUT_NAME);
    }
}

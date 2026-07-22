use std::path::Path;
use std::sync::Arc;

use leptos::config::{get_configuration, ConfFile, LeptosOptions};

use crate::host::assets::{site_pkg_dir, DEFAULT_OUTPUT_NAME};

const WEB_CRATE_CARGO_TOML: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
const DEVELOPMENT_ROUTES_ENV: &str = "ERSATZRS_ENABLE_DEVELOPMENT_ROUTES";

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

/// Whether development-only HTTP and admin routes may be registered.
///
/// The compile-time feature keeps these handlers out of normal release builds;
/// the environment variable is a second, explicit runtime opt-in for local
/// development builds that include the feature.
#[must_use]
#[allow(clippy::disallowed_methods)] // This second gate is intentionally environment-only and must be explicit per process.
pub fn development_routes_enabled() -> bool {
    cfg!(feature = "development-routes")
        && development_routes_opted_in(std::env::var(DEVELOPMENT_ROUTES_ENV).ok().as_deref())
}

fn development_routes_opted_in(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
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

    #[test]
    fn development_route_opt_in_accepts_only_explicit_truthy_values() {
        for value in ["1", "true", "TRUE", " yes ", "On"] {
            assert!(development_routes_opted_in(Some(value)), "{value}");
        }
        for value in ["", "0", "false", "no", "off", "development"] {
            assert!(!development_routes_opted_in(Some(value)), "{value}");
        }
        assert!(!development_routes_opted_in(None));
    }
}

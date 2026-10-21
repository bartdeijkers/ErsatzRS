use std::path::PathBuf;
use std::sync::Arc;

use leptos::config::{get_configuration, ConfFile, LeptosOptions};

use crate::host::assets::{site_pkg_dir, DEFAULT_OUTPUT_NAME};

const WEB_CRATE_CARGO_TOML: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
const DEVELOPMENT_ROUTES_ENV: &str = "ERSATZRS_ENABLE_DEVELOPMENT_ROUTES";
const STREAMING_ADDR_ENV: &str = "ERSATZRS_STREAMING_ADDR";

/// Optional public streaming-only listener; absence retains shared service.
#[allow(clippy::disallowed_methods)] // Listener binding is intentionally startup environment-only.
pub(crate) fn streaming_listen_addr() -> Result<Option<std::net::SocketAddr>, String> {
    match std::env::var(STREAMING_ADDR_ENV) {
        Ok(value) => parse_streaming_listen_addr(Some(&value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("{STREAMING_ADDR_ENV} must be a socket address"))
        }
    }
}

fn parse_streaming_listen_addr(
    value: Option<&str>,
) -> Result<Option<std::net::SocketAddr>, String> {
    value
        .map(|value| {
            value
                .trim()
                .parse()
                .map_err(|_| format!("{STREAMING_ADDR_ENV} must be an IP address and port"))
        })
        .transpose()
}

#[allow(
    clippy::disallowed_methods,
    reason = "Startup resolves Cargo/nextest's runtime manifest before the compiled development fallback"
)]
pub fn leptos_configuration() -> ConfFile {
    let manifest = runtime_manifest(std::env::var_os("CARGO_MANIFEST_DIR"));
    let mut config = if manifest.is_file() {
        get_configuration(manifest.to_str())
    } else {
        get_configuration(None)
    };

    let config = config.as_mut().expect("valid leptos config");
    normalize_release_options(&mut config.leptos_options);
    config.clone()
}

fn runtime_manifest(crate_dir: Option<std::ffi::OsString>) -> PathBuf {
    crate_dir.map_or_else(
        || PathBuf::from(WEB_CRATE_CARGO_TOML),
        |root| PathBuf::from(root).join("Cargo.toml"),
    )
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
    fn runtime_manifest_uses_the_execution_checkout_and_retains_packaged_fallback() {
        let runtime = PathBuf::from("relocated-checkout").join("crates/ersatzrs-web");
        assert_eq!(
            runtime_manifest(Some(runtime.clone().into_os_string())),
            runtime.join("Cargo.toml")
        );
        assert_eq!(runtime_manifest(None), PathBuf::from(WEB_CRATE_CARGO_TOML));
    }

    #[test]
    fn streaming_address_requires_an_explicit_socket_address() {
        assert_eq!(parse_streaming_listen_addr(None).unwrap(), None);
        for value in ["127.0.0.1:8410", "[::1]:8410", " 0.0.0.0:0 "] {
            assert_eq!(
                parse_streaming_listen_addr(Some(value)).unwrap(),
                Some(value.trim().parse().unwrap())
            );
        }
        for value in [
            "",
            " ",
            "localhost:8410",
            "127.0.0.1",
            "127.0.0.1:65536",
            "https://127.0.0.1:8410",
        ] {
            assert!(parse_streaming_listen_addr(Some(value)).is_err(), "{value}");
        }
    }

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

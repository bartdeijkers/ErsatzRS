use leptos::config::LeptosOptions;
use std::path::{Path, PathBuf};

const WEB_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub fn resolve_pkg_dir(options: &LeptosOptions) -> Option<PathBuf> {
    let site_root = options.site_root.to_string();
    let pkg_dir = options.site_pkg_dir.to_string();

    candidate_site_roots(site_root)
        .into_iter()
        .map(|root| root.join(&pkg_dir))
        .find(|candidate| candidate.is_dir())
}

fn candidate_site_roots(site_root: impl AsRef<Path>) -> Vec<PathBuf> {
    let site_root = site_root.as_ref();
    let configured = PathBuf::from(site_root);
    if configured.is_absolute() {
        return vec![configured];
    }

    vec![
        Path::new(WEB_CRATE_DIR).join(site_root),
        Path::new(WEB_CRATE_DIR)
            .join("..")
            .join("..")
            .join(site_root),
        configured,
    ]
}

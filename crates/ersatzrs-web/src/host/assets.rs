use leptos::config::LeptosOptions;
use std::path::{Path, PathBuf};

const WEB_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

pub fn resolve_public_dir() -> Option<PathBuf> {
    candidate_runtime_dirs()
        .into_iter()
        .map(|root| root.join("public"))
        .chain(std::iter::once(Path::new(WEB_CRATE_DIR).join("public")))
        .find(|candidate| candidate.join("bundle.css").is_file())
}

pub fn resolve_pkg_dir(options: &LeptosOptions) -> Option<PathBuf> {
    let site_root = options.site_root.to_string();
    let pkg_dir = options.site_pkg_dir.to_string();

    candidate_site_roots(site_root)
        .into_iter()
        .map(|root| root.join(&pkg_dir))
        .find(|candidate| candidate.is_dir())
}

fn candidate_runtime_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
        }
    }

    if let Ok(current_dir) = std::env::current_dir() {
        dirs.push(current_dir);
    }

    dirs
}

fn candidate_site_roots(site_root: impl AsRef<Path>) -> Vec<PathBuf> {
    let site_root = site_root.as_ref();
    let configured = PathBuf::from(site_root);
    if configured.is_absolute() {
        return vec![configured];
    }

    let mut candidates: Vec<PathBuf> = candidate_runtime_dirs()
        .into_iter()
        .map(|root| root.join(site_root))
        .collect();

    candidates.extend([
        Path::new(WEB_CRATE_DIR).join(site_root),
        Path::new(WEB_CRATE_DIR)
            .join("..")
            .join("..")
            .join(site_root),
        configured,
    ]);

    candidates
}

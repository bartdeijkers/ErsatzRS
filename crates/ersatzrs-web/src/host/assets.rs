use leptos::config::LeptosOptions;
use std::path::{Path, PathBuf};

const WEB_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");
pub(crate) const DEFAULT_OUTPUT_NAME: &str = "ersatzrs-web";
const DEFAULT_SITE_ROOT: &str = "target/site";
const BUNDLED_SITE_ROOT: &str = "site";
const DEFAULT_SITE_PKG_DIR: &str = "pkg";
const PUBLIC_MARKER_FILE: &str = "bundle.css";

/// Resolve the directory serving static admin-UI assets (CSS, fonts, images,
/// hand-written JS) under `/public`.
///
/// `cargo-leptos` copies the `assets-dir` (`public/`) contents into the top
/// level of the site root alongside `pkg/` at build time, so a built site
/// root is already the merged directory releases and the Docker image ship
/// as the single `site/` folder — check there first. Fall back to the crate's
/// own `public/` source directory (and a legacy standalone `public/` sibling)
/// for `cargo run` without a prior `cargo leptos build`.
pub fn resolve_public_dir(options: &LeptosOptions) -> Option<PathBuf> {
    let site_root = options.site_root.to_string();
    resolve_public_dir_from_dirs(
        candidate_site_roots(site_root),
        &candidate_runtime_dirs(),
        Path::new(WEB_CRATE_DIR),
    )
}

fn resolve_public_dir_from_dirs(
    site_roots: Vec<PathBuf>,
    runtime_dirs: &[PathBuf],
    crate_dir: &Path,
) -> Option<PathBuf> {
    site_roots
        .into_iter()
        .find(|candidate| candidate.join(PUBLIC_MARKER_FILE).is_file())
        .or_else(|| {
            runtime_dirs
                .iter()
                .map(|root| root.join("public"))
                .chain(std::iter::once(crate_dir.join("public")))
                .find(|candidate| candidate.join(PUBLIC_MARKER_FILE).is_file())
        })
}

pub fn resolve_pkg_dir(options: &LeptosOptions) -> Option<PathBuf> {
    resolve_hydration_pkg_dir(options)
}

pub fn resolve_hydration_pkg_dir(options: &LeptosOptions) -> Option<PathBuf> {
    let site_root = options.site_root.to_string();
    resolve_hydration_pkg_dir_from_site_roots(options, candidate_site_roots(site_root))
}

fn resolve_hydration_pkg_dir_from_site_roots(
    options: &LeptosOptions,
    site_roots: Vec<PathBuf>,
) -> Option<PathBuf> {
    let output_name = output_name(options);
    let js_name = format!("{output_name}.js");
    let wasm_names = [
        format!("{output_name}_bg.wasm"),
        format!("{output_name}.wasm"),
    ];
    let pkg_dir = site_pkg_dir(options).to_string();

    site_roots
        .into_iter()
        .map(|root| root.join(&pkg_dir))
        .find(|candidate| {
            candidate.join(&js_name).is_file()
                && wasm_names.iter().any(|name| candidate.join(name).is_file())
        })
}

pub(crate) fn output_name(options: &LeptosOptions) -> &str {
    if options.output_name.is_empty() {
        DEFAULT_OUTPUT_NAME
    } else {
        &options.output_name
    }
}

pub(crate) fn site_pkg_dir(options: &LeptosOptions) -> &str {
    if options.site_pkg_dir.is_empty() {
        DEFAULT_SITE_PKG_DIR
    } else {
        &options.site_pkg_dir
    }
}

pub(crate) fn candidate_runtime_dirs() -> Vec<PathBuf> {
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
    candidate_site_roots_from_dirs(
        site_root,
        &candidate_runtime_dirs(),
        Path::new(WEB_CRATE_DIR),
    )
}

fn candidate_site_roots_from_dirs(
    site_root: impl AsRef<Path>,
    runtime_dirs: &[PathBuf],
    crate_dir: &Path,
) -> Vec<PathBuf> {
    let site_root = site_root.as_ref();
    let configured = PathBuf::from(site_root);
    if configured.is_absolute() {
        return vec![configured];
    }

    let mut candidates: Vec<PathBuf> = runtime_dirs
        .iter()
        .map(|root| root.join(site_root))
        .collect();

    if site_root.as_os_str().is_empty() || site_root == Path::new(DEFAULT_SITE_ROOT) {
        candidates.extend(runtime_dirs.iter().map(|root| root.join(BUNDLED_SITE_ROOT)));
    }

    candidates.extend([
        crate_dir.join(site_root),
        crate_dir.join("..").join("..").join(site_root),
        configured,
    ]);

    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_target_site_also_checks_release_site_dir() {
        let runtime = PathBuf::from("/opt/ersatzrs");
        let candidates = candidate_site_roots_from_dirs(
            DEFAULT_SITE_ROOT,
            std::slice::from_ref(&runtime),
            Path::new("/src/crate"),
        );

        assert!(candidates.contains(&runtime.join(DEFAULT_SITE_ROOT)));
        assert!(candidates.contains(&runtime.join(BUNDLED_SITE_ROOT)));
    }

    #[test]
    fn empty_output_name_uses_release_basename() {
        let options = LeptosOptions::builder().output_name("").build();
        assert_eq!(output_name(&options), DEFAULT_OUTPUT_NAME);
    }

    #[test]
    fn release_site_pkg_is_valid_hydration_dir_for_default_site_root() {
        let temp = tempfile::tempdir().expect("temp dir");
        let pkg = temp
            .path()
            .join(BUNDLED_SITE_ROOT)
            .join(DEFAULT_SITE_PKG_DIR);
        std::fs::create_dir_all(&pkg).expect("create pkg dir");
        std::fs::write(pkg.join("ersatzrs-web.js"), "").expect("write js");
        std::fs::write(pkg.join("ersatzrs-web.wasm"), "").expect("write wasm");

        let options = LeptosOptions::builder()
            .output_name(DEFAULT_OUTPUT_NAME)
            .site_root(DEFAULT_SITE_ROOT)
            .build();
        let site_roots = candidate_site_roots_from_dirs(
            DEFAULT_SITE_ROOT,
            &[temp.path().into()],
            Path::new("/src/crate"),
        );

        assert_eq!(
            resolve_hydration_pkg_dir_from_site_roots(&options, site_roots).as_deref(),
            Some(pkg.as_path())
        );
    }

    #[test]
    fn public_dir_resolves_to_merged_site_root_when_present() {
        let temp = tempfile::tempdir().expect("temp dir");
        let site = temp.path().join(BUNDLED_SITE_ROOT);
        std::fs::create_dir_all(&site).expect("create site dir");
        std::fs::write(site.join(PUBLIC_MARKER_FILE), "").expect("write bundle.css");

        let site_roots = candidate_site_roots_from_dirs(
            DEFAULT_SITE_ROOT,
            &[temp.path().into()],
            Path::new("/src/crate"),
        );

        assert_eq!(
            resolve_public_dir_from_dirs(
                site_roots,
                &[temp.path().into()],
                Path::new("/src/crate")
            )
            .as_deref(),
            Some(site.as_path())
        );
    }

    #[test]
    fn public_dir_falls_back_to_legacy_sibling_when_site_root_has_no_assets() {
        let temp = tempfile::tempdir().expect("temp dir");
        let public = temp.path().join("public");
        std::fs::create_dir_all(&public).expect("create public dir");
        std::fs::write(public.join(PUBLIC_MARKER_FILE), "").expect("write bundle.css");

        // No bundle.css under any site-root candidate, so this must fall back
        // to the legacy standalone `public/` sibling next to the binary.
        let site_roots = candidate_site_roots_from_dirs(
            DEFAULT_SITE_ROOT,
            &[temp.path().into()],
            Path::new("/src/crate"),
        );

        assert_eq!(
            resolve_public_dir_from_dirs(
                site_roots,
                &[temp.path().into()],
                Path::new("/src/crate")
            )
            .as_deref(),
            Some(public.as_path())
        );
    }
}

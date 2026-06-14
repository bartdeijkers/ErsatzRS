use leptos::config::LeptosOptions;
use std::path::{Path, PathBuf};

const WEB_CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");
pub(crate) const DEFAULT_OUTPUT_NAME: &str = "ersatzrs-web";
const DEFAULT_SITE_ROOT: &str = "target/site";
const BUNDLED_SITE_ROOT: &str = "site";
const DEFAULT_SITE_PKG_DIR: &str = "pkg";

pub fn resolve_public_dir() -> Option<PathBuf> {
    candidate_runtime_dirs()
        .into_iter()
        .map(|root| root.join("public"))
        .chain(std::iter::once(Path::new(WEB_CRATE_DIR).join("public")))
        .find(|candidate| candidate.join("bundle.css").is_file())
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
}

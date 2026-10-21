//! Runtime fixture roots for tests reused through a nextest archive.

#[allow(
    clippy::disallowed_methods,
    reason = "Cargo and nextest supply the runtime package root for relocatable test fixtures"
)]
pub(crate) fn manifest_dir(compiled_root: &str) -> std::path::PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| compiled_root.into(), std::path::PathBuf::from)
}

macro_rules! test_manifest_dir {
    () => {
        $crate::test_manifest::manifest_dir(env!("CARGO_MANIFEST_DIR"))
    };
}

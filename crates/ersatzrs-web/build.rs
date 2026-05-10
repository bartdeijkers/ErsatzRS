fn main() {
    println!("cargo:rerun-if-changed=src/admin");

    // Cross-compile and minimal-tooling builds (e.g. `cross build`) ship the
    // CSS bundle from the leptos host build instead of regenerating it here.
    // Warn and continue when stylance-cli is missing so those builds succeed.
    let status = match std::process::Command::new("stylance")
        .args([
            env!("CARGO_MANIFEST_DIR"),
            "--output-file",
            "public/bundle.css",
        ])
        .status()
    {
        Ok(status) => status,
        Err(e) => {
            println!(
                "cargo:warning=stylance-cli not found ({e}); \
                 skipping CSS bundle regeneration. \
                 Install with `cargo install stylance-cli` for host builds."
            );
            return;
        }
    };

    assert!(status.success(), "stylance bundle failed");
}

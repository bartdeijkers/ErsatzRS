fn main() {
    println!("cargo:rerun-if-changed=src/admin");

    let status = std::process::Command::new("stylance")
        .args([
            env!("CARGO_MANIFEST_DIR"),
            "--output-file",
            "public/bundle.css",
        ])
        .status()
        .unwrap_or_else(|e| {
            panic!("stylance-cli not found: {e}\nInstall with: cargo install stylance-cli")
        });

    assert!(status.success(), "stylance bundle failed");
}

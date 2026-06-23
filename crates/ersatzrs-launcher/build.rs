fn main() {
    println!("cargo:rerun-if-changed=ersatzrs_launcher.rc");
    println!("cargo:rerun-if-changed=../../artwork/Ersatzrs.ico");

    // Build scripts are compiled for the host, so `#[cfg(windows)]` misses WSL
    // and Linux-hosted Windows cross builds. Use Cargo's target cfg instead.
    let target_os_is_windows =
        std::env::vars().any(|(key, value)| key == "CARGO_CFG_TARGET_OS" && value == "windows");

    if target_os_is_windows {
        embed_resource::compile("ersatzrs_launcher.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("compile Windows launcher resources");
        static_vcruntime::metabuild();
    }
}

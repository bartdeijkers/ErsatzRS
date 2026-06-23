fn main() {
    println!("cargo:rerun-if-changed=ersatzrs_launcher.rc");
    println!("cargo:rerun-if-changed=../../artwork/Ersatzrs.ico");

    // Build scripts are compiled for the host, so `#[cfg(windows)]` misses WSL
    // and Linux-hosted Windows cross builds. Use Cargo's target cfg instead.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("ersatzrs_launcher.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("compile Windows launcher resources");
        static_vcruntime::metabuild();
    }
}

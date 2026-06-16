fn main() {
    // On Windows, embed the tray icon (a named resource `ersatzrs-icon` that
    // tray-item loads at runtime) and version info from the .rc file, and
    // statically link the MSVC CRT. Everything else is a no-op so the crate
    // builds cleanly on non-Windows hosts.
    #[cfg(windows)]
    {
        let _ = embed_resource::compile("ersatzrs_launcher.rc", embed_resource::NONE);
        static_vcruntime::metabuild();
    }
}

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=ersatzrs_launcher.rc");
    println!("cargo:rerun-if-changed=../../artwork/Ersatzrs.ico");
    println!("cargo:rerun-if-env-changed=GITHUB_REF_NAME");

    // Build scripts are compiled for the host, so `#[cfg(windows)]` misses WSL
    // and Linux-hosted Windows cross builds. Use Cargo's target cfg instead.
    let env: Vec<(String, String)> = std::env::vars().collect();
    let var = |name: &str| {
        env.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };

    if var("CARGO_CFG_TARGET_OS") == Some("windows") {
        let version = release_version(var("GITHUB_REF_NAME"))
            .unwrap_or_else(|| var("CARGO_PKG_VERSION").unwrap_or("0.0.0").to_owned());
        let manifest_dir = PathBuf::from(var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
        let icon = manifest_dir.join("../../artwork/Ersatzrs.ico");
        let template = std::fs::read_to_string(manifest_dir.join("ersatzrs_launcher.rc"))
            .expect("read Windows launcher resource template");
        let rendered = render_resource(&template, &icon.to_string_lossy(), &version);
        let out = PathBuf::from(var("OUT_DIR").expect("OUT_DIR")).join("ersatzrs_launcher.rc");
        std::fs::write(&out, rendered).expect("write Windows launcher resources");

        embed_resource::compile(&out, embed_resource::NONE)
            .manifest_optional()
            .expect("compile Windows launcher resources");
        static_vcruntime::metabuild();
    }
}

/// Release workflows build from a `vX.Y.Z[-suffix]` tag; show that version in
/// the Windows file properties instead of the workspace crate version.
fn release_version(ref_name: Option<&str>) -> Option<String> {
    let version = ref_name?.strip_prefix('v')?;
    let core = version.split(['-', '+']).next()?;
    let parts: Vec<&str> = core.split('.').collect();
    (parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())))
    .then(|| version.to_owned())
}

/// Fill the `.rc` template. `FILEVERSION` takes four 16-bit numbers, so a
/// pre-release suffix is kept only in the string values.
fn render_resource(template: &str, icon_path: &str, version: &str) -> String {
    let mut numbers = version
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0))
        .collect::<Vec<_>>();
    numbers.resize(4, 0);
    let file_version = numbers
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    // `.rc` string literals treat backslashes as escapes; forward slashes are
    // accepted by rc.exe and keep Windows paths intact.
    template
        .replace("@ICON_PATH@", &icon_path.replace('\\', "/"))
        .replace("@FILE_VERSION@", &file_version)
        .replace("@VERSION@", &version.replace('"', ""))
}

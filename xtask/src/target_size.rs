use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::Command,
};

const DEFAULT_THRESHOLD_GIB: u64 = 40;
const GIB: u64 = 1024 * 1024 * 1024;

pub fn warn_if_oversized() {
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"));
    if !target_dir.is_dir() {
        return;
    }

    let threshold_gib = env::var("ERSATZRS_TARGET_CLEAN_THRESHOLD_GIB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_THRESHOLD_GIB);
    let threshold_bytes = threshold_gib.saturating_mul(GIB);

    match target_size_bytes(&target_dir) {
        Ok(size) if size >= threshold_bytes => {
            let size_gib = size as f64 / GIB as f64;
            eprintln!(
                "warning: {} is {size_gib:.1} GiB; run 'cargo clean' before continuing large builds (threshold: {threshold_gib} GiB)",
                target_dir.display()
            );
        }
        Ok(_) => {}
        Err(error) => {
            eprintln!(
                "warning: could not check {} size: {error}",
                target_dir.display()
            );
        }
    }
}

fn target_size_bytes(path: &Path) -> io::Result<u64> {
    du_size_bytes(path).or_else(|_| dir_size(path))
}

fn du_size_bytes(path: &Path) -> io::Result<u64> {
    let output = Command::new("du").arg("-sk").arg(path).output()?;
    if !output.status.success() {
        return Err(io::Error::other("du -sk failed"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(kib) = stdout
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return Err(io::Error::other("du -sk returned no size"));
    };
    Ok(kib.saturating_mul(1024))
}

fn dir_size(path: &Path) -> io::Result<u64> {
    let mut total = 0_u64;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total = total.saturating_add(dir_size(&entry.path())?);
        } else {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

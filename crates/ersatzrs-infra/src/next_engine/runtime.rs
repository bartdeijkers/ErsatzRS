//! Locate the optional executable without falling back after an explicit override.

use std::io;
use std::path::{Path, PathBuf};

/// Resolve `ETV_NEXT_FOLDER`, or the directory beside the host executable.
///
/// An override containing `~` as its first path component is expanded against
/// the supplied home directory. Other user-name expansion is not supported.
///
/// # Errors
/// Returns a redacted I/O error when the directory cannot be resolved or the
/// executable is missing, inaccessible, or not a regular file.
pub fn resolve_channel_binary(
    override_folder: Option<&Path>,
    host_executable: &Path,
    home: Option<&Path>,
) -> io::Result<PathBuf> {
    let folder = match override_folder.filter(|folder| !folder.as_os_str().is_empty()) {
        Some(folder) => match folder.strip_prefix("~") {
            Ok(suffix) => home
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        "Next override requires a home directory",
                    )
                })?
                .join(suffix),
            Err(_) => folder.to_path_buf(),
        },
        None => host_executable
            .parent()
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "Next executable directory is unavailable",
                )
            })?
            .to_path_buf(),
    };
    let executable = folder.join(format!("ersatztv-channel{}", std::env::consts::EXE_SUFFIX));
    let metadata = std::fs::metadata(&executable)
        .map_err(|error| io::Error::new(error.kind(), "Next channel executable is unavailable"))?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Next channel executable is not a file",
        ));
    }
    Ok(executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable(folder: &Path) -> PathBuf {
        let path = folder.join(format!("ersatztv-channel{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&path, b"synthetic runtime fixture").unwrap();
        path
    }

    #[test]
    fn override_takes_precedence_and_missing_override_never_uses_bundle() {
        let bundle = tempfile::tempdir().unwrap();
        let alternate = tempfile::tempdir().unwrap();
        let bundled = executable(bundle.path());
        let selected = executable(alternate.path());
        let host = bundle.path().join("host");
        assert_eq!(resolve_channel_binary(None, &host, None).unwrap(), bundled);
        assert_eq!(
            resolve_channel_binary(Some(alternate.path()), &host, None).unwrap(),
            selected
        );
        std::fs::remove_file(selected).unwrap();
        let error = resolve_channel_binary(Some(alternate.path()), &host, None).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!error
            .to_string()
            .contains(&alternate.path().display().to_string()));
    }

    #[test]
    fn home_expansion_and_directory_rejection_are_platform_neutral() {
        let home = tempfile::tempdir().unwrap();
        let folder = home.path().join("engine");
        std::fs::create_dir(&folder).unwrap();
        let selected = executable(&folder);
        let override_folder = Path::new("~").join("engine");
        let host = home.path().join("host");
        assert_eq!(
            resolve_channel_binary(Some(&override_folder), &host, Some(home.path())).unwrap(),
            selected
        );
        assert!(resolve_channel_binary(Some(&override_folder), &host, None).is_err());
        std::fs::remove_file(&selected).unwrap();
        std::fs::create_dir(selected).unwrap();
        assert_eq!(
            resolve_channel_binary(Some(&folder), &host, None)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}

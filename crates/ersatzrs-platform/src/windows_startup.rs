//! Per-user Windows startup registration for the packaged tray launcher.

use std::io;
#[cfg(any(windows, test))]
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

use thiserror::Error;

#[cfg(windows)]
const LAUNCHER_NAME: &str = "ErsatzRS.exe";

/// Effective state of the current user's Windows startup registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The current platform does not provide Windows startup registration.
    Unsupported,
    /// No registration exists for ErsatzRS.
    Disabled,
    /// The registration points at the packaged launcher beside the web host.
    Enabled,
    /// A registration exists but points at a different launcher location.
    NeedsRepair,
    /// The packaged launcher cannot be resolved.
    Unavailable,
}

/// Failure while updating or inspecting Windows startup registration.
#[derive(Debug, Error)]
pub enum Error {
    /// The operation is unavailable on this operating system.
    #[error("Windows startup registration is not supported on this platform")]
    Unsupported,
    /// The running executable path could not be resolved.
    #[error("failed to resolve the ErsatzRS server executable: {0}")]
    ResolveExecutable(#[source] io::Error),
    /// The packaged launcher was not found beside the running web host.
    #[error("ErsatzRS.exe was not found next to ersatzrs-web.exe")]
    LauncherUnavailable,
    /// The current user's registry entry could not be read or updated.
    #[error("Windows startup registry access failed: {0}")]
    Registry(#[source] io::Error),
}

/// Inspect the current user's Windows startup registration.
///
/// # Errors
///
/// Returns [`Error::ResolveExecutable`] when the running web host path cannot
/// be resolved, or [`Error::Registry`] when the current user's startup
/// registration cannot be read.
pub fn status() -> Result<Status, Error> {
    #[cfg(windows)]
    {
        status_windows()
    }
    #[cfg(not(windows))]
    {
        Ok(Status::Unsupported)
    }
}

/// Enable or disable the current user's Windows startup registration.
///
/// # Errors
///
/// Returns [`Error::Unsupported`] outside Windows, [`Error::ResolveExecutable`]
/// or [`Error::LauncherUnavailable`] when enabling startup without a resolvable
/// packaged launcher, or [`Error::Registry`] when the current user's
/// registration cannot be updated.
pub fn set_enabled(enabled: bool) -> Result<(), Error> {
    #[cfg(windows)]
    {
        set_enabled_windows(enabled)
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err(Error::Unsupported)
    }
}

#[cfg(any(windows, test))]
fn classify_status(expected_command: Option<&str>, registered_command: Option<&str>) -> Status {
    match (expected_command, registered_command) {
        (None, _) => Status::Unavailable,
        (Some(_), None) => Status::Disabled,
        (Some(expected), Some(registered)) if registered.trim().eq_ignore_ascii_case(expected) => {
            Status::Enabled
        }
        (Some(_), Some(_)) => Status::NeedsRepair,
    }
}

#[cfg(any(windows, test))]
fn launcher_command(path: &Path) -> String {
    format!(r#""{}""#, path.display())
}

#[cfg(windows)]
fn launcher_path() -> Result<PathBuf, Error> {
    let executable = std::env::current_exe().map_err(Error::ResolveExecutable)?;
    executable
        .parent()
        .map(|directory| directory.join(LAUNCHER_NAME))
        .filter(|path| path.is_file())
        .ok_or(Error::LauncherUnavailable)
}

#[cfg(windows)]
fn status_windows() -> Result<Status, Error> {
    let expected_command = match launcher_path() {
        Ok(launcher) => Some(launcher_command(&launcher)),
        Err(Error::LauncherUnavailable) => None,
        Err(error) => return Err(error),
    };
    let registered_command = windows_registry::read_registration().map_err(Error::Registry)?;
    Ok(classify_status(
        expected_command.as_deref(),
        registered_command.as_deref(),
    ))
}

#[cfg(windows)]
fn set_enabled_windows(enabled: bool) -> Result<(), Error> {
    if enabled {
        let launcher = launcher_path()?;
        windows_registry::write_registration(&launcher_command(&launcher)).map_err(Error::Registry)
    } else {
        windows_registry::delete_registration().map_err(Error::Registry)
    }
}

#[cfg(windows)]
mod windows_registry {
    use std::io;

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "ErsatzRS";

    pub(super) fn read_registration() -> io::Result<Option<String>> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let run = match current_user.open_subkey_with_flags(RUN_KEY, KEY_READ) {
            Ok(run) => run,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        read_registration_from(&run)
    }

    fn read_registration_from(run: &RegKey) -> io::Result<Option<String>> {
        match run.get_value(VALUE_NAME) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub(super) fn write_registration(command: &str) -> io::Result<()> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let (run, _) = current_user.create_subkey(RUN_KEY)?;
        write_registration_to(&run, command)
    }

    fn write_registration_to(run: &RegKey, command: &str) -> io::Result<()> {
        run.set_value(VALUE_NAME, &command)
    }

    pub(super) fn delete_registration() -> io::Result<()> {
        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let run = match current_user.open_subkey_with_flags(RUN_KEY, KEY_READ | KEY_WRITE) {
            Ok(run) => run,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        delete_registration_from(&run)
    }

    fn delete_registration_from(run: &RegKey) -> io::Result<()> {
        match run.delete_value(VALUE_NAME) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    #[cfg(test)]
    mod tests {
        use std::time::{SystemTime, UNIX_EPOCH};

        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        use super::{delete_registration_from, read_registration_from, write_registration_to};

        #[test]
        fn windows_startup_registry_round_trip_uses_an_isolated_subkey() {
            let current_user = RegKey::predef(HKEY_CURRENT_USER);
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock should be after the Unix epoch")
                .as_nanos();
            let path = format!(r"Software\ErsatzRS\Tests\WindowsStartup-{nonce}");
            let (test_key, _) = current_user
                .create_subkey(&path)
                .expect("temporary registry key should be created");

            assert_eq!(
                read_registration_from(&test_key).expect("empty registration should be readable"),
                None
            );
            write_registration_to(&test_key, r#""C:\ErsatzRS\ErsatzRS.exe""#)
                .expect("registration should be written");
            assert_eq!(
                read_registration_from(&test_key).expect("registration should be readable"),
                Some(r#""C:\ErsatzRS\ErsatzRS.exe""#.to_owned())
            );
            delete_registration_from(&test_key).expect("registration should be deleted");
            assert_eq!(
                read_registration_from(&test_key).expect("deleted registration should be absent"),
                None
            );

            drop(test_key);
            current_user
                .delete_subkey_all(&path)
                .expect("temporary registry key should be removed");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{classify_status, launcher_command, Status};

    #[test]
    fn windows_startup_launcher_command_quotes_paths_with_spaces() {
        assert_eq!(
            launcher_command(Path::new(r"C:\Program Files\ErsatzRS\ErsatzRS.exe")),
            r#""C:\Program Files\ErsatzRS\ErsatzRS.exe""#
        );
    }

    #[test]
    fn windows_startup_registration_matching_is_case_insensitive_and_trimmed() {
        assert_eq!(
            classify_status(
                Some(r#""C:\ErsatzRS\ErsatzRS.exe""#),
                Some(r#" "C:\ERSATZRS\ERSATZRS.EXE" "#),
            ),
            Status::Enabled
        );
    }

    #[test]
    fn windows_startup_registration_classifies_absent_and_stale_values() {
        let expected = r#""C:\ErsatzRS\ErsatzRS.exe""#;

        assert_eq!(classify_status(Some(expected), None), Status::Disabled);
        assert_eq!(
            classify_status(Some(expected), Some(r#""C:\Old\ErsatzRS.exe""#)),
            Status::NeedsRepair
        );
    }

    #[test]
    fn windows_startup_missing_launcher_is_unavailable() {
        assert_eq!(
            classify_status(None, Some(r#""C:\Old\ErsatzRS\ErsatzRS.exe""#)),
            Status::Unavailable
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn windows_startup_has_an_explicit_non_windows_counterpart() {
        assert_eq!(
            super::status().expect("non-Windows status should be inspectable"),
            Status::Unsupported
        );
        assert!(matches!(
            super::set_enabled(true),
            Err(super::Error::Unsupported)
        ));
    }
}

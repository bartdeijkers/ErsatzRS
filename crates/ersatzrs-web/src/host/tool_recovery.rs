//! Explicit offline retirement of one native tool's process-cleanup receipts.

use std::{ffi::OsString, path::PathBuf};

use ersatzrs_infra::addon_tool_gate::{recover_tool_receipts, recovery_tool_identity};

/// Public packaged-server action, dispatched before any runtime startup.
pub const RECOVERY_ARGUMENT: &str = "--recover-addon-tool";
const USAGE: &str = "Usage: ersatzrs-web --recover-addon-tool --tool-path <absolute-executable-path> [--state-dir <absolute-profile-directory>] --confirm-all-processes-stopped. First stop the server, scanner, and all related provider and tool processes.";

struct Options {
    tool_path: Option<PathBuf>,
    state_dir: Option<PathBuf>,
}

fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Options, &'static str> {
    let mut arguments = arguments.into_iter();
    let mut options = Options {
        tool_path: None,
        state_dir: None,
    };
    let mut confirmed = false;
    while let Some(argument) = arguments.next() {
        match argument.to_str() {
            Some("--tool-path") if options.tool_path.is_none() => {
                options.tool_path = Some(PathBuf::from(arguments.next().ok_or(USAGE)?));
            }
            Some("--state-dir") if options.state_dir.is_none() => {
                options.state_dir = Some(PathBuf::from(arguments.next().ok_or(USAGE)?));
            }
            Some("--confirm-all-processes-stopped") if !confirmed => confirmed = true,
            _ => return Err(USAGE),
        }
    }
    if !confirmed
        || options.tool_path.is_none()
        || options
            .tool_path
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        || options
            .state_dir
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
    {
        return Err(USAGE);
    }
    Ok(options)
}

/// Run the offline action with arguments following [`RECOVERY_ARGUMENT`].
///
/// Only the selected tool's receipts are retired; executables and media are not
/// repaired or removed. The acknowledgement is required even for an empty store.
///
/// # Errors
/// Returns redacted usage, busy, or storage diagnostics without starting services.
pub fn run(arguments: impl IntoIterator<Item = OsString>) -> Result<usize, &'static str> {
    let options = parse(arguments)?;
    let identity = recovery_tool_identity(&options.tool_path.ok_or(USAGE)?)
        .map_err(|_| "Tool identity could not be resolved. Check the absolute executable path and access to its parent directory.")?;
    let root = options
        .state_dir
        .unwrap_or_else(ersatzrs_infra::file_system_layout::app_state_folder)
        .join("addons")
        .join("tool-leases");
    recover_tool_receipts(&root, &identity, true).map_err(|error| {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            "Tool recovery refused: a live invocation still owns the tool. Stop the server, scanner, and related provider and tool processes first."
        } else {
            "Tool recovery could not safely retire the selected receipts. Check the exact tool identity, profile directory, and storage access."
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acknowledgement_and_one_absolute_tool_path_are_required() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("tool").into_os_string();
        assert!(parse([OsString::from("--tool-path"), path.clone()]).is_err());
        assert!(parse([OsString::from("--confirm-all-processes-stopped")]).is_err());
        assert!(parse([
            OsString::from("--tool-path"),
            path,
            OsString::from("--confirm-all-processes-stopped")
        ])
        .is_ok());
        assert!(parse([
            OsString::from("--tool-path"),
            OsString::from("relative"),
            OsString::from("--confirm-all-processes-stopped")
        ])
        .is_err());
    }

    #[test]
    fn packaged_action_uses_only_the_selected_profile_and_tool() {
        use ersatzrs_infra::addon_tool_gate::{ToolLease, ToolLeaseMode};
        let temporary = tempfile::tempdir().unwrap();
        let tool = temporary.path().join("tool");
        std::fs::write(&tool, b"identity").unwrap();
        let root = temporary.path().join("addons/tool-leases");
        drop(
            ToolLease::try_acquire(&root, std::slice::from_ref(&tool), ToolLeaseMode::Update)
                .unwrap()
                .unwrap(),
        );
        let args = vec![
            OsString::from("--tool-path"),
            tool.into_os_string(),
            OsString::from("--state-dir"),
            temporary.path().as_os_str().to_owned(),
            OsString::from("--confirm-all-processes-stopped"),
        ];
        assert_eq!(run(args.clone()).unwrap(), 1);
        assert_eq!(run(args).unwrap(), 0);
    }
}

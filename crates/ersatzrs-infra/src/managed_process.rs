//! Bounded supervisors share the existing Unix process group / Windows job owner.

use std::process::ExitStatus;
use tokio::process::{ChildStderr, ChildStdout, Command};

/// Own an external process tree; explicit completion confirms descendant cleanup.
/// Drop requests termination, but callers must await `wait` or `kill` before
/// releasing files, reservations or other resources that its descendants use.
pub struct ManagedProcess(crate::addons::ManagedAddonChild);

impl ManagedProcess {
    /// Spawn through the same process-group/job adapter used by provider workers.
    /// # Errors
    /// Returns spawn or process-tree attachment failures.
    pub fn spawn(command: Command) -> std::io::Result<Self> {
        crate::addons::spawn_managed_addon_process(command).map(Self)
    }

    /// Take the configured diagnostic output pipe.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.0.stdout().take()
    }

    /// Take the configured diagnostic error pipe.
    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.0.stderr().take()
    }

    /// Wait for the leader and confirm the entire process tree has terminated.
    /// # Errors
    /// Returns wait or descendant-cleanup failures.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.0.wait().await
    }

    /// Terminate and reap the entire process tree.
    /// # Errors
    /// Returns termination or cleanup failures.
    pub async fn kill(&mut self) -> std::io::Result<()> {
        self.0.kill().await
    }
}

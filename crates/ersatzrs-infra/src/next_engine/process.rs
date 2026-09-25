//! Bounded startup and whole-process-tree ownership for the optional engine.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{timeout, timeout_at, Instant};

use crate::addons::{spawn_managed_addon_process, ManagedAddonChild};

use super::protocol::ChannelConfig;

const MAX_CONFIG_BYTES: usize = 256 * 1024;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Process diagnostics exclude raw engine output, configuration and private paths.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("Next startup preparation failed: {0}")]
    Preparation(&'static str),
    #[error("Next configuration cannot be serialized within the size limit")]
    Configuration,
    #[error("Next startup deadline expired")]
    StartupTimeout,
    #[error("Next exited before readiness (exit code {0:?})")]
    ExitedBeforeReady(Option<i32>),
    #[error("Next process cleanup deadline expired")]
    CleanupTimeout,
    #[error("Next {operation} failed ({kind:?})")]
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
    },
}

/// Owns the same managed Unix group / Windows job used by add-on processes.
/// Dropping the owner also terminates descendants; normal shutdown awaits them.
pub struct NextProcess {
    child: ManagedAddonChild,
    output_folder: PathBuf,
}

impl NextProcess {
    /// Start Next and wait for its ready marker within one shared deadline.
    ///
    /// The caller must reserve the channel's session and exclusively own the
    /// output directory before calling. Next clears its output directory on
    /// startup; this adapter removes the old ready marker before spawning, so it
    /// cannot acknowledge an earlier session. Generated configuration travels on stdin.
    /// Raw child output is discarded: it can contain provider URLs and paths.
    ///
    /// # Errors
    /// Returns a redacted setup, exit or timeout error. Startup failure kills and
    /// reaps the managed tree before returning, or reports unconfirmed cleanup.
    pub async fn start(
        executable: &Path,
        number: &str,
        output_folder: &Path,
        config: &ChannelConfig,
        deadline: Instant,
    ) -> Result<Self, ProcessError> {
        Self::start_with_overlays(executable, number, output_folder, config, None, deadline).await
    }

    /// Start with optional Next-owned configuration overlays.
    ///
    /// Existing `default.json` and `{number}.json` files follow stdin in that
    /// order. Next performs the merge, with later files taking precedence.
    /// Discovery shares the startup deadline; absent files are optional.
    ///
    /// # Errors
    /// Returns a redacted error for unsafe channel filenames, filesystem errors,
    /// or the same startup and cleanup failures as [`Self::start`].
    pub async fn start_with_overlays(
        executable: &Path,
        number: &str,
        output_folder: &Path,
        config: &ChannelConfig,
        overlay_directory: Option<&Path>,
        deadline: Instant,
    ) -> Result<Self, ProcessError> {
        Self::start_with_session(
            executable,
            number,
            output_folder,
            config,
            overlay_directory,
            None,
            deadline,
        )
        .await
    }

    /// Supply a reserved callback identity only to this spawned Next process.
    #[allow(
        clippy::too_many_arguments,
        reason = "Process startup forwards the explicit channel configuration and session identity."
    )]
    pub(super) async fn start_with_session(
        executable: &Path,
        number: &str,
        output_folder: &Path,
        config: &ChannelConfig,
        overlay_directory: Option<&Path>,
        instance: Option<uuid::Uuid>,
        deadline: Instant,
    ) -> Result<Self, ProcessError> {
        let mut command = channel_command(
            executable,
            number,
            output_folder,
            overlay_directory,
            deadline,
        )
        .await?;
        set_session_environment(&mut command, instance);
        Self::start_command(command, output_folder, config, deadline).await
    }

    async fn start_command(
        mut command: Command,
        output_folder: &Path,
        config: &ChannelConfig,
        deadline: Instant,
    ) -> Result<Self, ProcessError> {
        if Instant::now() >= deadline {
            return Err(ProcessError::StartupTimeout);
        }
        let json = serde_json::to_vec(config).map_err(|_| ProcessError::Configuration)?;
        if json.len() > MAX_CONFIG_BYTES {
            return Err(ProcessError::Configuration);
        }
        timeout_at(deadline, prepare_output(output_folder))
            .await
            .map_err(|_| ProcessError::StartupTimeout)??;
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child =
            spawn_managed_addon_process(command).map_err(|error| io_error("spawn", &error))?;
        let mut process = Self {
            child,
            output_folder: output_folder.to_owned(),
        };
        let started = timeout_at(deadline, process.initialize(&json))
            .await
            .unwrap_or(Err(ProcessError::StartupTimeout));
        if let Err(error) = started {
            process.stop().await?;
            return Err(error);
        }
        Ok(process)
    }

    async fn initialize(&mut self, config: &[u8]) -> Result<(), ProcessError> {
        let mut stdin = self.child.stdin().take().ok_or_else(|| {
            io_error(
                "open configuration input",
                &io::Error::from(io::ErrorKind::BrokenPipe),
            )
        })?;
        stdin
            .write_all(config)
            .await
            .map_err(|error| io_error("write configuration", &error))?;
        stdin
            .shutdown()
            .await
            .map_err(|error| io_error("close configuration", &error))?;
        drop(stdin);
        let ready = self.output_folder.join(".ready");
        tokio::select! {
            biased;
            status = self.child.wait() => {
                let status = status.map_err(|error| io_error("wait", &error))?;
                Err(ProcessError::ExitedBeforeReady(status.code()))
            }
            result = wait_ready(&ready) => result,
        }
    }

    /// Refresh Next's heartbeat while the host's session/idle policy permits it.
    ///
    /// # Errors
    /// Returns a redacted error if the heartbeat cannot be written.
    pub async fn touch(&self) -> Result<(), ProcessError> {
        tokio::fs::write(self.output_folder.join(".heartbeat"), b"")
            .await
            .map_err(|error| io_error("heartbeat", &error))
    }

    /// Wait for exit and reclaim descendants before reporting the exit status.
    ///
    /// # Errors
    /// Returns a redacted process wait/cleanup error.
    pub async fn wait(&mut self) -> Result<ExitStatus, ProcessError> {
        self.child
            .wait()
            .await
            .map_err(|error| io_error("wait", &error))
    }

    /// Kill the complete process tree and bound the cleanup confirmation wait.
    ///
    /// # Errors
    /// Returns an explicit error if cleanup cannot be confirmed within five seconds.
    pub async fn stop(&mut self) -> Result<(), ProcessError> {
        timeout(CLEANUP_TIMEOUT, self.child.kill())
            .await
            .map_err(|_| ProcessError::CleanupTimeout)?
            .map_err(|error| io_error("stop", &error))
    }
}

async fn channel_command(
    executable: &Path,
    number: &str,
    output_folder: &Path,
    overlay_directory: Option<&Path>,
    deadline: Instant,
) -> Result<Command, ProcessError> {
    if Instant::now() >= deadline {
        return Err(ProcessError::StartupTimeout);
    }
    let mut command = Command::new(executable);
    command
        .arg("run")
        .arg("--output-folder")
        .arg(output_folder)
        .arg(format!("--number={number}"))
        .arg("-");
    if let Some(directory) = overlay_directory {
        // Channel numbers are numeric, optionally with a decimal subchannel.
        // Validate on every platform, including Windows separators on Unix.
        if number.is_empty()
            || !number
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            || number.matches('.').count() > 1
        {
            return Err(ProcessError::Preparation("invalid overlay channel number"));
        }
        timeout_at(deadline, async {
            for filename in ["default.json".to_owned(), format!("{number}.json")] {
                let path = directory.join(filename);
                match tokio::fs::metadata(&path).await {
                    Ok(metadata) if metadata.is_file() => {
                        command.arg(path);
                    }
                    Ok(_) => {
                        return Err(io_error(
                            "inspect configuration overlay",
                            &io::Error::from(io::ErrorKind::InvalidData),
                        ));
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => return Err(io_error("inspect configuration overlay", &error)),
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| ProcessError::StartupTimeout)??;
    }
    Ok(command)
}

async fn prepare_output(folder: &Path) -> Result<(), ProcessError> {
    tokio::fs::create_dir_all(folder)
        .await
        .map_err(|error| io_error("prepare output", &error))?;
    match tokio::fs::remove_file(folder.join(".ready")).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove stale readiness", &error)),
    }
}

async fn wait_ready(path: &Path) -> Result<(), ProcessError> {
    loop {
        match tokio::fs::metadata(path).await {
            Ok(metadata) if metadata.is_file() => return Ok(()),
            Ok(_) => {
                return Err(io_error(
                    "read readiness",
                    &io::Error::from(io::ErrorKind::InvalidData),
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("read readiness", &error)),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn io_error(operation: &'static str, error: &io::Error) -> ProcessError {
    ProcessError::Io {
        operation,
        kind: error.kind(),
    }
}

fn set_session_environment(command: &mut Command, instance: Option<uuid::Uuid>) {
    // Never inherit a stale identity from the host environment.
    command.env_remove(super::media::SESSION_ENV);
    if let Some(instance) = instance {
        command.env(super::media::SESSION_ENV, instance.to_string());
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "process_runtime_tests.rs"]
mod runtime_tests;

#[cfg(test)]
pub(super) async fn fixture_process(
    folder: &Path,
    mode: &str,
) -> Result<NextProcess, ProcessError> {
    NextProcess::start_command(
        tests::fixture_command(mode, folder),
        folder,
        &tests::config(),
        Instant::now() + Duration::from_secs(10),
    )
    .await
}

#[cfg(test)]
#[path = "config_overlay_tests.rs"]
mod config_overlay_tests;

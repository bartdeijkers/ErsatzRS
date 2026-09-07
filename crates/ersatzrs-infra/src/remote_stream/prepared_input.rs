//! A bounded startup prefix and its still-owned add-on transfer process.

use std::path::Path;
use std::process::ExitStatus;
use std::sync::{atomic::AtomicU8, Arc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::ChildStdout;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use ersatzrs_ffmpeg::FfmpegCommand;

use crate::addons::ManagedAddonChild;

use super::{
    command_has_file_backed_secret, finish_stderr_task, read_stderr_tail,
    remote_stream_environment_secrets, run_remote_stream_producer, spawn_remote_stream_consumer,
    spawn_remote_stream_producer_with_stdin, PreparedProducerReplay, RemoteStreamCommand,
    RemoteStreamError, RemoteStreamProcess, RemoteStreamProducerDiagnostics,
    RemoteStreamProducerPipe, RemoteStreamSpawnContext,
};

const PREFIX_BYTES: usize = 64 * 1024;
const MAX_REQUEST_BYTES: usize = 512 * 1024;

/// Playback-owned producer started once, with a bounded replayable media prefix.
/// Dropping this owner terminates its managed process group/job; prefer `cancel`
/// to await cleanup when preparation or metadata probing fails.
pub struct PreparedRemoteStreamInput {
    producer: Option<ManagedAddonChild>,
    stdout: Option<ChildStdout>,
    stderr_task: Option<JoinHandle<String>>,
    prefix: Vec<u8>,
    exited: Option<ExitStatus>,
    diagnostics: Option<RemoteStreamProducerDiagnostics>,
    temporary_directory: Option<PreparedInputDirectory>,
}

// An interrupted cleanup future must not delete files before group/job reaping.
struct PreparedInputDirectory(Option<tempfile::TempDir>);

impl PreparedInputDirectory {
    fn remove_after_reap(mut self) {
        drop(self.0.take());
    }
}

impl Drop for PreparedInputDirectory {
    fn drop(&mut self) {
        if let Some(directory) = self.0.take() {
            let _ = directory.keep();
        }
    }
}

impl std::fmt::Debug for PreparedRemoteStreamInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedRemoteStreamInput")
            .field("prefix_bytes", &self.prefix.len())
            .field("producer_exited", &self.exited.is_some())
            .finish_non_exhaustive()
    }
}

impl PreparedRemoteStreamInput {
    /// Start one producer, send bounded JSON on stdin, and retain at most 64 KiB.
    /// The original deadline covers request writing, prefix reads and short EOF
    /// process completion. Signal cancellation and await this future.
    ///
    /// # Errors
    /// Returns static startup errors; no request, media or producer diagnostics
    /// are included in the error returned to callers.
    pub async fn start(
        command: &RemoteStreamCommand,
        request_json: &[u8],
        context: RemoteStreamSpawnContext,
        deadline: tokio::time::Instant,
        mut cancellation: oneshot::Receiver<()>,
    ) -> Result<Self, RemoteStreamError> {
        if request_json.len() > MAX_REQUEST_BYTES {
            return Err(RemoteStreamError::PreparedRequestTooLarge);
        }
        if !matches!(
            cancellation.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ) {
            return Err(RemoteStreamError::PreparedCancelled);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(RemoteStreamError::PreparedTimeout);
        }
        let temporary_directory = tempfile::Builder::new()
            .prefix("ersatzrs-prepared-input-")
            .tempdir()
            .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
        let mut producer_command = command.clone();
        for variable in ["TMPDIR", "TMP", "TEMP"] {
            producer_command.environment.insert(
                variable.to_owned(),
                temporary_directory.path().to_string_lossy().into_owned(),
            );
        }
        let mut producer = spawn_remote_stream_producer_with_stdin(&producer_command, true)
            .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
        let stdout = producer.stdout().take();
        let stderr_task = producer
            .stderr()
            .take()
            .map(|stderr| tokio::spawn(read_stderr_tail(stderr)));
        let mut input = Self {
            producer: Some(producer),
            stdout,
            stderr_task,
            prefix: Vec::with_capacity(PREFIX_BYTES),
            exited: None,
            temporary_directory: Some(PreparedInputDirectory(Some(temporary_directory))),
            diagnostics: Some(RemoteStreamProducerDiagnostics {
                label: "prepared add-on".to_owned(),
                context,
                effective_seek: None,
                secrets: remote_stream_environment_secrets(command),
                suppress_diagnostics: command_has_file_backed_secret(command),
                prepared: None,
            }),
        };
        let outcome = tokio::select! {
            biased;
            _ = &mut cancellation => Err(RemoteStreamError::PreparedCancelled),
            () = tokio::time::sleep_until(deadline) => Err(RemoteStreamError::PreparedTimeout),
            result = input.read_prefix(request_json) => result,
        };
        let outcome = outcome.and_then(|()| {
            if !matches!(
                cancellation.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            ) {
                Err(RemoteStreamError::PreparedCancelled)
            } else if tokio::time::Instant::now() >= deadline {
                Err(RemoteStreamError::PreparedTimeout)
            } else {
                Ok(())
            }
        });
        if let Err(error) = outcome {
            input.cancel().await?;
            return Err(error);
        }
        Ok(input)
    }

    async fn read_prefix(&mut self, request_json: &[u8]) -> Result<(), RemoteStreamError> {
        let producer = self
            .producer
            .as_mut()
            .ok_or(RemoteStreamError::PreparedProducerFailed)?;
        let mut stdin = producer
            .stdin()
            .take()
            .ok_or(RemoteStreamError::PreparedProducerFailed)?;
        stdin
            .write_all(request_json)
            .await
            .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
        stdin
            .shutdown()
            .await
            .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
        drop(stdin);
        let stdout = self
            .stdout
            .as_mut()
            .ok_or(RemoteStreamError::MissingStdout)?;
        let mut chunk = [0_u8; 8192];
        while self.prefix.len() < PREFIX_BYTES {
            let length = chunk.len().min(PREFIX_BYTES - self.prefix.len());
            let read = stdout
                .read(&mut chunk[..length])
                .await
                .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
            if read == 0 {
                let status = producer
                    .wait()
                    .await
                    .map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
                self.exited = Some(status);
                if !status.success() || self.prefix.is_empty() {
                    return Err(RemoteStreamError::PreparedProducerFailed);
                }
                break;
            }
            self.prefix.extend_from_slice(&chunk[..read]);
        }
        Ok(())
    }

    /// Borrow bounded bytes for probing; playback later replays these exact bytes.
    #[must_use]
    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }

    /// Terminate the managed producer and await its bounded stderr reader.
    ///
    /// # Errors
    /// Returns `PreparedTerminationUnconfirmed` if descendant cleanup could not
    /// be confirmed; private temporary files are retained in that case.
    pub async fn cancel(mut self) -> Result<(), RemoteStreamError> {
        self.stdout.take();
        cleanup_prepared_input(
            self.producer.take(),
            self.stderr_task.take(),
            self.temporary_directory.take(),
            self.exited.is_some(),
        )
        .await
    }
}

impl Drop for PreparedRemoteStreamInput {
    fn drop(&mut self) {
        if let Some(producer) = self.producer.as_mut() {
            let _ = producer.start_kill();
        }
        self.stdout.take();
        let producer = self.producer.take();
        let stderr = self.stderr_task.take();
        let directory = self.temporary_directory.take();
        if producer.is_none() && stderr.is_none() && directory.is_none() {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(cleanup_prepared_input(
                producer,
                stderr,
                directory,
                self.exited.is_some(),
            ));
        } else {
            if let Some(stderr) = stderr {
                stderr.abort();
            }
            // Without an executor, the directory guard keeps private files
            // until a future cleanup can establish descendant completion.
            drop(directory);
        }
    }
}

async fn cleanup_prepared_input(
    mut producer: Option<ManagedAddonChild>,
    stderr: Option<JoinHandle<String>>,
    directory: Option<PreparedInputDirectory>,
    already_reaped: bool,
) -> Result<(), RemoteStreamError> {
    let reaped = if already_reaped {
        true
    } else if let Some(producer) = producer.as_mut() {
        producer.kill().await.is_ok()
    } else {
        true
    };
    drop(producer);
    if let Some(stderr) = stderr {
        let _ = finish_stderr_task(stderr).await;
    }
    if reaped {
        if let Some(directory) = directory {
            directory.remove_after_reap();
        }
        Ok(())
    } else {
        tracing::warn!("prepared producer cleanup could not confirm descendant exit; retaining its private temporary directory");
        Err(RemoteStreamError::PreparedTerminationUnconfirmed)
    }
}

/// Attach the already-started producer to one encoder, replaying its probe prefix.
/// No producer is restarted and output remains bounded by normal pipe backpressure.
///
/// # Errors
/// Returns a spawn error only after the prepared producer has been cancelled
/// and awaited when the consumer could not be started.
pub async fn spawn_ffmpeg_with_prepared_remote_input(
    command: &FfmpegCommand,
    working_directory: Option<&Path>,
    mut prepared: PreparedRemoteStreamInput,
    context: RemoteStreamSpawnContext,
) -> Result<RemoteStreamProcess, RemoteStreamError> {
    let consumer =
        spawn_remote_stream_consumer(command, &command.arguments, working_directory, true);
    let (mut consumer, filter_graph_file) = match consumer {
        Ok(consumer) => consumer,
        Err(error) => {
            prepared.cancel().await?;
            return Err(error);
        }
    };
    let (Some(stdout), Some(consumer_stdin), Some(mut diagnostics)) = (
        prepared.stdout.take(),
        consumer.stdin.take(),
        prepared.diagnostics.take(),
    ) else {
        let _ = consumer.kill().await;
        prepared.cancel().await?;
        return Err(RemoteStreamError::PreparedProducerFailed);
    };
    let Some(producer) = prepared.producer.take() else {
        let _ = consumer.kill().await;
        prepared.cancel().await?;
        return Err(RemoteStreamError::PreparedProducerFailed);
    };
    let result = Arc::new(AtomicU8::new(0));
    diagnostics.context = context;
    diagnostics.prepared = Some(PreparedProducerReplay {
        prefix: std::mem::take(&mut prepared.prefix),
        exited: prepared.exited,
        result: Arc::clone(&result),
    });
    let (cancel, cancellation) = oneshot::channel();
    let producer_task = run_remote_stream_producer(
        producer,
        stdout,
        consumer_stdin,
        prepared.stderr_task.take(),
        cancellation,
        diagnostics,
    );
    let directory = prepared.temporary_directory.take();
    let task = tokio::spawn(async move {
        if producer_task.await {
            if let Some(directory) = directory {
                directory.remove_after_reap();
            }
        }
    });
    Ok(RemoteStreamProcess {
        child: consumer,
        filter_graph_file,
        producer_pipe: Some(RemoteStreamProducerPipe {
            cancel: Some(cancel),
            task: Some(task),
            prepared_result: Some(result),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::Duration;

    #[tokio::test]
    async fn prepared_input_rejects_cancelled_expired_and_oversized_requests_before_spawn() {
        let command = RemoteStreamCommand {
            program: "nonexistent-prepared-producer".into(),
            args: Vec::new(),
            working_directory: PathBuf::from("."),
            environment: BTreeMap::new(),
        };
        let (sender, receiver) = oneshot::channel();
        sender.send(()).unwrap();
        assert!(matches!(
            PreparedRemoteStreamInput::start(
                &command,
                b"{}",
                RemoteStreamSpawnContext::probe(),
                tokio::time::Instant::now() + Duration::from_secs(15),
                receiver,
            )
            .await,
            Err(RemoteStreamError::PreparedCancelled)
        ));
        let (_sender, receiver) = oneshot::channel();
        assert!(matches!(
            PreparedRemoteStreamInput::start(
                &command,
                b"{}",
                RemoteStreamSpawnContext::probe(),
                tokio::time::Instant::now(),
                receiver,
            )
            .await,
            Err(RemoteStreamError::PreparedTimeout)
        ));
        let (_sender, receiver) = oneshot::channel();
        assert!(matches!(
            PreparedRemoteStreamInput::start(
                &command,
                &vec![0; MAX_REQUEST_BYTES + 1],
                RemoteStreamSpawnContext::probe(),
                tokio::time::Instant::now() + Duration::from_secs(15),
                receiver,
            )
            .await,
            Err(RemoteStreamError::PreparedRequestTooLarge)
        ));
    }

    // These process fixtures use a Unix shell; preflight coverage above is portable.
    #[cfg(unix)]
    fn commands(script: &str) -> (RemoteStreamCommand, FfmpegCommand) {
        (
            RemoteStreamCommand {
                program: "sh".into(),
                args: vec!["-c".into(), script.into()],
                working_directory: PathBuf::from("."),
                environment: BTreeMap::new(),
            },
            FfmpegCommand {
                executable: "sh".into(),
                arguments: vec!["-c".into(), "cat".into()],
                environment: Vec::new(),
            },
        )
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prepared_input_replays_short_prefix_exactly_once_after_successful_eof() {
        let (producer, consumer) = commands(
            "test \"$TMPDIR\" = \"$TMP\" && test \"$TMP\" = \"$TEMP\" || exit 7; printf fixture > \"$TMPDIR/source.info.json\"; cat; printf tail",
        );
        let (_sender, receiver) = oneshot::channel();
        let prepared = PreparedRemoteStreamInput::start(
            &producer,
            b"{}",
            RemoteStreamSpawnContext::probe(),
            tokio::time::Instant::now() + Duration::from_secs(15),
            receiver,
        )
        .await
        .unwrap();
        assert_eq!(prepared.prefix(), b"{}tail");
        assert!(prepared.exited.is_some_and(|status| status.success()));
        let directory = prepared
            .temporary_directory
            .as_ref()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .path()
            .to_path_buf();
        assert!(directory.join("source.info.json").exists());
        let mut process = spawn_ffmpeg_with_prepared_remote_input(
            &consumer,
            None,
            prepared,
            RemoteStreamSpawnContext::probe(),
        )
        .await
        .unwrap();
        let mut stdout = process.stdout.take().unwrap();
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.unwrap();
        assert!(process.wait().await.unwrap().success());
        assert_eq!(bytes, b"{}tail");
        assert!(!directory.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prepared_input_cancel_and_drop_reap_before_removing_private_directory() {
        for explicit_cancel in [false, true] {
            let (producer, _) = commands(
                "cat >/dev/null; printf fixture > \"$TMPDIR/source.info.json\"; head -c 70000 /dev/zero; while :; do :; done",
            );
            let (_sender, receiver) = oneshot::channel();
            let prepared = PreparedRemoteStreamInput::start(
                &producer,
                b"{}",
                RemoteStreamSpawnContext::probe(),
                tokio::time::Instant::now() + Duration::from_secs(15),
                receiver,
            )
            .await
            .unwrap();
            let pid = i32::try_from(prepared.producer.as_ref().unwrap().id().unwrap()).unwrap();
            let directory = prepared
                .temporary_directory
                .as_ref()
                .unwrap()
                .0
                .as_ref()
                .unwrap()
                .path()
                .to_path_buf();
            assert!(directory.join("source.info.json").exists());
            if explicit_cancel {
                prepared.cancel().await.unwrap();
            } else {
                drop(prepared);
            }
            tokio::time::timeout(Duration::from_secs(2), async {
                while directory.exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(
                nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
                Err(nix::errno::Errno::ESRCH)
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prepared_input_continues_after_prefix_and_reports_producer_failure() {
        let (producer, consumer) = commands("cat >/dev/null; head -c 70000 /dev/zero; exit 7");
        let (_sender, receiver) = oneshot::channel();
        let prepared = PreparedRemoteStreamInput::start(
            &producer,
            b"{}",
            RemoteStreamSpawnContext::probe(),
            tokio::time::Instant::now() + Duration::from_secs(15),
            receiver,
        )
        .await
        .unwrap();
        assert_eq!(prepared.prefix().len(), PREFIX_BYTES);
        let mut process = spawn_ffmpeg_with_prepared_remote_input(
            &consumer,
            None,
            prepared,
            RemoteStreamSpawnContext::probe(),
        )
        .await
        .unwrap();
        let mut stdout = process.stdout.take().unwrap();
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.unwrap();
        assert_eq!(bytes.len(), 70000);
        assert!(
            process.wait().await.is_err(),
            "consumer success cannot hide producer failure"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn prepared_input_cancellation_reaps_producer_that_closed_stdout() {
        let temporary = tempfile::tempdir().unwrap();
        let pid_path = temporary.path().join("producer.pid");
        let (mut producer, _) = commands(
            "printf '%s' $$ > \"$1\"; cat >/dev/null; printf short; exec 1>&-; while :; do :; done",
        );
        producer
            .args
            .extend(["fixture".into(), pid_path.to_string_lossy().into_owned()]);
        let (sender, receiver) = oneshot::channel();
        let operation = PreparedRemoteStreamInput::start(
            &producer,
            b"{}",
            RemoteStreamSpawnContext::probe(),
            tokio::time::Instant::now() + Duration::from_secs(15),
            receiver,
        );
        tokio::pin!(operation);
        let started = async {
            while !pid_path.exists() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        };
        tokio::select! {
            result = &mut operation => panic!("producer ended early: {result:?}"),
            result = tokio::time::timeout(Duration::from_secs(2), started) => result.unwrap(),
        }
        sender.send(()).unwrap();
        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), operation)
                .await
                .unwrap(),
            Err(RemoteStreamError::PreparedCancelled)
        ));
        let pid: i32 = std::fs::read_to_string(pid_path).unwrap().parse().unwrap();
        assert_eq!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH)
        );
    }
}

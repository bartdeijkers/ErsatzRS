//! A bounded startup prefix and its still-owned add-on transfer process.

use futures_util::FutureExt as _;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::{atomic::AtomicU8, Arc};
use tokio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
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

fn check_prepared_startup(
    deadline: tokio::time::Instant,
    cancellation: &mut oneshot::Receiver<()>,
) -> Result<(), RemoteStreamError> {
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
}

/// Playback-owned producer started once, with a bounded replayable media prefix.
/// Dropping this owner terminates its managed process group/job; prefer `cancel`
/// to await cleanup when preparation or metadata probing fails.
pub struct PreparedRemoteStreamInput {
    pub(super) native_caption_binding: Option<Box<super::prepared_teletext::NativeCaptionBinding>>,
    playback_capacity: Option<super::source_caption::SourceCaptionRetention>,
    producer: Option<ManagedAddonChild>,
    stdout: Option<ChildStdout>,
    stderr_task: Option<JoinHandle<String>>,
    prefix: Vec<u8>,
    exited: Option<ExitStatus>,
    diagnostics: Option<RemoteStreamProducerDiagnostics>,
    temporary_directory: Option<PreparedInputDirectory>,
}

// An interrupted cleanup future must not delete files before group/job reaping.
struct PreparedInputDirectory(
    Option<tempfile::TempDir>,
    Vec<crate::dynamic_text::DynamicTextController>,
);

impl PreparedInputDirectory {
    async fn remove_after_reap(mut self) {
        crate::dynamic_text::shutdown_dynamic_text_controllers(std::mem::take(&mut self.1)).await;
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
    /// Retain an existing playback reservation until actual producer/consumer
    /// cleanup, including cancellation, abandoned input and failed spawn.
    ///
    /// The caller retains admission while preparing this input, then attaches it
    /// before yielding ownership. This grants neither new admission nor source,
    /// page or clock authority. Unconfirmed cleanup quarantines the reservation.
    ///
    /// # Errors
    /// Rejects a second attachment without replacing the existing owner.
    pub fn retain_playback_capacity(
        &mut self,
        capacity: crate::transcode_capacity::ActiveTranscodeReservation,
    ) -> Result<(), RemoteStreamError> {
        if self.playback_capacity.is_some() {
            return Err(RemoteStreamError::PreparedCaptionUnavailable);
        }
        self.playback_capacity =
            Some(super::source_caption::SourceCaptionRetention::playback_capacity_only(capacity));
        Ok(())
    }

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
        cancellation: oneshot::Receiver<()>,
    ) -> Result<Self, RemoteStreamError> {
        Self::start_inner(
            command,
            Some(request_json),
            context,
            deadline,
            cancellation,
            None,
            PREFIX_BYTES,
        )
        .await
    }

    /// Start an ordinary installed add-on without writing a prepared request.
    /// Its stdin is closed, while startup and cleanup follow the prepared path.
    ///
    /// # Errors
    /// Returns redacted startup, cancellation or deadline errors.
    pub async fn start_without_request(
        command: &RemoteStreamCommand,
        context: RemoteStreamSpawnContext,
        deadline: tokio::time::Instant,
        cancellation: oneshot::Receiver<()>,
    ) -> Result<Self, RemoteStreamError> {
        Self::start_inner(
            command,
            None,
            context,
            deadline,
            cancellation,
            None,
            PREFIX_BYTES,
        )
        .await
    }

    pub(super) async fn start_caption_prefix(
        command: &RemoteStreamCommand,
        context: RemoteStreamSpawnContext,
        deadline: tokio::time::Instant,
        cancellation: oneshot::Receiver<()>,
    ) -> Result<Self, RemoteStreamError> {
        Self::start_inner(command, None, context, deadline, cancellation, None, 1).await
    }

    /// Start a canvas producer with ownership of its generated files and live
    /// text controllers. These follow the managed child through startup, byte
    /// transfer and cancellation, and are removed only after confirmed reaping.
    ///
    /// # Errors
    /// Returns redacted startup errors; uncertain cleanup retains private files.
    pub async fn start_with_graphics(
        command: &RemoteStreamCommand,
        context: RemoteStreamSpawnContext,
        deadline: tokio::time::Instant,
        cancellation: oneshot::Receiver<()>,
        directory: tempfile::TempDir,
        controllers: Vec<crate::dynamic_text::DynamicTextController>,
    ) -> Result<Self, RemoteStreamError> {
        Self::start_inner(
            command,
            None,
            context,
            deadline,
            cancellation,
            Some(PreparedInputDirectory(Some(directory), controllers)),
            1,
        )
        .await
    }

    async fn start_inner(
        command: &RemoteStreamCommand,
        request_json: Option<&[u8]>,
        context: RemoteStreamSpawnContext,
        deadline: tokio::time::Instant,
        mut cancellation: oneshot::Receiver<()>,
        artifacts: Option<PreparedInputDirectory>,
        minimum_prefix: usize,
    ) -> Result<Self, RemoteStreamError> {
        let invalid = if request_json.is_some_and(|request| request.len() > MAX_REQUEST_BYTES) {
            Some(RemoteStreamError::PreparedRequestTooLarge)
        } else {
            check_prepared_startup(deadline, &mut cancellation).err()
        };
        if let Some(error) = invalid {
            if let Some(artifacts) = artifacts {
                artifacts.remove_after_reap().await;
            }
            return Err(error);
        }
        let graphics = artifacts.is_some();
        let temporary_directory = match artifacts {
            Some(artifacts) => artifacts,
            None => PreparedInputDirectory(
                Some(
                    tempfile::Builder::new()
                        .prefix("ersatzrs-prepared-input-")
                        .tempdir()
                        .map_err(|_| RemoteStreamError::PreparedProducerFailed)?,
                ),
                Vec::new(),
            ),
        };
        let temporary_path = temporary_directory
            .0
            .as_ref()
            .ok_or(RemoteStreamError::PreparedProducerFailed)?
            .path();
        let mut producer_command = command.clone();
        for variable in ["TMPDIR", "TMP", "TEMP"] {
            producer_command.environment.insert(
                variable.to_owned(),
                temporary_path.to_string_lossy().into_owned(),
            );
        }
        let Ok(mut producer) =
            spawn_remote_stream_producer_with_stdin(&producer_command, request_json.is_some())
        else {
            temporary_directory.remove_after_reap().await;
            return Err(RemoteStreamError::PreparedProducerFailed);
        };
        let stdout = producer.stdout().take();
        let stderr_task = producer
            .stderr()
            .take()
            .map(|stderr| tokio::spawn(read_stderr_tail(stderr)));
        let mut input = Self {
            native_caption_binding: None,
            playback_capacity: None,
            producer: Some(producer),
            stdout,
            stderr_task,
            prefix: Vec::with_capacity(PREFIX_BYTES),
            exited: None,
            temporary_directory: Some(temporary_directory),
            diagnostics: Some(RemoteStreamProducerDiagnostics {
                label: if graphics {
                    "graphics canvas"
                } else {
                    "prepared add-on"
                }
                .to_owned(),
                context,
                effective_seek: None,
                secrets: remote_stream_environment_secrets(command),
                suppress_diagnostics: graphics || command_has_file_backed_secret(command),
                prepared: None,
            }),
        };
        let outcome = tokio::select! {
            biased;
            _ = &mut cancellation => Err(RemoteStreamError::PreparedCancelled),
            () = tokio::time::sleep_until(deadline) => Err(RemoteStreamError::PreparedTimeout),
            result = input.read_prefix(request_json, minimum_prefix) => result,
        };
        let outcome = outcome.and_then(|()| check_prepared_startup(deadline, &mut cancellation));
        if let Err(error) = outcome {
            input.cancel().await?;
            return Err(error);
        }
        Ok(input)
    }

    async fn read_prefix(
        &mut self,
        request_json: Option<&[u8]>,
        minimum_bytes: usize,
    ) -> Result<(), RemoteStreamError> {
        let producer = self
            .producer
            .as_mut()
            .ok_or(RemoteStreamError::PreparedProducerFailed)?;
        if let Some(request_json) = request_json {
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
        }
        let stdout = self
            .stdout
            .as_mut()
            .ok_or(RemoteStreamError::MissingStdout)?;
        let mut chunk = [0_u8; 8192];
        while self.prefix.len() < minimum_bytes {
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

    // Opportunistically retain another available chunk, never waiting for a
    // low-rate live source to fill a minimum prefix or reach EOF.
    pub(super) fn extend_caption_prefix(&mut self) -> Result<bool, RemoteStreamError> {
        if self.prefix.len() == PREFIX_BYTES || self.exited.is_some() {
            return Ok(false);
        }
        let mut chunk = [0; 8192];
        let length = chunk.len().min(PREFIX_BYTES - self.prefix.len());
        let stdout = self
            .stdout
            .as_mut()
            .ok_or(RemoteStreamError::MissingStdout)?;
        let Some(read) = stdout.read(&mut chunk[..length]).now_or_never() else {
            return Ok(false);
        };
        let read = read.map_err(|_| RemoteStreamError::PreparedProducerFailed)?;
        self.check_caption_producer_status()?;
        self.prefix.extend_from_slice(&chunk[..read]);
        Ok(read != 0)
    }

    pub(super) fn check_caption_producer_status(&mut self) -> Result<(), RemoteStreamError> {
        let producer = self
            .producer
            .as_mut()
            .ok_or(RemoteStreamError::PreparedProducerFailed)?;
        if let Some(status) = producer
            .try_wait()
            .map_err(|_| RemoteStreamError::PreparedProducerFailed)?
        {
            if !status.success() {
                return Err(RemoteStreamError::PreparedProducerFailed);
            }
        }
        Ok(())
    }

    /// Forward the original bytes to one owned sink with normal backpressure.
    /// The returned owner must be shut down and awaited by the playback session.
    /// Dropping it signals cancellation but cannot await process cleanup.
    ///
    /// # Errors
    /// Returns a redacted error after cleanup if preparation lost its producer.
    pub async fn transfer_to<W>(
        mut self,
        sink: W,
    ) -> Result<PreparedRemoteStreamTransfer, RemoteStreamError>
    where
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let Some(stdout) = self.stdout.take() else {
            self.cancel().await?;
            return Err(RemoteStreamError::PreparedProducerFailed);
        };
        let Some(mut diagnostics) = self.diagnostics.take() else {
            self.cancel().await?;
            return Err(RemoteStreamError::PreparedProducerFailed);
        };
        let Some(producer) = self.producer.take() else {
            self.cancel().await?;
            return Err(RemoteStreamError::PreparedProducerFailed);
        };
        let result = Arc::new(AtomicU8::new(0));
        diagnostics.prepared = Some(PreparedProducerReplay {
            prefix: std::mem::take(&mut self.prefix),
            exited: self.exited,
            result: Arc::clone(&result),
        });
        let (cancel, cancellation) = oneshot::channel();
        let producer_task = run_remote_stream_producer(
            producer,
            stdout,
            sink,
            self.stderr_task.take(),
            cancellation,
            diagnostics,
        );
        let directory = self.temporary_directory.take();
        let mut capacity = self.playback_capacity.take();
        let task = tokio::spawn(async move {
            if !producer_task.await {
                return Err(RemoteStreamError::PreparedTerminationUnconfirmed);
            }
            if let Some(directory) = directory {
                directory.remove_after_reap().await;
            }
            if let Some(capacity) = &mut capacity {
                capacity.confirm_reaped();
            }
            if result.load(std::sync::atomic::Ordering::Acquire) == 2 {
                Err(RemoteStreamError::PreparedProducerFailed)
            } else {
                Ok(())
            }
        });
        Ok(PreparedRemoteStreamTransfer {
            cancel: Some(cancel),
            task: Some(task),
        })
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
            self.playback_capacity.take(),
        )
        .await
    }
}

/// Playback ownership of byte forwarding and managed producer cleanup.
#[derive(Debug)]
pub struct PreparedRemoteStreamTransfer {
    cancel: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<(), RemoteStreamError>>>,
}

impl PreparedRemoteStreamTransfer {
    /// Signal cancellation; call `wait` or `shutdown` to confirm cleanup.
    pub fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }

    /// Await byte transfer and cleanup without interrupting the producer.
    /// This wait may be cancelled and resumed without losing its task owner.
    ///
    /// # Errors
    /// Reports transfer/producer failure or unconfirmed cleanup.
    pub async fn wait(&mut self) -> Result<(), RemoteStreamError> {
        let Some(task) = self.task.as_mut() else {
            return Ok(());
        };
        let result = task
            .await
            .map_err(|_| RemoteStreamError::PreparedTerminationUnconfirmed);
        self.task.take();
        result?
    }

    /// Cancel and await cleanup, including its bounded stderr reader.
    ///
    /// # Errors
    /// Reports producer failure or unconfirmed cleanup.
    pub async fn shutdown(&mut self) -> Result<(), RemoteStreamError> {
        self.cancel();
        self.wait().await
    }
}

impl Drop for PreparedRemoteStreamTransfer {
    fn drop(&mut self) {
        self.cancel();
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
        let capacity = self.playback_capacity.take();
        if producer.is_none() && stderr.is_none() && directory.is_none() && capacity.is_none() {
            return;
        }
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(cleanup_prepared_input(
                producer,
                stderr,
                directory,
                self.exited.is_some(),
                capacity,
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
    mut capacity: Option<super::source_caption::SourceCaptionRetention>,
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
            directory.remove_after_reap().await;
        }
        if let Some(capacity) = &mut capacity {
            capacity.confirm_reaped();
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
    if prepared
        .native_caption_binding
        .as_ref()
        .is_some_and(|binding| !binding.matches(command))
    {
        prepared.cancel().await?;
        return Err(RemoteStreamError::PreparedCaptionUnavailable);
    }
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
        if consumer.kill().await.is_err() {
            if let Some(capacity) = &prepared.playback_capacity {
                capacity.retain_until_restart();
            }
        }
        prepared.cancel().await?;
        return Err(RemoteStreamError::PreparedProducerFailed);
    };
    let Some(producer) = prepared.producer.take() else {
        if consumer.kill().await.is_err() {
            if let Some(capacity) = &prepared.playback_capacity {
                capacity.retain_until_restart();
            }
        }
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
        let reaped = producer_task.await;
        if reaped {
            if let Some(directory) = directory {
                directory.remove_after_reap().await;
            }
        }
        reaped
    });
    Ok(RemoteStreamProcess {
        child: consumer,
        consumer_command: super::source_caption::ConsumerCommandSeal(command.clone()),
        filter_graph_file,
        source_caption: prepared.playback_capacity.take(),
        producer_pipe: Some(RemoteStreamProducerPipe {
            cancel: Some(cancel),
            task: Some(task),
            prepared_result: Some(result),
            cleanup_failed: false,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::time::Duration;

    async fn graphics_artifacts() -> (
        tempfile::TempDir,
        Vec<crate::dynamic_text::DynamicTextController>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let controller = crate::dynamic_text::DynamicTextController::start(
            crate::dynamic_text::DynamicTextControllerRequest {
                broadcast_start_time: None,
                metadata_refresh: None,
                element_id: 1,
                template: "canvas".to_owned(),
                variables: serde_json::json!({}),
                text_file: directory.path().join("text.txt"),
                refresh_interval_milliseconds: 100,
                playback_offset_seconds: 0.0,
                playback_duration_seconds: 60.0,
            },
        )
        .await
        .unwrap();
        (directory, vec![controller])
    }

    #[tokio::test]
    async fn graphics_input_preflight_and_spawn_failure_remove_owned_artifacts() {
        for mode in ["cancelled", "expired", "spawn"] {
            let (directory, controllers) = graphics_artifacts().await;
            let path = directory.path().to_owned();
            let command = RemoteStreamCommand {
                tool_paths: Vec::new(),
                program: path.join("missing-producer").to_string_lossy().into_owned(),
                args: Vec::new(),
                working_directory: path.clone(),
                environment: BTreeMap::new(),
            };
            let (sender, receiver) = oneshot::channel();
            if mode == "cancelled" {
                sender.send(()).unwrap();
            }
            let deadline = tokio::time::Instant::now()
                + if mode == "expired" {
                    Duration::ZERO
                } else {
                    Duration::from_secs(15)
                };
            let result = PreparedRemoteStreamInput::start_with_graphics(
                &command,
                RemoteStreamSpawnContext::probe(),
                deadline,
                receiver,
                directory,
                controllers,
            )
            .await;
            match mode {
                "cancelled" => assert!(matches!(result, Err(RemoteStreamError::PreparedCancelled))),
                "expired" => assert!(matches!(result, Err(RemoteStreamError::PreparedTimeout))),
                _ => assert!(matches!(
                    result,
                    Err(RemoteStreamError::PreparedProducerFailed)
                )),
            }
            assert!(
                !path.exists(),
                "{mode} must stop controllers and remove files"
            );
        }
    }

    // Unix shell fixtures exercise the shared managed-process owner. The
    // portable preflight test above also covers Windows admission failures.
    #[cfg(unix)]
    #[tokio::test]
    async fn graphics_input_admits_first_output_without_waiting_for_probe_prefix_or_eof() {
        let (directory, controllers) = graphics_artifacts().await;
        let path = directory.path().to_owned();
        let (command, _) = commands("printf N; while :; do :; done");
        let (_sender, receiver) = oneshot::channel();
        let prepared = PreparedRemoteStreamInput::start_with_graphics(
            &command,
            RemoteStreamSpawnContext::probe(),
            tokio::time::Instant::now() + Duration::from_secs(2),
            receiver,
            directory,
            controllers,
        )
        .await
        .expect("one output byte must admit a running canvas producer");
        assert_eq!(prepared.prefix(), b"N");
        assert!(prepared.exited.is_none());
        assert!(path.join("text.txt").exists());
        prepared.cancel().await.unwrap();
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn caption_prefix_admits_one_live_byte_and_never_waits_for_more() {
        let (command, _) = commands("printf N; while :; do sleep 1; done");
        let (_sender, receiver) = oneshot::channel();
        let mut prepared = PreparedRemoteStreamInput::start_caption_prefix(
            &command,
            RemoteStreamSpawnContext::probe(),
            tokio::time::Instant::now() + Duration::from_secs(2),
            receiver,
        )
        .await
        .unwrap();
        assert_eq!(prepared.prefix(), b"N");
        assert!(!prepared.extend_caption_prefix().unwrap());
        assert_eq!(prepared.prefix(), b"N");
        prepared.cancel().await.unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn graphics_input_keeps_files_until_producer_exit_and_transfer_cleanup() {
        for mode in ["eof", "cancel", "disconnect", "shutdown", "drop"] {
            let (directory, controllers) = graphics_artifacts().await;
            let path = directory.path().to_owned();
            let script = if mode == "eof" {
                "cat \"$TMPDIR/text.txt\""
            } else {
                "test -f \"$TMPDIR/text.txt\" || exit 7; head -c 70000 /dev/zero; while :; do :; done"
            };
            let (command, _) = commands(script);
            let (_sender, receiver) = oneshot::channel();
            let prepared = PreparedRemoteStreamInput::start_with_graphics(
                &command,
                RemoteStreamSpawnContext::probe(),
                tokio::time::Instant::now() + Duration::from_secs(15),
                receiver,
                directory,
                controllers,
            )
            .await
            .unwrap();
            assert!(path.join("text.txt").exists());
            assert!(prepared.diagnostics.as_ref().unwrap().suppress_diagnostics);
            let pid = prepared.producer.as_ref().unwrap().id();
            if mode == "cancel" {
                prepared.cancel().await.unwrap();
            } else {
                let (writer, mut reader) = tokio::io::duplex(1);
                let mut transfer = prepared.transfer_to(writer).await.unwrap();
                match mode {
                    "eof" => {
                        let mut bytes = Vec::new();
                        reader.read_to_end(&mut bytes).await.unwrap();
                        transfer.wait().await.unwrap();
                        assert_eq!(bytes, b"canvas");
                    }
                    "disconnect" => {
                        drop(reader);
                        assert!(matches!(
                            transfer.wait().await,
                            Err(RemoteStreamError::PreparedProducerFailed)
                        ));
                    }
                    "shutdown" => {
                        transfer.shutdown().await.unwrap();
                    }
                    _ => {
                        drop(transfer);
                    }
                }
            }
            tokio::time::timeout(Duration::from_secs(2), async {
                while path.exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            if let Some(pid) = pid {
                assert_eq!(
                    nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(i32::try_from(pid).unwrap()),
                        None
                    ),
                    Err(nix::errno::Errno::ESRCH)
                );
            }
        }
    }

    #[tokio::test]
    async fn prepared_input_rejects_cancelled_expired_and_oversized_requests_before_spawn() {
        let command = RemoteStreamCommand {
            tool_paths: Vec::new(),
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
                tool_paths: Vec::new(),
                program: "sh".into(),
                args: vec!["-c".into(), script.into()],
                working_directory: PathBuf::from("."),
                environment: BTreeMap::new(),
            },
            FfmpegCommand {
                mpegts_output: None,
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
    #[allow(
        clippy::await_holding_lock,
        reason = "The process-global capacity test lock covers actual native cleanup before failed-spawn admission is released"
    )]
    async fn prepared_capacity_failed_consumer_spawn_reaps_before_releasing_admission() {
        use crate::transcode_capacity::{
            active_transcode_snapshot, active_transcode_test_guard_for_tests,
            reset_active_transcode_state_for_tests, try_reserve_active_transcode,
            ActiveTranscodeSlot,
        };
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
        let (producer, mut consumer) =
            commands("cat >/dev/null; head -c 70000 /dev/zero; exec sleep 30");
        let (_sender, receiver) = oneshot::channel();
        let mut prepared = PreparedRemoteStreamInput::start(
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
            .to_owned();
        let reservation =
            try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("failed-consumer"))
                .unwrap();
        prepared
            .retain_playback_capacity(reservation.clone())
            .unwrap();
        drop(reservation);
        assert_eq!(active_transcode_snapshot(1).active, 1);
        consumer.executable = directory
            .join("missing-consumer")
            .to_string_lossy()
            .into_owned();
        assert!(matches!(
            spawn_ffmpeg_with_prepared_remote_input(
                &consumer,
                None,
                prepared,
                RemoteStreamSpawnContext::probe()
            )
            .await,
            Err(RemoteStreamError::Spawn(_))
        ));
        assert!(!directory.exists());
        assert_eq!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH)
        );
        assert_eq!(active_transcode_snapshot(1).active, 0);
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(
        clippy::await_holding_lock,
        reason = "The process-global capacity test lock must serialize native child lifetimes through joined cleanup"
    )]
    async fn prepared_live_capacity_follows_consumer_reap_and_quarantines_abandonment() {
        use crate::transcode_capacity::{
            active_transcode_snapshot, active_transcode_test_guard_for_tests,
            reset_active_transcode_state_for_tests, try_reserve_active_transcode,
            ActiveTranscodeSlot,
        };
        let _guard = active_transcode_test_guard_for_tests();
        for abandon in [false, true] {
            reset_active_transcode_state_for_tests();
            let (producer, consumer) =
                commands("cat >/dev/null; head -c 70000 /dev/zero; exec sleep 30");
            let (_sender, receiver) = oneshot::channel();
            let mut prepared = PreparedRemoteStreamInput::start(
                &producer,
                b"{}",
                RemoteStreamSpawnContext::probe(),
                tokio::time::Instant::now() + Duration::from_secs(15),
                receiver,
            )
            .await
            .unwrap();
            let producer_pid =
                i32::try_from(prepared.producer.as_ref().unwrap().id().unwrap()).unwrap();
            let directory = prepared
                .temporary_directory
                .as_ref()
                .unwrap()
                .0
                .as_ref()
                .unwrap()
                .path()
                .to_owned();
            let reservation = try_reserve_active_transcode(
                1,
                ActiveTranscodeSlot::new_mpeg_ts("prepared-consumer"),
            )
            .unwrap();
            prepared
                .retain_playback_capacity(reservation.clone())
                .unwrap();
            drop(reservation);
            let mut process = spawn_ffmpeg_with_prepared_remote_input(
                &consumer,
                None,
                prepared,
                RemoteStreamSpawnContext::probe(),
            )
            .await
            .unwrap();
            assert_eq!(active_transcode_snapshot(1).active, 1);
            let consumer_pid = i32::try_from(process.id().unwrap()).unwrap();
            if !abandon {
                process.kill().await.unwrap();
            }
            drop(process);
            // Abandonment cannot claim that either child has finished.
            assert_eq!(active_transcode_snapshot(1).active, i32::from(abandon));
            tokio::time::timeout(Duration::from_secs(2), async {
                while directory.exists()
                    || [producer_pid, consumer_pid].iter().any(|pid| {
                        nix::sys::signal::kill(nix::unistd::Pid::from_raw(*pid), None)
                            != Err(nix::errno::Errno::ESRCH)
                    })
                {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await
            .unwrap();
            assert_eq!(active_transcode_snapshot(1).active, i32::from(abandon));
        }
        reset_active_transcode_state_for_tests();
    }

    #[cfg(unix)]
    #[tokio::test]
    #[allow(
        clippy::await_holding_lock,
        reason = "The process-global capacity test lock must serialize native child lifetimes through joined cleanup"
    )]
    async fn prepared_input_cancel_and_drop_reap_before_removing_private_directory() {
        use crate::transcode_capacity::{
            active_transcode_snapshot, active_transcode_test_guard_for_tests,
            reset_active_transcode_state_for_tests, try_reserve_active_transcode,
            ActiveTranscodeSlot,
        };
        let _guard = active_transcode_test_guard_for_tests();
        reset_active_transcode_state_for_tests();
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
            let mut prepared = prepared;
            let reservation =
                try_reserve_active_transcode(1, ActiveTranscodeSlot::new_mpeg_ts("prepared-live"))
                    .unwrap();
            prepared
                .retain_playback_capacity(reservation.clone())
                .unwrap();
            assert!(prepared
                .retain_playback_capacity(reservation.clone())
                .is_err());
            drop(reservation);
            assert_eq!(active_transcode_snapshot(1).active, 1);
            if explicit_cancel {
                prepared.cancel().await.unwrap();
            } else {
                drop(prepared);
                // Drop schedules cleanup; it cannot release admission itself.
                assert_eq!(active_transcode_snapshot(1).active, 1);
                assert!(try_reserve_active_transcode(
                    1,
                    ActiveTranscodeSlot::new_mpeg_ts("next-live")
                )
                .is_none());
            }
            tokio::time::timeout(Duration::from_secs(2), async {
                while directory.exists() || active_transcode_snapshot(1).active != 0 {
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
    async fn managed_sink_preserves_bytes_and_does_not_invent_a_request() {
        let (producer, _) = commands("cat; printf tail");
        let (_sender, receiver) = oneshot::channel();
        let prepared = PreparedRemoteStreamInput::start_without_request(
            &producer,
            RemoteStreamSpawnContext::probe(),
            tokio::time::Instant::now() + Duration::from_secs(15),
            receiver,
        )
        .await
        .unwrap();
        assert_eq!(prepared.prefix(), b"tail");
        let directory = prepared
            .temporary_directory
            .as_ref()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .path()
            .to_path_buf();
        let (writer, mut reader) = tokio::io::duplex(2);
        let mut transfer = prepared.transfer_to(writer).await.unwrap();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await.unwrap();
        transfer.wait().await.unwrap();
        assert_eq!(bytes, b"tail");
        assert!(!directory.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_sink_preserves_prefix_and_detects_late_producer_failure() {
        for exit_code in [0, 7] {
            let (producer, _) = commands(&format!(
                "cat >/dev/null; head -c 70000 /dev/zero; printf tail; exit {exit_code}"
            ));
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
            let (writer, mut reader) = tokio::io::duplex(1024);
            let mut transfer = prepared.transfer_to(writer).await.unwrap();
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await.unwrap();
            let outcome = transfer.wait().await;
            assert_eq!(outcome.is_ok(), exit_code == 0);
            assert_eq!(bytes.len(), 70004);
            assert!(bytes[..70000].iter().all(|byte| *byte == 0));
            assert_eq!(&bytes[70000..], b"tail");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_sink_shutdown_drop_and_disconnected_reader_reap_producer() {
        for mode in ["shutdown", "drop", "disconnect"] {
            let (producer, _) =
                commands("cat >/dev/null; head -c 70000 /dev/zero; while :; do :; done");
            let (_sender, receiver) = oneshot::channel();
            let prepared = PreparedRemoteStreamInput::start_without_request(
                &producer,
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
            let (writer, reader) = tokio::io::duplex(1);
            let mut transfer = prepared.transfer_to(writer).await.unwrap();
            match mode {
                "shutdown" => {
                    // The sink is blocked; a timed-out wait must retain ownership.
                    assert!(
                        tokio::time::timeout(Duration::from_millis(10), transfer.wait())
                            .await
                            .is_err()
                    );
                    transfer.shutdown().await.unwrap();
                    drop(reader);
                }
                "disconnect" => {
                    drop(reader);
                    assert!(matches!(
                        transfer.wait().await,
                        Err(RemoteStreamError::PreparedProducerFailed)
                    ));
                }
                _ => {
                    drop(transfer);
                    drop(reader);
                }
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
                Err(nix::errno::Errno::ESRCH),
            );
        }
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

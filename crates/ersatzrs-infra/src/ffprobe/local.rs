//! Local probe ownership without changing ordinary arguments or scan deadlines.
//!
//! The independent worker owns the managed process tree and both readers until
//! completion. Caller loss requests cancellation through response-channel
//! closure. Ordinary probes have no total timeout; optional auxiliary probes
//! may supply an absolute deadline. Cleanup and pipe shutdown always have fresh
//! bounded budgets. Operation scopes track these workers for managed
//! coordinator draining; this module owns no scanner lease itself.
//! Managed process groups/jobs do not contain deliberately detached processes,
//! and runtime teardown cannot await an asynchronous drop fallback.

use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::local_cleanup_scope::{self, AdmissionError, OwnerGuard};
use super::{parse_ffprobe_json, FfprobeCommand, FfprobeError, FfprobeMediaVersion};
use crate::managed_process::ManagedProcess;

const MAX_LOCAL_STDOUT_BYTES: usize = super::MAX_REMOTE_FFPROBE_OUTPUT_BYTES;
const MAX_LOCAL_STDERR_TAIL_BYTES: usize = 16 * 1024;
#[cfg(test)]
const MAX_DIAGNOSTIC_STDERR_BYTES: usize = MAX_LOCAL_STDOUT_BYTES;

#[derive(Clone, Copy)]
enum StderrCapture {
    Tail,
    #[cfg(test)]
    Complete,
}
// The managed group/job confirmation already has a five-second inner budget.
const PROCESS_CLEANUP_TIMEOUT: Duration = Duration::from_secs(6);
const READER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

type Response = oneshot::Sender<super::Result<LocalProbeOutput>>;

struct LocalProbeOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: String,
}

/// Successful complete native-test diagnostics; no partial trace is returned.
#[cfg(test)]
#[derive(Debug)]
pub(super) struct LocalProbeDiagnostics {
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
enum LocalProbeProblem {
    #[error("local ffprobe stdout exceeded its size limit")]
    OutputTooLarge,
    #[error("local ffprobe stdout could not be read")]
    StdoutRead,
    #[error("local ffprobe stderr could not be read")]
    StderrRead,
    #[error("local ffprobe output reader failed")]
    ReaderTask,
    #[error("local ffprobe output pipes did not close")]
    ReaderShutdown,
    #[error("local ffprobe process wait failed")]
    ProcessWait,
    #[error("local ffprobe process cleanup could not be confirmed")]
    CleanupUnconfirmed,
    #[error("local ffprobe output pipe unavailable")]
    PipeUnavailable,
    #[error("local ffprobe caller cancelled")]
    Cancelled,
    #[error("local ffprobe operation deadline expired")]
    Timeout,
}

impl LocalProbeProblem {
    fn into_error(self, path: String) -> FfprobeError {
        match self {
            Self::OutputTooLarge => FfprobeError::OutputTooLarge { path },
            Self::CleanupUnconfirmed => FfprobeError::CleanupUnconfirmed { path },
            Self::Cancelled => FfprobeError::Cancelled { path },
            Self::Timeout => FfprobeError::Timeout { path },
            _ => FfprobeError::Spawn {
                path,
                source: io::Error::other(self),
            },
        }
    }
}

pub(super) async fn extract(
    command: FfprobeCommand,
    path: PathBuf,
) -> super::Result<FfprobeMediaVersion> {
    let stdout = capture(command, path.clone(), None).await?;
    parse_ffprobe_json(&path, &stdout).map_err(|source| FfprobeError::Json {
        path: path.display().to_string(),
        source,
    })
}

pub(super) async fn capture(
    command: FfprobeCommand,
    path: PathBuf,
    deadline: Option<Instant>,
) -> super::Result<Vec<u8>> {
    capture_retained(command, path, deadline, None).await
}

/// The actual process owner retains admission and the source association through
/// process and pipe cleanup, independently of the awaiting response future.
pub(super) struct ProbeRetention {
    // Field destruction must close/remove the actual input before releasing
    // admission, including the confirmed-cleanup path through ordinary Drop.
    snapshot: Option<tempfile::NamedTempFile>,
    _source: std::sync::Arc<super::LocalTeletextSourceGuard>,
    hold: ersatzrs_core::locking::TeletextWorkHold,
    confirmed: bool,
}

impl ProbeRetention {
    pub(super) fn new(
        hold: ersatzrs_core::locking::TeletextWorkHold,
        source: std::sync::Arc<super::LocalTeletextSourceGuard>,
    ) -> Self {
        Self {
            hold,
            _source: source,
            snapshot: None,
            confirmed: true,
        }
    }

    pub(super) fn with_snapshot(mut self, snapshot: tempfile::NamedTempFile) -> Self {
        self.snapshot = Some(snapshot);
        self
    }
}

impl Drop for ProbeRetention {
    fn drop(&mut self) {
        if !self.confirmed {
            self.hold.retain_until_restart();
            // An unconfirmed process may still be opening/reading this file.
            // Preserve its bounded private input rather than revoke it early.
            if let Some(snapshot) = self.snapshot.take() {
                let mut path = snapshot.into_temp_path();
                path.disable_cleanup(true);
                tracing::warn!("unconfirmed source teletext decoder input retained");
            }
        }
    }
}

pub(super) async fn capture_retained(
    command: FfprobeCommand,
    path: PathBuf,
    deadline: Option<Instant>,
    retention: Option<ProbeRetention>,
) -> super::Result<Vec<u8>> {
    capture_retained_output(command, path, deadline, retention, StderrCapture::Tail)
        .await
        .map(|output| output.stdout)
}

/// Capture successful native-test stdout and complete UTF-8 diagnostics under
/// the same actual worker, source retention and operation scope as probes.
/// Stderr is bounded to eight MiB; overflow or invalid UTF-8 fails the whole
/// operation instead of silently trimming or replacing diagnostic events.
#[cfg(test)]
pub(super) async fn capture_retained_diagnostics(
    command: FfprobeCommand,
    path: PathBuf,
    deadline: Option<Instant>,
    retention: Option<ProbeRetention>,
) -> super::Result<LocalProbeDiagnostics> {
    let output =
        capture_retained_output(command, path, deadline, retention, StderrCapture::Complete)
            .await?;
    Ok(LocalProbeDiagnostics {
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

async fn capture_retained_output(
    command: FfprobeCommand,
    path: PathBuf,
    deadline: Option<Instant>,
    retention: Option<ProbeRetention>,
    stderr_capture: StderrCapture,
) -> super::Result<LocalProbeOutput> {
    let label = path.display().to_string();
    let owner = local_cleanup_scope::register_current().map_err(|problem| match problem {
        AdmissionError::Closed => FfprobeError::Cancelled {
            path: label.clone(),
        },
        AdmissionError::Unconfirmed => FfprobeError::CleanupUnconfirmed {
            path: label.clone(),
        },
        AdmissionError::Capacity => FfprobeError::Spawn {
            path: label.clone(),
            source: io::Error::other("local ffprobe cleanup scope capacity is busy"),
        },
    })?;
    let (mut response, receiver) = oneshot::channel();
    let owner_label = label.clone();
    // This worker intentionally outlives an abandoned caller only to perform
    // cleanup. No metadata publication or database write occurs in the worker.
    tokio::spawn(async move {
        let mut retention = retention;
        if let Some(retention) = &mut retention {
            retention.confirmed = false;
        }
        let result = run_owned_scoped(
            command,
            &owner_label,
            &mut response,
            owner.as_ref(),
            deadline,
            stderr_capture,
        )
        .await;
        let confirmed = !matches!(result, Err(FfprobeError::CleanupUnconfirmed { .. }));
        if let Some(retention) = &mut retention {
            retention.confirmed = confirmed;
        }
        // Dispose the completed temporary input before reporting the owner idle.
        // Its admitted-work hold survives this actual file cleanup as well.
        drop(retention);
        if let Some(owner) = owner {
            owner.complete(confirmed);
        }
        if !confirmed {
            tracing::warn!("local ffprobe process cleanup could not be confirmed");
        }
        let _ = response.send(result);
    });
    let output = receiver
        .await
        .map_err(|_| FfprobeError::CleanupUnconfirmed {
            path: label.clone(),
        })??;
    if !output.status.success() {
        return Err(FfprobeError::Process {
            path: label,
            status: output.status,
            stderr: output.stderr,
        });
    }
    Ok(output)
}

#[cfg(test)]
async fn run_owned(
    command: FfprobeCommand,
    label: &str,
    response: &mut Response,
) -> super::Result<LocalProbeOutput> {
    run_owned_scoped(command, label, response, None, None, StderrCapture::Tail).await
}

async fn run_owned_scoped(
    command: FfprobeCommand,
    label: &str,
    response: &mut Response,
    owner: Option<&OwnerGuard>,
    deadline: Option<Instant>,
    stderr_capture: StderrCapture,
) -> super::Result<LocalProbeOutput> {
    // Channel closure is level triggered, including caller loss before the
    // task's first poll; no cancellation notification registration gap exists.
    if response.is_closed() || owner.is_some_and(OwnerGuard::cancellation_requested) {
        return Err(LocalProbeProblem::Cancelled.into_error(label.to_owned()));
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(LocalProbeProblem::Timeout.into_error(label.to_owned()));
    }
    let mut command_builder = Command::new(command.program);
    command_builder
        .args(command.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child =
        ManagedProcess::spawn(command_builder).map_err(|source| FfprobeError::Spawn {
            path: label.to_owned(),
            source,
        })?;
    let stdout = child.take_stdout();
    let stderr = child.take_stderr();
    let (Some(stdout), Some(stderr)) = (stdout, stderr) else {
        let problem = match terminate(&mut child).await {
            Ok(()) => LocalProbeProblem::PipeUnavailable,
            Err(problem) => problem,
        };
        return Err(problem.into_error(label.to_owned()));
    };
    run_process(
        child,
        ProbeReaders::spawn(stdout, stderr, stderr_capture),
        response,
        owner,
        deadline,
    )
    .await
    .map_err(|problem| problem.into_error(label.to_owned()))
}

async fn run_process(
    mut child: ManagedProcess,
    mut readers: ProbeReaders,
    response: &mut Response,
    owner: Option<&OwnerGuard>,
    deadline: Option<Instant>,
) -> Result<LocalProbeOutput, LocalProbeProblem> {
    let process_result = loop {
        tokio::select! {
            biased;
            () = response.closed() => break Err(LocalProbeProblem::Cancelled),
            () = scope_cancelled(owner) => break Err(LocalProbeProblem::Cancelled),
            () = operation_expired(deadline) => break Err(LocalProbeProblem::Timeout),
            result = await_reader(&mut readers.stdout_task), if readers.stdout_task.is_some() => {
                if let Err(problem) = readers.accept_stdout(result) {
                    break Err(problem);
                }
            }
            result = await_reader(&mut readers.stderr_task), if readers.stderr_task.is_some() => {
                if let Err(problem) = readers.accept_stderr(result) {
                    break Err(problem);
                }
            }
            result = child.wait() => break result.map_err(|_| LocalProbeProblem::ProcessWait),
        }
    };
    // Cleanup has its own fresh budget. Caller loss and overflow must not turn
    // an expired operation budget into an unawaited process kill.
    let cleanup_result = if process_result.is_err() {
        terminate(&mut child).await
    } else {
        Ok(()) // ManagedProcess::wait already confirms the entire tree's exit.
    };
    let reader_result = readers.collect().await;
    cleanup_result?;
    let status = process_result?;
    let (stdout, stderr) = reader_result?;
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(LocalProbeProblem::Timeout);
    }
    Ok(LocalProbeOutput {
        status,
        stdout,
        stderr,
    })
}

async fn operation_expired(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

async fn scope_cancelled(owner: Option<&OwnerGuard>) {
    match owner {
        Some(owner) => owner.cancelled().await,
        None => std::future::pending().await,
    }
}

async fn terminate(child: &mut ManagedProcess) -> Result<(), LocalProbeProblem> {
    match tokio::time::timeout(PROCESS_CLEANUP_TIMEOUT, child.kill()).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) | Err(_) => Err(LocalProbeProblem::CleanupUnconfirmed),
    }
}

type ReaderResult<T> = Result<Result<T, LocalProbeProblem>, tokio::task::JoinError>;

struct ProbeReaders {
    stdout_task: Option<JoinHandle<Result<Vec<u8>, LocalProbeProblem>>>,
    stderr_task: Option<JoinHandle<Result<String, LocalProbeProblem>>>,
    stdout: Option<Result<Vec<u8>, LocalProbeProblem>>,
    stderr: Option<Result<String, LocalProbeProblem>>,
}

impl ProbeReaders {
    fn spawn(
        stdout: impl AsyncRead + Unpin + Send + 'static,
        stderr: impl AsyncRead + Unpin + Send + 'static,
        stderr_capture: StderrCapture,
    ) -> Self {
        Self {
            stdout_task: Some(tokio::spawn(read_stdout(stdout))),
            stderr_task: Some(tokio::spawn(read_stderr(stderr, stderr_capture))),
            stdout: None,
            stderr: None,
        }
    }

    fn accept_stdout(&mut self, result: ReaderResult<Vec<u8>>) -> Result<(), LocalProbeProblem> {
        self.stdout_task.take();
        let result = result.map_err(|_| LocalProbeProblem::ReaderTask).flatten();
        let problem = result.as_ref().err().copied();
        self.stdout = Some(result);
        problem.map_or(Ok(()), Err)
    }

    fn accept_stderr(&mut self, result: ReaderResult<String>) -> Result<(), LocalProbeProblem> {
        self.stderr_task.take();
        let result = result.map_err(|_| LocalProbeProblem::ReaderTask).flatten();
        let problem = result.as_ref().err().copied();
        self.stderr = Some(result);
        problem.map_or(Ok(()), Err)
    }

    async fn collect(&mut self) -> Result<(Vec<u8>, String), LocalProbeProblem> {
        let collected = tokio::time::timeout(READER_SHUTDOWN_TIMEOUT, async {
            if self.stdout_task.is_some() {
                let result = await_reader(&mut self.stdout_task).await;
                // Record completion before the next await, even on failure.
                let _ = self.accept_stdout(result);
            }
            if self.stderr_task.is_some() {
                let result = await_reader(&mut self.stderr_task).await;
                let _ = self.accept_stderr(result);
            }
        })
        .await;
        if collected.is_err() {
            self.abort_and_join().await;
            return Err(LocalProbeProblem::ReaderShutdown);
        }
        let stdout = self.stdout.take().ok_or(LocalProbeProblem::ReaderTask)??;
        let stderr = self.stderr.take().ok_or(LocalProbeProblem::ReaderTask)??;
        Ok((stdout, stderr))
    }

    async fn abort_and_join(&mut self) {
        if let Some(task) = self.stdout_task.take() {
            task.abort();
            let _ = task.await;
        }
        if let Some(task) = self.stderr_task.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for ProbeReaders {
    fn drop(&mut self) {
        // Exceptional owner unwind/runtime teardown cannot await. Normal,
        // cancelled and failed operations always collect or abort-and-join.
        if let Some(task) = &self.stdout_task {
            task.abort();
        }
        if let Some(task) = &self.stderr_task {
            task.abort();
        }
    }
}

async fn await_reader<T>(task: &mut Option<JoinHandle<T>>) -> Result<T, tokio::task::JoinError> {
    match task {
        Some(task) => task.await,
        None => std::future::pending().await,
    }
}

async fn read_stdout(mut stdout: impl AsyncRead + Unpin) -> Result<Vec<u8>, LocalProbeProblem> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let length = stdout
            .read(&mut chunk)
            .await
            .map_err(|_| LocalProbeProblem::StdoutRead)?;
        if length == 0 {
            return Ok(bytes);
        }
        if bytes.len() + length > MAX_LOCAL_STDOUT_BYTES {
            return Err(LocalProbeProblem::OutputTooLarge);
        }
        bytes.extend_from_slice(&chunk[..length]);
    }
}

async fn read_stderr(
    stderr: impl AsyncRead + Unpin,
    capture: StderrCapture,
) -> Result<String, LocalProbeProblem> {
    match capture {
        StderrCapture::Tail => read_stderr_tail(stderr).await,
        #[cfg(test)]
        StderrCapture::Complete => read_complete_stderr(stderr).await,
    }
}

#[cfg(test)]
async fn read_complete_stderr(
    mut stderr: impl AsyncRead + Unpin,
) -> Result<String, LocalProbeProblem> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let length = stderr
            .read(&mut chunk)
            .await
            .map_err(|_| LocalProbeProblem::StderrRead)?;
        if length == 0 {
            return String::from_utf8(bytes).map_err(|_| LocalProbeProblem::StderrRead);
        }
        if bytes.len() + length > MAX_DIAGNOSTIC_STDERR_BYTES {
            return Err(LocalProbeProblem::OutputTooLarge);
        }
        bytes.extend_from_slice(&chunk[..length]);
    }
}

async fn read_stderr_tail(mut stderr: impl AsyncRead + Unpin) -> Result<String, LocalProbeProblem> {
    let mut bytes = VecDeque::<u8>::with_capacity(MAX_LOCAL_STDERR_TAIL_BYTES);
    let mut chunk = [0; 4096];
    loop {
        let length = stderr
            .read(&mut chunk)
            .await
            .map_err(|_| LocalProbeProblem::StderrRead)?;
        if length == 0 {
            return Ok(
                String::from_utf8_lossy(&bytes.into_iter().collect::<Vec<_>>()).into_owned(),
            );
        }
        let overflow = (bytes.len() + length).saturating_sub(MAX_LOCAL_STDERR_TAIL_BYTES);
        bytes.drain(..overflow);
        bytes.extend(&chunk[..length]);
    }
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;

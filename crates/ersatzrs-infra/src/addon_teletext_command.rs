//! Bounded owned teletext command execution, before installed-package admission.
//!
//! The caller supplies a host-validated resolved command. This primitive neither
//! resolves nor activates a capability, validates programme semantics, parses
//! producer output, or authorizes publication. A worker owns the managed process
//! and all pipe tasks even after the caller abandons its response future.

use std::time::Duration;

use ersatzrs_addon_contract::{TeletextPagesRequest, MAX_TELETEXT_PAGES_INPUT_BYTES};
use ersatzrs_domain::TeletextPageContract;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::oneshot,
    task::JoinHandle,
    time::Instant,
};

use super::{tool_execution, ManagedAddonChild, ResolvedAddonCommand};

const EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
const PROCESS_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const PIPE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_STDOUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 16 * 1024;

/// Static command failures without producer text, settings, paths or guide data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextCommandError {
    #[error("teletext command request is invalid")]
    InvalidRequest,
    #[error("teletext command was cancelled")]
    Cancelled,
    #[error("teletext command exceeded its execution deadline")]
    TimedOut,
    #[error("teletext command could not be started")]
    SpawnFailed,
    #[error("teletext command pipe or process wait failed")]
    IoFailed,
    #[error("teletext command stdout exceeded its byte limit")]
    OutputTooLarge,
    #[error("teletext command stderr exceeded its byte limit")]
    StderrTooLarge,
    #[error("teletext command exited unsuccessfully")]
    Failed,
    #[error("teletext command cleanup could not be confirmed")]
    CleanupUnconfirmed,
}

/// Execute an already resolved host command with one strict bounded v1 guide request.
///
/// The absolute 30-second deadline begins when first polled, includes preflight,
/// stdin and complete process/output handling, and never restarts between stages.
/// The 4 MiB request cap and strict wire parse precede spawn. Output retains at
/// most 8 MiB stdout and 16 KiB stderr; overflow fails as soon as observed. Stderr
/// is discarded without interpreting or returning producer diagnostics.
///
/// Cancellation, including dropping its sender or this caller future, leaves
/// the owned worker to kill/reap the managed group/job and abort/join pipe tasks.
/// Cleanup has separate five-second process and one-second pipe budgets; uncertain
/// cleanup remains distinct from ordinary failure. Existing native-tool lease
/// quarantine semantics remain owned by `ManagedAddonChild`.
///
/// # Errors
/// Returns a static request/process/pipe/deadline/cancellation failure, or distinct
/// cleanup uncertainty. Successful raw bytes still need host response validation.
pub async fn run(
    command: ResolvedAddonCommand,
    input: Vec<u8>,
    cancellation: Option<oneshot::Receiver<()>>,
) -> Result<Vec<u8>, TeletextCommandError> {
    run_with_deadline(
        command,
        input,
        cancellation,
        Instant::now() + EXECUTION_TIMEOUT,
    )
    .await
}

pub(super) async fn run_with_deadline(
    command: ResolvedAddonCommand,
    input: Vec<u8>,
    cancellation: Option<oneshot::Receiver<()>>,
    deadline: Instant,
) -> Result<Vec<u8>, TeletextCommandError> {
    run_with_contract_deadline(
        command,
        input,
        cancellation,
        deadline,
        TeletextPageContract::V1,
    )
    .await
}

/// Execute only the contract already selected by the host's profile snapshot.
/// Neither request nor response contents negotiate a different capability.
pub(super) async fn run_with_contract_deadline(
    command: ResolvedAddonCommand,
    input: Vec<u8>,
    cancellation: Option<oneshot::Receiver<()>>,
    deadline: Instant,
    contract: TeletextPageContract,
) -> Result<Vec<u8>, TeletextCommandError> {
    if input.len() > MAX_TELETEXT_PAGES_INPUT_BYTES
        || TeletextPagesRequest::from_json_for_schema(&input, contract.schema_version()).is_err()
    {
        return Err(TeletextCommandError::InvalidRequest);
    }
    let (response, result) = oneshot::channel();
    tokio::spawn(async move {
        let mut response = response;
        let outcome = run_owned(
            command,
            input,
            cancellation,
            deadline,
            contract,
            &mut response,
        )
        .await;
        let _ = response.send(outcome);
    });
    result
        .await
        .unwrap_or(Err(TeletextCommandError::CleanupUnconfirmed))
}

type PipeTask = JoinHandle<Result<Vec<u8>, PipeFailure>>;
type CommandResponse = oneshot::Sender<Result<Vec<u8>, TeletextCommandError>>;

#[derive(Debug, Clone, Copy)]
enum PipeFailure {
    Io,
    Overflow,
}

#[derive(Default)]
struct PipeTasks {
    stdin: Option<PipeTask>,
    stdout: Option<PipeTask>,
    stderr: Option<PipeTask>,
}

impl PipeTasks {
    fn abort(&self) {
        for task in [&self.stdin, &self.stdout, &self.stderr]
            .into_iter()
            .flatten()
        {
            task.abort();
        }
    }

    async fn join(&mut self) {
        tokio::join!(
            join_slot(&mut self.stdin),
            join_slot(&mut self.stdout),
            join_slot(&mut self.stderr)
        );
    }
}

// Any unexpected unwind still requests cancellation of all owned pipe tasks.
impl Drop for PipeTasks {
    fn drop(&mut self) {
        self.abort();
    }
}

async fn join_slot(slot: &mut Option<PipeTask>) {
    if let Some(task) = slot.as_mut() {
        let _ = task.await;
    }
    *slot = None;
}

async fn poll_slot(
    slot: &mut Option<PipeTask>,
) -> Result<Result<Vec<u8>, PipeFailure>, tokio::task::JoinError> {
    if let Some(task) = slot.as_mut() {
        task.await
    } else {
        std::future::pending().await
    }
}

async fn cancelled(cancellation: &mut Option<oneshot::Receiver<()>>) {
    if let Some(cancellation) = cancellation {
        let _ = cancellation.await;
    } else {
        std::future::pending::<()>().await;
    }
}

fn cancellation_requested(cancellation: &mut Option<oneshot::Receiver<()>>) -> bool {
    cancellation.as_mut().is_some_and(|receiver| {
        matches!(
            receiver.try_recv(),
            Ok(()) | Err(oneshot::error::TryRecvError::Closed)
        )
    })
}

async fn run_owned(
    mut command: ResolvedAddonCommand,
    input: Vec<u8>,
    mut cancellation: Option<oneshot::Receiver<()>>,
    deadline: Instant,
    contract: TeletextPageContract,
    response: &mut CommandResponse,
) -> Result<Vec<u8>, TeletextCommandError> {
    if response.is_closed() || cancellation_requested(&mut cancellation) {
        return Err(TeletextCommandError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(TeletextCommandError::TimedOut);
    }
    command.environment.insert(
        "ERSATZRS_ADDON_CAPABILITY".to_owned(),
        contract.capability().to_owned(),
    );
    let mut child = tool_execution::spawn_bounded_addon_process(
        &command,
        true,
        crate::addon_tool_gate::ToolLeaseMode::Use,
    )
    .map_err(|error| match error {
        super::AddonError::ToolCleanupRequired | super::AddonError::TerminationUnconfirmed => {
            TeletextCommandError::CleanupUnconfirmed
        }
        _ => TeletextCommandError::SpawnFailed,
    })?;
    let mut tasks = PipeTasks::default();
    let pipes = (
        child.stdin().take(),
        child.stdout().take(),
        child.stderr().take(),
    );
    let (Some(mut stdin), Some(stdout), Some(stderr)) = pipes else {
        return cleaned_result(
            &mut child,
            &mut tasks,
            false,
            Err(TeletextCommandError::IoFailed),
        )
        .await;
    };
    tasks.stdin = Some(tokio::spawn(async move {
        stdin.write_all(&input).await.map_err(|_| PipeFailure::Io)?;
        stdin.shutdown().await.map_err(|_| PipeFailure::Io)?;
        Ok(Vec::new())
    }));
    tasks.stdout = Some(tokio::spawn(read_bounded(stdout, MAX_STDOUT_BYTES)));
    tasks.stderr = Some(tokio::spawn(read_bounded(stderr, MAX_STDERR_BYTES)));
    let (outcome, process_done) = supervise(
        &mut child,
        &mut tasks,
        &mut cancellation,
        deadline,
        response,
    )
    .await;
    cleaned_result(&mut child, &mut tasks, process_done, outcome).await
}

async fn supervise(
    child: &mut ManagedAddonChild,
    tasks: &mut PipeTasks,
    cancellation: &mut Option<oneshot::Receiver<()>>,
    deadline: Instant,
    response: &mut CommandResponse,
) -> (Result<Vec<u8>, TeletextCommandError>, bool) {
    let mut process_done = false;
    let mut stdout = Vec::new();
    loop {
        let error = tokio::select! {
            biased;
            () = response.closed() => Some(TeletextCommandError::Cancelled),
            () = cancelled(cancellation) => Some(TeletextCommandError::Cancelled),
            () = tokio::time::sleep_until(deadline) => Some(TeletextCommandError::TimedOut),
            result = poll_slot(&mut tasks.stdout), if tasks.stdout.is_some() => {
                tasks.stdout = None;
                match result {
                    Ok(Ok(bytes)) => { stdout = bytes; None }
                    Ok(Err(PipeFailure::Overflow)) => Some(TeletextCommandError::OutputTooLarge),
                    _ => Some(TeletextCommandError::IoFailed),
                }
            }
            result = poll_slot(&mut tasks.stderr), if tasks.stderr.is_some() => {
                tasks.stderr = None;
                match result {
                    Ok(Ok(_)) => None,
                    Ok(Err(PipeFailure::Overflow)) => Some(TeletextCommandError::StderrTooLarge),
                    _ => Some(TeletextCommandError::IoFailed),
                }
            }
            result = poll_slot(&mut tasks.stdin), if tasks.stdin.is_some() => {
                tasks.stdin = None;
                match result {
                    Ok(Ok(_)) => None,
                    _ => Some(TeletextCommandError::IoFailed),
                }
            }
            result = child.wait(), if !process_done => {
                match result {
                    Ok(status) => {
                        process_done = true;
                        if status.success() { None } else { Some(TeletextCommandError::Failed) }
                    }
                    Err(_) => Some(TeletextCommandError::IoFailed),
                }
            }
        };
        if let Some(error) = error {
            return (Err(error), process_done);
        }
        if process_done && tasks.stdin.is_none() && tasks.stdout.is_none() && tasks.stderr.is_none()
        {
            return (Ok(stdout), true);
        }
    }
}

async fn cleaned_result(
    child: &mut ManagedAddonChild,
    tasks: &mut PipeTasks,
    process_done: bool,
    outcome: Result<Vec<u8>, TeletextCommandError>,
) -> Result<Vec<u8>, TeletextCommandError> {
    let process_confirmed = process_done
        || tokio::time::timeout(PROCESS_CLEANUP_TIMEOUT, child.kill())
            .await
            .is_ok_and(|result| result.is_ok());
    tasks.abort();
    let pipes_confirmed = tokio::time::timeout(PIPE_CLEANUP_TIMEOUT, tasks.join())
        .await
        .is_ok();
    if !pipes_confirmed {
        // These tasks only perform bounded native-pipe I/O. They have already
        // been aborted; join cancellation before the owner releases handles,
        // retaining uncertainty if the cleanup budget was exceeded.
        tasks.abort();
        tasks.join().await;
    }
    if process_confirmed && pipes_confirmed {
        outcome
    } else {
        Err(TeletextCommandError::CleanupUnconfirmed)
    }
}

async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> Result<Vec<u8>, PipeFailure> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = reader
            .read(&mut buffer)
            .await
            .map_err(|_| PipeFailure::Io)?;
        if count == 0 {
            return Ok(output);
        }
        if count > limit - output.len() {
            return Err(PipeFailure::Overflow);
        }
        output.extend_from_slice(&buffer[..count]);
    }
}

#[cfg(test)]
#[path = "addon_teletext_command_tests.rs"]
mod tests;

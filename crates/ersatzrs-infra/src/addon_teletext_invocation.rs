//! Preparatory installed-package teletext invocation, without service activation.
//!
//! This adapter supports global settings or an explicitly supplied, revision-fenced
//! base channel/profile selection. It owns resolution through native filesystem settlement, then owns
//! command cancellation through cleanup. The receipt is a revision observation,
//! not immutable package bytes, a validated page set or publication authority.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use ersatzrs_addon_contract::{TeletextPagesRequest, MAX_TELETEXT_PAGES_INPUT_BYTES};
use ersatzrs_core::repositories::{TeletextProfileMutation, TeletextProfileRepository as _};
use ersatzrs_domain::TeletextSelectionSnapshot;
use sqlx::Row;
use tokio::{sync::oneshot, time::Instant};

use super::{
    teletext_command::{self, TeletextCommandError},
    AddonError, AddonRuntimeContext, ResolvedAddonCommand,
};
use crate::db::DbPool;

const EXECUTION_TIMEOUT: Duration = Duration::from_secs(30);
const RESOLUTION_SETTLEMENT_TIMEOUT: Duration = Duration::from_secs(5);

/// Raw producer output and the installed revision observed immediately prelaunch.
/// Neither member grants promotion or certifies immutable package provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextInvocationReceipt {
    pub stdout: Vec<u8>,
    pub state_revision: String,
}

/// Unpublished producer output associated with a checked base configuration.
/// Generation is request correlation, not channel ownership or promotion authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextProfileInvocationReceipt {
    pub stdout: Vec<u8>,
    pub selection: TeletextSelectionSnapshot,
    pub generation: String,
}

impl std::fmt::Debug for TeletextProfileInvocationReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeletextProfileInvocationReceipt")
            .field("stdout_bytes", &self.stdout.len())
            .field("selection", &self.selection)
            .field("generation", &"[redacted]")
            .finish()
    }
}

#[derive(PartialEq, Eq)]
struct InvocationReceipt {
    stdout: Vec<u8>,
    state_revision: String,
    selection: Option<TeletextSelectionSnapshot>,
    generation: String,
}

impl std::fmt::Debug for InvocationReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InvocationReceipt")
            .field("stdout_bytes", &self.stdout.len())
            .field("selection", &self.selection)
            .field("generation", &"[redacted]")
            .finish_non_exhaustive()
    }
}

/// Static failures; producer diagnostics and guide/configuration values are absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextInvocationError {
    #[error("teletext invocation request is invalid")]
    InvalidRequest,
    #[error("teletext installed-package admission was rejected")]
    AdmissionRejected,
    #[error("teletext invocation was cancelled")]
    Cancelled,
    #[error("teletext invocation exceeded its execution deadline")]
    TimedOut,
    #[error("teletext invocation cleanup could not be confirmed")]
    CleanupUnconfirmed,
    #[error(transparent)]
    Command(TeletextCommandError),
}

/// Invoke the fixed `teletext.pages.v1/pages` operation of an enabled package.
///
/// A single absolute 30-second deadline begins before strict 4 MiB request
/// preflight. Package compatibility, dependencies, content hash, contained
/// platform entrypoint and existing global settings are checked, then enabled
/// status and the exact state revision are reloaded before command launch.
/// Generic invocation remains unable to dispatch this preparatory capability.
///
/// Cancellation or caller abandonment signals cooperative filesystem fences;
/// the owned resolver is never dropped while its blocking job is pending.
/// Native calls are not interruptible. After five seconds of settlement an
/// uncertain result may return, but the worker retains and joins that job and
/// cannot start a later stage or command. Command execution retains its separate
/// existing cleanup budgets. No database connection is supplied to the producer.
/// Profile overrides and channel/configuration/generation promotion remain open.
///
/// # Errors
/// Returns static request, admission, cancellation, deadline or command failures,
/// preserving cleanup uncertainty. Raw output needs application validation.
pub async fn invoke(
    pool: DbPool,
    addon_id: String,
    runtime: AddonRuntimeContext,
    input: Vec<u8>,
    cancellation: Option<oneshot::Receiver<()>>,
) -> Result<TeletextInvocationReceipt, TeletextInvocationError> {
    let fence = ResolutionFence::new(Instant::now() + EXECUTION_TIMEOUT);
    invoke_with_fence(
        Invocation {
            pool,
            addon_id,
            runtime,
            input,
            cancellation,
            selection: None,
        },
        fence,
        RESOLUTION_SETTLEMENT_TIMEOUT,
    )
    .await
    .map(|receipt| TeletextInvocationReceipt {
        stdout: receipt.stdout,
        state_revision: receipt.state_revision,
    })
}

/// Invoke using one enabled base channel binding and its effective settings.
///
/// The selection is re-read before filesystem/tool resolution and immediately
/// prelaunch. Supplied request guide channels may differ from the output channel;
/// generation remains exact opaque correlation. No settings or enablement change
/// occurs here, and the receipt cannot authorize later publication.
///
/// # Errors
/// Returns static failures and preserves owned resolution/process cleanup uncertainty.
pub async fn invoke_profile(
    pool: DbPool,
    selection: TeletextSelectionSnapshot,
    runtime: AddonRuntimeContext,
    input: Vec<u8>,
    cancellation: Option<oneshot::Receiver<()>>,
) -> Result<TeletextProfileInvocationReceipt, TeletextInvocationError> {
    let fence = ResolutionFence::new(Instant::now() + EXECUTION_TIMEOUT);
    let receipt = invoke_with_fence(
        Invocation {
            pool,
            addon_id: selection.addon_id.clone(),
            runtime,
            input,
            cancellation,
            selection: Some(selection),
        },
        fence,
        RESOLUTION_SETTLEMENT_TIMEOUT,
    )
    .await?;
    Ok(TeletextProfileInvocationReceipt {
        stdout: receipt.stdout,
        selection: receipt
            .selection
            .ok_or(TeletextInvocationError::AdmissionRejected)?,
        generation: receipt.generation,
    })
}

struct Invocation {
    pool: DbPool,
    addon_id: String,
    runtime: AddonRuntimeContext,
    input: Vec<u8>,
    cancellation: Option<oneshot::Receiver<()>>,
    selection: Option<TeletextSelectionSnapshot>,
}

type Response = oneshot::Sender<Result<InvocationReceipt, TeletextInvocationError>>;

async fn invoke_with_fence(
    invocation: Invocation,
    fence: ResolutionFence,
    settlement_timeout: Duration,
) -> Result<InvocationReceipt, TeletextInvocationError> {
    if invocation.input.len() > MAX_TELETEXT_PAGES_INPUT_BYTES {
        return Err(TeletextInvocationError::InvalidRequest);
    }
    let generation = TeletextPagesRequest::from_json(&invocation.input)
        .map_err(|_| TeletextInvocationError::InvalidRequest)?
        .generation;
    let (response, result) = oneshot::channel();
    tokio::spawn(async move {
        #[cfg(test)]
        let completed = Arc::clone(&fence.worker_completed);
        let mut response = Some(response);
        let outcome = invoke_owned(
            invocation,
            generation,
            fence,
            settlement_timeout,
            &mut response,
        )
        .await;
        #[cfg(test)]
        completed.store(true, Ordering::Release);
        if let Some(response) = response {
            let _ = response.send(outcome);
        }
    });
    result
        .await
        .unwrap_or(Err(TeletextInvocationError::CleanupUnconfirmed))
}

/// One operation's cooperative stop flag and absolute deadline, not a task tracker.
/// The invocation worker retains the entire resolution future until its joins finish.
#[derive(Clone)]
pub(super) struct ResolutionFence {
    stopped: Arc<AtomicBool>,
    deadline: Instant,
    #[cfg(test)]
    hook: Option<Arc<dyn Fn(&'static str) + Send + Sync>>,
    #[cfg(test)]
    worker_completed: Arc<AtomicBool>,
}

impl ResolutionFence {
    fn new(deadline: Instant) -> Self {
        Self {
            stopped: Arc::new(AtomicBool::new(false)),
            deadline,
            #[cfg(test)]
            hook: None,
            #[cfg(test)]
            worker_completed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
    }

    pub(super) fn check(&self) -> Result<(), AddonError> {
        if self.stopped.load(Ordering::Acquire) || Instant::now() >= self.deadline {
            Err(AddonError::Process("package resolution stopped".to_owned()))
        } else {
            Ok(())
        }
    }

    #[cfg(test)]
    pub(super) fn filesystem_hook(&self, stage: &'static str) {
        if let Some(hook) = &self.hook {
            hook(stage);
        }
    }
}

async fn caller_closed(response: &mut Option<Response>) {
    if let Some(response) = response {
        response.closed().await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn cancelled(cancellation: &mut Option<oneshot::Receiver<()>>) {
    if let Some(cancellation) = cancellation {
        let _ = cancellation.await;
    } else {
        std::future::pending::<()>().await;
    }
}

fn already_cancelled(cancellation: &mut Option<oneshot::Receiver<()>>) -> bool {
    cancellation.as_mut().is_some_and(|receiver| {
        matches!(
            receiver.try_recv(),
            Ok(()) | Err(oneshot::error::TryRecvError::Closed)
        )
    })
}

async fn invoke_owned(
    mut invocation: Invocation,
    generation: String,
    fence: ResolutionFence,
    settlement_timeout: Duration,
    response: &mut Option<Response>,
) -> Result<InvocationReceipt, TeletextInvocationError> {
    if response.as_ref().is_none_or(Response::is_closed)
        || already_cancelled(&mut invocation.cancellation)
    {
        return Err(TeletextInvocationError::Cancelled);
    }
    if fence.check().is_err() {
        return Err(TeletextInvocationError::TimedOut);
    }
    let resolution = resolve_invocation_for_launch(
        &invocation.pool,
        &invocation.addon_id,
        &invocation.runtime,
        invocation.selection.as_ref(),
        &fence,
    );
    tokio::pin!(resolution);
    let selected = tokio::select! {
        biased;
        () = caller_closed(response) => Err(TeletextInvocationError::Cancelled),
        () = cancelled(&mut invocation.cancellation) => Err(TeletextInvocationError::Cancelled),
        () = tokio::time::sleep_until(fence.deadline) => Err(TeletextInvocationError::TimedOut),
        result = &mut resolution => Ok(result),
    };
    let command = match selected {
        Ok(result) => {
            if matches!(
                &result,
                Err(AddonError::ToolCleanupRequired | AddonError::TerminationUnconfirmed)
            ) {
                return Err(TeletextInvocationError::CleanupUnconfirmed);
            }
            if response.as_ref().is_none_or(Response::is_closed)
                || already_cancelled(&mut invocation.cancellation)
            {
                return Err(TeletextInvocationError::Cancelled);
            }
            if Instant::now() >= fence.deadline {
                return Err(TeletextInvocationError::TimedOut);
            }
            result.map_err(|error| admission_error(&error))?
        }
        Err(reason) => {
            fence.stop();
            match tokio::time::timeout(settlement_timeout, &mut resolution).await {
                Ok(Err(AddonError::ToolCleanupRequired | AddonError::TerminationUnconfirmed)) => {
                    return Err(TeletextInvocationError::CleanupUnconfirmed);
                }
                Ok(_) => return Err(reason),
                Err(_) => {
                    if let Some(response) = response.take() {
                        let _ = response.send(Err(TeletextInvocationError::CleanupUnconfirmed));
                    }
                    // Never abandon an in-flight blocking job. Its cooperative
                    // fence prevents further stages when the native call returns.
                    let _ = resolution.await;
                    return Err(TeletextInvocationError::CleanupUnconfirmed);
                }
            }
        }
    };
    // The resolution future borrows only read-only inputs; its complete result
    // is retained before moving the raw request into the command owner.
    let receipt = execute(
        command,
        invocation.input,
        &mut invocation.cancellation,
        fence.deadline,
        response,
    )
    .await?;
    Ok(InvocationReceipt {
        stdout: receipt.stdout,
        state_revision: receipt.state_revision,
        selection: invocation.selection.clone(),
        generation,
    })
}

async fn resolve_invocation_for_launch(
    pool: &DbPool,
    addon_id: &str,
    runtime: &AddonRuntimeContext,
    selection: Option<&TeletextSelectionSnapshot>,
    fence: &ResolutionFence,
) -> Result<ResolvedAddonCommand, AddonError> {
    if let Some(selection) = selection {
        resolve_profile_for_launch(pool, selection, runtime, fence).await
    } else {
        resolve_for_launch(pool, addon_id, runtime, fence).await
    }
}

async fn check_selection(
    pool: &DbPool,
    selection: &TeletextSelectionSnapshot,
    fence: &ResolutionFence,
) -> Result<(), AddonError> {
    fence.check()?;
    let current = crate::repositories::SqliteTeletextProfileRepository(pool.clone())
        .invocation_snapshot(
            selection.channel_id,
            &selection.addon_id,
            selection.binding_revision,
        )
        .await
        .map_err(|_| AddonError::Database("profile selection unavailable".to_owned()))?;
    fence.check()?;
    if !matches!(current, TeletextProfileMutation::Saved(current) if current == *selection) {
        return Err(AddonError::Validation(
            "profile selection changed".to_owned(),
        ));
    }
    Ok(())
}

async fn resolve_profile_for_launch(
    pool: &DbPool,
    selection: &TeletextSelectionSnapshot,
    runtime: &AddonRuntimeContext,
    fence: &ResolutionFence,
) -> Result<ResolvedAddonCommand, AddonError> {
    check_selection(pool, selection, fence).await?;
    // Read-only transaction: bind the loaded manifest to its exact installation identity.
    let mut tx = pool.begin().await.map_err(super::database_error)?;
    let installation = super::load_installed_addon_in_transaction(&mut tx, &selection.addon_id)
        .await?
        .ok_or_else(|| AddonError::Validation("profile installation unavailable".to_owned()))?;
    let identity = super::teletext_profile_settings::installation_identity(&mut tx, &installation)
        .await
        .map_err(|_| AddonError::Database("profile installation unavailable".to_owned()))?;
    if !installation.enabled || identity != selection.installation {
        return Err(AddonError::Validation(
            "profile installation changed".to_owned(),
        ));
    }
    // The actual manifest object and its raw fingerprint came from one SQLite
    // snapshot. No filesystem/native-tool stage may use a different installation.
    drop(tx);
    fence.check()?;
    let settings = selection
        .effective_settings
        .iter()
        .map(super::teletext_profile_settings::to_addon_setting)
        .collect::<Vec<_>>();
    let verified =
        super::resolve_verified_entrypoint(&installation, super::current_rid()?, Some(fence))
            .await?;
    let command = super::complete_addon_command(
        &installation,
        verified,
        "pages",
        runtime,
        &settings,
        Some(fence),
    )
    .await?;
    check_selection(pool, selection, fence).await?;
    Ok(command)
}

async fn resolve_for_launch(
    pool: &DbPool,
    addon_id: &str,
    runtime: &AddonRuntimeContext,
    fence: &ResolutionFence,
) -> Result<ResolvedAddonCommand, AddonError> {
    let command = super::resolve_addon_command_body(
        pool,
        addon_id,
        "teletext.pages.v1",
        "pages",
        runtime,
        true,
        Some(fence),
    )
    .await?;
    fence.check()?;
    let current = sqlx::query(
        r#"SELECT "Enabled", "StateRevision" FROM "AddonInstallation" WHERE "AddonId" = ?"#,
    )
    .bind(addon_id)
    .fetch_optional(pool)
    .await
    .map_err(super::database_error)?;
    fence.check()?;
    let current = current.ok_or_else(|| {
        AddonError::Validation("installed package changed before launch".to_owned())
    })?;
    let enabled: i64 = current.try_get("Enabled").map_err(super::database_error)?;
    let revision: String = current
        .try_get("StateRevision")
        .map_err(super::database_error)?;
    if enabled == 0 || revision != command.state_revision {
        return Err(AddonError::Validation(
            "installed package changed before launch".to_owned(),
        ));
    }
    fence.check()?;
    Ok(command)
}

fn admission_error(error: &AddonError) -> TeletextInvocationError {
    match error {
        AddonError::ToolCleanupRequired | AddonError::TerminationUnconfirmed => {
            TeletextInvocationError::CleanupUnconfirmed
        }
        _ => TeletextInvocationError::AdmissionRejected,
    }
}

fn command_error(error: TeletextCommandError) -> TeletextInvocationError {
    match error {
        TeletextCommandError::InvalidRequest => TeletextInvocationError::InvalidRequest,
        TeletextCommandError::Cancelled => TeletextInvocationError::Cancelled,
        TeletextCommandError::TimedOut => TeletextInvocationError::TimedOut,
        TeletextCommandError::CleanupUnconfirmed => TeletextInvocationError::CleanupUnconfirmed,
        other => TeletextInvocationError::Command(other),
    }
}

async fn execute(
    command: ResolvedAddonCommand,
    input: Vec<u8>,
    cancellation: &mut Option<oneshot::Receiver<()>>,
    deadline: Instant,
    response: &mut Option<Response>,
) -> Result<TeletextInvocationReceipt, TeletextInvocationError> {
    let revision = command.state_revision.clone();
    let (cancel_command, command_cancelled) = oneshot::channel();
    let execution =
        teletext_command::run_with_deadline(command, input, Some(command_cancelled), deadline);
    tokio::pin!(execution);
    let selected = tokio::select! {
        biased;
        () = caller_closed(response) => None,
        () = cancelled(cancellation) => None,
        result = &mut execution => Some(result),
    };
    let output = if let Some(result) = selected {
        result.map_err(command_error)?
    } else {
        let _ = cancel_command.send(());
        return match execution.await {
            Err(TeletextCommandError::CleanupUnconfirmed) => {
                Err(TeletextInvocationError::CleanupUnconfirmed)
            }
            _ => Err(TeletextInvocationError::Cancelled),
        };
    };
    Ok(TeletextInvocationReceipt {
        stdout: output,
        state_revision: revision,
    })
}

#[cfg(test)]
#[path = "addon_teletext_invocation_tests.rs"]
mod tests;

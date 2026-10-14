//! Preparatory installed-package teletext invocation, without service activation.
//!
//! This adapter supports global settings, revision-fenced base profiles and
//! whole-channel effective base/published Scripted selections. It owns resolution
//! through native filesystem settlement, then command cancellation through cleanup.
//! Receipts retain observations, not immutable package bytes, validated pages or
//! publication authority.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use ersatzrs_addon_contract::{TeletextPagesRequest, MAX_TELETEXT_PAGES_INPUT_BYTES};
use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_core::repositories::{TeletextProfileMutation, TeletextProfileRepository as _};
use ersatzrs_domain::{
    TeletextEffectiveBindingSnapshot, TeletextEffectiveChannelSelectionSnapshot,
    TeletextInstallationSnapshot, TeletextOverrideState, TeletextPageContract,
    TeletextProfileSetting, TeletextSelectionSnapshot,
};
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

/// Unpublished output retaining the complete observed channel authority.
/// A Scripted origin retains its actual playout and interval; it is never a base binding.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEffectiveProfileInvocationReceipt {
    pub stdout: Vec<u8>,
    pub selection: Arc<TeletextEffectiveChannelSelectionSnapshot>,
    pub addon_id: String,
    pub generation: String,
}

impl std::fmt::Debug for TeletextEffectiveProfileInvocationReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TeletextEffectiveProfileInvocationReceipt")
            .field("stdout_bytes", &self.stdout.len())
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}

/// Invoke one enabled member of an exact effective channel selection.
/// The owned worker rechecks the whole set before resolution and prelaunch.
/// Its deadline cannot extend past a captured scheduled transition, or the
/// caller's original deadline. Abandonment retains the existing cleanup owner.
/// # Errors
/// Returns static admission, request, deadline or native cleanup failures.
pub async fn invoke_effective_profile_with_deadline_and_hold(
    pool: DbPool,
    selection: Arc<TeletextEffectiveChannelSelectionSnapshot>,
    addon_id: String,
    runtime: AddonRuntimeContext,
    input: Vec<u8>,
    deadline: std::time::Instant,
    hold: TeletextWorkHold,
) -> Result<TeletextEffectiveProfileInvocationReceipt, TeletextInvocationError> {
    let fence = effective_fence(&selection, deadline)?;
    let receipt = invoke_with_fence(
        Invocation {
            pool,
            addon_id: addon_id.clone(),
            runtime,
            input,
            cancellation: None,
            selection: None,
            effective_selection: Some(Arc::clone(&selection)),
            hold: Some(hold),
        },
        fence,
        RESOLUTION_SETTLEMENT_TIMEOUT,
    )
    .await?;
    Ok(TeletextEffectiveProfileInvocationReceipt {
        stdout: receipt.stdout,
        selection,
        addon_id,
        generation: receipt.generation,
    })
}

fn effective_fence(
    selection: &TeletextEffectiveChannelSelectionSnapshot,
    deadline: std::time::Instant,
) -> Result<ResolutionFence, TeletextInvocationError> {
    // Capture the monotonic clock first so conversion cannot renew the UTC horizon.
    let now = Instant::now();
    let mut deadline = Instant::from_std(deadline).min(now + EXECUTION_TIMEOUT);
    if let Some(boundary) = selection.valid_until {
        let remaining = (boundary - chrono::Utc::now())
            .to_std()
            .map_err(|_| TeletextInvocationError::TimedOut)?;
        deadline = deadline.min(now + remaining.min(EXECUTION_TIMEOUT));
    }
    if deadline <= Instant::now() {
        return Err(TeletextInvocationError::TimedOut);
    }
    Ok(ResolutionFence::new(deadline))
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
/// This legacy global route remains explicitly v1, even when v2 is also declared;
/// it is never a retry or fallback for profile invocation. Promotion remains open.
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
            effective_selection: None,
            hold: None,
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

/// Invoke using one enabled channel binding, effective settings and pinned contract.
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
    invoke_profile_with_deadline(
        pool,
        selection,
        runtime,
        input,
        std::time::Instant::now() + EXECUTION_TIMEOUT,
        cancellation,
    )
    .await
}

/// Invoke a profile within the earlier of a supplied shared deadline and the
/// existing 30-second per-invocation ceiling. Resolution, process and pipes use
/// that same deadline; cancellation and settlement ownership are unchanged.
///
/// # Errors
/// Returns the existing static invocation failures, including cleanup uncertainty.
pub async fn invoke_profile_with_deadline(
    pool: DbPool,
    selection: TeletextSelectionSnapshot,
    runtime: AddonRuntimeContext,
    input: Vec<u8>,
    deadline: std::time::Instant,
    cancellation: Option<oneshot::Receiver<()>>,
) -> Result<TeletextProfileInvocationReceipt, TeletextInvocationError> {
    invoke_profile_before_owned(
        pool,
        selection,
        runtime,
        input,
        deadline,
        cancellation,
        None,
    )
    .await
}

/// Invoke under a shared work hold retained by the actual resolution owner.
/// Early cleanup uncertainty does not release admission while a native join is
/// still pending. The hold carries no selection or publication authority.
/// # Errors
/// Returns the same static invocation and cleanup errors as the deadline API.
pub async fn invoke_profile_with_deadline_and_hold(
    pool: DbPool,
    selection: TeletextSelectionSnapshot,
    runtime: AddonRuntimeContext,
    input: Vec<u8>,
    deadline: std::time::Instant,
    cancellation: Option<oneshot::Receiver<()>>,
    hold: TeletextWorkHold,
) -> Result<TeletextProfileInvocationReceipt, TeletextInvocationError> {
    invoke_profile_before_owned(
        pool,
        selection,
        runtime,
        input,
        deadline,
        cancellation,
        Some(hold),
    )
    .await
}

async fn invoke_profile_before_owned(
    pool: DbPool,
    selection: TeletextSelectionSnapshot,
    runtime: AddonRuntimeContext,
    input: Vec<u8>,
    deadline: std::time::Instant,
    cancellation: Option<oneshot::Receiver<()>>,
    hold: Option<TeletextWorkHold>,
) -> Result<TeletextProfileInvocationReceipt, TeletextInvocationError> {
    let fence =
        ResolutionFence::new(Instant::from_std(deadline).min(Instant::now() + EXECUTION_TIMEOUT));
    let receipt = invoke_with_fence(
        Invocation {
            pool,
            addon_id: selection.addon_id.clone(),
            runtime,
            input,
            cancellation,
            selection: Some(selection),
            effective_selection: None,
            hold,
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
    effective_selection: Option<Arc<TeletextEffectiveChannelSelectionSnapshot>>,
    hold: Option<TeletextWorkHold>,
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
    let contract = match (&invocation.selection, &invocation.effective_selection) {
        (Some(_), Some(_)) => return Err(TeletextInvocationError::AdmissionRejected),
        (_, Some(selection)) => effective_binding(selection, &invocation.addon_id)
            .and_then(binding_contract)
            .map_err(|_| TeletextInvocationError::AdmissionRejected)?,
        (selection, None) => selection
            .as_ref()
            .map_or(TeletextPageContract::V1, |s| s.contract),
    };
    let generation =
        TeletextPagesRequest::from_json_for_schema(&invocation.input, contract.schema_version())
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
            contract,
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
    contract: TeletextPageContract,
    fence: ResolutionFence,
    settlement_timeout: Duration,
    response: &mut Option<Response>,
) -> Result<InvocationReceipt, TeletextInvocationError> {
    // Move the hold into this actual worker before any resolution or command
    // stage. It stays here through early uncertainty responses and native joins,
    // independently of whether the requesting future is still alive.
    let work_hold = invocation.hold.take();
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
        invocation.effective_selection.as_deref(),
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
            if retain_resolution_cleanup(work_hold.as_ref(), &result) {
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
            if let Ok(result) = tokio::time::timeout(settlement_timeout, &mut resolution).await {
                if retain_resolution_cleanup(work_hold.as_ref(), &result) {
                    return Err(TeletextInvocationError::CleanupUnconfirmed);
                }
                return Err(reason);
            }
            if let Some(response) = response.take() {
                let _ = response.send(Err(TeletextInvocationError::CleanupUnconfirmed));
            }
            // Never abandon an in-flight blocking job. Its cooperative
            // fence prevents further stages when the native call returns.
            let result = resolution.await;
            retain_resolution_cleanup(work_hold.as_ref(), &result);
            return Err(TeletextInvocationError::CleanupUnconfirmed);
        }
    };
    // The resolution future borrows only read-only inputs; its complete result
    // is retained before moving the raw request into the command owner.
    let outcome = execute(
        command,
        invocation.input,
        &mut invocation.cancellation,
        fence.deadline,
        contract,
        response,
        work_hold.clone(),
    )
    .await;
    // The command error is emitted only after its owned cleanup attempts. An
    // unproved process exit may still own Drop fallback work; never recycle
    // this admission based solely on the requesting future having returned.
    if matches!(&outcome, Err(TeletextInvocationError::CleanupUnconfirmed)) {
        if let Some(hold) = &work_hold {
            hold.retain_until_restart();
        }
    }
    let receipt = outcome?;
    Ok(InvocationReceipt {
        stdout: receipt.stdout,
        state_revision: receipt.state_revision,
        selection: invocation.selection.clone(),
        generation,
    })
}

fn retain_resolution_cleanup(
    hold: Option<&TeletextWorkHold>,
    result: &Result<ResolvedAddonCommand, AddonError>,
) -> bool {
    let uncertain = matches!(
        result,
        Err(AddonError::ToolCleanupRequired | AddonError::TerminationUnconfirmed)
    );
    if uncertain {
        if let Some(hold) = hold {
            hold.retain_until_restart();
        }
    }
    uncertain
}

async fn resolve_invocation_for_launch(
    pool: &DbPool,
    addon_id: &str,
    runtime: &AddonRuntimeContext,
    selection: Option<&TeletextSelectionSnapshot>,
    effective: Option<&TeletextEffectiveChannelSelectionSnapshot>,
    fence: &ResolutionFence,
) -> Result<ResolvedAddonCommand, AddonError> {
    if let Some(selection) = effective {
        resolve_selected_for_launch(
            pool,
            ProfileAuthority::Effective {
                selection,
                addon_id,
            },
            runtime,
            fence,
        )
        .await
    } else if let Some(selection) = selection {
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

fn effective_binding<'a>(
    selection: &'a TeletextEffectiveChannelSelectionSnapshot,
    addon_id: &str,
) -> Result<&'a TeletextEffectiveBindingSnapshot, AddonError> {
    if selection.channel_id <= 0
        || selection
            .bindings
            .iter()
            .any(|b| b.channel_id() != selection.channel_id)
    {
        return Err(AddonError::Validation(
            "effective channel scope changed".to_owned(),
        ));
    }
    let mut matching = selection
        .bindings
        .iter()
        .filter(|b| b.addon_id() == addon_id);
    let binding = matching
        .next()
        .filter(|b| b.enabled())
        .ok_or_else(|| AddonError::Validation("effective profile unavailable".to_owned()))?;
    if matching.next().is_some() {
        return Err(AddonError::Validation(
            "effective membership changed".to_owned(),
        ));
    }
    Ok(binding)
}

fn binding_contract(
    binding: &TeletextEffectiveBindingSnapshot,
) -> Result<TeletextPageContract, AddonError> {
    match binding {
        TeletextEffectiveBindingSnapshot::Base(base) => Ok(base.contract),
        TeletextEffectiveBindingSnapshot::Scripted(published) => match &published.interval.state {
            TeletextOverrideState::Enabled(candidate) => Ok(candidate.contract),
            TeletextOverrideState::Off { .. } => Err(AddonError::Validation(
                "effective profile is off".to_owned(),
            )),
        },
    }
}

enum ProfileAuthority<'a> {
    Base(&'a TeletextSelectionSnapshot),
    Effective {
        selection: &'a TeletextEffectiveChannelSelectionSnapshot,
        addon_id: &'a str,
    },
}

struct ProfileData<'a> {
    addon_id: &'a str,
    contract: TeletextPageContract,
    installation: &'a TeletextInstallationSnapshot,
    settings: &'a [TeletextProfileSetting],
}

impl ProfileAuthority<'_> {
    async fn check(&self, pool: &DbPool, fence: &ResolutionFence) -> Result<(), AddonError> {
        match self {
            Self::Base(selection) => check_selection(pool, selection, fence).await,
            Self::Effective {
                selection,
                addon_id,
            } => {
                fence.check()?;
                effective_binding(selection, addon_id)?;
                let current = crate::repositories::SqliteTeletextProfileRepository(pool.clone())
                    .recheck_effective_channel_snapshot(selection, chrono::Utc::now())
                    .await
                    .map_err(|_| {
                        AddonError::Database("effective profile unavailable".to_owned())
                    })?;
                fence.check()?;
                if !matches!(current, TeletextProfileMutation::Saved(current) if current == **selection)
                {
                    return Err(AddonError::Validation(
                        "effective selection changed".to_owned(),
                    ));
                }
                Ok(())
            }
        }
    }

    fn data(&self) -> Result<ProfileData<'_>, AddonError> {
        match self {
            Self::Base(base) => Ok(ProfileData {
                addon_id: &base.addon_id,
                contract: base.contract,
                installation: &base.installation,
                settings: &base.effective_settings,
            }),
            Self::Effective {
                selection,
                addon_id,
            } => match effective_binding(selection, addon_id)? {
                TeletextEffectiveBindingSnapshot::Base(base) => Ok(ProfileData {
                    addon_id: &base.addon_id,
                    contract: base.contract,
                    installation: &base.installation,
                    settings: &base.effective_settings,
                }),
                TeletextEffectiveBindingSnapshot::Scripted(published) => {
                    match &published.interval.state {
                        TeletextOverrideState::Enabled(candidate) => Ok(ProfileData {
                            addon_id: &candidate.addon_id,
                            contract: candidate.contract,
                            installation: &candidate.installation,
                            settings: &candidate.effective_settings,
                        }),
                        TeletextOverrideState::Off { .. } => Err(AddonError::Validation(
                            "effective profile is off".to_owned(),
                        )),
                    }
                }
            },
        }
    }
}

async fn resolve_profile_for_launch(
    pool: &DbPool,
    selection: &TeletextSelectionSnapshot,
    runtime: &AddonRuntimeContext,
    fence: &ResolutionFence,
) -> Result<ResolvedAddonCommand, AddonError> {
    resolve_selected_for_launch(pool, ProfileAuthority::Base(selection), runtime, fence).await
}

async fn resolve_selected_for_launch(
    pool: &DbPool,
    authority: ProfileAuthority<'_>,
    runtime: &AddonRuntimeContext,
    fence: &ResolutionFence,
) -> Result<ResolvedAddonCommand, AddonError> {
    authority.check(pool, fence).await?;
    let selection = authority.data()?;
    // Read-only transaction: bind the loaded manifest to its exact installation identity.
    let mut tx = pool.begin().await.map_err(super::database_error)?;
    let installation = super::load_installed_addon_in_transaction(&mut tx, selection.addon_id)
        .await?
        .ok_or_else(|| AddonError::Validation("profile installation unavailable".to_owned()))?;
    let identity = super::teletext_profile_settings::installation_identity(&mut tx, &installation)
        .await
        .map_err(|_| AddonError::Database("profile installation unavailable".to_owned()))?;
    if !installation.enabled
        || identity != *selection.installation
        || super::teletext_profile_settings::preferred_contract(&installation.manifest)
            != Some(selection.contract)
    {
        return Err(AddonError::Validation(
            "profile installation changed".to_owned(),
        ));
    }
    // The actual manifest object and its raw fingerprint came from one SQLite
    // snapshot. No filesystem/native-tool stage may use a different installation.
    drop(tx);
    fence.check()?;
    let settings = selection
        .settings
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
    authority.check(pool, fence).await?;
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
    contract: TeletextPageContract,
    response: &mut Option<Response>,
    hold: Option<TeletextWorkHold>,
) -> Result<TeletextInvocationReceipt, TeletextInvocationError> {
    let revision = command.state_revision.clone();
    let (cancel_command, command_cancelled) = oneshot::channel();
    let execution = teletext_command::run_with_contract_deadline_and_hold(
        command,
        input,
        Some(command_cancelled),
        deadline,
        contract,
        hold,
    );
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

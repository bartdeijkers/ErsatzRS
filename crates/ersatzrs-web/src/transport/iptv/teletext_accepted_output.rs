//! Accepted originals meet real final-command hints and source PSI here.
//!
//! Explicit host policy is required. This boundary neither chooses production
//! defaults nor installs/invokes packages. A fresh application read fences the
//! selected generation before any source consumption; ongoing live ownership,
//! receiver-cache retirement and actual client acceptance remain separate.

use std::{
    sync::{atomic::AtomicU8, Arc},
    time::Duration,
};

use ersatzrs_app::{teletext_design_documents::TeletextDesignDocumentBudget, ApplicationServices};
use ersatzrs_core::repositories::TeletextGenerationMutation;
use ersatzrs_ffmpeg::{
    teletext::{TeletextOutputError, TeletextServiceDescriptor},
    FfmpegCommand,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Notify,
    time::{timeout_at, Instant},
};

use crate::host::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_generations::{
        reload_accepted_teletext_generation, AcceptedTeletextOutputCandidate,
        NativeTeletextGenerationError,
    },
};

use super::{
    retained_stream_buffer::{ReserveError, ReservePolicy},
    teletext_binding::TeletextOutputBinding,
    teletext_optional_output::{
        forward_optional_seeded_retained_output, forward_optional_seeded_teletext_output,
        replay_direct_prefix, replay_retained_prefix, OptionalTeletextOutput,
        SeededOptionalTeletextStartup,
    },
    teletext_output::TeletextPumpError,
    teletext_retained_output::{allocation_model_capacity, RetainedTeletextError},
    teletext_source_policy::{TeletextQualifiedSourcePolicy, TeletextSourcePolicy},
    teletext_source_probe::{
        probe_teletext_source, TeletextSourceProbeError, TeletextSourceProbeFailure,
        TeletextSourceProbePolicy,
    },
    teletext_startup::TeletextStartupPrefix,
};

const READ_BYTES: usize = 256 * 188;
const MAX_BYTES: usize = 32 * 1024 * 1024;

/// Exact accepted native content awaiting command qualification and real PSI.
/// Keeping this value does not freeze effective channel selection. Every new
/// forwarding call rechecks that selection with the same application function.
pub struct TeletextAcceptedOutputIntent {
    candidate: Arc<AcceptedTeletextOutputCandidate>,
    policy: TeletextSourcePolicy,
    max_selection_bytes: usize,
}

/// Native recovery and policy failures, without exposing original documents.
#[derive(Debug, thiserror::Error)]
pub enum AcceptedTeletextIntentLoadError {
    /// Existing original-compilation or selected-generation storage failure.
    #[error(transparent)]
    Generation(#[from] NativeTeletextGenerationError),
    /// Invalid explicit intent bound.
    #[error(transparent)]
    Configuration(#[from] TeletextPumpError),
    /// The complete recovery, including repository awaits, exceeded its deadline.
    #[error("accepted teletext output recovery exceeded its deadline")]
    TimedOut,
}

/// Load only selected accepted originals under one absolute native deadline.
/// Pending documents remain unpublished. Existing native recovery owns complete
/// compilation/fingerprint checks; this adapter adds no package invocation or
/// compiler defaults. The ordinary process owner decides optional-failure policy.
/// # Errors
/// Returns native recovery/policy failures or timeout across the entire await.
pub async fn load_accepted_teletext_output_intent(
    application: &ApplicationServices,
    channel_id: i64,
    budget: TeletextDesignDocumentBudget,
    encoding: TeletextChannelDocumentEncodingPolicy,
    policy: TeletextSourcePolicy,
    deadline: std::time::Instant,
) -> Result<Option<TeletextAcceptedOutputIntent>, AcceptedTeletextIntentLoadError> {
    let owner_deadline = Instant::from_std(deadline);
    if Instant::now() >= owner_deadline {
        return Err(AcceptedTeletextIntentLoadError::TimedOut);
    }
    if budget.max_serialized_bytes == 0 {
        return Err(TeletextPumpError::Configuration.into());
    }
    let recovered = timeout_at(
        owner_deadline,
        reload_accepted_teletext_generation(application, channel_id, budget, encoding, deadline),
    )
    .await
    .map_err(|_| AcceptedTeletextIntentLoadError::TimedOut)?;
    // Nested admission can observe the same expired instant first. Retain one
    // public timeout category before propagating any nested recovery result.
    if Instant::now() >= owner_deadline {
        return Err(AcceptedTeletextIntentLoadError::TimedOut);
    }
    let candidate = recovered?;
    candidate
        .map(|candidate| {
            TeletextAcceptedOutputIntent::new(
                Arc::new(candidate),
                policy,
                budget.max_serialized_bytes,
            )
            .map_err(Into::into)
        })
        .transpose()
}

impl TeletextAcceptedOutputIntent {
    /// Retain the sealed native candidate and explicit host limits without copying
    /// its compiled service or guessing any source identifier.
    /// # Errors
    /// Rejects a zero serialized-selection bound.
    pub fn new(
        candidate: Arc<AcceptedTeletextOutputCandidate>,
        policy: TeletextSourcePolicy,
        max_selection_bytes: usize,
    ) -> Result<Self, TeletextPumpError> {
        if max_selection_bytes == 0 {
            return Err(TeletextPumpError::Configuration);
        }
        Ok(Self {
            candidate,
            policy,
            max_selection_bytes,
        })
    }

    /// Original immutable selected identity and shared compiled content.
    #[must_use]
    pub const fn candidate(&self) -> &Arc<AcceptedTeletextOutputCandidate> {
        &self.candidate
    }

    /// Checked host limits; neither transport identities nor compiler defaults.
    #[must_use]
    pub const fn policy(&self) -> TeletextSourcePolicy {
        self.policy
    }

    async fn current(&self, application: &ApplicationServices, deadline: Instant) -> bool {
        let confirmed = timeout_at(
            deadline,
            application.confirm_current_teletext_generation(
                self.candidate.generation(),
                self.candidate.selection(),
                self.max_selection_bytes,
            ),
        )
        .await;
        Instant::now() < deadline
            && matches!(confirmed, Ok(Ok(TeletextGenerationMutation::Accepted(()))))
    }
}

async fn prepare_source(
    source: &mut (impl AsyncRead + Unpin),
    intent: &TeletextAcceptedOutputIntent,
    policy: TeletextQualifiedSourcePolicy,
    owner_deadline: Instant,
    retained: bool,
) -> Result<(TeletextOutputBinding, TeletextStartupPrefix), TeletextSourceProbeFailure> {
    let limits = policy.limits();
    let invalid = || TeletextSourceProbeFailure {
        raw_prefix: Vec::new(),
        reason: TeletextSourceProbeError::Configuration,
    };
    let deadline = Instant::now()
        .checked_add(limits.probe_maximum_time)
        .ok_or_else(invalid)?
        .min(owner_deadline);
    let descriptor = TeletextServiceDescriptor::from_authored_service(intent.candidate.service());
    let admitted = probe_teletext_source(
        source,
        &descriptor,
        limits.elementary_pid,
        TeletextSourceProbePolicy {
            maximum_bytes: limits.probe_maximum_bytes,
            deadline,
            first_byte_timeout: limits.first_byte_timeout,
        },
    )
    .await?;
    // Requested probe bounds were checked before reading. Charge the actual
    // retained allocation again before the constructor creates a validation
    // gate; seeded forwarding will independently charge its later overlap.
    if retained && retained_seed_preflight(limits, admitted.prefix.retained_bytes()).is_err() {
        return Err(TeletextSourceProbeFailure {
            raw_prefix: admitted.prefix.into_bytes(),
            reason: TeletextSourceProbeError::Configuration,
        });
    }
    let binding = policy
        .output_config(&admitted.plan)
        .ok()
        .and_then(|config| {
            TeletextOutputBinding::new_authored_shared(
                config,
                Arc::clone(intent.candidate.service()),
                limits.first_byte_timeout,
            )
            .ok()
        });
    match binding {
        Some(binding) => Ok((binding, admitted.prefix)),
        None => Err(TeletextSourceProbeFailure {
            raw_prefix: admitted.prefix.into_bytes(),
            reason: TeletextSourceProbeError::Configuration,
        }),
    }
}

pub(super) fn source_error(reason: TeletextSourceProbeError) -> (TeletextPumpError, bool) {
    match reason {
        TeletextSourceProbeError::Read(kind) => (TeletextPumpError::Read(kind), true),
        TeletextSourceProbeError::Empty => (TeletextPumpError::EmptyOutput, true),
        TeletextSourceProbeError::FirstByteTimeout => (TeletextPumpError::FirstByteTimeout, true),
        TeletextSourceProbeError::Psi(error) => (TeletextOutputError::Psi(error).into(), false),
        TeletextSourceProbeError::Deadline => (TeletextOutputError::Deadline.into(), false),
        _ => (TeletextPumpError::Configuration, false),
    }
}

/// Forward one explicitly selected accepted generation on final MPEG-TS stdout.
/// The caller owns the actual process/command association and its cleanup.
/// Unsupported command, stale selection or pre-write admission rejection replays
/// every consumed byte and continues the same source. Any sink-write attempt
/// seals that output epoch; existing seeded pumps enforce its later failures.
/// # Errors
/// Actual source/sink faults and committed authored-epoch failures remain terminal.
pub async fn forward_accepted_teletext_output(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    command: &FfmpegCommand,
    intent: &TeletextAcceptedOutputIntent,
    application: &ApplicationServices,
    deadline: Instant,
) -> Result<OptionalTeletextOutput, TeletextPumpError> {
    let qualified = command
        .mpegts_output_policy()
        .and_then(|command| intent.policy.qualify(command).ok());
    // A command that cannot carry this service must take the ordinary path
    // without waiting for optional authority that cannot change that outcome.
    if qualified.is_none() || !intent.current(application, deadline).await {
        return Ok(OptionalTeletextOutput {
            bytes: replay_direct_prefix(source, sink, Vec::new()).await?,
            rejection: Some(TeletextPumpError::Configuration),
        });
    }
    let qualified = qualified.ok_or(TeletextPumpError::Configuration)?;
    match prepare_source(source, intent, qualified, deadline, false).await {
        Ok((binding, prefix)) => {
            // Source discovery awaited bytes after the first authority read.
            // A selection changed during that await must not start a stale
            // authored epoch; preserve its exact consumed prefix instead.
            if !intent.current(application, deadline).await {
                return Ok(OptionalTeletextOutput {
                    bytes: replay_direct_prefix(source, sink, prefix.into_bytes()).await?,
                    rejection: Some(TeletextPumpError::Configuration),
                });
            }
            forward_optional_seeded_teletext_output(
                source,
                sink,
                &binding,
                prefix,
                qualified.limits().startup_maximum_bytes,
            )
            .await
        }
        Err(failure) => {
            let (error, terminal) = source_error(failure.reason);
            if terminal {
                return Err(error);
            }
            Ok(OptionalTeletextOutput {
                bytes: replay_direct_prefix(source, sink, failure.raw_prefix).await?,
                rejection: Some(error),
            })
        }
    }
}

/// Content-free retained-output failure. The ordinary reserve and authored pump
/// keep their original typed causes privately; no source bytes enter diagnostics.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct AcceptedRetainedTeletextError(#[from] RetainedTeletextError);

/// Sink-confirmed byte count and optional pre-write authored rejection.
#[derive(Debug)]
pub struct AcceptedRetainedTeletextOutput {
    /// Actual total written bytes from the same source.
    pub bytes: u64,
    /// Teletext rejection with successful ordinary reserve forwarding.
    pub rejection: Option<AcceptedRetainedTeletextError>,
}

/// Borrowed ordinary reserve ownership reused by an accepted output epoch.
/// The producer owner supplies its existing terminal state and cancellation;
/// this adapter does not create a process, task or alternative reserve policy.
pub struct AcceptedRetainedTeletextEpoch<'a> {
    reserve: ReservePolicy,
    terminal: &'a AtomicU8,
    startup: Instant,
    cancel: &'a Notify,
}

impl<'a> AcceptedRetainedTeletextEpoch<'a> {
    /// Construct the same ordinary reserve for its declared rate and remainder.
    /// # Errors
    /// Rejects the ordinary reserve's invalid rate, remainder or memory bounds.
    pub fn new(
        muxrate_bps: u64,
        remaining: Option<Duration>,
        terminal: &'a AtomicU8,
        startup: Instant,
        cancel: &'a Notify,
    ) -> Result<Self, AcceptedRetainedTeletextError> {
        let reserve =
            ReservePolicy::new(muxrate_bps, remaining).map_err(RetainedTeletextError::from)?;
        Ok(Self {
            reserve,
            terminal,
            startup,
            cancel,
        })
    }
}

fn retained_preflight(
    reserve: ReservePolicy,
    qualified: TeletextQualifiedSourcePolicy,
    muxrate_bps: u64,
) -> Result<(), RetainedTeletextError> {
    let limits = qualified.limits();
    let (base, per_packet) = allocation_model_capacity(limits.max_buffer_packets)?;
    let journal = limits.startup_maximum_bytes;
    let probe = limits.probe_maximum_bytes;
    let invalid = || RetainedTeletextError::Configuration;
    retained_seed_preflight(limits, probe)?;
    let processing = MAX_BYTES
        .checked_sub(base)
        .and_then(|x| x.checked_sub(journal))
        .and_then(|x| x.checked_sub(READ_BYTES))
        .ok_or_else(invalid)?
        / per_packet;
    let fallback_peak = reserve
        .capacity_bytes()
        .checked_add(probe)
        .and_then(|x| x.checked_add(2 * READ_BYTES))
        .ok_or_else(invalid)?;
    let span = (reserve.capacity_bytes() as u128 * 8_000_000_000)
        .div_ceil(u128::from(reserve.muxrate_bps()));
    let minimum_hold = Duration::from_nanos(u64::try_from(span).map_err(|_| invalid())?)
        .checked_add(limits.playout_delay)
        .and_then(|x| x.checked_add(limits.max_lateness))
        .and_then(|x| x.checked_add(Duration::from_millis(20)))
        .ok_or_else(invalid)?;
    if muxrate_bps != reserve.muxrate_bps()
        || fallback_peak > MAX_BYTES
        || limits.max_buffer_packets < reserve.capacity_bytes().div_ceil(188)
        || processing as u128 <= u128::from(muxrate_bps) / (50 * 1504)
        || limits.max_hold <= minimum_hold
    {
        return Err(invalid());
    }
    Ok(())
}

fn retained_seed_preflight(
    limits: super::teletext_source_policy::TeletextSourcePolicyLimits,
    actual_prefix_capacity: usize,
) -> Result<(), RetainedTeletextError> {
    let (base, _) = allocation_model_capacity(limits.max_buffer_packets)?;
    base.checked_add(limits.startup_maximum_bytes)
        .and_then(|bytes| bytes.checked_add(actual_prefix_capacity))
        .and_then(|bytes| bytes.checked_add(READ_BYTES))
        .filter(|bytes| *bytes <= MAX_BYTES)
        .map(|_| ())
        .ok_or(RetainedTeletextError::Configuration)
}

async fn current_retained_intent(
    intent: &TeletextAcceptedOutputIntent,
    application: &ApplicationServices,
    startup: Instant,
    cancel: &Notify,
) -> Result<bool, AcceptedRetainedTeletextError> {
    tokio::select! { biased;
        () = cancel.notified() => Err(RetainedTeletextError::Reserve(ReserveError::Cancelled).into()),
        () = tokio::time::sleep_until(startup) => Err(RetainedTeletextError::Reserve(ReserveError::Deadline).into()),
        current = intent.current(application, startup) => Ok(current),
    }
}

/// Use the original reserve/startup/cancellation with a late actual-source binding.
/// Checks requested allocation overlap before probing or allocating a validation
/// gate. Missing authority and pre-write rejection preserve ordinary playback.
/// # Errors
/// Original reserve cancellation/deadline, source/sink faults or a committed
/// authored epoch failure remain terminal. The caller retains process ownership.
pub async fn forward_accepted_retained_teletext_output(
    source: &mut (impl AsyncRead + Unpin),
    sink: &mut (impl AsyncWrite + Unpin),
    command: &FfmpegCommand,
    intent: &TeletextAcceptedOutputIntent,
    application: &ApplicationServices,
    epoch: AcceptedRetainedTeletextEpoch<'_>,
) -> Result<AcceptedRetainedTeletextOutput, AcceptedRetainedTeletextError> {
    let AcceptedRetainedTeletextEpoch {
        reserve,
        terminal,
        startup,
        cancel,
    } = epoch;
    let hints = command.mpegts_output_policy();
    let qualified = hints.and_then(|hints| intent.policy.qualify(hints).ok());
    let qualified = qualified.filter(|qualified| {
        hints.is_some_and(|hints| {
            retained_preflight(reserve, *qualified, hints.muxrate_bps()).is_ok()
        })
    });
    // Known configuration/command rejection cannot become injectable through
    // a repository lookup. Preserve ordinary reserve progress even if optional
    // generation storage is currently waiting for a connection.
    let current = qualified.is_some()
        && current_retained_intent(intent, application, startup, cancel).await?;
    let Some(qualified) = qualified.filter(|_| current) else {
        let bytes =
            replay_retained_prefix(source, sink, reserve, terminal, startup, cancel, Vec::new())
                .await?;
        return Ok(AcceptedRetainedTeletextOutput {
            bytes,
            rejection: Some(RetainedTeletextError::Configuration.into()),
        });
    };
    let prepared = tokio::select! { biased;
        () = cancel.notified() => return Err(RetainedTeletextError::Reserve(ReserveError::Cancelled).into()),
        () = tokio::time::sleep_until(startup) => return Err(RetainedTeletextError::Reserve(ReserveError::Deadline).into()),
        result = prepare_source(source, intent, qualified, startup, true) => result,
    };
    match prepared {
        Ok((binding, prefix)) => {
            // Retain the original reserve/cancellation deadline while fencing
            // mutations that committed during actual source discovery.
            let current = current_retained_intent(intent, application, startup, cancel).await?;
            if !current {
                return Ok(AcceptedRetainedTeletextOutput {
                    bytes: replay_retained_prefix(
                        source,
                        sink,
                        reserve,
                        terminal,
                        startup,
                        cancel,
                        prefix.into_bytes(),
                    )
                    .await?,
                    rejection: Some(RetainedTeletextError::Configuration.into()),
                });
            }
            let output = forward_optional_seeded_retained_output(
                source,
                sink,
                reserve,
                terminal,
                startup,
                cancel,
                SeededOptionalTeletextStartup {
                    binding: &binding,
                    maximum_bytes: qualified.limits().startup_maximum_bytes,
                    prefix,
                },
            )
            .await?;
            Ok(AcceptedRetainedTeletextOutput {
                bytes: output.bytes,
                rejection: output.rejection.map(Into::into),
            })
        }
        Err(failure) => {
            let (error, terminal_error) = source_error(failure.reason);
            if terminal_error {
                return Err(match error {
                    TeletextPumpError::Read(kind) => RetainedTeletextError::Read(kind),
                    TeletextPumpError::EmptyOutput => ReserveError::InvalidOutput.into(),
                    _ => ReserveError::Deadline.into(),
                }
                .into());
            }
            let bytes = replay_retained_prefix(
                source,
                sink,
                reserve,
                terminal,
                startup,
                cancel,
                failure.raw_prefix,
            )
            .await?;
            Ok(AcceptedRetainedTeletextOutput {
                bytes,
                rejection: Some(RetainedTeletextError::Configuration.into()),
            })
        }
    }
}

#[cfg(test)]
#[path = "teletext_accepted_output_tests.rs"]
mod tests;

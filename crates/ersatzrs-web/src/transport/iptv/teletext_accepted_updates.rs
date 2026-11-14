//! Response-owned recovery of committed accepted output, independent of hints.
//!
//! This monitor publishes proposals, never sink acknowledgements or a generation.
//! Every read has a fresh, explicit bound. Losing current selection withdraws
//! authored output at the pump's safe boundary while ordinary AV continues.
//! Valid withdrawal proposals have no resource/cache checks. Unexpected
//! withdrawal invariant faults are terminal pump errors, never recoverable
//! rejections that leave unauthorized PES running. A reported rejected withdrawal
//! is therefore a defensive ownership/sequence fault, not an applied disable.

use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use ersatzrs_app::{teletext_design_documents::TeletextDesignDocumentBudget, ApplicationServices};
use ersatzrs_core::repositories::{
    TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
    TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION, TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION,
};
use ersatzrs_domain::{TeletextAcceptedGeneration, TeletextOutputGenerationObservation};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextServiceDescriptor};
use tokio::{sync::watch, task::JoinHandle, time::Instant};
use tracing::Instrument;

#[path = "teletext_expiry_preparation.rs"]
mod expiry_preparation;
use expiry_preparation::ExpiryPreparation;

use crate::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy;

use crate::transport::iptv::{
    teletext_binding::{
        teletext_output_control_channel, TeletextOutputControl, TeletextOutputTransitionPhase,
        TeletextOutputUpdate,
    },
    teletext_selected_output::{
        load_selected_teletext_output_candidate,
        load_selected_teletext_output_candidate_with_guide, observe_selected_teletext_output,
        observe_selected_teletext_output_with_guide, GuideOutputIdentity, GuideRuntime,
        SelectedTeletextOutputCandidate,
    },
    teletext_source_policy::TeletextSourcePolicy,
};

/// Explicit monitor cadence and complete read/recovery allowance. There are no
/// implicit producer, compiler, polling or source-identity defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TeletextAcceptedUpdatePolicy {
    poll_interval: Duration,
    recovery_timeout: Duration,
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("teletext accepted-output monitoring policy or ownership is invalid")]
pub struct TeletextAcceptedUpdatesConfigurationError;

impl TeletextAcceptedUpdatePolicy {
    /// Share the explicitly configured observation cadence with guide maintenance.
    pub(crate) const fn poll_interval(self) -> Duration {
        self.poll_interval
    }

    /// Reject zero and unrepresentable timings before any task or read exists.
    /// # Errors
    /// Rejects zero or unrepresentable polling/recovery durations.
    pub fn new(
        poll_interval: Duration,
        recovery_timeout: Duration,
    ) -> Result<Self, TeletextAcceptedUpdatesConfigurationError> {
        let now = Instant::now();
        if poll_interval.is_zero()
            || recovery_timeout.is_zero()
            || now.checked_add(poll_interval).is_none()
            || now.checked_add(recovery_timeout).is_none()
        {
            return Err(TeletextAcceptedUpdatesConfigurationError);
        }
        Ok(Self {
            poll_interval,
            recovery_timeout,
        })
    }
}

/// Explicit native dependencies retained only by one actual response owner.
#[derive(Clone)]
pub(super) struct TeletextAcceptedUpdatesContext {
    pub(super) application: Arc<ApplicationServices>,
    pub(super) channel_id: i64,
    pub(super) budget: TeletextDesignDocumentBudget,
    pub(super) encoding: TeletextChannelDocumentEncodingPolicy,
    pub(super) source: TeletextSourcePolicy,
    pub(super) timing: TeletextAcceptedUpdatePolicy,
    pub(super) guide: Option<Arc<GuideRuntime>>,
}

/// Dropping the actual response owner ends its monitor. Existing native recovery
/// holds admission in each real blocking worker even if this task is aborted.
pub(super) struct TeletextAcceptedUpdatesOwner {
    task: Option<JoinHandle<()>>,
}

impl TeletextAcceptedUpdatesOwner {
    /// Observe natural receiver-closure completion, useful for ordered teardown.
    #[cfg(test)]
    pub(super) async fn stopped(mut self) -> Result<(), tokio::task::JoinError> {
        if let Some(task) = self.task.take() {
            task.await
        } else {
            Ok(())
        }
    }
}

impl Drop for TeletextAcceptedUpdatesOwner {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

/// Start one latest-state monitor for one output lifetime. `initial` is the
/// already seeded sealed service; absence requires a physically inactive gate.
/// Pending requests and package invocation are deliberately absent from this API.
pub(super) fn start_accepted_updates(
    context: TeletextAcceptedUpdatesContext,
    initial: Option<Arc<SelectedTeletextOutputCandidate>>,
) -> Result<
    (TeletextOutputControl, TeletextAcceptedUpdatesOwner),
    TeletextAcceptedUpdatesConfigurationError,
> {
    if context.channel_id <= 0
        || context.budget.max_serialized_bytes == 0
        || initial.as_ref().is_some_and(|candidate| {
            candidate.generation().request.channel_id != context.channel_id
                || candidate.channel_id() != context.channel_id
        })
    {
        return Err(TeletextAcceptedUpdatesConfigurationError);
    }
    let timing = context.timing;
    let channel_id = context.channel_id;
    let (updates, mut control, mut status) = teletext_output_control_channel();
    control.observe_native_output(
        super::super::teletext_output_observation::ObservationContext {
            application: context.application.clone(),
            channel_id,
            initial: ersatzrs_domain::TeletextOutputSelectionObservation {
                sequence: 0,
                generation: initial
                    .as_ref()
                    .and_then(|candidate| candidate.output_generation_observation()),
                authored: initial
                    .as_ref()
                    .is_some_and(|candidate| candidate.service().is_some()),
            },
        },
    );
    let trace_id = control.trace_id();
    if let Some(candidate) = &initial {
        trace_native_candidate(candidate, trace_id, "monitor_initial");
    }
    let observe = move || {
        let value = status.borrow_and_update();
        value.sequence().map(|sequence| (sequence, value.phase()))
    };
    let task = tokio::spawn(
        run_monitor(
            NativeBackend {
                trace_id,
                expiry_preparation: Mutex::new(None),
                guide_validity: Mutex::new(initial.as_ref().and_then(|candidate| {
                    candidate.guide_deadline().map(|deadline| {
                        (
                            candidate.generation().clone(),
                            candidate.guide_identity().cloned(),
                            deadline,
                        )
                    })
                })),
                latest_guide: Mutex::new(None),
                prepared_handover: Mutex::new(None),
                next_transition: Mutex::new(
                    initial
                        .as_ref()
                        .and_then(|candidate| candidate.next_transition()),
                ),
                context,
            },
            channel_id,
            timing,
            initial,
            updates,
            observe,
        )
        .instrument(tracing::debug_span!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            "teletext_accepted_monitor",
            proposal_slot = %trace_id,
            channel_id,
        )),
    );
    Ok((control, TeletextAcceptedUpdatesOwner { task: Some(task) }))
}

fn trace_native_candidate(
    candidate: &SelectedTeletextOutputCandidate,
    trace_id: uuid::Uuid,
    event: &'static str,
) {
    let identity = candidate.generation();
    let valid_until = candidate.effective_valid_until();
    let guide = candidate.guide_refresh_confirmation();
    let now = Instant::now();
    tracing::debug!(
        target: "ersatzrs_web::transport::iptv::teletext_provenance",
        event,
        proposal_slot = %trace_id,
        channel_id = candidate.channel_id(),
        generation_id = identity.id,
        request_id = identity.request.id,
        design_id = identity.request.design_id,
        revision = identity.request.revision,
        artifact_version = identity.artifact_version,
        authored_selected = candidate.service().is_some(),
        original_valid_until = ?valid_until,
        guide_seed_id = ?guide.map(|value| value.seed.id),
        guide_runtime_id = ?guide.and_then(|value| value.runtime.as_ref().map(|runtime| runtime.id)),
        guide_valid_until = ?guide.map(|value| value.available_until),
        next_transition_remaining_ns = ?candidate.next_transition().map(|next| next.saturating_duration_since(now).as_nanos()),
        next_transition_overdue_ns = ?candidate.next_transition().and_then(|next| now.checked_duration_since(next)).map(|elapsed| elapsed.as_nanos()),
        observed_utc = %chrono::Utc::now().to_rfc3339(),
        "teletext monitor observed a sealed candidate"
    );
}

#[derive(Clone, Copy, Debug)]
struct ReadFailure;

// Private test seam; production candidates can only be recovered by the existing
// native validator. It adds no application bus, storage schema or public receipt.
#[async_trait::async_trait]
trait UpdateBackend: Send + Sync + 'static {
    type Candidate: Clone + Send + Sync + 'static;
    type ContextIdentity: Clone + Eq + Send + Sync + 'static;

    fn candidate_context(_candidate: &Self::Candidate) -> Option<Self::ContextIdentity> {
        None
    }
    fn current_context(&self) -> Option<Self::ContextIdentity> {
        None
    }
    fn generation_observation(
        candidate: &Self::Candidate,
    ) -> Option<TeletextOutputGenerationObservation> {
        super::super::teletext_output_observation::generation(Self::identity(candidate))
    }
    async fn reauthorize_cached(
        &self,
        candidate: &Self::Candidate,
        deadline: Instant,
    ) -> Result<Option<Self::Candidate>, ReadFailure> {
        Ok(self
            .confirm(candidate, deadline)
            .await?
            .then(|| candidate.clone()))
    }

    fn identity(candidate: &Self::Candidate) -> &TeletextAcceptedGeneration;
    fn service(candidate: &Self::Candidate) -> Option<Arc<EncodedTeletextAuthoredService>>;
    fn original_transition(_candidate: &Self::Candidate) -> Option<Instant> {
        None
    }
    async fn confirm_successor_selection(
        &self,
        _cached: &Self::Candidate,
        _successor: &TeletextAcceptedGeneration,
        _deadline: Instant,
    ) -> Result<bool, ReadFailure> {
        Ok(false)
    }
    fn next_transition(&self) -> Option<Instant> {
        None
    }
    fn take_expired_transition(&self, _now: Instant) -> bool {
        false
    }
    async fn current(&self) -> Result<Option<TeletextAcceptedGeneration>, ReadFailure>;
    async fn confirm(
        &self,
        candidate: &Self::Candidate,
        deadline: Instant,
    ) -> Result<bool, ReadFailure>;
    async fn recover(&self, deadline: Instant) -> Result<Option<Self::Candidate>, ReadFailure>;
    // Optional native work only; preparation never authorizes a proposal.
    async fn prepare_expiry(
        &self,
        _candidate: &Self::Candidate,
        _deadline: Instant,
    ) -> Result<(), ReadFailure> {
        Ok(())
    }
}

/// Preserve a native-qualified guide boundary across cheap metadata polling.
/// A new identity must obtain its own native proof; old expiry cannot authorize it.
fn guide_observation_transition(
    observed: Option<&TeletextAcceptedGeneration>,
    retained: Option<&(TeletextAcceptedGeneration, Instant)>,
    scope: Option<Instant>,
    now: Instant,
) -> (bool, Option<Instant>) {
    let guide = retained
        .filter(|(generation, _)| observed == Some(generation))
        .map(|(_, boundary)| *boundary);
    if guide.is_some_and(|boundary| now >= boundary) {
        return (true, scope);
    }
    (
        false,
        match (scope, guide) {
            (Some(scope), Some(guide)) => Some(scope.min(guide)),
            (scope, guide) => scope.or(guide),
        },
    )
}

fn guide_context_observation_transition<I: Eq>(
    observed: Option<&TeletextAcceptedGeneration>,
    context: Option<&I>,
    retained: Option<&(TeletextAcceptedGeneration, Option<I>, Instant)>,
    scope: Option<Instant>,
    now: Instant,
) -> (bool, Option<Instant>) {
    let matching = retained
        .filter(|(_, previous, _)| previous.as_ref() == context)
        .map(|(generation, _, deadline)| (generation.clone(), *deadline));
    guide_observation_transition(observed, matching.as_ref(), scope, now)
}

struct NativeBackend {
    trace_id: uuid::Uuid,
    context: TeletextAcceptedUpdatesContext,
    next_transition: Mutex<Option<Instant>>,
    expiry_preparation: Mutex<Option<ExpiryPreparation>>,
    guide_validity: Mutex<
        Option<(
            TeletextAcceptedGeneration,
            Option<GuideOutputIdentity>,
            Instant,
        )>,
    >,
    latest_guide: Mutex<Option<ersatzrs_core::repositories::TeletextGuideRefreshCurrentMetadata>>,
    prepared_handover: Mutex<Option<Arc<SelectedTeletextOutputCandidate>>>,
}

impl NativeBackend {
    async fn prepare_guide_handover(
        &self,
        cached: &SelectedTeletextOutputCandidate,
        successor: &TeletextAcceptedGeneration,
        deadline: Instant,
    ) -> Result<bool, ReadFailure> {
        let Some(guide) = self.context.guide.as_deref().filter(|guide| guide.refresh) else {
            return Ok(false);
        };
        if cached.generation() != successor || cached.guide_identity().is_none() {
            return Ok(false);
        }
        let deadline = cached
            .next_transition()
            .map_or(deadline, |cap| deadline.min(cap));
        if Instant::now() >= deadline {
            return Ok(false);
        }
        let prepared: Option<Arc<SelectedTeletextOutputCandidate>> =
            crate::transport::iptv::teletext_selected_output::prepare_current_guide_successor(
                &self.context.application,
                self.context.channel_id,
                self.context.budget,
                self.context.encoding.clone(),
                deadline.into_std(),
                guide,
                self.context.timing.poll_interval,
            )
            .await
            .map_err(|_| ReadFailure)?;
        let Some(prepared) = prepared else {
            return Ok(false);
        };
        if Instant::now() >= deadline
            || !cached.can_bridge_guide_successor(&prepared)
            || prepared.guide_identity().cloned() != self.current_context()
        {
            return Ok(false);
        }
        // This is one already compiled, currently confirmed successor, not an
        // authority for old bytes. Recovery consumes it once and confirms again.
        *self.prepared_handover.lock().map_err(|_| ReadFailure)? = Some(prepared);
        Ok(true)
    }

    pub(super) fn record_guide_validity(
        &self,
        candidate: &SelectedTeletextOutputCandidate,
    ) -> Result<(), ReadFailure> {
        *self.guide_validity.lock().map_err(|_| ReadFailure)? =
            candidate.guide_deadline().map(|deadline| {
                (
                    candidate.generation().clone(),
                    candidate.guide_identity().cloned(),
                    deadline,
                )
            });
        // A stale reauthorization can shorten its cap during this very poll.
        // Wake at that already qualified bound rather than one poll after it.
        if let Some(cap) = candidate.next_transition() {
            let mut next = self.next_transition.lock().map_err(|_| ReadFailure)?;
            *next = Some(next.map_or(cap, |previous| previous.min(cap)));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl UpdateBackend for NativeBackend {
    type Candidate = Arc<SelectedTeletextOutputCandidate>;
    type ContextIdentity = GuideOutputIdentity;
    fn candidate_context(candidate: &Self::Candidate) -> Option<Self::ContextIdentity> {
        candidate.guide_identity().cloned()
    }
    fn current_context(&self) -> Option<Self::ContextIdentity> {
        self.latest_guide
            .lock()
            .ok()
            .and_then(|metadata| metadata.as_ref().map(GuideOutputIdentity::from_metadata))
    }
    fn generation_observation(
        candidate: &Self::Candidate,
    ) -> Option<TeletextOutputGenerationObservation> {
        candidate.output_generation_observation()
    }
    async fn reauthorize_cached(
        &self,
        candidate: &Self::Candidate,
        deadline: Instant,
    ) -> Result<Option<Self::Candidate>, ReadFailure> {
        if candidate.guide_identity().is_some() {
            let metadata = self
                .latest_guide
                .lock()
                .map_err(|_| ReadFailure)?
                .clone()
                .ok_or(ReadFailure)?;
            let candidate = candidate
                .reauthorize_guide_cached(
                    &metadata,
                    &self.context.application,
                    self.context.budget.max_serialized_bytes,
                    deadline.into_std(),
                    self.context.timing.poll_interval,
                )
                .await;
            if let Some(candidate) = &candidate {
                self.record_guide_validity(candidate)?;
            }
            Ok(candidate)
        } else {
            Ok(self
                .confirm(candidate, deadline)
                .await?
                .then(|| candidate.clone()))
        }
    }

    fn identity(candidate: &Self::Candidate) -> &TeletextAcceptedGeneration {
        candidate.generation()
    }

    fn service(candidate: &Self::Candidate) -> Option<Arc<EncodedTeletextAuthoredService>> {
        candidate.service().map(Arc::clone)
    }

    fn original_transition(candidate: &Self::Candidate) -> Option<Instant> {
        candidate.next_transition()
    }

    async fn confirm_successor_selection(
        &self,
        cached: &Self::Candidate,
        successor: &TeletextAcceptedGeneration,
        deadline: Instant,
    ) -> Result<bool, ReadFailure> {
        if cached.guide_identity().is_some() {
            return self
                .prepare_guide_handover(cached, successor, deadline)
                .await;
        }
        Ok(cached
            .confirm_successor_selection_cached(
                successor,
                &self.context.application,
                self.context.budget.max_serialized_bytes,
                deadline.into_std(),
                self.context.timing.poll_interval,
            )
            .await)
    }

    fn next_transition(&self) -> Option<Instant> {
        self.next_transition.lock().ok().and_then(|next| *next)
    }

    fn take_expired_transition(&self, now: Instant) -> bool {
        if let Ok(mut next) = self.next_transition.lock() {
            if next.is_some_and(|boundary| now >= boundary) {
                *next = None;
                return true;
            }
        }
        false
    }

    async fn current(&self) -> Result<Option<TeletextAcceptedGeneration>, ReadFailure> {
        // Cancelled or superseded warm attempts cannot survive a new observation.
        self.prepared_handover
            .lock()
            .map_err(|_| ReadFailure)?
            .take();
        let observed = observe_selected_teletext_output_with_guide(
            &self.context.application,
            self.context.channel_id,
            self.context.guide.as_deref(),
            self.context.budget.max_serialized_bytes,
        )
        .await
        .map_err(|_| ReadFailure)?;
        let context = observed.guide_identity();
        let retained = self.guide_validity.lock().map_err(|_| ReadFailure)?;
        let (expired, next) = guide_context_observation_transition(
            observed.generation.as_ref(),
            context.as_ref(),
            retained.as_ref(),
            observed.next_transition,
            Instant::now(),
        );
        *self.latest_guide.lock().map_err(|_| ReadFailure)? = observed.guide_refresh;
        *self.next_transition.lock().map_err(|_| ReadFailure)? = next;
        // Absent after expiry without rescheduling a past deadline. The retained
        // record is diagnostic metadata, never renewed current-guide authority.
        Ok(if expired { None } else { observed.generation })
    }

    async fn confirm(
        &self,
        candidate: &Self::Candidate,
        deadline: Instant,
    ) -> Result<bool, ReadFailure> {
        Ok(candidate
            .confirm_cached(
                &self.context.application,
                self.context.budget.max_serialized_bytes,
                deadline.into_std(),
                self.context.timing.poll_interval,
            )
            .await)
    }

    async fn recover(&self, deadline: Instant) -> Result<Option<Self::Candidate>, ReadFailure> {
        let prepared = self
            .prepared_handover
            .lock()
            .map_err(|_| ReadFailure)?
            .take();
        if let Some(prepared) = prepared {
            self.record_guide_validity(&prepared)?;
            trace_native_candidate(&prepared, self.trace_id, "monitor_recovered");
            return Ok(Some(prepared));
        }
        if let Some(candidate) = self.recover_prepared_expiry(deadline).await? {
            return Ok(Some(candidate));
        }
        let candidate = if let Some(guide) = &self.context.guide {
            load_selected_teletext_output_candidate_with_guide(
                &self.context.application,
                self.context.channel_id,
                self.context.budget,
                self.context.encoding.clone(),
                deadline.into_std(),
                Some(guide),
            )
            .await
        } else {
            load_selected_teletext_output_candidate(
                &self.context.application,
                self.context.channel_id,
                self.context.budget,
                self.context.encoding.clone(),
                deadline.into_std(),
            )
            .await
        }
        .map_err(|_| ReadFailure)?;
        if let Some(candidate) = &candidate {
            self.record_guide_validity(candidate)?;
            trace_native_candidate(candidate, self.trace_id, "monitor_recovered");
        }
        Ok(candidate)
    }

    async fn prepare_expiry(
        &self,
        candidate: &Self::Candidate,
        deadline: Instant,
    ) -> Result<(), ReadFailure> {
        self.prepare_native_expiry(candidate, deadline).await
    }
}

struct MonitorState<C, I = ()> {
    cached: Option<C>,
    active: bool,
    sequence: u64,
    proposal: Option<(u64, Option<TeletextAcceptedGeneration>)>,
    rejected: Option<TeletextAcceptedGeneration>,
    proposal_context: Option<I>,
    rejected_context: Option<I>,
    // Seeded content is already installed. Later proposals gain this witness
    // only through Applied for their exact sequence, never cached/Waiting state.
    handover_confirmed: bool,
    // The expiry withdrawal can be coalesced by the bounded latest-state slot.
    // Unacknowledged successors must withdraw at the same ACK if rejected.
    expiry_replacement: bool,
    observed_absence: Option<i64>,
}

impl<C, I: Clone> MonitorState<C, I> {
    fn new(initial: Option<C>) -> Self {
        Self {
            active: initial.is_some(),
            handover_confirmed: initial.is_some(),
            cached: initial,
            sequence: 0,
            proposal: None,
            rejected: None,
            proposal_context: None,
            rejected_context: None,
            expiry_replacement: false,
            observed_absence: None,
        }
    }

    fn withdraw(&mut self, updates: &watch::Sender<Option<TeletextOutputUpdate>>) -> bool {
        self.withdraw_with_identity(updates, None)
    }

    fn withdraw_with_identity(
        &mut self,
        updates: &watch::Sender<Option<TeletextOutputUpdate>>,
        identity: Option<&TeletextAcceptedGeneration>,
    ) -> bool {
        self.handover_confirmed = false;
        if !self.active
            && identity.is_none_or(|identity| self.observed_absence == Some(identity.id))
        {
            return true;
        }
        let Some(sequence) = self.sequence.checked_add(1) else {
            return false;
        };
        let Ok(update) = TeletextOutputUpdate::withdraw(sequence) else {
            return false;
        };
        let update = match identity {
            Some(identity) => update.with_generation(identity),
            None => update,
        };
        self.sequence = sequence;
        self.active = false;
        self.observed_absence = identity.map(|identity| identity.id);
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event = "monitor_proposal", sequence, action = "withdraw",
            observed_utc = %chrono::Utc::now().to_rfc3339(),
            "teletext monitor proposed withdrawal"
        );
        self.proposal = Some((sequence, None));
        self.proposal_context = None;
        updates.send_replace(Some(update));
        true
    }

    fn replace(
        &mut self,
        updates: &watch::Sender<Option<TeletextOutputUpdate>>,
        identity: &TeletextAcceptedGeneration,
        service: Arc<EncodedTeletextAuthoredService>,
        context: Option<I>,
        observation: Option<TeletextOutputGenerationObservation>,
    ) -> bool {
        // Reserve the last representable sequence for fail-closed withdrawal.
        let Some(sequence) = self
            .sequence
            .checked_add(1)
            .filter(|value| *value < u64::MAX)
        else {
            return false;
        };
        let update = if self.expiry_replacement {
            TeletextOutputUpdate::replace_after_expiry(sequence, service)
        } else {
            TeletextOutputUpdate::replace(sequence, service)
        };
        let Ok(update) = update else {
            return false;
        };
        let update = update.with_generation_observation(observation);
        self.sequence = sequence;
        self.active = true;
        self.observed_absence = None;
        self.handover_confirmed = false;
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event = "monitor_proposal", sequence, action = "replace",
            channel_id = identity.request.channel_id,
            generation_id = identity.id, request_id = identity.request.id,
            design_id = identity.request.design_id, revision = identity.request.revision,
            observed_utc = %chrono::Utc::now().to_rfc3339(),
            "teletext monitor proposed a recovered generation"
        );
        self.proposal = Some((sequence, Some(identity.clone())));
        self.proposal_context = context;
        updates.send_replace(Some(update));
        true
    }

    fn observe_rejection(
        &mut self,
        status: Option<(u64, TeletextOutputTransitionPhase)>,
        updates: &watch::Sender<Option<TeletextOutputUpdate>>,
        channel_id: i64,
    ) -> bool {
        if let Some((sequence, TeletextOutputTransitionPhase::Applied)) = status {
            if let Some((proposed, identity)) = &self.proposal {
                if *proposed == sequence {
                    // Submission, obsolete ACKs and rejection cannot retire the
                    // expired service. Keep fallback on every superseding
                    // proposal until this exact replacement/removal is applied.
                    self.expiry_replacement = false;
                    self.handover_confirmed = identity.is_some();
                }
            }
            return true;
        }
        let Some((sequence, TeletextOutputTransitionPhase::Rejected(reason))) = status else {
            return true;
        };
        let Some((proposal_sequence, identity)) = &self.proposal else {
            return true;
        };
        if sequence != *proposal_sequence {
            return true;
        }
        tracing::warn!(
            channel_id,
            sequence,
            ?reason,
            "live teletext proposal rejected; retaining ordinary playback"
        );
        if let Some(identity) = identity {
            self.rejected = Some(identity.clone());
            self.rejected_context = self.proposal_context.clone();
            // A rejected replacement leaves the old service in the gate. The
            // newer withdrawal retires that unauthorized service safely.
            self.withdraw(updates)
        } else {
            // Valid withdrawal invariant faults are terminal in PumpControl.
            // With this exclusive monotonic sender, a Rejected withdrawal cannot
            // be a normal current/source state. Preserve its explicit failure:
            // never fabricate Applied, renew old authority or start a retry loop.
            tracing::error!(channel_id, sequence, ?reason, "teletext withdrawal ownership failed; monitor stopped without an applied acknowledgement");
            false
        }
    }
}

/// None means the last output receiver disappeared. Native blocking work already
/// owns its own hold and deadline when the cancelled await is dropped here.
async fn bounded<T>(
    updates: &watch::Sender<Option<TeletextOutputUpdate>>,
    deadline: Instant,
    work: impl Future<Output = Result<T, ReadFailure>>,
) -> Option<Result<T, ReadFailure>> {
    tokio::select! {
        biased;
        () = updates.closed() => None,
        result = tokio::time::timeout_at(deadline, work) => Some(match result {
            Ok(result) if Instant::now() < deadline => result,
            _ => Err(ReadFailure),
        }),
    }
}

async fn reconcile<B: UpdateBackend>(
    backend: &B,
    state: &mut MonitorState<B::Candidate, B::ContextIdentity>,
    updates: &watch::Sender<Option<TeletextOutputUpdate>>,
    channel_id: i64,
    deadline: Instant,
) -> bool {
    let deadline = state
        .cached
        .as_ref()
        // Only retained live content carries the old authority boundary.
        // After withdrawal, its cached bytes cannot fence a new selection.
        .filter(|_| state.active)
        .and_then(B::original_transition)
        .map_or(deadline, |boundary| deadline.min(boundary));
    let Some(current) = bounded(updates, deadline, backend.current()).await else {
        state.withdraw(updates);
        return false;
    };
    let identity = match current {
        Ok(Some(identity)) if identity.request.channel_id == channel_id => identity,
        Ok(None) => {
            state.cached = None;
            return state.withdraw(updates);
        }
        _ => return state.withdraw(updates),
    };
    let context = backend.current_context();
    if state.rejected.as_ref() == Some(&identity)
        && state.rejected_context.as_ref() == context.as_ref()
    {
        return state.withdraw(updates);
    }
    if state.cached.as_ref().is_some_and(|candidate| {
        B::identity(candidate) == &identity && B::candidate_context(candidate) == context
    }) {
        let Some(candidate) = state.cached.as_ref() else {
            return false;
        };
        let previous = B::generation_observation(candidate);
        let Some(confirmed) = bounded(
            updates,
            deadline,
            backend.reauthorize_cached(candidate, deadline),
        )
        .await
        else {
            return false;
        };
        let Ok(Some(candidate)) = confirmed else {
            return state.withdraw(updates);
        };
        if B::identity(&candidate) != &identity || B::candidate_context(&candidate) != context {
            return state.withdraw(updates);
        }
        let observation = B::generation_observation(&candidate);
        let result = match B::service(&candidate) {
            Some(_) if state.active && observation == previous => true,
            Some(service) => state.replace(
                updates,
                &identity,
                service,
                B::candidate_context(&candidate),
                observation,
            ),
            None => state.withdraw_with_identity(updates, Some(&identity)),
        };
        if result {
            state.cached = Some(candidate);
        }
        return result;
    }
    let retain = authorize_handover(backend, state, updates, &identity, deadline).await;
    // All legacy, Off, changed-selection and unproven cases still withdraw
    // before recovery. Only a fresh same-selection authorization permits the
    // existing active service to bridge this same bounded operation.
    if !retain && !state.withdraw(updates) {
        return false;
    }
    recover_successor(
        backend, state, updates, channel_id, &identity, deadline, retain,
    )
    .await
}

async fn authorize_handover<B: UpdateBackend>(
    backend: &B,
    state: &MonitorState<B::Candidate, B::ContextIdentity>,
    updates: &watch::Sender<Option<TeletextOutputUpdate>>,
    successor: &TeletextAcceptedGeneration,
    deadline: Instant,
) -> bool {
    let Some(cached) = state.cached.as_ref().filter(|_| state.active) else {
        return false;
    };
    if !state.handover_confirmed
        || state.proposal.as_ref().is_some_and(|(_, identity)| {
            identity.as_ref() != Some(B::identity(cached))
                || state.proposal_context != B::candidate_context(cached)
        })
        || B::identity(cached).artifact_version != successor.artifact_version
        || !matches!(
            successor.artifact_version,
            TELETEXT_LIFECYCLE_GENERATION_ARTIFACT_VERSION
                | TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION
                | TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION
                | TELETEXT_EFFECTIVE_GUIDE_GENERATION_ARTIFACT_VERSION
        )
        || B::service(cached).is_none_or(|service| service.resource_lifecycle_variants().is_none())
    {
        return false;
    }
    matches!(
        bounded(
            updates,
            deadline,
            backend.confirm_successor_selection(cached, successor, deadline)
        )
        .await,
        Some(Ok(true))
    )
}

async fn recover_successor<B: UpdateBackend>(
    backend: &B,
    state: &mut MonitorState<B::Candidate, B::ContextIdentity>,
    updates: &watch::Sender<Option<TeletextOutputUpdate>>,
    channel_id: i64,
    observed: &TeletextAcceptedGeneration,
    deadline: Instant,
    retain: bool,
) -> bool {
    let Some(recovered) = bounded(updates, deadline, backend.recover(deadline)).await else {
        state.withdraw(updates);
        return false;
    };
    let candidate = match recovered {
        Ok(Some(candidate)) if B::identity(&candidate).request.channel_id == channel_id => {
            candidate
        }
        _ => return state.withdraw(updates),
    };
    if (state.rejected.as_ref() == Some(B::identity(&candidate))
        && state.rejected_context == B::candidate_context(&candidate))
        || (retain
            && (B::identity(&candidate) != observed
                || B::candidate_context(&candidate) != backend.current_context()))
    {
        return state.withdraw(updates);
    }
    // The successor authorization never carries across another identity. A
    // descriptor change retains the pre-existing withdrawal/reannounce path;
    // content-only replacement reaches the gate's atomic ACK/retirement checks.
    if retain {
        let same_descriptor = state
            .cached
            .as_ref()
            .and_then(B::service)
            .zip(B::service(&candidate))
            .is_some_and(|(old, new)| {
                TeletextServiceDescriptor::from_authored_service(&old)
                    == TeletextServiceDescriptor::from_authored_service(&new)
            });
        if !same_descriptor && !state.withdraw(updates) {
            return false;
        }
    }
    let Some(confirmed) = bounded(updates, deadline, backend.confirm(&candidate, deadline)).await
    else {
        state.withdraw(updates);
        return false;
    };
    if !matches!(confirmed, Ok(true)) {
        return state.withdraw(updates);
    }
    // No await separates exact full-artifact/current confirmation from proposal.
    let replacement = match B::service(&candidate) {
        Some(service) => state.replace(
            updates,
            B::identity(&candidate),
            service,
            B::candidate_context(&candidate),
            B::generation_observation(&candidate),
        ),
        None => state.withdraw_with_identity(updates, Some(B::identity(&candidate))),
    };
    if replacement {
        state.cached = Some(candidate);
        state.rejected = None;
        state.rejected_context = None;
    }
    replacement
}

async fn run_monitor<B: UpdateBackend>(
    backend: B,
    channel_id: i64,
    timing: TeletextAcceptedUpdatePolicy,
    initial: Option<B::Candidate>,
    updates: watch::Sender<Option<TeletextOutputUpdate>>,
    mut observe: impl FnMut() -> Option<(u64, TeletextOutputTransitionPhase)> + Send + 'static,
) {
    let mut state = MonitorState::new(initial);
    state.active = state.cached.as_ref().and_then(B::service).is_some();
    if !state.active {
        state.observed_absence = state
            .cached
            .as_ref()
            .map(|candidate| B::identity(candidate).id);
    }
    loop {
        if updates.is_closed() || !state.observe_rejection(observe(), &updates, channel_id) {
            break;
        }
        if backend.take_expired_transition(Instant::now()) {
            tracing::debug!(
                target: "ersatzrs_web::transport::iptv::teletext_provenance",
                event = "scope_boundary_expired",
                generation_id = ?state.cached.as_ref().map(|candidate| B::identity(candidate).id),
                design_id = ?state.cached.as_ref().map(|candidate| B::identity(candidate).request.design_id),
                revision = ?state.cached.as_ref().map(|candidate| B::identity(candidate).request.revision),
                observed_utc = %chrono::Utc::now().to_rfc3339(),
                "teletext monitor reached its original transition boundary"
            );
            // Withdraw first; a blocked read cannot extend the previous scope.
            state.cached = None;
            state.expiry_replacement = true;
            if !state.withdraw(&updates) {
                break;
            }
        }
        let Some(mut deadline) = Instant::now().checked_add(timing.recovery_timeout) else {
            state.withdraw(&updates);
            break;
        };
        if let Some(boundary) = backend.next_transition() {
            deadline = deadline.min(boundary);
        }
        if !reconcile(&backend, &mut state, &updates, channel_id, deadline).await {
            state.withdraw(&updates);
            break;
        }
        if let Some(candidate) = &state.cached {
            let preparation_deadline = B::original_transition(candidate)
                .map_or(deadline, |boundary| deadline.min(boundary));
            if Instant::now() < preparation_deadline
                && bounded(
                    &updates,
                    preparation_deadline,
                    backend.prepare_expiry(candidate, preparation_deadline),
                )
                .await
                .is_none()
            {
                state.withdraw(&updates);
                break;
            }
        }
        let Some(mut next) = Instant::now().checked_add(timing.poll_interval) else {
            state.withdraw(&updates);
            break;
        };
        if let Some(boundary) = backend.next_transition() {
            next = next.min(boundary);
        }
        // One operation at a time, no tick catch-up burst or detached recovery.
        tokio::select! {
            biased;
            () = updates.closed() => break,
            () = tokio::time::sleep_until(next) => {},
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::TeletextPublicationRequest;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };
    use tokio::sync::Notify;

    #[derive(Clone)]
    struct Candidate {
        identity: TeletextAcceptedGeneration,
        service: Arc<EncodedTeletextAuthoredService>,
        service_present: bool,
        original_transition: Option<Instant>,
        selection_context: u8,
        guide_context: Option<u64>,
        guide_revision: i64,
        guide_stale: bool,
    }

    fn candidate(id: i64, channel_id: i64) -> Candidate {
        Candidate {
            identity: TeletextAcceptedGeneration {
                id,
                request: TeletextPublicationRequest {
                    id,
                    channel_id,
                    design_id: 7,
                    revision: id,
                    digest: "a".repeat(64),
                },
                artifact_version: 1,
                artifact_digest: "b".repeat(64),
            },
            service_present: true,
            original_transition: None,
            selection_context: 1,
            guide_context: None,
            guide_revision: 1,
            guide_stale: false,
            service: Arc::new(
                crate::transport::iptv::teletext_binding::authored_tests::authored_service_with_counter(3),
            ),
        }
    }

    #[test]
    fn guide_metadata_polls_preserve_original_expiry_without_renewal_or_busy_loop() {
        let current = candidate(1, 3).identity;
        let successor = candidate(2, 3).identity;
        let now = Instant::now();
        let boundary = now + Duration::from_secs(5);
        let later_scope = now + Duration::from_secs(15);
        let retained = (current.clone(), boundary);
        assert_eq!(
            guide_observation_transition(Some(&current), Some(&retained), None, now),
            (false, Some(boundary))
        );
        assert_eq!(
            guide_observation_transition(Some(&current), Some(&retained), Some(later_scope), now),
            (false, Some(boundary))
        );
        let earlier_scope = now + Duration::from_secs(2);
        assert_eq!(
            guide_observation_transition(Some(&current), Some(&retained), Some(earlier_scope), now),
            (false, Some(earlier_scope))
        );
        assert_eq!(
            guide_observation_transition(Some(&current), Some(&retained), None, boundary),
            (true, None)
        );
        assert_eq!(
            guide_observation_transition(
                Some(&current),
                Some(&retained),
                Some(later_scope),
                boundary
            ),
            (true, Some(later_scope))
        );
        // Another generation is not qualified by its predecessor's proof.
        assert_eq!(
            guide_observation_transition(Some(&successor), Some(&retained), None, boundary),
            (false, None)
        );
        assert_eq!(
            guide_observation_transition(None, Some(&retained), None, boundary),
            (false, None)
        );
    }

    #[test]
    fn confirmed_off_proposes_one_identified_absence_without_repeated_noop_updates() {
        let (updates, _control, _) = teletext_output_control_channel();
        let mut state = MonitorState::<Candidate>::new(None);
        let mut off = candidate(11, 44).identity;
        off.artifact_version = TELETEXT_EFFECTIVE_LIFECYCLE_GENERATION_ARTIFACT_VERSION;
        assert!(state.withdraw_with_identity(&updates, Some(&off)));
        assert!(!state.active);
        assert_eq!(state.sequence, 1);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 1);
        assert!(state.withdraw_with_identity(&updates, Some(&off)));
        assert_eq!(state.sequence, 1);
        off.id = 12;
        assert!(state.withdraw_with_identity(&updates, Some(&off)));
        assert_eq!(state.sequence, 2);
        assert!(state.withdraw(&updates));
        assert_eq!(state.sequence, 2);
    }

    #[derive(Clone, Copy)]
    enum RecoveryScenario {
        Current,
        Absent,
        Unavailable,
    }

    struct Store {
        accepted: Option<Candidate>,
        eligible: bool,
        read_failure: bool,
        recovery: RecoveryScenario,
        handover: Result<bool, ReadFailure>,
        after_handover: Option<Candidate>,
        read_release: Option<Arc<Notify>>,
        recovery_release: Option<Arc<Notify>>,
    }

    #[derive(Clone)]
    struct Backend {
        store: Arc<Mutex<Store>>,
        reads: Arc<AtomicUsize>,
        recoveries: Arc<AtomicUsize>,
        confirmations: Arc<Mutex<Vec<i64>>>,
        read_entered: Arc<Notify>,
        recovery_entered: Arc<Notify>,
        recovering: Arc<AtomicUsize>,
        handover_checks: Arc<AtomicUsize>,
        transition: Arc<Mutex<Option<Instant>>>,
        preparation_release: Arc<Mutex<Option<Arc<Notify>>>>,
        preparation_entered: Arc<Notify>,
    }

    impl Backend {
        fn new(accepted: Option<Candidate>) -> Self {
            Self {
                store: Arc::new(Mutex::new(Store {
                    accepted,
                    eligible: true,
                    read_failure: false,
                    recovery: RecoveryScenario::Current,
                    handover: Ok(false),
                    after_handover: None,
                    read_release: None,
                    recovery_release: None,
                })),
                reads: Arc::new(AtomicUsize::new(0)),
                recoveries: Arc::new(AtomicUsize::new(0)),
                confirmations: Arc::new(Mutex::new(Vec::new())),
                read_entered: Arc::new(Notify::new()),
                recovery_entered: Arc::new(Notify::new()),
                recovering: Arc::new(AtomicUsize::new(0)),
                handover_checks: Arc::new(AtomicUsize::new(0)),
                transition: Arc::new(Mutex::new(None)),
                preparation_release: Arc::new(Mutex::new(None)),
                preparation_entered: Arc::new(Notify::new()),
            }
        }
    }

    struct RecoveryFuture(Arc<AtomicUsize>);
    impl Drop for RecoveryFuture {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[async_trait::async_trait]
    impl UpdateBackend for Backend {
        type Candidate = Candidate;
        type ContextIdentity = u64;
        fn candidate_context(candidate: &Candidate) -> Option<u64> {
            candidate.guide_context
        }
        fn current_context(&self) -> Option<u64> {
            self.store
                .lock()
                .unwrap()
                .accepted
                .as_ref()
                .and_then(|candidate| candidate.guide_context)
        }
        fn generation_observation(
            candidate: &Candidate,
        ) -> Option<TeletextOutputGenerationObservation> {
            let mut observation =
                super::super::super::teletext_output_observation::generation(&candidate.identity)?;
            observation.guide_refresh = candidate.guide_context.map(|snapshot| {
                ersatzrs_domain::TeletextGuideRefreshOutputObservation {
                    seed_id: 1,
                    snapshot_id: Some(i64::try_from(snapshot).unwrap()),
                    revision: candidate.guide_revision,
                    captured_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
                    stale: candidate.guide_stale,
                }
            });
            Some(observation)
        }
        async fn reauthorize_cached(
            &self,
            candidate: &Candidate,
            deadline: Instant,
        ) -> Result<Option<Candidate>, ReadFailure> {
            if candidate.guide_context.is_some() {
                let store = self.store.lock().unwrap();
                Ok(store
                    .accepted
                    .as_ref()
                    .filter(|current| {
                        store.eligible
                            && current.identity == candidate.identity
                            && current.guide_context == candidate.guide_context
                    })
                    .map(|current| {
                        let mut current = current.clone();
                        current.original_transition =
                            match (candidate.original_transition, current.original_transition) {
                                (Some(old), Some(new)) => Some(old.min(new)),
                                (old, new) => old.or(new),
                            };
                        current
                    }))
            } else {
                Ok(self
                    .confirm(candidate, deadline)
                    .await?
                    .then(|| candidate.clone()))
            }
        }

        fn identity(candidate: &Candidate) -> &TeletextAcceptedGeneration {
            &candidate.identity
        }
        fn service(candidate: &Candidate) -> Option<Arc<EncodedTeletextAuthoredService>> {
            candidate
                .service_present
                .then(|| Arc::clone(&candidate.service))
        }
        fn original_transition(candidate: &Candidate) -> Option<Instant> {
            candidate.original_transition
        }

        async fn prepare_expiry(
            &self,
            candidate: &Candidate,
            _deadline: Instant,
        ) -> Result<(), ReadFailure> {
            if candidate.original_transition.is_some() {
                let release = self.preparation_release.lock().unwrap().clone();
                self.preparation_entered.notify_one();
                if let Some(release) = release {
                    release.notified().await;
                }
            }
            Ok(())
        }

        async fn confirm_successor_selection(
            &self,
            cached: &Candidate,
            successor: &TeletextAcceptedGeneration,
            _deadline: Instant,
        ) -> Result<bool, ReadFailure> {
            self.handover_checks.fetch_add(1, Ordering::SeqCst);
            let mut store = self.store.lock().unwrap();
            let confirmed = store.handover?
                && store.accepted.as_ref().is_some_and(|candidate| {
                    &candidate.identity == successor
                        && candidate.selection_context == cached.selection_context
                });
            if confirmed {
                if let Some(next) = store.after_handover.take() {
                    store.accepted = Some(next);
                }
            }
            Ok(confirmed)
        }

        fn next_transition(&self) -> Option<Instant> {
            *self.transition.lock().unwrap()
        }
        fn take_expired_transition(&self, now: Instant) -> bool {
            let mut next = self.transition.lock().unwrap();
            if next.is_some_and(|boundary| now >= boundary) {
                *next = None;
                true
            } else {
                false
            }
        }
        async fn current(&self) -> Result<Option<TeletextAcceptedGeneration>, ReadFailure> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            let (value, failure, release) = {
                let store = self.store.lock().unwrap();
                (
                    store.accepted.clone(),
                    store.read_failure,
                    store.read_release.clone(),
                )
            };
            self.read_entered.notify_one();
            if let Some(release) = release {
                release.notified().await;
            }
            if failure {
                Err(ReadFailure)
            } else {
                Ok(value.map(|candidate| candidate.identity))
            }
        }
        async fn confirm(
            &self,
            candidate: &Candidate,
            _deadline: Instant,
        ) -> Result<bool, ReadFailure> {
            self.confirmations
                .lock()
                .unwrap()
                .push(candidate.identity.id);
            let store = self.store.lock().unwrap();
            Ok(store.eligible
                && store.accepted.as_ref().is_some_and(|current| {
                    current.identity == candidate.identity
                        && current.guide_context == candidate.guide_context
                }))
        }
        async fn recover(&self, _deadline: Instant) -> Result<Option<Candidate>, ReadFailure> {
            self.recoveries.fetch_add(1, Ordering::SeqCst);
            self.recovering.fetch_add(1, Ordering::SeqCst);
            let _future = RecoveryFuture(Arc::clone(&self.recovering));
            let (candidate, scenario, release) = {
                let store = self.store.lock().unwrap();
                (
                    if matches!(store.recovery, RecoveryScenario::Absent) {
                        None
                    } else {
                        store.accepted.clone()
                    },
                    store.recovery,
                    store.recovery_release.clone(),
                )
            };
            self.recovery_entered.notify_one();
            if let Some(release) = release {
                release.notified().await;
            }
            match scenario {
                RecoveryScenario::Unavailable => Err(ReadFailure),
                RecoveryScenario::Current | RecoveryScenario::Absent => Ok(candidate),
            }
        }
    }

    fn timing() -> TeletextAcceptedUpdatePolicy {
        TeletextAcceptedUpdatePolicy::new(Duration::from_secs(2), Duration::from_secs(5)).unwrap()
    }
    fn deadline() -> Instant {
        Instant::now() + timing().recovery_timeout
    }

    #[test]
    fn invalid_monitor_policy_cannot_start_unbounded_reads_or_zero_cadence() {
        for (poll, recovery) in [
            (Duration::ZERO, Duration::from_secs(1)),
            (Duration::from_secs(1), Duration::ZERO),
            (Duration::MAX, Duration::from_secs(1)),
            (Duration::from_secs(1), Duration::MAX),
        ] {
            assert!(TeletextAcceptedUpdatePolicy::new(poll, recovery).is_err());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn durable_off_and_replacement_are_observed_without_mutation_hints() {
        let initial = candidate(1, 9);
        let backend = Backend::new(Some(initial.clone()));
        let (updates, _control, _) = teletext_output_control_channel();
        let mut state = MonitorState::new(Some(initial));
        backend.store.lock().unwrap().eligible = false;
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(!state.active);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 1);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 0);
        {
            let mut store = backend.store.lock().unwrap();
            store.accepted = Some(candidate(2, 9));
            store.eligible = true;
        }
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(state.active);
        assert_eq!(state.cached.as_ref().unwrap().identity.id, 2);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn initially_inactive_output_waits_for_accepted_state_and_isolates_other_channels() {
        let backend = Backend::new(None);
        let other = Backend::new(Some(candidate(1, 10)));
        let (updates, _control, _) = teletext_output_control_channel();
        let (other_updates, _other_control, _) = teletext_output_control_channel();
        let mut state = MonitorState::new(None);
        let mut other_state = MonitorState::new(other.store.lock().unwrap().accepted.clone());
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(updates.borrow().is_none());
        backend.store.lock().unwrap().accepted = Some(candidate(2, 9));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(reconcile(&other, &mut other_state, &other_updates, 10, deadline()).await);
        assert!(state.active);
        assert_eq!(
            state.cached.as_ref().unwrap().identity.request.channel_id,
            9
        );
        assert!(other_state.active);
        assert!(other_updates.borrow().is_none());
        assert_eq!(other.recoveries.load(Ordering::SeqCst), 0);
        backend.store.lock().unwrap().accepted = Some(candidate(3, 10));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(!state.active);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn accepted_change_during_recovery_cannot_publish_obsolete_result() {
        let backend = Backend::new(Some(candidate(2, 9)));
        let release = Arc::new(Notify::new());
        backend.store.lock().unwrap().recovery_release = Some(Arc::clone(&release));
        let (updates, _control, _) = teletext_output_control_channel();
        let task_backend = backend.clone();
        let task_updates = updates.clone();
        let task = tokio::spawn(async move {
            let mut state = MonitorState::new(Some(candidate(1, 9)));
            assert!(reconcile(&task_backend, &mut state, &task_updates, 9, deadline()).await);
            state
        });
        backend.recovery_entered.notified().await;
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 1);
        {
            let mut store = backend.store.lock().unwrap();
            store.accepted = Some(candidate(3, 9));
            store.recovery_release = None;
        }
        release.notify_one();
        let mut state = task.await.unwrap();
        assert!(!state.active);
        assert_eq!(*backend.confirmations.lock().unwrap(), vec![2]);
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(state.cached.as_ref().unwrap().identity.id, 3);
        assert_eq!(*backend.confirmations.lock().unwrap(), vec![2, 3]);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn bounded_read_failure_and_recovery_failure_withdraw_without_renewing_old_authority() {
        let backend = Backend::new(Some(candidate(1, 9)));
        backend.store.lock().unwrap().read_release = Some(Arc::new(Notify::new()));
        let (updates, _control, _) = teletext_output_control_channel();
        let task_backend = backend.clone();
        let task_updates = updates.clone();
        let task = tokio::spawn(async move {
            let mut state = MonitorState::new(Some(candidate(1, 9)));
            assert!(reconcile(&task_backend, &mut state, &task_updates, 9, deadline()).await);
            state
        });
        backend.read_entered.notified().await;
        tokio::time::advance(timing().recovery_timeout).await;
        let mut state = task.await.unwrap();
        assert!(!state.active);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 1);
        {
            let mut store = backend.store.lock().unwrap();
            store.read_release = None;
            store.read_failure = true;
        }
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(!state.active);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 1);
        {
            let mut store = backend.store.lock().unwrap();
            store.read_release = None;
            store.read_failure = false;
            store.accepted = Some(candidate(2, 9));
            store.recovery = RecoveryScenario::Unavailable;
        }
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(!state.active);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 1);
        backend.store.lock().unwrap().recovery = RecoveryScenario::Current;
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(state.active);
        assert_eq!(state.cached.as_ref().unwrap().identity.id, 2);
    }

    #[tokio::test(start_paused = true)]
    async fn unchanged_rejected_generation_is_withdrawn_once_without_recovery_retry_loop() {
        use crate::transport::iptv::teletext_binding::TeletextOutputTransitionRejection;
        let backend = Backend::new(Some(candidate(2, 9)));
        let (updates, _control, _) = teletext_output_control_channel();
        let mut state = MonitorState::new(Some(candidate(1, 9)));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(state.observe_rejection(
            Some((
                2,
                TeletextOutputTransitionPhase::Rejected(
                    TeletextOutputTransitionRejection::CacheReconciliation
                )
            )),
            &updates,
            9
        ));
        assert!(!state.active);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 3);
        for _ in 0..3 {
            assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        }
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 1);
        assert_eq!(*backend.confirmations.lock().unwrap(), vec![2]);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 3);
        backend.store.lock().unwrap().accepted = Some(candidate(3, 9));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(state.active);
        assert!(state.rejected.is_none());
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 4);
        assert!(state.observe_rejection(
            Some((
                2,
                TeletextOutputTransitionPhase::Rejected(
                    TeletextOutputTransitionRejection::ResourceUpdate
                )
            )),
            &updates,
            9
        ));
        assert!(
            state.active,
            "an obsolete rejection cannot withdraw a newer proposal"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn unexpected_rejected_withdrawal_stops_without_claiming_applied_or_retrying() {
        use crate::transport::iptv::teletext_binding::TeletextOutputTransitionRejection;
        let initial = candidate(1, 9);
        let backend = Backend::new(Some(initial.clone()));
        backend.store.lock().unwrap().eligible = false;
        let (updates, _control, _) = teletext_output_control_channel();
        let mut proposals = updates.subscribe();
        let observed = Arc::new(Mutex::new(None));
        let task_observed = Arc::clone(&observed);
        let task = tokio::spawn(run_monitor(
            backend.clone(),
            9,
            timing(),
            Some(initial),
            updates,
            move || *task_observed.lock().unwrap(),
        ));
        let owner = TeletextAcceptedUpdatesOwner { task: Some(task) };
        proposals.changed().await.unwrap();
        assert_eq!(proposals.borrow().as_ref().unwrap().sequence(), 1);
        let rejection = Some((
            1,
            TeletextOutputTransitionPhase::Rejected(TeletextOutputTransitionRejection::Sequence),
        ));
        *observed.lock().unwrap() = rejection;
        tokio::time::advance(timing().poll_interval).await;
        owner.stopped().await.unwrap();
        assert_eq!(*observed.lock().unwrap(), rejection);
        assert!(proposals.has_changed().is_err());
        assert_eq!(proposals.borrow().as_ref().unwrap().sequence(), 1);
        assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 0);
        tokio::time::advance(Duration::from_mins(1)).await;
        assert_eq!(backend.reads.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn receiver_closure_cancels_blocked_recovery_and_stops_future_polls() {
        let backend = Backend::new(Some(candidate(2, 9)));
        backend.store.lock().unwrap().recovery_release = Some(Arc::new(Notify::new()));
        let (updates, control, _) = teletext_output_control_channel();
        let task = tokio::spawn(run_monitor(
            backend.clone(),
            9,
            timing(),
            None,
            updates,
            || None,
        ));
        let owner = TeletextAcceptedUpdatesOwner { task: Some(task) };
        backend.recovery_entered.notified().await;
        assert_eq!(backend.recovering.load(Ordering::SeqCst), 1);
        drop(control);
        owner.stopped().await.unwrap();
        assert_eq!(backend.recovering.load(Ordering::SeqCst), 0);
        let reads = backend.reads.load(Ordering::SeqCst);
        tokio::time::advance(Duration::from_mins(1)).await;
        assert_eq!(backend.reads.load(Ordering::SeqCst), reads);
    }
    #[tokio::test(start_paused = true)]
    async fn accepted_absence_is_cached_and_confirmed_without_repeated_recovery() {
        let initial = candidate(1, 9);
        let mut absent = candidate(2, 9);
        absent.service_present = false;
        let backend = Backend::new(Some(absent));
        let (updates, _control, _) = teletext_output_control_channel();
        let mut state = MonitorState::new(Some(initial));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert!(!state.active);
        assert_eq!(state.cached.as_ref().unwrap().identity.id, 2);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 1);
        // The pre-recovery fail-closed withdrawal has no generation authority.
        // Confirmed Off requests a distinct receipt association, not recovery.
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
        assert_eq!(state.observed_absence, Some(2));
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 1);
        assert_eq!(*backend.confirmations.lock().unwrap(), vec![2, 2]);
        assert_eq!(updates.borrow().as_ref().unwrap().sequence(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn original_transition_withdraws_while_read_waits_before_long_poll_or_recovery_limit() {
        let initial = candidate(1, 9);
        let backend = Backend::new(Some(initial.clone()));
        *backend.transition.lock().unwrap() = Some(Instant::now() + Duration::from_secs(1));
        backend.store.lock().unwrap().read_release = Some(Arc::new(Notify::new()));
        let timing =
            TeletextAcceptedUpdatePolicy::new(Duration::from_secs(10), Duration::from_secs(20))
                .unwrap();
        let (updates, control, _) = teletext_output_control_channel();
        let mut observed = updates.subscribe();
        let owner = tokio::spawn(run_monitor(
            backend.clone(),
            9,
            timing,
            Some(initial),
            updates,
            || None,
        ));
        backend.read_entered.notified().await;
        backend.store.lock().unwrap().eligible = false;
        tokio::time::advance(Duration::from_secs(1)).await;
        observed.changed().await.unwrap();
        assert_eq!(observed.borrow().as_ref().unwrap().sequence(), 1);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 0);
        assert!(backend.confirmations.lock().unwrap().is_empty());
        drop(observed);
        drop(control);
        owner.await.unwrap();
    }
    mod handover {
        include!("tests/handover.rs");
    }
    mod expiry_preparation {
        include!("tests/expiry_preparation.rs");
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_runtime_under_the_same_manual_root_is_recovered_without_old_expiry_poisoning() {
        let mut initial = candidate(1, 9);
        initial.guide_context = Some(10);
        initial.identity.artifact_version = TELETEXT_GUIDE_GENERATION_ARTIFACT_VERSION;
        let old_cap = Instant::now() + Duration::from_secs(1);
        let retained = (initial.identity.clone(), Some(10_u64), old_cap);
        assert_eq!(
            guide_context_observation_transition(
                Some(&initial.identity),
                Some(&11),
                Some(&retained),
                None,
                old_cap
            ),
            (false, None)
        );
        assert_eq!(
            guide_context_observation_transition(
                Some(&initial.identity),
                Some(&10),
                Some(&retained),
                None,
                old_cap
            ),
            (true, None)
        );
        let backend = Backend::new(Some(initial.clone()));
        let (updates, _control, _) = teletext_output_control_channel();
        let mut state = MonitorState::new(Some(initial.clone()));
        let mut successor = initial;
        successor.guide_context = Some(11);
        successor.service = Arc::new(
            crate::transport::iptv::teletext_binding::authored_tests::authored_service_with_counter(
                4,
            ),
        );
        backend.store.lock().unwrap().accepted = Some(successor);
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(state.cached.as_ref().unwrap().guide_context, Some(11));
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 1);
        assert_eq!(state.proposal_context, Some(11));
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejected_runtime_does_not_reject_the_next_snapshot_of_the_same_manual_root() {
        let mut initial = candidate(1, 9);
        initial.guide_context = Some(10);
        let backend = Backend::new(Some(initial.clone()));
        let (updates, _control, _) = teletext_output_control_channel();
        let mut state = MonitorState::new(Some(initial));
        backend
            .store
            .lock()
            .unwrap()
            .accepted
            .as_mut()
            .unwrap()
            .guide_context = Some(11);
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        let sequence = state.sequence;
        assert!(state.observe_rejection(
            Some((
                sequence,
                TeletextOutputTransitionPhase::Rejected(
                    crate::transport::iptv::teletext_binding::TeletextOutputTransitionRejection::Proposal
                )
            )),
            &updates,
            9
        ));
        assert_eq!(state.rejected_context, Some(11));
        backend
            .store
            .lock()
            .unwrap()
            .accepted
            .as_mut()
            .unwrap()
            .guide_context = Some(12);
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(state.cached.as_ref().unwrap().guide_context, Some(12));
        assert!(state.rejected.is_none());
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn cached_guide_stale_reauthorization_requires_no_recovery_and_keeps_its_original_cap() {
        let mut initial = candidate(1, 9);
        initial.guide_context = Some(10);
        let cap = Instant::now() + Duration::from_secs(30);
        initial.original_transition = Some(cap);
        let backend = Backend::new(Some(initial.clone()));
        let (updates, _control, _) = teletext_output_control_channel();
        let mut state = MonitorState::new(Some(initial));
        {
            let mut store = backend.store.lock().unwrap();
            let current = store.accepted.as_mut().unwrap();
            current.guide_stale = true;
            current.guide_revision = 2;
            current.original_transition = Some(cap + Duration::from_secs(30));
        }
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        let cached = state.cached.as_ref().unwrap();
        assert!(cached.guide_stale);
        assert_eq!(cached.original_transition, Some(cap));
        let observed = Backend::generation_observation(cached)
            .unwrap()
            .guide_refresh
            .unwrap();
        assert_eq!(observed.snapshot_id, Some(10));
        assert_eq!(observed.revision, 2);
        assert!(observed.stale);
        assert_eq!(state.sequence, 1);
        assert_eq!(backend.recoveries.load(Ordering::SeqCst), 0);
        assert!(backend.confirmations.lock().unwrap().is_empty());
        assert!(reconcile(&backend, &mut state, &updates, 9, deadline()).await);
        assert_eq!(state.sequence, 1);
    }
}

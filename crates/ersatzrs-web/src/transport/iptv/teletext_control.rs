//! Latest trusted proposals for one live output epoch, never publication authority.

use std::{fmt, num::NonZeroU64, sync::Arc, time::Duration};

use crate::transport::iptv::teletext_output_observation::{ObservationContext, PumpObservation};
use ersatzrs_domain::{TeletextOutputGenerationObservation, TeletextOutputSelectionObservation};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextMultiplexError, TeletextOutputAcknowledgement,
    TeletextOutputError, TeletextOutputGate, TeletextOutputSchedule, TeletextPmtInstallError,
    TeletextPsiError, TeletextTransportError,
};
use tokio::sync::watch;

#[derive(Clone)]
enum Action {
    Replace(Arc<EncodedTeletextAuthoredService>),
    Withdraw,
}

/// Caller-owned engine proposal; no selection or durable generation is certified.
#[derive(Clone)]
pub struct TeletextOutputUpdate {
    sequence: NonZeroU64,
    action: Action,
    withdraw_if_rejected: bool,
    rejected: Option<TeletextOutputTransitionRejection>,
    generation: Option<TeletextOutputGenerationObservation>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext output update sequence must be positive")]
pub struct TeletextOutputUpdateError;

impl TeletextOutputUpdate {
    /// Propose the exact complete shared service.
    /// # Errors
    /// Rejects sequence zero. Each output epoch also rejects nonincreasing updates.
    pub fn replace(
        sequence: u64,
        service: Arc<EncodedTeletextAuthoredService>,
    ) -> Result<Self, TeletextOutputUpdateError> {
        Ok(Self {
            sequence: NonZeroU64::new(sequence).ok_or(TeletextOutputUpdateError)?,
            action: Action::Replace(service),
            withdraw_if_rejected: false,
            rejected: None,
            generation: None,
        })
    }
    /// A freshly confirmed post-expiry successor must never leave expired
    /// payload running when its native replacement is rejected at the ACK.
    pub(in crate::transport::iptv) fn replace_after_expiry(
        sequence: u64,
        service: Arc<EncodedTeletextAuthoredService>,
    ) -> Result<Self, TeletextOutputUpdateError> {
        let mut update = Self::replace(sequence, service)?;
        update.withdraw_if_rejected = true;
        Ok(update)
    }
    /// Propose descriptor withdrawal after an acknowledged full-cycle boundary.
    /// # Errors
    /// Rejects sequence zero; this is not an immediate-disable timing guarantee.
    pub fn withdraw(sequence: u64) -> Result<Self, TeletextOutputUpdateError> {
        Ok(Self {
            sequence: NonZeroU64::new(sequence).ok_or(TeletextOutputUpdateError)?,
            action: Action::Withdraw,
            withdraw_if_rejected: false,
            rejected: None,
            generation: None,
        })
    }
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence.get()
    }

    pub(in crate::transport::iptv) fn with_generation(
        mut self,
        identity: &ersatzrs_domain::TeletextAcceptedGeneration,
    ) -> Self {
        self.generation = crate::transport::iptv::teletext_output_observation::generation(identity);
        self
    }

    /// Diagnostic association follows the actual native proposal/ACK; it grants
    /// no publication authority and never fabricates a manual generation ID.
    pub(in crate::transport::iptv) fn with_generation_observation(
        mut self,
        observation: Option<TeletextOutputGenerationObservation>,
    ) -> Self {
        self.generation = observation;
        self
    }

    fn observation(&self) -> TeletextOutputSelectionObservation {
        TeletextOutputSelectionObservation {
            sequence: self.sequence(),
            generation: self.generation.clone(),
            authored: matches!(self.action, Action::Replace(_)),
        }
    }
}
impl fmt::Debug for TeletextOutputUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextOutputUpdate")
            .field("sequence", &self.sequence())
            .field("replacement", &matches!(self.action, Action::Replace(_)))
            .finish_non_exhaustive()
    }
}

/// Nonterminal proposal rejection, without service/program text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextOutputTransitionRejection {
    Sequence,
    ResourceUpdate,
    CacheReconciliation,
    Proposal,
    SourceEnded,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextOutputTransitionPhase {
    Idle,
    Waiting,
    Applied,
    Rejected(TeletextOutputTransitionRejection),
}

/// Latest observation from ONE output epoch. Shared bindings may replace this
/// observation with another epoch's state; this never acknowledges all outputs.
#[derive(Clone)]
pub struct TeletextOutputTransitionStatus {
    epoch: Option<Arc<()>>,
    sequence: Option<NonZeroU64>,
    phase: TeletextOutputTransitionPhase,
}
impl TeletextOutputTransitionStatus {
    #[must_use]
    pub fn same_epoch(&self, other: &Self) -> bool {
        matches!((&self.epoch, &other.epoch), (Some(a), Some(b)) if Arc::ptr_eq(a, b))
    }
    #[must_use]
    pub fn sequence(&self) -> Option<u64> {
        self.sequence.map(NonZeroU64::get)
    }
    #[must_use]
    pub const fn phase(&self) -> TeletextOutputTransitionPhase {
        self.phase
    }
}
impl fmt::Debug for TeletextOutputTransitionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextOutputTransitionStatus")
            .field("sequence", &self.sequence())
            .field("phase", &self.phase)
            .field("output_epoch", &self.epoch.is_some())
            .finish()
    }
}

/// Optional latest-state inputs. Clone creates an independent per-gate observer;
/// a shared status channel remains a latest observation, never an aggregate ACK.
#[derive(Clone)]
pub struct TeletextOutputControl {
    updates: watch::Receiver<Option<TeletextOutputUpdate>>,
    status: watch::Sender<TeletextOutputTransitionStatus>,
    trace_id: uuid::Uuid,
    observation: Option<ObservationContext>,
}

impl TeletextOutputControl {
    pub(in crate::transport::iptv) fn observe_native_output(
        &mut self,
        context: ObservationContext,
    ) {
        self.observation = Some(context);
    }
    pub(in crate::transport::iptv) const fn trace_id(&self) -> uuid::Uuid {
        self.trace_id
    }
}

/// Create bounded watch slots, without tasks or current-generation authority.
#[must_use]
pub fn teletext_output_control_channel() -> (
    watch::Sender<Option<TeletextOutputUpdate>>,
    TeletextOutputControl,
    watch::Receiver<TeletextOutputTransitionStatus>,
) {
    let (updates, receiver) = watch::channel(None);
    let (status, observer) = watch::channel(TeletextOutputTransitionStatus {
        epoch: None,
        sequence: None,
        phase: TeletextOutputTransitionPhase::Idle,
    });
    (
        updates,
        TeletextOutputControl {
            updates: receiver,
            status,
            trace_id: uuid::Uuid::new_v4(),
            observation: None,
        },
        observer,
    )
}

#[derive(Clone, Copy)]
enum InstalledAction {
    Replace,
    Withdraw,
}

pub(in crate::transport::iptv) struct PumpControl {
    control: TeletextOutputControl,
    epoch: Arc<()>,
    trace_epoch: uuid::Uuid,
    // Observation only: latest successful gate installation, never latest proposal.
    trace_active_sequence: u64,
    initialized: bool,
    seen: u64,
    desired: Option<TeletextOutputUpdate>,
    installed: Option<(NonZeroU64, InstalledAction)>,
    capped_failed: bool,
    source_ended: bool,
    observation: Option<PumpObservation>,
}
impl PumpControl {
    pub(in crate::transport::iptv) fn new(control: TeletextOutputControl) -> Self {
        let trace_epoch = uuid::Uuid::new_v4();
        let observation = control
            .observation
            .as_ref()
            .and_then(|context| PumpObservation::new(context, trace_epoch));
        Self {
            control,
            epoch: Arc::new(()),
            trace_epoch,
            trace_active_sequence: 0,
            initialized: false,
            seen: 0,
            desired: None,
            installed: None,
            capped_failed: false,
            source_ended: false,
            observation,
        }
    }
    pub(in crate::transport::iptv) fn trace_window_prepared(
        &self,
        gate: &TeletextOutputGate,
        schedule: &TeletextOutputSchedule,
        now: Duration,
    ) {
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event = "sink_window_prepared",
            proposal_slot = %self.control.trace_id,
            output_epoch = %self.trace_epoch,
            active_sequence = self.trace_active_sequence,
            authored_selected = gate.authored_payload_selected(),
            first_packet = schedule.first_packet(),
            end_packet = u128::from(schedule.first_packet()) + schedule.packet_count() as u128,
            first_due_elapsed_ns = schedule.first_due().as_nanos(),
            prepared_elapsed_ns = now.as_nanos(),
            observed_utc = %chrono::Utc::now().to_rfc3339(),
            "teletext sink window prepared before delivery"
        );
    }
    pub(in crate::transport::iptv) fn trace_first_packet_written(
        &self,
        first_packet: u64,
        packet: &[u8; 188],
        now: Duration,
    ) {
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event = "sink_window_first_packet_written",
            first_packet_sha256 = %packet_sha256(packet),
            proposal_slot = %self.control.trace_id,
            output_epoch = %self.trace_epoch,
            active_sequence = self.trace_active_sequence,
            first_packet,
            written_elapsed_ns = now.as_nanos(),
            observed_utc = %chrono::Utc::now().to_rfc3339(),
            "teletext sink wrote the first packet of a prepared window"
        );
    }
    pub(in crate::transport::iptv) fn trace_window(
        &self,
        gate: &TeletextOutputGate,
        receipt: &TeletextOutputAcknowledgement,
        first_packet: u64,
        now: Duration,
    ) {
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event = "sink_window_flushed",
            active_sequence = self.trace_active_sequence,
            authored_selected = gate.authored_payload_selected(),
            proposal_slot = %self.control.trace_id,
            output_epoch = %self.trace_epoch,
            first_packet,
            end_packet = receipt.end_packet(),
            accepted_elapsed_ns = now.as_nanos(),
            observed_utc = %chrono::Utc::now().to_rfc3339(),
            cycle_boundary = receipt.is_cycle_boundary(),
            complete_cycle = receipt.contains_complete_cycle(),
            signalling_announced = receipt.signalling_announced(),
            "teletext sink accepted a flushed window"
        );
    }
    fn trace_transition(
        &self,
        event: &'static str,
        sequence: NonZeroU64,
        action: InstalledAction,
        receipt: &TeletextOutputAcknowledgement,
        now: Duration,
    ) {
        tracing::debug!(
            target: "ersatzrs_web::transport::iptv::teletext_provenance",
            event,
            proposal_slot = %self.control.trace_id,
            output_epoch = %self.trace_epoch,
            sequence = sequence.get(),
            action = match action { InstalledAction::Replace => "replace", InstalledAction::Withdraw => "withdraw" },
            end_packet = receipt.end_packet(),
            accepted_elapsed_ns = now.as_nanos(),
            observed_utc = %chrono::Utc::now().to_rfc3339(),
            cycle_boundary = receipt.is_cycle_boundary(),
            complete_cycle = receipt.contains_complete_cycle(),
            signalling_announced = receipt.signalling_announced(),
            "teletext transition at an actual sink acknowledgement"
        );
    }
    fn publish(&mut self, sequence: NonZeroU64, phase: TeletextOutputTransitionPhase) {
        if let Some(observation) = &mut self.observation {
            match phase {
                TeletextOutputTransitionPhase::Applied => observation.acknowledge(sequence.get()),
                TeletextOutputTransitionPhase::Rejected(_) => observation.reject(sequence.get()),
                _ => {}
            }
        }
        self.control
            .status
            .send_replace(TeletextOutputTransitionStatus {
                epoch: Some(Arc::clone(&self.epoch)),
                sequence: Some(sequence),
                phase,
            });
    }
    pub(in crate::transport::iptv) fn observe(&mut self) {
        let update = {
            // Ref reports the version difference even after the last sender
            // closes. Receiver::has_changed would hide an unseen final value.
            let value = self.control.updates.borrow_and_update();
            if self.initialized && !value.has_changed() {
                return;
            }
            value.clone()
        };
        self.initialized = true;
        let Some(update) = update else {
            return;
        };
        if update.sequence() <= self.seen {
            self.publish(
                update.sequence,
                TeletextOutputTransitionPhase::Rejected(
                    TeletextOutputTransitionRejection::Sequence,
                ),
            );
            return;
        }
        self.seen = update.sequence();
        if let Some(observation) = &mut self.observation {
            observation.requested(update.observation());
        }
        self.publish(update.sequence, TeletextOutputTransitionPhase::Waiting);
        self.desired = Some(update);
        self.reject_late_replacement();
    }
    /// Actual EOF closes new activation, while requested removal remains valid.
    /// Already installed content must still satisfy its complete-cycle checks.
    pub(in crate::transport::iptv) fn source_ended(&mut self) {
        self.source_ended = true;
        self.observe();
        self.reject_late_replacement();
    }
    fn reject_late_replacement(&mut self) {
        if !self.source_ended {
            return;
        }
        if let Some(update) = &self.desired {
            if matches!(update.action, Action::Replace(_)) {
                let sequence = update.sequence;
                if update.withdraw_if_rejected {
                    let mut fallback = update.clone();
                    fallback.action = Action::Withdraw;
                    fallback.rejected = Some(TeletextOutputTransitionRejection::SourceEnded);
                    self.desired = Some(fallback);
                } else {
                    self.desired = None;
                }
                self.publish(
                    sequence,
                    TeletextOutputTransitionPhase::Rejected(
                        TeletextOutputTransitionRejection::SourceEnded,
                    ),
                );
            }
        }
    }
    pub(in crate::transport::iptv) fn window_end(
        &mut self,
        gate: &TeletextOutputGate,
        intervals: usize,
    ) -> Result<Option<u64>, TeletextOutputError> {
        self.observe();
        if !self.capped_failed
            && (self.desired.is_some()
                || matches!(self.installed, Some((_, InstalledAction::Replace))))
        {
            gate.transition_window_end(intervals)
        } else {
            gate.window_end(intervals)
        }
    }
    /// Retry a failed capped prefix only after actual physical progress. Ordinary
    /// growth retains the existing buffer/deadline bounds and never drops AV.
    pub(in crate::transport::iptv) fn preparation_failed(
        &mut self,
        gate: &TeletextOutputGate,
        intervals: usize,
        end: u64,
    ) -> Result<bool, TeletextOutputError> {
        self.capped_failed = true;
        Ok(gate
            .window_end(intervals)?
            .is_some_and(|ordinary| ordinary != end))
    }
    pub(in crate::transport::iptv) fn acknowledge(
        &mut self,
        gate: &mut TeletextOutputGate,
        receipt: &TeletextOutputAcknowledgement,
        now: Duration,
        allow_install: bool,
    ) -> Result<(), TeletextOutputError> {
        self.capped_failed = false;
        self.observe();
        if let Some(observation) = &mut self.observation {
            observation.initial_receipt(
                receipt.contains_complete_cycle(),
                receipt.signalling_announced() && !gate.authored_payload_selected(),
            );
        }
        if let Some((sequence, action)) = self.installed {
            let delivered = match action {
                InstalledAction::Replace => receipt.contains_complete_cycle(),
                InstalledAction::Withdraw => {
                    receipt.signalling_announced() && !gate.authored_payload_selected()
                }
            };
            if delivered {
                self.trace_transition("transition_applied", sequence, action, receipt, now);
                self.publish(sequence, TeletextOutputTransitionPhase::Applied);
                self.installed = None;
            }
        }
        if !allow_install || self.installed.is_some() || !receipt.is_cycle_boundary() {
            return Ok(());
        }
        let Some(update) = self.desired.take() else {
            return Ok(());
        };
        let already_withdrawn =
            matches!(update.action, Action::Withdraw) && !gate.authored_payload_selected();
        let result = match &update.action {
            Action::Replace(service) => {
                gate.replace_authored_at_acknowledged_boundary(receipt, service, now)
            }
            Action::Withdraw => gate.withdraw_at_acknowledged_boundary(receipt, now),
        };
        self.withdraw_rejected_expiry(gate, receipt, now, &update, &result)?;
        if result.is_ok() {
            self.trace_transition(
                if update.rejected.is_some() {
                    "expiry_replacement_fallback"
                } else if already_withdrawn {
                    "transition_applied"
                } else {
                    "transition_installed"
                },
                update.sequence,
                match update.action {
                    Action::Replace(_) => InstalledAction::Replace,
                    Action::Withdraw => InstalledAction::Withdraw,
                },
                receipt,
                now,
            );
            self.trace_active_sequence = update.sequence();
        }
        self.finish_proposal(update, result, already_withdrawn)
    }
    fn withdraw_rejected_expiry(
        &mut self,
        gate: &mut TeletextOutputGate,
        receipt: &TeletextOutputAcknowledgement,
        now: Duration,
        update: &TeletextOutputUpdate,
        result: &Result<(), TeletextOutputError>,
    ) -> Result<(), TeletextOutputError> {
        if update.withdraw_if_rejected && result.as_ref().err().and_then(rejection).is_some() {
            // Native rejection preserves this receipt and the old gate state.
            // Remove expired payload synchronously before another window; any
            // withdrawal failure is terminal, never a recoverable Base refusal.
            gate.withdraw_at_acknowledged_boundary(receipt, now)?;
            self.trace_transition(
                "expiry_replacement_fallback",
                update.sequence,
                InstalledAction::Withdraw,
                receipt,
                now,
            );
            self.trace_active_sequence = update.sequence();
        }
        Ok(())
    }
    fn finish_proposal(
        &mut self,
        update: TeletextOutputUpdate,
        result: Result<(), TeletextOutputError>,
        already_withdrawn: bool,
    ) -> Result<(), TeletextOutputError> {
        match result {
            Ok(()) => {
                if let Some(reason) = update.rejected {
                    // EOF converted only the physical action to withdrawal.
                    // The rejected successor never gains an installed/Applied
                    // replacement status under its original sequence.
                    self.publish(
                        update.sequence,
                        TeletextOutputTransitionPhase::Rejected(reason),
                    );
                    return Ok(());
                }
                if already_withdrawn {
                    // The gate verified this fresh receipt and requires already
                    // acknowledged withdrawal PSI before accepting the no-op.
                    if let Some(observation) = &mut self.observation {
                        observation.installed(update.observation());
                    }
                    self.publish(update.sequence, TeletextOutputTransitionPhase::Applied);
                    return Ok(());
                }
                let action = match update.action {
                    Action::Replace(_) => InstalledAction::Replace,
                    Action::Withdraw => InstalledAction::Withdraw,
                };
                self.installed = Some((update.sequence, action));
                if let Some(observation) = &mut self.observation {
                    observation.installed(update.observation());
                }
                self.publish(update.sequence, TeletextOutputTransitionPhase::Waiting);
            }
            Err(
                TeletextOutputError::Boundary
                | TeletextOutputError::Pending
                | TeletextOutputError::Psi(TeletextPsiError::Incomplete)
                | TeletextOutputError::Pmt(TeletextPmtInstallError::Incomplete),
            ) => self.desired = Some(update),
            // A failed withdrawal must stop this epoch rather than continue
            // emitting content whose removal the caller already requested.
            Err(error) if matches!(update.action, Action::Withdraw) => return Err(error),
            Err(error) => {
                let Some(reason) = rejection(&error) else {
                    return Err(error);
                };
                self.publish(
                    update.sequence,
                    TeletextOutputTransitionPhase::Rejected(reason),
                );
            }
        }
        Ok(())
    }
}

// Keep ordinary replacement classification and expiry fallback identical.
fn rejection(error: &TeletextOutputError) -> Option<TeletextOutputTransitionRejection> {
    match error {
        TeletextOutputError::Transport(TeletextTransportError::ResourceUpdate { .. }) => {
            Some(TeletextOutputTransitionRejection::ResourceUpdate)
        }
        TeletextOutputError::CacheReconciliationRequired => {
            Some(TeletextOutputTransitionRejection::CacheReconciliation)
        }
        TeletextOutputError::Configuration
        | TeletextOutputError::Transport(_)
        | TeletextOutputError::Pmt(
            TeletextPmtInstallError::Reconfiguration
            | TeletextPmtInstallError::Multiplex(TeletextMultiplexError::SectionCapacity),
        ) => Some(TeletextOutputTransitionRejection::Proposal),
        _ => None,
    }
}

// Called only from an enabled tracing field expression, once per window.
fn packet_sha256(packet: &[u8; 188]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut hash = String::with_capacity(64);
    for byte in <sha2::Sha256 as sha2::Digest>::digest(packet) {
        hash.push(char::from(HEX[usize::from(byte >> 4)]));
        hash.push(char::from(HEX[usize::from(byte & 15)]));
    }
    hash
}

#[cfg(test)]
#[path = "teletext_control_tests.rs"]
mod tests;

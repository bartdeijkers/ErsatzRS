//! Latest trusted proposals for one live output epoch, never publication authority.

use std::{fmt, num::NonZeroU64, sync::Arc, time::Duration};

use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextMultiplexError, TeletextOutputAcknowledgement,
    TeletextOutputError, TeletextOutputGate, TeletextPmtInstallError, TeletextPsiError,
    TeletextTransportError,
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
        })
    }
    /// Propose descriptor withdrawal after an acknowledged full-cycle boundary.
    /// # Errors
    /// Rejects sequence zero; this is not an immediate-disable timing guarantee.
    pub fn withdraw(sequence: u64) -> Result<Self, TeletextOutputUpdateError> {
        Ok(Self {
            sequence: NonZeroU64::new(sequence).ok_or(TeletextOutputUpdateError)?,
            action: Action::Withdraw,
        })
    }
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence.get()
    }
}
impl fmt::Debug for TeletextOutputUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextOutputUpdate")
            .field("sequence", &self.sequence())
            .field("replacement", &matches!(self.action, Action::Replace(_)))
            .finish()
    }
}

/// Nonterminal proposal rejection, without service/program text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextOutputTransitionRejection {
    Sequence,
    ResourceUpdate,
    CacheReconciliation,
    Proposal,
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
    initialized: bool,
    seen: u64,
    desired: Option<TeletextOutputUpdate>,
    installed: Option<(NonZeroU64, InstalledAction)>,
    capped_failed: bool,
}
impl PumpControl {
    pub(in crate::transport::iptv) fn new(control: TeletextOutputControl) -> Self {
        Self {
            control,
            epoch: Arc::new(()),
            initialized: false,
            seen: 0,
            desired: None,
            installed: None,
            capped_failed: false,
        }
    }
    fn publish(&self, sequence: NonZeroU64, phase: TeletextOutputTransitionPhase) {
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
        self.publish(update.sequence, TeletextOutputTransitionPhase::Waiting);
        self.desired = Some(update);
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
        if let Some((sequence, action)) = self.installed {
            let delivered = match action {
                InstalledAction::Replace => receipt.contains_complete_cycle(),
                InstalledAction::Withdraw => {
                    receipt.signalling_announced() && !gate.authored_payload_selected()
                }
            };
            if delivered {
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
        self.finish_proposal(update, result, already_withdrawn)
    }
    fn finish_proposal(
        &mut self,
        update: TeletextOutputUpdate,
        result: Result<(), TeletextOutputError>,
        already_withdrawn: bool,
    ) -> Result<(), TeletextOutputError> {
        match result {
            Ok(()) => {
                if already_withdrawn {
                    // The gate verified this fresh receipt and requires already
                    // acknowledged withdrawal PSI before accepting the no-op.
                    self.publish(update.sequence, TeletextOutputTransitionPhase::Applied);
                    return Ok(());
                }
                let action = match update.action {
                    Action::Replace(_) => InstalledAction::Replace,
                    Action::Withdraw => InstalledAction::Withdraw,
                };
                self.installed = Some((update.sequence, action));
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
                let reason = match error {
                    TeletextOutputError::Transport(TeletextTransportError::ResourceUpdate {
                        ..
                    }) => TeletextOutputTransitionRejection::ResourceUpdate,
                    TeletextOutputError::CacheReconciliationRequired => {
                        TeletextOutputTransitionRejection::CacheReconciliation
                    }
                    TeletextOutputError::Configuration
                    | TeletextOutputError::Transport(_)
                    | TeletextOutputError::Pmt(
                        TeletextPmtInstallError::Reconfiguration
                        | TeletextPmtInstallError::Multiplex(TeletextMultiplexError::SectionCapacity),
                    ) => TeletextOutputTransitionRejection::Proposal,
                    _ => return Err(error),
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

#[cfg(test)]
#[path = "teletext_control_tests.rs"]
mod tests;

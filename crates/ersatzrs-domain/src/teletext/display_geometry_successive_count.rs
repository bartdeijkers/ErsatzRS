//! Positioned successive openings (EN 300 706 Annex A.8(3)).
//!
//! Original call AP precedes its offset. Calls and display writes share raw scan
//! order; descendant writes can intervene between caller positions. Equal-cell
//! cross-emitter writes and unpositioned resets retain uncertainty, never an
//! invented physical order or complete object/page admission certificate.

use std::mem::size_of;

use super::{
    TeletextColumnOperation, TeletextDisplayGeometryError, TeletextDisplayGeometryFailure,
    TeletextEnhancementCommand, TeletextEnhancementSource, TeletextPageAddress,
    TeletextSpatialEvent, TeletextSpatialInvocationId, TeletextSpatialRootSource,
    TeletextSpatialTrace,
};

/// Reason the original action cannot establish a definite successive run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextSuccessiveOpenPendingReason {
    /// The original potential write lies outside the addressed display plane.
    UnpositionedPotentialReset,
    /// Call AP lies outside the raw addressed display plane.
    UnpositionedOpening,
    /// Different emitters share a position containing a potential reset.
    CrossEmitterOrder,
}

/// Original potential reset retained without inventing a valid display cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSuccessiveOpenPending {
    event_ordinal: usize,
    event: TeletextSpatialEvent,
    reason: TeletextSuccessiveOpenPendingReason,
}

impl TeletextSuccessiveOpenPending {
    #[must_use]
    pub const fn event_ordinal(&self) -> usize {
        self.event_ordinal
    }
    /// Exact original AP, without filtering or clipping; plane-colour actions
    /// do not acquire a fictitious addressed cell from this diagnostic position.
    #[must_use]
    pub const fn position(&self) -> (u16, u16) {
        let position = if matches!(
            self.event.command(),
            TeletextEnhancementCommand::InvokeObject { .. }
        ) {
            self.event.absolute_before()
        } else {
            self.event.absolute_after()
        };
        (position.row(), position.column())
    }
    #[must_use]
    pub const fn location(&self) -> TeletextEnhancementSource {
        self.event.source()
    }
    #[must_use]
    pub const fn root_source(&self) -> TeletextSpatialRootSource {
        self.event.root_source()
    }
    /// Executing emitter; `None` denotes the original main/default program.
    #[must_use]
    pub const fn invocation(&self) -> Option<TeletextSpatialInvocationId> {
        self.event.invocation()
    }
    #[must_use]
    pub const fn reason(&self) -> TeletextSuccessiveOpenPendingReason {
        self.reason
    }
}

/// Known positioned runs, not a physical-order or page-readiness certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextObjectSuccessiveCount {
    maximum_known_successive: usize,
    pending: Vec<TeletextSuccessiveOpenPending>,
}

impl TeletextObjectSuccessiveCount {
    /// Maximum proven positioned run. An unpositioned potential reset disables
    /// the combined lower-bound claim entirely; equal-cell mixed-source reset
    /// groups break runs conservatively. All uncertainty remains in `pending()`.
    #[must_use]
    pub const fn maximum_known_successive(&self) -> usize {
        self.maximum_known_successive
    }
    #[must_use]
    pub fn pending(&self) -> &[TeletextSuccessiveOpenPending] {
        &self.pending
    }
}

pub(super) fn successive_storage_bytes(events: usize, _instances: usize) -> Option<usize> {
    events
        .checked_mul(size_of::<PositionedAction>())?
        .checked_add(events.checked_mul(size_of::<TeletextSuccessiveOpenPending>())?)
}

pub(super) fn successive_work(events: usize, _instances: usize) -> Option<usize> {
    let levels = usize::BITS - events.leading_zeros();
    events
        .checked_mul(levels as usize + 1)?
        .checked_mul(2)?
        .checked_add(events.checked_mul(4)?)
}

#[derive(Clone, Copy)]
enum ActionKind {
    Open(TeletextSpatialInvocationId),
    Write,
}

#[derive(Clone, Copy)]
struct PositionedAction {
    event_ordinal: usize,
    event: TeletextSpatialEvent,
    kind: ActionKind,
    position: (u16, u16),
}

#[derive(Clone, Copy)]
enum Action {
    None,
    Positioned {
        kind: ActionKind,
        position: (u16, u16),
    },
    Pending(TeletextSuccessiveOpenPendingReason),
}

pub(super) fn count_successive_objects(
    owner: TeletextPageAddress,
    original: &TeletextSpatialTrace,
) -> Result<TeletextObjectSuccessiveCount, TeletextDisplayGeometryError> {
    let events = original.events();
    let instances = original.invocations().len();
    if successive_storage_bytes(events.len(), instances)
        .is_none_or(|bytes| bytes > isize::MAX as usize)
        || successive_work(events.len(), instances).is_none()
    {
        return Err(TeletextDisplayGeometryError {
            owner,
            position: None,
            location: None,
            root_source: None,
            invocation: None,
            reason: TeletextDisplayGeometryFailure::AllocationSize,
        });
    }
    let action_count = events
        .iter()
        .filter(|&&event| matches!(action(event), Action::Positioned { .. }))
        .count();
    let mut actions = Vec::with_capacity(action_count);
    // Pending witnesses are disjoint original actions, at most one per event.
    // Reserve the already-fenced upper bound so growth never exceeds storage.
    let mut pending = Vec::with_capacity(events.len());
    let mut unknown_reset = false;
    for (event_ordinal, &event) in events.iter().enumerate() {
        match action(event) {
            Action::None => {}
            Action::Positioned { kind, position } => {
                actions.push(PositionedAction {
                    event_ordinal,
                    event,
                    kind,
                    position,
                });
            }
            Action::Pending(reason) => {
                unknown_reset |=
                    reason == TeletextSuccessiveOpenPendingReason::UnpositionedPotentialReset;
                pending.push(TeletextSuccessiveOpenPending {
                    event_ordinal,
                    event,
                    reason,
                });
            }
        }
    }
    // Only within one emitter does ordinal establish exact command order.
    // Other keys stabilize diagnostics without assigning physical tie priority.
    actions.sort_unstable_by_key(|item| {
        (
            raw_rank(item.position),
            item.event.invocation().map_or(0, |id| id.get() + 1),
            item.event_ordinal,
        )
    });
    let maximum_known_successive = scan_groups(owner, &actions, &mut pending, !unknown_reset)?;
    pending.sort_unstable_by_key(TeletextSuccessiveOpenPending::event_ordinal);
    Ok(TeletextObjectSuccessiveCount {
        maximum_known_successive,
        pending,
    })
}

fn scan_groups(
    owner: TeletextPageAddress,
    actions: &[PositionedAction],
    pending: &mut Vec<TeletextSuccessiveOpenPending>,
    known_resets: bool,
) -> Result<usize, TeletextDisplayGeometryError> {
    let (mut start, mut run, mut maximum) = (0, 0, 0);
    while start < actions.len() {
        let mut finish = start + 1;
        while finish < actions.len() && actions[finish].position == actions[start].position {
            finish += 1;
        }
        let group = &actions[start..finish];
        let mixed = group
            .iter()
            .any(|item| item.event.invocation() != group[0].event.invocation());
        if mixed
            && group
                .iter()
                .any(|item| matches!(item.kind, ActionKind::Write))
        {
            run = 0;
            for item in group {
                pending.push(TeletextSuccessiveOpenPending {
                    event_ordinal: item.event_ordinal,
                    event: item.event,
                    reason: TeletextSuccessiveOpenPendingReason::CrossEmitterOrder,
                });
            }
        } else {
            for item in group {
                match item.kind {
                    ActionKind::Write => run = 0,
                    ActionKind::Open(child) if known_resets => {
                        // Every increment consumes a distinct bounded original action.
                        run += 1;
                        maximum = maximum.max(run);
                        if run > 15 {
                            return Err(TeletextDisplayGeometryError {
                                owner,
                                position: Some(item.position),
                                location: Some(item.event.source()),
                                root_source: Some(item.event.root_source()),
                                invocation: Some(child),
                                reason:
                                    TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens {
                                        count: run,
                                        maximum: 15,
                                    },
                            });
                        }
                    }
                    ActionKind::Open(_) => {}
                }
            }
        }
        start = finish;
    }
    Ok(maximum)
}

fn raw_rank((row, column): (u16, u16)) -> u32 {
    u32::from(row) * 72 + u32::from(column)
}

fn action(event: TeletextSpatialEvent) -> Action {
    let before = event.absolute_before();
    let after = event.absolute_after();
    match event.command() {
        TeletextEnhancementCommand::InvokeObject { .. } => {
            let Some(child) = event.called_invocation() else {
                return Action::None;
            };
            if before.row() <= 24 && before.column() <= 71 {
                Action::Positioned {
                    kind: ActionKind::Open(child),
                    position: (before.row(), before.column()),
                }
            } else {
                Action::Pending(TeletextSuccessiveOpenPendingReason::UnpositionedOpening)
            }
        }
        TeletextEnhancementCommand::SetScreenColour { .. }
        | TeletextEnhancementCommand::SetRowColour { .. } => {
            // Attribute writes have a screen/row plane, not a fictitious column.
            Action::Pending(TeletextSuccessiveOpenPendingReason::UnpositionedPotentialReset)
        }
        TeletextEnhancementCommand::AtColumn { operation, .. } if is_write(operation) => {
            if after.row() <= 24
                && after.column() <= 71
                && (after.row() != 0 || (8..=39).contains(&after.column()))
            {
                Action::Positioned {
                    kind: ActionKind::Write,
                    position: (after.row(), after.column()),
                }
            } else {
                Action::Pending(TeletextSuccessiveOpenPendingReason::UnpositionedPotentialReset)
            }
        }
        _ => Action::None,
    }
}

fn is_write(operation: TeletextColumnOperation) -> bool {
    matches!(
        operation,
        TeletextColumnOperation::G0 { .. }
            | TeletextColumnOperation::G1 { .. }
            | TeletextColumnOperation::G1Alpha { .. }
            | TeletextColumnOperation::G2 { .. }
            | TeletextColumnOperation::G3 { .. }
            | TeletextColumnOperation::G0Diacritic { .. }
            | TeletextColumnOperation::Drcs { .. }
            | TeletextColumnOperation::Foreground { .. }
            | TeletextColumnOperation::Background { .. }
            | TeletextColumnOperation::Flash { .. }
            | TeletextColumnOperation::Attributes { .. }
    )
}

//! Positioned successive openings (EN 300 706 Annex A.8(3)).
//!
//! Original call AP precedes its offset. Calls and display writes share raw scan
//! order; descendant writes can intervene between caller positions. Equal-cell
//! cross-emitter writes and unpositioned resets retain uncertainty, never an
//! invented physical order or complete object/page admission certificate.

use std::mem::size_of;

use super::{
    TeletextColumnOperation, TeletextDisplayGeometryError, TeletextDisplayGeometryFailure,
    TeletextEffectiveProgramSource, TeletextEnhancementCommand, TeletextEnhancementSource,
    TeletextPageAddress, TeletextSpatialEvent, TeletextSpatialInvocationId,
    TeletextSpatialRootSource, TeletextSpatialTrace,
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
    /// Maximum proven positioned run. With one emitter, potential resets split
    /// known runs at their exact original ordinal. Otherwise unpositioned resets
    /// disable the combined lower-bound claim and mixed-source ties break runs
    /// conservatively. An explicit Main prefix before every reset's root call
    /// can retain its own lower bound. With one writing object, its activating
    /// non-writing ancestors also bound its original runs. All uncertainty
    /// remains in `pending()`.
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

pub(super) fn successive_work(events: usize, instances: usize) -> Option<usize> {
    let levels = usize::BITS - events.leading_zeros();
    events
        .checked_mul(levels as usize + 1)?
        .checked_mul(2)?
        // Group prefix passes plus one writer pass and one selected-emitter
        // scan; the strictly decreasing ancestor walk visits at most instances.
        .checked_add(events.checked_mul(8)?)?
        .checked_add(instances)
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
    let mut first_emitter = None;
    let mut mixed_emitters = false;
    for (event_ordinal, &event) in events.iter().enumerate() {
        let current = action(event);
        if !matches!(current, Action::None) {
            let emitter = event.invocation();
            if let Some(first) = first_emitter {
                mixed_emitters |= first != emitter;
            } else {
                first_emitter = Some(emitter);
            }
        }
        match current {
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
    let maximum_known_successive = if unknown_reset && !mixed_emitters {
        // Every relevant action belongs to one actual emitter. Its original
        // ordinal is exact without assigning a screen cell to a plane write.
        // This linear scan replaces, rather than extends, the fenced sort/scan.
        scan_single_emitter(owner, events, first_emitter.flatten(), 0)?
    } else {
        // Other keys stabilize diagnostics without assigning physical tie priority.
        actions.sort_unstable_by_key(|item| {
            (
                raw_rank(item.position),
                item.event.invocation().map_or(0, |id| id.get() + 1),
                item.event_ordinal,
            )
        });
        scan_groups(
            owner,
            &actions,
            &mut pending,
            !unknown_reset,
            original.original_program_source() == &TeletextEffectiveProgramSource::Local,
        )?
    };
    let maximum_known_successive =
        maximum_known_successive.max(object_emitter_bound(owner, original)?);
    pending.sort_unstable_by_key(TeletextSuccessiveOpenPending::event_ordinal);
    Ok(TeletextObjectSuccessiveCount {
        maximum_known_successive,
        pending,
    })
}

fn scan_single_emitter(
    owner: TeletextPageAddress,
    events: &[TeletextSpatialEvent],
    emitter: Option<TeletextSpatialInvocationId>,
    initial: usize,
) -> Result<usize, TeletextDisplayGeometryError> {
    let (mut run, mut maximum) = (initial, initial);
    for &event in events {
        if event.invocation() != emitter {
            continue;
        }
        match action(event) {
            Action::Positioned {
                kind: ActionKind::Open(child),
                position,
            } => {
                run += 1;
                maximum = maximum.max(run);
                if run > 15 {
                    return Err(opening_error(owner, event, child, position, run));
                }
            }
            // A possible reset is only a conservative run barrier. Keeping its
            // pending witness does not certify it as an actual display write.
            Action::Positioned {
                kind: ActionKind::Write,
                ..
            }
            | Action::Pending(_) => run = 0,
            Action::None => {}
        }
    }
    Ok(maximum)
}

fn object_emitter_bound(
    owner: TeletextPageAddress,
    original: &TeletextSpatialTrace,
) -> Result<usize, TeletextDisplayGeometryError> {
    if original.original_program_source() != &TeletextEffectiveProgramSource::Local {
        return Ok(0);
    }
    let Some(writer) = sole_object_writer(original.events()) else {
        return Ok(0);
    };
    let Some(initial) = ancestor_open_count(original, writer) else {
        return Ok(0);
    };
    if initial > 15 {
        if let Some(event) = original
            .events()
            .iter()
            .find(|event| event.called_invocation() == Some(writer))
        {
            let position = event.absolute_before();
            return Err(opening_error(
                owner,
                *event,
                writer,
                (position.row(), position.column()),
                initial,
            ));
        }
        return Ok(0);
    }
    // Every possible write belongs to this actual instance. Its mandatory
    // activating ancestors precede its first action; other emitters cannot
    // reset the known run. No preorder-as-screen-order inference is required.
    scan_single_emitter(owner, original.events(), Some(writer), initial)
}

fn sole_object_writer(events: &[TeletextSpatialEvent]) -> Option<TeletextSpatialInvocationId> {
    let mut writer = None;
    for &event in events {
        if matches!(
            action(event),
            Action::Positioned {
                kind: ActionKind::Write,
                ..
            } | Action::Pending(TeletextSuccessiveOpenPendingReason::UnpositionedPotentialReset)
        ) {
            let emitter = event.invocation()?;
            if writer.is_some_and(|previous| previous != emitter) {
                return None;
            }
            writer = Some(emitter);
        }
    }
    writer
}

fn ancestor_open_count(
    original: &TeletextSpatialTrace,
    writer: TeletextSpatialInvocationId,
) -> Option<usize> {
    let mut next = Some(writer);
    let mut count = 0_usize;
    while let Some(id) = next {
        let call = original.invocations().get(id.get())?;
        let position = call.caller_position();
        if position.row() > 24
            || position.column() > 71
            || !matches!(call.root_source(), TeletextSpatialRootSource::Local { .. })
        {
            return None;
        }
        count = count.checked_add(1)?;
        next = call.parent();
        if next.is_some_and(|parent| parent.get() >= id.get()) {
            return None;
        }
    }
    Some(count)
}

fn opening_error(
    owner: TeletextPageAddress,
    event: TeletextSpatialEvent,
    child: TeletextSpatialInvocationId,
    position: (u16, u16),
    count: usize,
) -> TeletextDisplayGeometryError {
    TeletextDisplayGeometryError {
        owner,
        position: Some(position),
        location: Some(event.source()),
        root_source: Some(event.root_source()),
        invocation: Some(child),
        reason: TeletextDisplayGeometryFailure::TooManySuccessiveObjectOpens { count, maximum: 15 },
    }
}

fn scan_groups(
    owner: TeletextPageAddress,
    actions: &[PositionedAction],
    pending: &mut Vec<TeletextSuccessiveOpenPending>,
    known_resets: bool,
    explicit_main: bool,
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
            if known_resets && explicit_main {
                maximum = maximum.max(main_prefix_before_resets(owner, group)?);
            }
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
                            return Err(opening_error(
                                owner,
                                item.event,
                                child,
                                item.position,
                                run,
                            ));
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

fn main_prefix_before_resets(
    owner: TeletextPageAddress,
    group: &[PositionedAction],
) -> Result<usize, TeletextDisplayGeometryError> {
    let mut first_reset_root = usize::MAX;
    for item in group {
        match item.kind {
            ActionKind::Open(_) if item.event.invocation().is_some() => return Ok(0),
            ActionKind::Open(_) => {}
            ActionKind::Write => {
                let TeletextSpatialRootSource::Local { command } = item.event.root_source() else {
                    return Ok(0);
                };
                first_reset_root = first_reset_root.min(command);
            }
        }
    }
    // EN300706 12.3.3 executes the body at invocation. A descendant cannot
    // reset Main before its own root call; its opening itself is included.
    // This proves only a prefix, never relative order between other emitters.
    let mut count = 0;
    for item in group {
        match item.kind {
            ActionKind::Open(child) if item.event.source().command <= first_reset_root => {
                count += 1;
                if count > 15 {
                    return Err(opening_error(
                        owner,
                        item.event,
                        child,
                        item.position,
                        count,
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(count)
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

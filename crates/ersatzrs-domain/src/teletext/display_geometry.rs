//! Bounded original-display coordinates (EN 300 706 §12.5.2 and Annex A.4).
//!
//! Raw panel scan order and selected visual order remain distinct. This model
//! retains every original contender without clipping or claiming final occupied
//! footprints, object-overlap acceptance, attributes, rasterization or readiness.

use std::mem::size_of;

use super::display_geometry_open_count::{
    count_open_objects, endpoint_storage_bytes, endpoint_work,
};

use super::{
    TeletextBaseDisplay, TeletextBaseDisplayCellKind, TeletextCandidateNonplacement,
    TeletextCandidatePlacement, TeletextColumnOperation, TeletextDisplayGeometry,
    TeletextEnhancementCommand, TeletextEnhancementSource, TeletextGlyphCandidate,
    TeletextGlyphLayers, TeletextGlyphPriority, TeletextObjectAddressInterval,
    TeletextObjectAreaPending, TeletextObjectDisplayRegion, TeletextObjectGeometryFact,
    TeletextObjectIntervalPending, TeletextObjectType, TeletextPageAddress,
    TeletextPlacedGlyphCandidate, TeletextRawDisplayArea, TeletextSpatialEvent,
    TeletextSpatialInvocationId, TeletextSpatialRootSource, TeletextSpatialTrace, TELETEXT_COLUMNS,
    TELETEXT_ROWS,
};

/// Trusted input-count policy, never a producer-deserializable capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDisplayGeometryBudget {
    pub max_candidates: usize,
    pub max_events: usize,
    pub max_invocations: usize,
}

/// Located static rejection, retaining the unchanged semantic source/instance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext display geometry for {owner:?} at {position:?}: {reason}")]
pub struct TeletextDisplayGeometryError {
    pub owner: TeletextPageAddress,
    pub position: Option<(u16, u16)>,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextDisplayGeometryFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDisplayGeometryFailure {
    #[error("candidate count {count} exceeds budget {maximum}")]
    CandidateBudget { count: usize, maximum: usize },
    #[error("event count {count} exceeds budget {maximum}")]
    EventBudget { count: usize, maximum: usize },
    #[error("invocation count {count} exceeds budget {maximum}")]
    InvocationBudget { count: usize, maximum: usize },
    #[error("input owner {actual:?} differs from Base owner")]
    OwnerMismatch { actual: TeletextPageAddress },
    #[error("geometry storage/work arithmetic exceeds supported range")]
    AllocationSize,
    #[error("original event {event} has inconsistent invocation/source")]
    EventMismatch { event: usize },
    #[error("original candidate {candidate} has inconsistent Base/event source")]
    CandidateMismatch { candidate: usize },
    #[error("own object writes cross from {previous:?} to {requested:?}")]
    CrossAreaWrite {
        previous: TeletextRawDisplayArea,
        requested: TeletextRawDisplayArea,
    },
    #[error("{count} known simultaneously open objects exceed maximum {maximum}")]
    TooManyOpenObjects { count: usize, maximum: usize },
}

/// Map exact raw candidates through the checked Base presentation's panel flags.
///
/// All three raw counts and exact linear storage/work arithmetic are fenced
/// before indexing or allocation. Base cells must equal the supplied immutable
/// trace; enhancement candidates must correspond one-for-one to original glyph
/// events and full source/instance identities. This correspondence is not a
/// generation certificate: the authoritative caller supplies the same service's
/// traces. Unsupported/out-of-display positions remain typed nonplacements,
/// not authoring prohibitions. No visual clipping, bounding rectangle, child
/// extent substitution or panel charset reset is inferred. Direct definite own
/// writes cannot cross selected areas (Annex A.4); a caller may invoke a separate
/// child in another area. Address-only/unplaced evidence remains typed pending.
/// Inclusive A.8 raw intervals preserve caller AP before offset and last direct
/// address. More than fifteen known simultaneously open intervals fail across
/// all object types, including inactive panels. Fifteen or fewer known intervals
/// do not certify pending intervals, successive opens, A.7 occupied overlap or C.3.
/// # Errors
/// Rejects budgets/storage arithmetic, owner mismatch, or inconsistent original
/// inputs, definite own writes crossing selected areas, or sixteen known open
/// objects. No partial result or caller mutation is returned.
pub fn resolve_teletext_display_geometry(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    layers: &TeletextGlyphLayers,
    budget: TeletextDisplayGeometryBudget,
) -> Result<TeletextDisplayGeometry, TeletextDisplayGeometryError> {
    preflight(base, original, layers, budget)?;
    let mut seen_events = vec![false; original.events().len()];
    let mut seen_base = vec![false; TELETEXT_ROWS * TELETEXT_COLUMNS];
    let mut seen_calls = vec![false; original.invocations().len()];
    validate_events(base.owner(), original, &mut seen_calls)?;
    let panels = base.presentation().map(|presentation| presentation.panels);
    let (left_width, right_width) =
        panels.map_or((0, 0), super::TeletextSidePanels::level25_widths);
    let mut records = Vec::with_capacity(layers.candidates().len());
    for (candidate_index, &candidate) in layers.candidates().iter().enumerate() {
        validate_candidate(
            base,
            original,
            candidate,
            candidate_index,
            &mut seen_events,
            &mut seen_base,
        )?;
        let area = raw_area(candidate.position().1);
        records.push(TeletextPlacedGlyphCandidate {
            candidate_index,
            candidate,
            area,
            placement: placement(candidate.position(), area, left_width, right_width),
        });
    }
    validate_completeness(base.owner(), original, &seen_events, &seen_base)?;
    let (object_regions, addresses, writes) =
        object_regions(base.owner(), original, left_width, right_width)?;
    let object_open_count = count_open_objects(base.owner(), &object_regions)?;
    Ok(TeletextDisplayGeometry {
        owner: base.owner(),
        panels,
        left_width,
        right_width,
        records,
        object_regions,
        addresses,
        writes,
        object_open_count,
    })
}

fn preflight(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    layers: &TeletextGlyphLayers,
    budget: TeletextDisplayGeometryBudget,
) -> Result<(), TeletextDisplayGeometryError> {
    let owner = base.owner();
    let fail = |reason| plain_error(owner, reason);
    let count = layers.candidates().len();
    if count > budget.max_candidates {
        return Err(candidate_error(
            owner,
            &layers.candidates()[budget.max_candidates],
            TeletextDisplayGeometryFailure::CandidateBudget {
                count,
                maximum: budget.max_candidates,
            },
        ));
    }
    let count = original.events().len();
    if count > budget.max_events {
        return Err(event_error(
            owner,
            original.events()[budget.max_events],
            TeletextDisplayGeometryFailure::EventBudget {
                count,
                maximum: budget.max_events,
            },
        ));
    }
    let count = original.invocations().len();
    if count > budget.max_invocations {
        let invocation = original.invocations()[budget.max_invocations];
        let mut error = fail(TeletextDisplayGeometryFailure::InvocationBudget {
            count,
            maximum: budget.max_invocations,
        });
        error.position = Some((invocation.origin().row(), invocation.origin().column()));
        error.location = Some(invocation.source());
        error.root_source = Some(invocation.root_source());
        error.invocation = Some(invocation.id());
        return Err(error);
    }
    for actual in [original.owner(), layers.owner()] {
        if actual != owner {
            return Err(fail(TeletextDisplayGeometryFailure::OwnerMismatch {
                actual,
            }));
        }
    }
    let storage = layers
        .candidates()
        .len()
        .checked_mul(size_of::<TeletextPlacedGlyphCandidate>())
        .and_then(|bytes| bytes.checked_add(original.events().len()))
        .and_then(|bytes| bytes.checked_add(original.invocations().len()))
        .and_then(|bytes| bytes.checked_add(TELETEXT_ROWS * TELETEXT_COLUMNS))
        .and_then(|bytes| {
            original
                .events()
                .len()
                .checked_mul(size_of::<TeletextObjectGeometryFact>() * 2)
                .and_then(|facts| bytes.checked_add(facts))
        })
        .and_then(|bytes| {
            original
                .invocations()
                .len()
                .checked_mul(size_of::<TeletextObjectDisplayRegion>())
                .and_then(|regions| bytes.checked_add(regions))
        })
        .and_then(|bytes| {
            endpoint_storage_bytes(original.invocations().len())
                .and_then(|endpoints| bytes.checked_add(endpoints))
        });
    let levels = usize::BITS - original.events().len().leading_zeros();
    let work = original
        .events()
        .len()
        .checked_mul(levels as usize + 1)
        .and_then(|sort| sort.checked_mul(2))
        .and_then(|sort| sort.checked_add(layers.candidates().len()))
        .and_then(|sort| sort.checked_add(original.invocations().len()))
        .and_then(|work| {
            endpoint_work(original.invocations().len())
                .and_then(|endpoints| work.checked_add(endpoints))
        });
    if storage.is_none_or(|bytes| bytes > isize::MAX as usize) || work.is_none() {
        return Err(fail(TeletextDisplayGeometryFailure::AllocationSize));
    }
    Ok(())
}

fn validate_events(
    owner: TeletextPageAddress,
    original: &TeletextSpatialTrace,
    seen_calls: &mut [bool],
) -> Result<(), TeletextDisplayGeometryError> {
    for (ordinal, &event) in original.events().iter().enumerate() {
        let fail = || {
            event_error(
                owner,
                event,
                TeletextDisplayGeometryFailure::EventMismatch { event: ordinal },
            )
        };
        let valid = match event.invocation() {
            None => event.source().object.is_none(),
            Some(id) => original
                .invocations()
                .get(id.get())
                .is_some_and(|instance| {
                    instance.id() == id
                        && event.source().object == Some(instance.target())
                        && event.root_source() == instance.root_source()
                }),
        };
        if !valid {
            return Err(fail());
        }
        match (event.command(), event.called_invocation()) {
            (
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    offset,
                },
                Some(id),
            ) => {
                let instance = original.invocations().get(id.get()).ok_or_else(fail)?;
                if instance.id() != id
                    || instance.target() != target
                    || instance.object_type() != object_type
                    || instance.offset() != offset
                    || instance.parent() != event.invocation()
                    || instance.source() != event.source()
                    || instance.root_source() != event.root_source()
                    || instance.caller_position() != event.absolute_before()
                    || seen_calls[id.get()]
                {
                    return Err(fail());
                }
                seen_calls[id.get()] = true;
            }
            (TeletextEnhancementCommand::InvokeObject { .. }, _) | (_, Some(_)) => {
                return Err(fail())
            }
            (_, None) => {}
        }
    }
    if seen_calls.iter().any(|&seen| !seen) {
        return Err(plain_error(
            owner,
            TeletextDisplayGeometryFailure::EventMismatch {
                event: original.events().len(),
            },
        ));
    }
    Ok(())
}

fn validate_candidate(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    candidate: TeletextGlyphCandidate,
    index: usize,
    seen_events: &mut [bool],
    seen_base: &mut [bool],
) -> Result<(), TeletextDisplayGeometryError> {
    let fail = || {
        candidate_error(
            base.owner(),
            &candidate,
            TeletextDisplayGeometryFailure::CandidateMismatch { candidate: index },
        )
    };
    if let Some(cell) = candidate.base() {
        let (row, column) = (cell.position().row(), cell.position().column());
        let ordinal = usize::from(row) * TELETEXT_COLUMNS + usize::from(column);
        if base.cell(row, column) != Some(&cell)
            || cell.kind() == TeletextBaseDisplayCellKind::HeaderOwned
            || candidate.position() != (u16::from(row), u16::from(column))
            || candidate.priority() != TeletextGlyphPriority::Base
            || seen_base[ordinal]
        {
            return Err(fail());
        }
        seen_base[ordinal] = true;
        return Ok(());
    }
    let ordinal = candidate.event_ordinal().ok_or_else(fail)?;
    let event = original.events().get(ordinal).ok_or_else(fail)?;
    let TeletextEnhancementCommand::AtColumn { operation, .. } = event.command() else {
        return Err(fail());
    };
    let priority = event
        .invocation()
        .map_or(TeletextGlyphPriority::Local, |id| {
            match original.invocations()[id.get()].object_type() {
                TeletextObjectType::Active => TeletextGlyphPriority::Active,
                TeletextObjectType::Adaptive => TeletextGlyphPriority::Adaptive,
                TeletextObjectType::Passive => TeletextGlyphPriority::Passive,
            }
        });
    if !is_glyph(operation)
        || candidate.operation() != Some(operation)
        || candidate.position()
            != (
                event.absolute_after().row(),
                event.absolute_after().column(),
            )
        || candidate.source() != Some(event.source())
        || candidate.root_source() != Some(event.root_source())
        || candidate.invocation() != event.invocation()
        || candidate.priority() != priority
        || seen_events[ordinal]
    {
        return Err(fail());
    }
    seen_events[ordinal] = true;
    Ok(())
}

fn validate_completeness(
    owner: TeletextPageAddress,
    original: &TeletextSpatialTrace,
    seen_events: &[bool],
    seen_base: &[bool],
) -> Result<(), TeletextDisplayGeometryError> {
    for (ordinal, &event) in original.events().iter().enumerate() {
        if matches!(event.command(), TeletextEnhancementCommand::AtColumn { operation, .. } if is_glyph(operation))
            && !seen_events[ordinal]
        {
            return Err(event_error(
                owner,
                event,
                TeletextDisplayGeometryFailure::EventMismatch { event: ordinal },
            ));
        }
    }
    if seen_base[8..].iter().any(|&seen| !seen) {
        return Err(plain_error(
            owner,
            TeletextDisplayGeometryFailure::CandidateMismatch { candidate: 0 },
        ));
    }
    Ok(())
}

fn is_glyph(operation: TeletextColumnOperation) -> bool {
    matches!(
        operation,
        TeletextColumnOperation::G0 { .. }
            | TeletextColumnOperation::G1 { .. }
            | TeletextColumnOperation::G1Alpha { .. }
            | TeletextColumnOperation::G2 { .. }
            | TeletextColumnOperation::G3 { .. }
            | TeletextColumnOperation::G0Diacritic { .. }
            | TeletextColumnOperation::Drcs { .. }
    )
}

type ObjectRegions = (
    Vec<TeletextObjectDisplayRegion>,
    Vec<TeletextObjectGeometryFact>,
    Vec<TeletextObjectGeometryFact>,
);

fn object_regions(
    owner: TeletextPageAddress,
    original: &TeletextSpatialTrace,
    left: u8,
    right: u8,
) -> Result<ObjectRegions, TeletextDisplayGeometryError> {
    let address_count = original
        .events()
        .iter()
        .filter(|event| event.invocation().is_some() && is_address(event.command()))
        .count();
    let write_count = original
        .events()
        .iter()
        .filter(|event| event.invocation().is_some() && is_write(event.command()))
        .count();
    let mut addresses = Vec::with_capacity(address_count);
    let mut writes = Vec::with_capacity(write_count);
    for (event_ordinal, &event) in original.events().iter().enumerate() {
        if event.invocation().is_some() {
            let fact = TeletextObjectGeometryFact {
                event_ordinal,
                event,
            };
            if is_address(event.command()) {
                addresses.push(fact);
            }
            if is_write(event.command()) {
                writes.push(fact);
            }
        }
    }
    // One shared vector per fact kind, not a separately allocated path/list per
    // object. Within an instance, original command order is retained exactly.
    let key = |fact: &TeletextObjectGeometryFact| {
        (
            fact.event
                .invocation()
                .map_or(usize::MAX, TeletextSpatialInvocationId::get),
            fact.event_ordinal,
        )
    };
    addresses.sort_unstable_by_key(key);
    writes.sort_unstable_by_key(key);
    let mut regions = Vec::with_capacity(original.invocations().len());
    let (mut address_cursor, mut write_cursor) = (0, 0);
    for &instance in original.invocations() {
        let (address_start, address_finish) =
            fact_span(&addresses, &mut address_cursor, instance.id());
        let (write_start, write_finish) = fact_span(&writes, &mut write_cursor, instance.id());
        let own_addresses = &addresses[address_start..address_finish];
        let own_writes = &writes[write_start..write_finish];
        let written_area = written_area(owner, own_writes, left, right)?;
        regions.push(TeletextObjectDisplayRegion {
            instance,
            written_area,
            area_pending: area_pending(own_addresses, written_area, left, right),
            interval: address_interval(instance.caller_position(), own_addresses.last()),
            address_start,
            address_finish,
            write_start,
            write_finish,
        });
    }
    Ok((regions, addresses, writes))
}

fn is_address(command: TeletextEnhancementCommand) -> bool {
    matches!(
        command,
        TeletextEnhancementCommand::SetPosition { .. }
            | TeletextEnhancementCommand::SetRowColour { .. }
            | TeletextEnhancementCommand::AtColumn { .. }
    )
}

fn is_write(command: TeletextEnhancementCommand) -> bool {
    match command {
        TeletextEnhancementCommand::AtColumn { operation, .. } => {
            is_glyph(operation)
                || matches!(
                    operation,
                    TeletextColumnOperation::Foreground { .. }
                        | TeletextColumnOperation::Background { .. }
                        | TeletextColumnOperation::Flash { .. }
                        | TeletextColumnOperation::Attributes { .. }
                )
        }
        _ => false,
    }
}

fn fact_span(
    facts: &[TeletextObjectGeometryFact],
    cursor: &mut usize,
    id: TeletextSpatialInvocationId,
) -> (usize, usize) {
    let start = *cursor;
    while facts
        .get(*cursor)
        .is_some_and(|fact| fact.event.invocation() == Some(id))
    {
        *cursor += 1;
    }
    (start, *cursor)
}

fn fact_position(fact: &TeletextObjectGeometryFact) -> (u16, u16) {
    let position = fact.event.absolute_after();
    (position.row(), position.column())
}

fn written_area(
    owner: TeletextPageAddress,
    facts: &[TeletextObjectGeometryFact],
    left: u8,
    right: u8,
) -> Result<Option<TeletextRawDisplayArea>, TeletextDisplayGeometryError> {
    let mut previous = None;
    for fact in facts {
        let position = fact_position(fact);
        let area = raw_area(position.1);
        if matches!(
            placement(position, area, left, right),
            TeletextCandidatePlacement::Placed { .. }
        ) {
            if let Some(previous) = previous {
                if previous != area {
                    return Err(event_error(
                        owner,
                        fact.event,
                        TeletextDisplayGeometryFailure::CrossAreaWrite {
                            previous,
                            requested: area,
                        },
                    ));
                }
            }
            previous = Some(area);
        }
    }
    Ok(previous)
}

fn area_pending(
    facts: &[TeletextObjectGeometryFact],
    written: Option<TeletextRawDisplayArea>,
    left: u8,
    right: u8,
) -> Option<TeletextObjectAreaPending> {
    let mut first_area = written;
    let mut crosses = false;
    let mut unplaced = false;
    for fact in facts {
        let position = fact_position(fact);
        let area = raw_area(position.1);
        if matches!(
            placement(position, area, left, right),
            TeletextCandidatePlacement::Unplaced(_)
        ) {
            unplaced = true;
        } else {
            if first_area.is_some_and(|previous| previous != area) {
                crosses = true;
            }
            first_area = Some(area);
        }
    }
    if unplaced {
        Some(TeletextObjectAreaPending::UnplacedAddress)
    } else if crosses {
        Some(TeletextObjectAreaPending::AddressCrossesAreas)
    } else if written.is_none() {
        Some(TeletextObjectAreaPending::NoDefiniteWrites)
    } else {
        None
    }
}

fn address_interval(
    caller: super::TeletextSpatialPosition,
    last: Option<&TeletextObjectGeometryFact>,
) -> TeletextObjectAddressInterval {
    use TeletextObjectAddressInterval::{Known, Pending};
    let start = (caller.row(), caller.column());
    let Some(last) = last else {
        return Pending {
            reason: TeletextObjectIntervalPending::NoDirectAddress,
            endpoint: None,
        };
    };
    let finish = fact_position(last);
    if start.0 > 24 || start.1 > 71 || finish.0 > 24 || finish.1 > 71 {
        return Pending {
            reason: TeletextObjectIntervalPending::OutsideDisplay,
            endpoint: Some(finish),
        };
    }
    if finish < start {
        return Pending {
            reason: TeletextObjectIntervalPending::EndBeforeStart,
            endpoint: Some(finish),
        };
    }
    Known { start, finish }
}

fn raw_area(column: u16) -> TeletextRawDisplayArea {
    match column {
        0..=39 => TeletextRawDisplayArea::Main,
        40..=55 => TeletextRawDisplayArea::RightPanel,
        56..=71 => TeletextRawDisplayArea::LeftPanel,
        _ => TeletextRawDisplayArea::Outside,
    }
}

fn placement(
    (row, column): (u16, u16),
    area: TeletextRawDisplayArea,
    left: u8,
    right: u8,
) -> TeletextCandidatePlacement {
    use TeletextCandidateNonplacement as Reason;
    use TeletextCandidatePlacement::{Placed, Unplaced};
    if row > 24 || area == TeletextRawDisplayArea::Outside {
        return Unplaced(Reason::OutsideDisplay);
    }
    if row == 0 && column < 8 {
        return Unplaced(Reason::HeaderOwned);
    }
    if row == 0 && area != TeletextRawDisplayArea::Main {
        return Unplaced(Reason::HeaderPanel);
    }
    let visual = match area {
        TeletextRawDisplayArea::Main => u16::from(left) + column,
        TeletextRawDisplayArea::RightPanel if right == 0 => return Unplaced(Reason::DisabledPanel),
        TeletextRawDisplayArea::LeftPanel if left == 0 => return Unplaced(Reason::DisabledPanel),
        TeletextRawDisplayArea::RightPanel if column < 40 + u16::from(right) => {
            u16::from(left) + column
        }
        TeletextRawDisplayArea::LeftPanel if column >= 72 - u16::from(left) => {
            column - (72 - u16::from(left))
        }
        _ => return Unplaced(Reason::OutsideActiveWidth),
    };
    // The checked branches above prove row<=24 and visual<72; conversion never
    // clamps. Retain a typed nonplacement if future area rules extend this range.
    match (u8::try_from(row), u8::try_from(visual)) {
        (Ok(row), Ok(column)) => Placed { row, column },
        _ => Unplaced(Reason::OutsideDisplay),
    }
}

fn plain_error(
    owner: TeletextPageAddress,
    reason: TeletextDisplayGeometryFailure,
) -> TeletextDisplayGeometryError {
    TeletextDisplayGeometryError {
        owner,
        position: None,
        location: None,
        root_source: None,
        invocation: None,
        reason,
    }
}

fn candidate_error(
    owner: TeletextPageAddress,
    candidate: &TeletextGlyphCandidate,
    reason: TeletextDisplayGeometryFailure,
) -> TeletextDisplayGeometryError {
    TeletextDisplayGeometryError {
        owner,
        position: Some(candidate.position()),
        location: candidate.source(),
        root_source: candidate.root_source(),
        invocation: candidate.invocation(),
        reason,
    }
}

fn event_error(
    owner: TeletextPageAddress,
    event: TeletextSpatialEvent,
    reason: TeletextDisplayGeometryFailure,
) -> TeletextDisplayGeometryError {
    TeletextDisplayGeometryError {
        owner,
        position: Some((
            event.absolute_after().row(),
            event.absolute_after().column(),
        )),
        location: Some(event.source()),
        root_source: Some(event.root_source()),
        invocation: event.invocation(),
        reason,
    }
}

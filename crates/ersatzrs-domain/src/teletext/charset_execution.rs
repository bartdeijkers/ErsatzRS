//! Original G0/G2 designation contexts (EN 300 706 Annex C.4).
//!
//! Local/Active writes share an absolute-row spatial context; logical recursive
//! event preorder is not transmission time. Adaptive/Passive contexts are fresh
//! per invocation and persist across their own rows. Every original contender
//! survives, including losing glyphs. This executes no ESC, character tables,
//! DRCS selection, geometry, rasterization, or publication/readiness policy.
//! In particular, a selected non-Latin primary does not certify the Unicode
//! fidelity of the separately retained Western Latin Base fallback bytes.

use std::mem::size_of;

use super::charset_execution_values::PendingFilter;
use super::{
    TeletextBaseDisplay, TeletextBaseDisplayCellKind, TeletextBaseG0Origin,
    TeletextBaseG0Selection, TeletextBaseSpacingControl, TeletextCharsetDeclaration,
    TeletextCharsetExecution, TeletextCharsetSelection, TeletextColumnOperation,
    TeletextEnhancementCommand, TeletextEnhancementSource, TeletextExecutedCharsetSelection,
    TeletextExecutedGlyphCharset, TeletextGlyphCandidate, TeletextGlyphCharsetUsage,
    TeletextGlyphLayers, TeletextGlyphPriority, TeletextObjectType, TeletextPageAddress,
    TeletextPendingCharsetSelection, TeletextSpatialEvent, TeletextSpatialInvocationId,
    TeletextSpatialRootSource, TeletextSpatialTrace, TELETEXT_COLUMNS, TELETEXT_ROWS,
};

/// Static, located rejection; no partial context or caller mutation escapes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext charset execution for {owner:?} at {location:?}: {reason}")]
pub struct TeletextCharsetExecutionError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextCharsetExecutionFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextCharsetExecutionFailure {
    #[error("input owner {original:?} differs from Base owner")]
    OwnerMismatch { original: TeletextPageAddress },
    #[error("original command count {count} exceeds budget {maximum}")]
    CommandBudget { count: usize, maximum: usize },
    #[error("glyph candidate {candidate} does not match supplied original inputs")]
    CandidateMismatch { candidate: usize },
    #[error("event {event} has inconsistent original invocation/source")]
    EventMismatch { event: usize },
    #[error("charset execution allocation size exceeds storage range")]
    AllocationSize,
    #[error("Base ESC at row {row}, column {column} requires an explicit second-G0 declaration; receiver-dependent context is not inferred")]
    DecoderDependentBaseG0 { row: u8, column: u8 },
}

#[derive(Debug, Clone, Copy)]
struct DeclarationGroup {
    position: (u16, u16),
    start: usize,
    finish: usize,
    previous: TeletextExecutedCharsetSelection,
    final_selection: TeletextExecutedCharsetSelection,
}

/// Resolve original glyph contexts without interpreting coded glyphs.
///
/// Exact original event count is bounded before indexing/allocation. Allocation
/// is linear in that count plus the fixed Base cells; shared declarations and
/// pending spans have no per-cell contender vector. Matching identities and
/// candidate/event correspondence do not establish immutable generation identity:
/// the authoritative caller must supply the same service's original inputs.
/// Original Base ESC toggles only its row-local G0; secondary never designates G2.
/// Shared context spans the absolute row, without inventing a panel-area reset.
/// # Errors
/// Rejects differing owners, exceeded command/allocation limits, and inconsistent
/// candidate/event/source correspondence. Equal-position physical-order questions
/// are retained as typed pending selections, rather than rejected or guessed.
pub fn resolve_teletext_charset_execution(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    glyph_layers: &TeletextGlyphLayers,
    max_commands: usize,
) -> Result<TeletextCharsetExecution, TeletextCharsetExecutionError> {
    let owner = base.owner();
    preflight_inputs(base, original, glyph_layers, max_commands)?;
    let declaration_count = original
        .events()
        .iter()
        .filter(|event| {
            matches!(
                event.command(),
                TeletextEnhancementCommand::AtColumn {
                    operation: TeletextColumnOperation::DesignateCharset { .. },
                    ..
                }
            )
        })
        .count();
    let shared_count = original
        .events()
        .iter()
        .filter(|&&event| {
            is_shared(original, event)
                && matches!(
                    event.command(),
                    TeletextEnhancementCommand::AtColumn {
                        operation: TeletextColumnOperation::DesignateCharset { .. },
                        ..
                    }
                )
        })
        .count();
    let page = TeletextExecutedCharsetSelection::known(TeletextCharsetSelection::PagePrimary(
        base.primary(),
    ));
    let mut execution = TeletextCharsetExecution {
        owner,
        primary: base.primary(),
        secondary: base.presentation().and_then(|value| value.secondary),
        national_option: base.national_option(),
        extension_present: base.presentation().is_some(),
        records: Vec::with_capacity(glyph_layers.candidates().len()),
        declarations: Vec::with_capacity(declaration_count),
        shared: Vec::with_capacity(shared_count),
        pending: Vec::with_capacity(original.events().len()),
    };
    let declarations = collect_declarations(original, &mut execution);
    let groups = group_shared_declarations(&mut execution, page);
    let states = resolve_event_contexts(original, &declarations, &groups, &mut execution, page);
    let base_g0 = base_g0_selections(base)?;
    let mut seen = vec![false; original.events().len()];
    for (index, &candidate) in glyph_layers.candidates().iter().enumerate() {
        validate_candidate(base, original, candidate, index, &mut seen)?;
        let (usage, selection) = if let Some(operation) = candidate.operation() {
            let usage =
                glyph_usage(operation).ok_or_else(|| candidate_error(owner, candidate, index))?;
            let selection = candidate
                .event_ordinal()
                .and_then(|ordinal| states[ordinal]);
            (usage, selection)
        } else {
            (TeletextGlyphCharsetUsage::BaseCoded, Some(page))
        };
        execution.records.push(TeletextExecutedGlyphCharset {
            candidate,
            usage,
            selection,
            base_g0: candidate.base().map(|cell| {
                base_g0[usize::from(cell.position().row())][usize::from(cell.position().column())]
            }),
        });
    }
    Ok(execution)
}

fn base_g0_selections(
    base: &TeletextBaseDisplay,
) -> Result<
    [[TeletextBaseG0Selection; TELETEXT_COLUMNS]; TELETEXT_ROWS],
    TeletextCharsetExecutionError,
> {
    let mut rows = [[TeletextBaseG0Selection {
        designation: base.primary(),
        origin: TeletextBaseG0Origin::RowPrimary { row: 0 },
    }; TELETEXT_COLUMNS]; TELETEXT_ROWS];
    for row in 0_u8..25 {
        let mut active = TeletextBaseG0Selection {
            designation: base.primary(),
            origin: TeletextBaseG0Origin::RowPrimary { row },
        };
        for column in 0_u8..40 {
            rows[usize::from(row)][usize::from(column)] = active;
            if let Some(cell) = base.cell(row, column) {
                if matches!(
                    cell.kind(),
                    TeletextBaseDisplayCellKind::SpacingControl {
                        control: TeletextBaseSpacingControl::Escape,
                        ..
                    }
                ) {
                    if base.presentation().is_none() {
                        return Err(plain_error(
                            base.owner(),
                            TeletextCharsetExecutionFailure::DecoderDependentBaseG0 { row, column },
                        ));
                    }
                    // Table33's absent-secondary sentinel disables the toggle;
                    // the original ESC cell still remains in the Base trace.
                    if let Some(secondary) = base.presentation().and_then(|value| value.secondary) {
                        active = if matches!(
                            active.origin,
                            TeletextBaseG0Origin::EscapeSecondary { .. }
                        ) {
                            TeletextBaseG0Selection {
                                designation: base.primary(),
                                origin: TeletextBaseG0Origin::EscapePrimary {
                                    position: cell.position(),
                                },
                            }
                        } else {
                            TeletextBaseG0Selection {
                                designation: secondary,
                                origin: TeletextBaseG0Origin::EscapeSecondary {
                                    position: cell.position(),
                                },
                            }
                        };
                    }
                }
            }
        }
    }
    Ok(rows)
}

fn preflight_inputs(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    layers: &TeletextGlyphLayers,
    maximum: usize,
) -> Result<(), TeletextCharsetExecutionError> {
    let owner = base.owner();
    let fail = |reason| plain_error(owner, reason);
    for actual in [original.owner(), layers.owner()] {
        if actual != owner {
            return Err(fail(TeletextCharsetExecutionFailure::OwnerMismatch {
                original: actual,
            }));
        }
    }
    let count = original.events().len();
    if count > maximum {
        let mut error = fail(TeletextCharsetExecutionFailure::CommandBudget { count, maximum });
        if let Some(&event) = original.events().get(maximum) {
            attach_event(&mut error, event);
        }
        return Err(error);
    }
    // Every invocation corresponds to one counted call event. All scratch and
    // pending registries are at most events; output is fixed Base + glyph events.
    // This checks arithmetic/storage bounds, not arbitrary JSON/RSS limits.
    let candidates_maximum = (TELETEXT_ROWS * TELETEXT_COLUMNS - 8)
        .checked_add(count)
        .ok_or_else(|| fail(TeletextCharsetExecutionFailure::AllocationSize))?;
    let allocation = count
        .checked_mul(
            size_of::<TeletextCharsetDeclaration>()
                + size_of::<DeclarationGroup>()
                + size_of::<TeletextPendingCharsetSelection>() * 2
                + size_of::<usize>() * 4
                + size_of::<TeletextExecutedCharsetSelection>() * 2,
        )
        .and_then(|value| {
            candidates_maximum
                .checked_mul(size_of::<TeletextExecutedGlyphCharset>())
                .and_then(|output| value.checked_add(output))
        })
        .filter(|&value| isize::try_from(value).is_ok());
    let search_levels = usize::BITS - count.leading_zeros();
    let work_bound = count
        .checked_mul(search_levels as usize + 1)
        .and_then(|value| value.checked_mul(8));
    if allocation.is_none() || work_bound.is_none() || original.invocations().len() > count {
        return Err(fail(TeletextCharsetExecutionFailure::AllocationSize));
    }
    let mut glyphs = 0;
    for (ordinal, &event) in original.events().iter().enumerate() {
        let valid = match event.invocation() {
            None => event.source().object.is_none(),
            Some(id) => original
                .invocations()
                .get(id.get())
                .is_some_and(|invocation| {
                    invocation.id() == id && event.source().object == Some(invocation.target())
                }),
        };
        if !valid {
            let mut error = fail(TeletextCharsetExecutionFailure::EventMismatch { event: ordinal });
            attach_event(&mut error, event);
            return Err(error);
        }
        if let TeletextEnhancementCommand::AtColumn { operation, .. } = event.command() {
            glyphs += usize::from(glyph_usage(operation).is_some());
        }
    }
    if layers.candidates().len() != TELETEXT_ROWS * TELETEXT_COLUMNS - 8 + glyphs {
        return Err(fail(TeletextCharsetExecutionFailure::CandidateMismatch {
            candidate: 0,
        }));
    }
    Ok(())
}

fn collect_declarations(
    original: &TeletextSpatialTrace,
    execution: &mut TeletextCharsetExecution,
) -> Vec<Option<usize>> {
    let mut indexes = vec![None; original.events().len()];
    for (ordinal, &event) in original.events().iter().enumerate() {
        if let TeletextEnhancementCommand::AtColumn {
            operation: TeletextColumnOperation::DesignateCharset { designation },
            ..
        } = event.command()
        {
            let index = execution.declarations.len();
            indexes[ordinal] = Some(index);
            execution.declarations.push(TeletextCharsetDeclaration {
                designation,
                event_ordinal: ordinal,
                position: (
                    event.absolute_after().row(),
                    event.absolute_after().column(),
                ),
                source: event.source(),
                root_source: event.root_source(),
                invocation: event.invocation(),
            });
            if is_shared(original, event) {
                execution.shared.push(index);
            }
        }
    }
    execution.shared.sort_unstable_by_key(|&index| {
        let declaration = execution.declarations[index];
        (
            declaration.position,
            declaration.invocation.map(TeletextSpatialInvocationId::get),
            declaration.source.command,
        )
    });
    indexes
}

fn group_shared_declarations(
    execution: &mut TeletextCharsetExecution,
    page: TeletextExecutedCharsetSelection,
) -> Vec<DeclarationGroup> {
    let mut groups: Vec<DeclarationGroup> = Vec::with_capacity(execution.shared.len());
    let mut start = 0;
    while start < execution.shared.len() {
        let position = execution.declarations[execution.shared[start]].position;
        let mut finish = start + 1;
        while finish < execution.shared.len()
            && execution.declarations[execution.shared[finish]].position == position
        {
            finish += 1;
        }
        let previous = groups
            .last()
            .filter(|group| group.position.0 == position.0)
            .map_or(page, |group| group.final_selection);
        let first = execution.declarations[execution.shared[start]];
        let last = execution.declarations[execution.shared[finish - 1]];
        let final_selection = if first.invocation == last.invocation {
            declaration_selection(execution, execution.shared[finish - 1])
        } else {
            add_pending(
                execution,
                TeletextPendingCharsetSelection {
                    position,
                    previous,
                    start,
                    finish,
                    filter: PendingFilter::Final,
                },
            )
        };
        groups.push(DeclarationGroup {
            position,
            start,
            finish,
            previous,
            final_selection,
        });
        start = finish;
    }
    groups
}

fn resolve_event_contexts(
    original: &TeletextSpatialTrace,
    declarations: &[Option<usize>],
    groups: &[DeclarationGroup],
    execution: &mut TeletextCharsetExecution,
    page: TeletextExecutedCharsetSelection,
) -> Vec<Option<TeletextExecutedCharsetSelection>> {
    let mut states = vec![None; original.events().len()];
    let mut private = vec![page; original.invocations().len()];
    for (ordinal, &event) in original.events().iter().enumerate() {
        if !is_shared(original, event) {
            // Preflight established this executing instance exists. Child events
            // update a distinct state, never the parent/caller's private context.
            if let Some(id) = event.invocation() {
                if let Some(index) = declarations[ordinal] {
                    private[id.get()] = declaration_selection(execution, index);
                }
                if event_needs_charset(event) {
                    states[ordinal] = Some(private[id.get()]);
                }
            }
        } else if event_needs_charset(event) {
            states[ordinal] = Some(shared_selection(event, groups, execution, page));
        }
    }
    states
}

fn shared_selection(
    event: TeletextSpatialEvent,
    groups: &[DeclarationGroup],
    execution: &mut TeletextCharsetExecution,
    page: TeletextExecutedCharsetSelection,
) -> TeletextExecutedCharsetSelection {
    let position = (
        event.absolute_after().row(),
        event.absolute_after().column(),
    );
    let end = groups.partition_point(|group| group.position <= position);
    let Some(group) = end
        .checked_sub(1)
        .and_then(|index| groups.get(index))
        .filter(|group| group.position.0 == position.0)
    else {
        return page;
    };
    if group.position != position {
        return group.final_selection;
    }
    // Within a position, declarations are sorted by instance then own command.
    // Binary boundaries avoid rescanning a large equal-position group for each
    // glyph; construction work is O(events log events), allocation O(events).
    let span = &execution.shared[group.start..group.finish];
    let instance = event.invocation().map(TeletextSpatialInvocationId::get);
    let first = span.partition_point(|&index| {
        execution.declarations[index]
            .invocation
            .map(TeletextSpatialInvocationId::get)
            < instance
    });
    let finish = span.partition_point(|&index| {
        execution.declarations[index]
            .invocation
            .map(TeletextSpatialInvocationId::get)
            <= instance
    });
    let own_span = &span[first..finish];
    let end = own_span.partition_point(|&index| {
        execution.declarations[index].source.command < event.source().command
    });
    let own = end.checked_sub(1).map(|index| own_span[index]);
    let cross = first > 0 || finish < span.len();
    let previous = own.map_or(group.previous, |index| {
        declaration_selection(execution, index)
    });
    if !cross {
        return previous;
    }
    add_pending(
        execution,
        TeletextPendingCharsetSelection {
            position,
            previous,
            start: group.start,
            finish: group.finish,
            filter: PendingFilter::Glyph {
                invocation: event.invocation(),
                command: event.source().command,
            },
        },
    )
}

fn declaration_selection(
    execution: &TeletextCharsetExecution,
    index: usize,
) -> TeletextExecutedCharsetSelection {
    let mut selection = TeletextExecutedCharsetSelection::known(
        TeletextCharsetSelection::Enhancement(execution.declarations[index].designation),
    );
    selection.declaration = Some(index);
    selection
}

fn add_pending(
    execution: &mut TeletextCharsetExecution,
    pending: TeletextPendingCharsetSelection,
) -> TeletextExecutedCharsetSelection {
    let index = execution.pending.len();
    execution.pending.push(pending);
    TeletextExecutedCharsetSelection::unresolved(index)
}

fn is_shared(original: &TeletextSpatialTrace, event: TeletextSpatialEvent) -> bool {
    event.invocation().is_none_or(|id| {
        original.invocations()[id.get()].object_type() == TeletextObjectType::Active
    })
}

fn event_needs_charset(event: TeletextSpatialEvent) -> bool {
    match event.command() {
        TeletextEnhancementCommand::AtColumn { operation, .. } => {
            glyph_usage(operation).is_some_and(|usage| usage.uses_g0() || usage.uses_g2())
        }
        _ => false,
    }
}

fn glyph_usage(operation: TeletextColumnOperation) -> Option<TeletextGlyphCharsetUsage> {
    Some(match operation {
        TeletextColumnOperation::G0 { .. } => TeletextGlyphCharsetUsage::G0,
        TeletextColumnOperation::G1Alpha { .. } => TeletextGlyphCharsetUsage::G1Alpha,
        TeletextColumnOperation::G2 { .. } => TeletextGlyphCharsetUsage::G2,
        TeletextColumnOperation::G0Diacritic { code, diacritic } => {
            TeletextGlyphCharsetUsage::G0Diacritic {
                diacritic,
                special_at: diacritic.get() == 0 && code.get() == 0x2a,
            }
        }
        TeletextColumnOperation::G1 { .. } => TeletextGlyphCharsetUsage::G1Mosaic,
        TeletextColumnOperation::G3 { .. } => TeletextGlyphCharsetUsage::G3,
        TeletextColumnOperation::Drcs { .. } => TeletextGlyphCharsetUsage::Drcs,
        _ => return None,
    })
}

fn validate_candidate(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    candidate: TeletextGlyphCandidate,
    index: usize,
    seen: &mut [bool],
) -> Result<(), TeletextCharsetExecutionError> {
    let fail = || candidate_error(base.owner(), candidate, index);
    if let Some(cell) = candidate.base() {
        let row = cell.position().row();
        let column = cell.position().column();
        if base.cell(row, column) != Some(&cell)
            || cell.kind() == TeletextBaseDisplayCellKind::HeaderOwned
            || candidate.position() != (u16::from(row), u16::from(column))
            || candidate.priority() != TeletextGlyphPriority::Base
        {
            return Err(fail());
        }
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
    if glyph_usage(operation).is_none()
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
        || seen[ordinal]
    {
        return Err(fail());
    }
    seen[ordinal] = true;
    Ok(())
}

fn plain_error(
    owner: TeletextPageAddress,
    reason: TeletextCharsetExecutionFailure,
) -> TeletextCharsetExecutionError {
    TeletextCharsetExecutionError {
        owner,
        location: None,
        root_source: None,
        invocation: None,
        reason,
    }
}
fn attach_event(error: &mut TeletextCharsetExecutionError, event: TeletextSpatialEvent) {
    error.location = Some(event.source());
    error.root_source = Some(event.root_source());
    error.invocation = event.invocation();
}
fn candidate_error(
    owner: TeletextPageAddress,
    candidate: TeletextGlyphCandidate,
    index: usize,
) -> TeletextCharsetExecutionError {
    TeletextCharsetExecutionError {
        owner,
        location: candidate.source(),
        root_source: candidate.root_source(),
        invocation: candidate.invocation(),
        reason: TeletextCharsetExecutionFailure::CandidateMismatch { candidate: index },
    }
}

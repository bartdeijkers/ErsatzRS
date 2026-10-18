//! Typed physical header-position candidates, never neutral wire fillers.
//!
//! A known original colour attribute is only a candidate: its real row-tail effects
//! remain visible to the existing physical replay and whole-model comparisons.
//! This module neither changes the authored target nor grants an edit operand.

use super::compile_context::TeletextCompileContext;
use super::{
    TeletextMainComposition, TeletextMainCompositionBudget, TeletextMainCompositionError,
    TeletextMainCompositionFailure, TeletextMainCompositionSource,
};
use ersatzrs_domain::{
    TeletextAttributePendingReason, TeletextAttributeSource, TeletextColourReference,
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextObjectScope, TeletextObjectType, TeletextSemanticService, TeletextSpatialInvocation,
    TeletextSpatialRootSource,
};

#[derive(Clone, Copy)]
struct Setter {
    before: usize,
    command: usize,
    column: u8,
    operation: TeletextColumnOperation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UnsettledHeaderPosition {
    /// Unresolved call index in the returned, already materialized candidate.
    pub before: usize,
    pub column: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScanEnd {
    Complete,
    Unsettled(UnsettledHeaderPosition),
    Unsupported,
}

struct Plan {
    setters: Vec<Setter>,
    end: ScanEnd,
}

/// Legacy all-or-nothing planner policy, retained for its focused regressions.
#[cfg(test)]
pub(super) fn materialize(
    composition: TeletextMainComposition,
    service: &TeletextSemanticService,
    budget: TeletextMainCompositionBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<TeletextMainComposition, TeletextMainCompositionError> {
    let plan = plan(&composition, budget)?;
    if plan.end != ScanEnd::Complete {
        // Unknown context or an unsupported position stays an explicit native
        // lowering failure. Do not guess a colour or return a partial recipe.
        return Ok(composition);
    }
    apply(composition, service, budget, context, &plan.setters)
}

/// Apply only checked setters preceding the first unresolved header context.
/// The caller may settle sparse, authenticated prefix evidence and resume. This
/// helper never chooses a pending writer or grants partial output authority.
pub(super) fn materialize_until_unsettled(
    composition: TeletextMainComposition,
    service: &TeletextSemanticService,
    budget: TeletextMainCompositionBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<(TeletextMainComposition, Option<UnsettledHeaderPosition>), TeletextMainCompositionError>
{
    let plan = plan(&composition, budget)?;
    let boundary = match plan.end {
        ScanEnd::Unsettled(mut boundary) => {
            // Every admitted setter precedes the boundary; the complete new
            // command count was checked before allocating the recipe.
            boundary.before += plan.setters.len();
            Some(boundary)
        }
        ScanEnd::Complete | ScanEnd::Unsupported => None,
    };
    let composition = apply(composition, service, budget, context, &plan.setters)?;
    Ok((composition, boundary))
}

fn apply(
    composition: TeletextMainComposition,
    service: &TeletextSemanticService,
    budget: TeletextMainCompositionBudget,
    context: &mut TeletextCompileContext<'_, '_>,
    setters: &[Setter],
) -> Result<TeletextMainComposition, TeletextMainCompositionError> {
    if setters.is_empty() {
        return Ok(composition);
    }
    let count = composition.program().commands().len() + setters.len();
    let mut commands = Vec::with_capacity(count);
    let mut provenance = Vec::with_capacity(count);
    let mut next = setters.iter().peekable();
    for (index, (&command, &source)) in composition
        .program()
        .commands()
        .iter()
        .zip(composition.provenance())
        .enumerate()
    {
        if let Some(setter) = next.peek().filter(|setter| setter.before == index) {
            commands.push(TeletextEnhancementCommand::AtColumn {
                column: setter.column,
                operation: setter.operation,
            });
            provenance.push(TeletextMainCompositionSource::HeaderPosition {
                command: setter.command,
            });
            next.next();
        }
        commands.push(command);
        provenance.push(source);
    }
    let program = TeletextEnhancementProgram::new(commands).map_err(|reason| {
        TeletextMainCompositionError {
            owner: composition.owner(),
            location: None,
            reason: reason.into(),
        }
    })?;
    composition.with_physical_main(service, program, provenance, budget, context)
}

fn plan(
    composition: &TeletextMainComposition,
    budget: TeletextMainCompositionBudget,
) -> Result<Plan, TeletextMainCompositionError> {
    let mut count = 0_usize;
    // Count and validate the complete recipe before allocating any candidate.
    let end = visit_setters(composition, |_, command, _, _| {
        count = count
            .checked_add(1)
            .ok_or_else(|| output_error(composition, budget, command))?;
        if composition
            .program()
            .commands()
            .len()
            .checked_add(count)
            .is_none_or(|total| total > budget.max_output_commands)
        {
            return Err(output_error(composition, budget, command));
        }
        Ok(())
    })?;
    let mut setters = Vec::with_capacity(count);
    visit_setters(composition, |before, command, column, operation| {
        setters.push(Setter {
            before,
            command,
            column,
            operation,
        });
        Ok(())
    })?;
    Ok(Plan { setters, end })
}

fn visit_setters(
    composition: &TeletextMainComposition,
    mut visit: impl FnMut(
        usize,
        usize,
        u8,
        TeletextColumnOperation,
    ) -> Result<(), TeletextMainCompositionError>,
) -> Result<ScanEnd, TeletextMainCompositionError> {
    let mut logical = (0, 0);
    let mut emitted = (0, 0);
    let mut source = None;
    for (index, (&command, &provenance)) in composition
        .program()
        .commands()
        .iter()
        .zip(composition.provenance())
        .enumerate()
    {
        match command {
            TeletextEnhancementCommand::SetPosition { position } => {
                logical = (position.row(), position.column());
                if position.row() != 0 {
                    emitted = logical;
                }
                source = Some(provenance);
            }
            TeletextEnhancementCommand::SetRowColour { row, .. } => {
                logical = (row, 0);
                emitted = logical;
            }
            TeletextEnhancementCommand::AtColumn { column, .. } => {
                logical.1 = column;
                emitted = logical;
            }
            TeletextEnhancementCommand::InvokeObject { .. } if logical != emitted => {
                if logical.0 != 0 || emitted.0 != 0 || logical.1 < 8 {
                    return Ok(ScanEnd::Unsupported);
                }
                let Some(TeletextMainCompositionSource::OriginalPage { command }) = source else {
                    return Ok(ScanEnd::Unsupported);
                };
                let operation = foreground(composition, logical.1)
                    .map(|colour| TeletextColumnOperation::Foreground { colour })
                    .or_else(|| {
                        current_local_background(composition, index, logical.1)
                            .map(|colour| TeletextColumnOperation::Background { colour })
                    });
                let Some(operation) = operation else {
                    return Ok(ScanEnd::Unsettled(UnsettledHeaderPosition {
                        before: index,
                        column: logical.1,
                    }));
                };
                visit(index, command, logical.1, operation)?;
                emitted = logical;
            }
            TeletextEnhancementCommand::SetScreenColour { .. }
            | TeletextEnhancementCommand::InvokeObject { .. } => {}
        }
    }
    Ok(ScanEnd::Complete)
}

pub(super) fn foreground(
    composition: &TeletextMainComposition,
    column: u8,
) -> Option<TeletextColourReference> {
    // Passive paint is not the Main/Active attribute context. In particular,
    // never copy the called Passive object's foreground onto the Main row tail.
    let foreground = composition
        .attribute_execution()
        .before_adaptive_cell(0, column)?
        .foreground();
    foreground.source()?;
    foreground.value()
}

fn current_local_background(
    composition: &TeletextMainComposition,
    before: usize,
    column: u8,
) -> Option<TeletextColourReference> {
    let call = current_local_active(composition, before, column)?;
    let attributes = composition.attribute_execution();
    let attributes_at_position = attributes.before_adaptive_cell(0, column)?;
    let pending = attributes
        .pending()
        .get(attributes_at_position.foreground().pending()?)?;
    if pending.reason() != TeletextAttributePendingReason::TransmissionOrder
        || pending.root_source() != call.root_source()
        || pending.invocation() != Some(call.id())
        || ![pending.first_source(), pending.second_source()]
            .into_iter()
            .all(|source| {
                matches!(source, Some(TeletextAttributeSource::Enhancement {
                location, root_source, invocation: Some(invocation), ..
            }) if location.object == Some(call.target()) && root_source == call.root_source()
                && invocation == call.id())
            })
    {
        return None;
    }
    let background = attributes_at_position.background();
    if background.pending().is_some()
        || !matches!(
            background.source(),
            Some(TeletextAttributeSource::Base { .. })
        )
    {
        return None;
    }
    // This real Base-background candidate sets the same AP without introducing
    // a Main foreground writer at the Active object's unresolved C.2 location.
    // Complete physical replay still checks its entire row-tail effect.
    background.value()
}

fn current_local_active(
    composition: &TeletextMainComposition,
    before: usize,
    column: u8,
) -> Option<TeletextSpatialInvocation> {
    let TeletextMainCompositionSource::OriginalPage { command } =
        *composition.provenance().get(before)?
    else {
        return None;
    };
    let TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset,
    } = *composition.program().commands().get(before)?
    else {
        return None;
    };
    if target.scope() != TeletextObjectScope::Local || object_type != TeletextObjectType::Active {
        return None;
    }
    let root = TeletextSpatialRootSource::Local { command };
    let mut calls = composition
        .spatial_trace()
        .invocations()
        .iter()
        .filter(|call| call.parent().is_none() && call.root_source() == root);
    let call = *calls.next()?;
    (calls.next().is_none()
        && call.source().object.is_none()
        && call.source().command == command
        && call.target() == target
        && call.object_type() == object_type
        && call.offset() == offset
        && call.caller_position().row() == 0
        && call.caller_position().column() == u16::from(column))
    .then_some(call)
}

fn output_error(
    composition: &TeletextMainComposition,
    budget: TeletextMainCompositionBudget,
    command: usize,
) -> TeletextMainCompositionError {
    TeletextMainCompositionError {
        owner: composition.owner(),
        location: Some(TeletextMainCompositionSource::HeaderPosition { command }),
        reason: TeletextMainCompositionFailure::OutputBudget {
            maximum: budget.max_output_commands,
        },
    }
}

#[cfg(test)]
mod tests;

pub(super) mod settlement;

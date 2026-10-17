//! Typed physical header-position candidates, never neutral wire fillers.
//!
//! A known original foreground is only a candidate: its real row-tail effects
//! remain visible to the existing physical replay and whole-model comparisons.
//! This module neither changes the authored target nor grants an edit operand.

use super::compile_context::TeletextCompileContext;
use super::{
    TeletextMainComposition, TeletextMainCompositionBudget, TeletextMainCompositionError,
    TeletextMainCompositionFailure, TeletextMainCompositionSource,
};
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextEnhancementProgram, TeletextSemanticService,
};

#[derive(Clone, Copy)]
struct Setter {
    before: usize,
    command: usize,
    column: u8,
    foreground: TeletextColourReference,
}

/// Called only by the normalized factory, which subsequently compares the real
/// typed candidate against this original target. Legacy composed-only callers
/// retain their existing located unresolved-lowering failure.
pub(super) fn materialize(
    composition: TeletextMainComposition,
    service: &TeletextSemanticService,
    budget: TeletextMainCompositionBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<TeletextMainComposition, TeletextMainCompositionError> {
    let Some(setters) = plan(&composition, budget)? else {
        // Unknown context or an unsupported position stays an explicit native
        // lowering failure. Do not guess a colour or return a partial recipe.
        return Ok(composition);
    };
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
                operation: TeletextColumnOperation::Foreground {
                    colour: setter.foreground,
                },
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
) -> Result<Option<Vec<Setter>>, TeletextMainCompositionError> {
    let mut count = 0_usize;
    // Count and validate the complete recipe before allocating any candidate.
    if !visit_setters(composition, |_, command, _, _| {
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
    })? {
        return Ok(None);
    }
    let mut setters = Vec::with_capacity(count);
    if !visit_setters(composition, |before, command, column, foreground| {
        setters.push(Setter {
            before,
            command,
            column,
            foreground,
        });
        Ok(())
    })? {
        return Ok(None);
    }
    Ok(Some(setters))
}

fn visit_setters(
    composition: &TeletextMainComposition,
    mut visit: impl FnMut(
        usize,
        usize,
        u8,
        TeletextColourReference,
    ) -> Result<(), TeletextMainCompositionError>,
) -> Result<bool, TeletextMainCompositionError> {
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
                    return Ok(false);
                }
                let Some(TeletextMainCompositionSource::OriginalPage { command }) = source else {
                    return Ok(false);
                };
                let Some(foreground) = foreground(composition, logical.1) else {
                    return Ok(false);
                };
                visit(index, command, logical.1, foreground)?;
                emitted = logical;
            }
            TeletextEnhancementCommand::SetScreenColour { .. }
            | TeletextEnhancementCommand::InvokeObject { .. } => {}
        }
    }
    Ok(true)
}

fn foreground(
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

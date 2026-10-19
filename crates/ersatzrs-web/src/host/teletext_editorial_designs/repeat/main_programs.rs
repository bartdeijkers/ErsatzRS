//! Root-relative Main correspondence without inventing an object identity.

use super::{check_deadline, NativeTeletextEditorialImportError, Result, TeletextEditorialChanges};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementProgram,
    TeletextLocalProgramOperandOverlay, TeletextMainLocalInvocationProgramOverlay,
    TeletextPageAddress, TeletextResolvedPageEnhancementDraft, TeletextServiceBudget,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

fn fail() -> NativeTeletextEditorialImportError {
    NativeTeletextEditorialImportError::Association
}

fn one(
    programs: &[TeletextMainLocalInvocationProgramOverlay],
    owner: TeletextPageAddress,
) -> Result<Option<&TeletextMainLocalInvocationProgramOverlay>> {
    let mut found = programs.iter().filter(|value| value.owner() == owner);
    let result = found.next();
    if found.next().is_some() {
        return Err(fail());
    }
    Ok(result)
}

fn apply_operands(
    commands: &mut [Command],
    owner: TeletextPageAddress,
    operands: &[TeletextLocalProgramOperandOverlay],
    deadline: Instant,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for operand in operands.iter().filter(|value| value.owner() == owner) {
        check_deadline(deadline)?;
        if !seen.insert(operand.command())
            || commands.get(operand.command())
                != Some(&Command::AtColumn {
                    column: operand.column(),
                    operation: operand.expected(),
                })
        {
            return Err(fail());
        }
        commands[operand.command()] = Command::AtColumn {
            column: operand.column(),
            operation: operand.replacement(),
        };
    }
    Ok(())
}

pub(super) fn merge(
    pages: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<(
    Vec<TeletextLocalProgramOperandOverlay>,
    Vec<TeletextMainLocalInvocationProgramOverlay>,
)> {
    let maximum = budget.max_commands.min(budget.max_structure_items);
    for changes in [previous, incoming] {
        if changes.main_programs.len() > maximum || changes.operands.len() > maximum {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
    }
    let owners: BTreeSet<_> = previous
        .main_programs
        .iter()
        .chain(&incoming.main_programs)
        .map(TeletextMainLocalInvocationProgramOverlay::owner)
        .collect();
    let unaffected = |changes: &TeletextEditorialChanges| {
        changes
            .operands
            .iter()
            .copied()
            .filter(|operand| !owners.contains(&operand.owner()))
            .collect::<Vec<_>>()
    };
    let mut operands = super::merge_operands(
        &unaffected(previous),
        &unaffected(incoming),
        budget,
        deadline,
    )?;
    let mut programs = Vec::new();
    for owner in owners {
        check_deadline(deadline)?;
        let original = pages
            .get(&owner)
            .and_then(|page| page.local_program.as_ref())
            .ok_or_else(fail)?;
        if original.commands().len() > 208 {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
        let mut commands = original.commands().to_vec();
        apply_operands(&mut commands, owner, &previous.operands, deadline)?;
        let mut retained: Vec<_> = (0..commands.len()).map(Some).collect();
        for (changes, apply_scalar) in [(previous, false), (incoming, true)] {
            if apply_scalar {
                apply_operands(&mut commands, owner, &changes.operands, deadline)?;
            }
            if let Some(program) = one(&changes.main_programs, owner)? {
                if commands != program.expected().commands() {
                    return Err(fail());
                }
                retained = program
                    .retained_commands()
                    .iter()
                    .map(|ordinal| {
                        ordinal.and_then(|ordinal| retained.get(ordinal).copied().flatten())
                    })
                    .collect();
                commands = program.replacement().commands().to_vec();
            }
        }
        collect(
            owner,
            original,
            &commands,
            retained,
            &mut operands,
            &mut programs,
            deadline,
        )?;
        if operands
            .len()
            .checked_add(programs.len())
            .is_none_or(|count| count > maximum)
        {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
    }
    Ok((operands, programs))
}

pub(super) fn overlay(
    provenance: &super::TeletextSavedEditorialProvenance,
    changes: &TeletextEditorialChanges,
) -> Result<super::TeletextSavedEditorialOverlay> {
    if changes.object_operands.is_empty()
        && changes.object_programs.is_empty()
        && changes.resource_operands.is_empty()
        && changes.resource_programs.is_empty()
        && changes.resource_additions.is_empty()
        && changes.bitmaps.is_empty()
    {
        return super::TeletextSavedEditorialOverlay::new_with_main_local_invocation_programs(
            provenance.clone(),
            changes.rows.clone(),
            changes.operands.clone(),
            changes.main_programs.clone(),
        )
        .map_err(Into::into);
    }
    // Qualify every sibling through its existing closed role before installing
    // the distinct Main role; this recursion removes Main exactly once.
    let mut siblings = changes.clone();
    let programs = std::mem::take(&mut siblings.main_programs);
    super::cumulative_overlay(provenance, &siblings)?
        .with_main_local_invocation_programs(programs)
        .map_err(Into::into)
}

fn collect(
    owner: TeletextPageAddress,
    original: &TeletextEnhancementProgram,
    commands: &[Command],
    retained: Vec<Option<usize>>,
    operands: &mut Vec<TeletextLocalProgramOperandOverlay>,
    programs: &mut Vec<TeletextMainLocalInvocationProgramOverlay>,
    deadline: Instant,
) -> Result<()> {
    let mut expected = original.commands().to_vec();
    for (command, source) in commands.iter().zip(&retained) {
        check_deadline(deadline)?;
        let Some(source) = source else {
            continue;
        };
        let before = *expected.get(*source).ok_or_else(fail)?;
        if before == *command {
            continue;
        }
        let (
            Command::AtColumn {
                column: old_column,
                operation: before,
            },
            Command::AtColumn {
                column,
                operation: after,
            },
        ) = (before, *command)
        else {
            return Err(fail());
        };
        if old_column != column {
            return Err(fail());
        }
        operands.push(
            TeletextLocalProgramOperandOverlay::new(owner, *source, column, before, after)
                .map_err(|_| fail())?,
        );
        expected[*source] = *command;
    }
    // Exact root undo is semantic equality, not a fabricated no-op format22.
    if expected == commands {
        return Ok(());
    }
    programs.push(
        TeletextMainLocalInvocationProgramOverlay::new(
            owner,
            TeletextEnhancementProgram::new(expected).map_err(|_| fail())?,
            TeletextEnhancementProgram::new(commands.to_vec()).map_err(|_| fail())?,
            retained,
        )
        .map_err(|_| fail())?,
    );
    Ok(())
}

//! Root-relative resource program correspondence; no display-owner aliases.
use super::{check_deadline, NativeTeletextEditorialImportError, Result, TeletextEditorialChanges};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectCatalog, TeletextEnhancementProgram,
    TeletextObjectIdentity, TeletextResourceObjectOperandOverlay as Operand,
    TeletextResourceObjectProgramOverlay as Program, TeletextServiceBudget,
};
use std::{collections::BTreeSet, time::Instant};
fn fail() -> NativeTeletextEditorialImportError {
    NativeTeletextEditorialImportError::Association
}

pub(super) fn merge(
    root: &TeletextEnhancementObjectCatalog,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<(Vec<Operand>, Vec<Program>)> {
    let maximum = budget.max_commands.min(budget.max_structure_items);
    let mut identities = BTreeSet::new();
    for changes in [previous, incoming] {
        if changes.resource_operands.len() > maximum || changes.resource_programs.len() > maximum {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
        for identity in changes
            .resource_operands
            .iter()
            .map(|v| v.identity())
            .chain(changes.resource_programs.iter().map(Program::identity))
        {
            check_deadline(deadline)?;
            identities.insert(identity);
        }
    }
    let (mut operands, mut programs) = (Vec::new(), Vec::new());
    for identity in identities {
        check_deadline(deadline)?;
        let original = &root.definition(identity).ok_or_else(fail)?.program;
        let mut current = original.clone();
        apply(
            &mut current,
            identity,
            &previous.resource_operands,
            deadline,
        )?;
        let old = one(&previous.resource_programs, identity)?;
        if let Some(old) = old {
            if old.expected() != &current {
                return Err(fail());
            }
            current = old.replacement().clone();
        }
        let mut retained = old.map_or_else(
            || (0..current.commands().len()).map(Some).collect::<Vec<_>>(),
            |old| old.retained_commands().to_vec(),
        );
        apply(
            &mut current,
            identity,
            &incoming.resource_operands,
            deadline,
        )?;
        if let Some(program) = one(&incoming.resource_programs, identity)? {
            if program.expected() != &current {
                return Err(fail());
            }
            retained = program
                .retained_commands()
                .iter()
                .map(|source| source.and_then(|n| retained.get(n).copied().flatten()))
                .collect();
            current = program.replacement().clone();
        }
        let (new_operands, new_program) = collect(identity, original, &current, &retained, budget)?;
        operands.extend(new_operands);
        programs.extend(new_program);
        if operands
            .len()
            .checked_add(programs.len())
            .is_none_or(|n| n > maximum)
        {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
    }
    Ok((operands, programs))
}
pub(super) fn one(
    programs: &[Program],
    identity: TeletextObjectIdentity,
) -> Result<Option<&Program>> {
    let mut values = programs.iter().filter(|p| p.identity() == identity);
    let selected = values.next();
    if values.next().is_some() {
        return Err(fail());
    }
    Ok(selected)
}
pub(super) fn apply(
    program: &mut TeletextEnhancementProgram,
    identity: TeletextObjectIdentity,
    operands: &[Operand],
    deadline: Instant,
) -> Result<()> {
    let mut commands = program.commands().to_vec();
    let mut seen = BTreeSet::new();
    for value in operands.iter().filter(|v| v.identity() == identity) {
        check_deadline(deadline)?;
        if !seen.insert(value.command())
            || commands.get(value.command())
                != Some(&TeletextEnhancementCommand::AtColumn {
                    column: value.column(),
                    operation: value.expected(),
                })
        {
            return Err(fail());
        }
        commands[value.command()] = TeletextEnhancementCommand::AtColumn {
            column: value.column(),
            operation: value.replacement(),
        };
    }
    *program = TeletextEnhancementProgram::new(commands).map_err(|_| fail())?;
    Ok(())
}
fn collect(
    identity: TeletextObjectIdentity,
    original: &TeletextEnhancementProgram,
    replacement: &TeletextEnhancementProgram,
    retained: &[Option<usize>],
    budget: TeletextServiceBudget,
) -> Result<(Vec<Operand>, Option<Program>)> {
    if original == replacement {
        return Ok((Vec::new(), None));
    }
    let structural = original.commands().len() != replacement.commands().len()
        || retained.iter().enumerate().any(|(i, v)| *v != Some(i))
        || original
            .commands()
            .iter()
            .zip(replacement.commands())
            .any(|(a, b)| match (a, b) {
                (
                    TeletextEnhancementCommand::AtColumn { column: a, .. },
                    TeletextEnhancementCommand::AtColumn { column: b, .. },
                ) => a != b,
                (
                    TeletextEnhancementCommand::InvokeObject { .. },
                    TeletextEnhancementCommand::InvokeObject { .. },
                ) => a != b,
                _ => false,
            });
    let mut operands = Vec::new();
    if !structural {
        for (ordinal, (a, b)) in original
            .commands()
            .iter()
            .zip(replacement.commands())
            .enumerate()
        {
            push(identity, ordinal, a, b, &mut operands)?;
        }
        return Ok((operands, None));
    }
    let mut expected = original.commands().to_vec();
    for (command, source) in replacement.commands().iter().zip(retained) {
        let Some(source) = *source else { continue };
        let before = original.commands().get(source).ok_or_else(fail)?;
        if !matches!(before,TeletextEnhancementCommand::AtColumn{operation,..} if operation.glyph_family().is_some())
            && !matches!(before, TeletextEnhancementCommand::InvokeObject { .. })
        {
            push(identity, source, before, command, &mut operands)?;
            expected[source] = *command;
        }
    }
    let expected = TeletextEnhancementProgram::new(expected).map_err(|_| fail())?;
    let constructor = if call_topology(original, replacement, retained) {
        Program::new_with_invocation_structure
    } else if replacement
        .commands()
        .iter()
        .zip(retained)
        .any(|(after, source)| {
            source
                .and_then(|ordinal| original.commands().get(ordinal))
                .is_some_and(|before| {
                    matches!(
                        (before, after),
                        (
                            TeletextEnhancementCommand::InvokeObject { .. },
                            TeletextEnhancementCommand::InvokeObject { .. }
                        )
                    ) && before != after
                })
        })
    {
        Program::new_with_invocation_changes
    } else {
        Program::new
    };
    let program = constructor(
        identity,
        expected,
        replacement.clone(),
        retained.to_vec(),
        budget,
    )
    .map_err(|_| fail())?;
    Ok((operands, Some(program)))
}
fn call_topology(
    original: &TeletextEnhancementProgram,
    replacement: &TeletextEnhancementProgram,
    retained: &[Option<usize>],
) -> bool {
    let call = |command: &TeletextEnhancementCommand| {
        matches!(command, TeletextEnhancementCommand::InvokeObject { .. })
    };
    let mut next = 0;
    for (command, ordinal) in replacement.commands().iter().zip(retained) {
        let Some(ordinal) = *ordinal else {
            if call(command) {
                return true;
            }
            continue;
        };
        if original
            .commands()
            .get(next..ordinal)
            .is_some_and(|slice| slice.iter().any(call))
        {
            return true;
        }
        next = ordinal.saturating_add(1);
    }
    original
        .commands()
        .get(next..)
        .is_some_and(|slice| slice.iter().any(call))
}
fn push(
    identity: TeletextObjectIdentity,
    ordinal: usize,
    before: &TeletextEnhancementCommand,
    after: &TeletextEnhancementCommand,
    output: &mut Vec<Operand>,
) -> Result<()> {
    if before == after {
        return Ok(());
    }
    let (
        TeletextEnhancementCommand::AtColumn {
            column: a,
            operation: expected,
        },
        TeletextEnhancementCommand::AtColumn {
            column: b,
            operation: replacement,
        },
    ) = (before, after)
    else {
        return Err(fail());
    };
    if a != b {
        return Err(fail());
    }
    output.push(Operand::new(identity, ordinal, *a, *expected, *replacement).map_err(|_| fail())?);
    Ok(())
}

#[cfg(test)]
#[path = "resource_programs_tests.rs"]
mod tests;

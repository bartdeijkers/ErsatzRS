//! Root-relative composition of checked Local program correspondence.
use super::{check_deadline, NativeTeletextEditorialImportError, Result, TeletextEditorialChanges};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectCatalog, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay, TeletextLocalObjectOperandOverlay,
    TeletextObjectIdentity, TeletextServiceBudget,
};
use std::{collections::BTreeSet, time::Instant};
fn fail() -> NativeTeletextEditorialImportError {
    NativeTeletextEditorialImportError::Association
}
fn glyph(command: &TeletextEnhancementCommand) -> bool {
    matches!(command, TeletextEnhancementCommand::AtColumn { operation, .. } if operation.glyph_family().is_some())
}
fn legacy_glyph(command: &TeletextEnhancementCommand) -> bool {
    matches!(command, TeletextEnhancementCommand::AtColumn { operation, .. }
        if matches!(operation.glyph_family(), Some(ersatzrs_domain::TeletextColumnGlyphFamily::G0 | ersatzrs_domain::TeletextColumnGlyphFamily::G1)))
}

#[derive(Clone, Copy)]
enum ProgramPolicy {
    Legacy,
    AllGlyphs,
    StyleCommands,
    InvocationChanges,
    InvocationStructure,
}

pub(super) fn merge(
    root: &TeletextEnhancementObjectCatalog,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<(
    Vec<TeletextLocalObjectOperandOverlay>,
    Vec<TeletextLocalObjectGlyphProgramOverlay>,
)> {
    let maximum = budget.max_commands.min(budget.max_structure_items);
    let mut identities = BTreeSet::new();
    for changes in [previous, incoming] {
        if changes.object_operands.len() > maximum || changes.object_programs.len() > maximum {
            return Err(NativeTeletextEditorialImportError::Budget);
        }
        for identity in changes
            .object_operands
            .iter()
            .map(|operand| operand.identity())
            .chain(
                changes
                    .object_programs
                    .iter()
                    .map(TeletextLocalObjectGlyphProgramOverlay::identity),
            )
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
        apply_operands(&mut current, identity, &previous.object_operands, deadline)?;
        let old = one_program(&previous.object_programs, identity)?;
        if let Some(program) = old {
            if program.expected() != &current {
                return Err(fail());
            }
            current = program.replacement().clone();
        }
        let mut retained: Vec<_> = if let Some(old) = old {
            old.retained_commands().to_vec()
        } else {
            (0..current.commands().len()).map(Some).collect()
        };
        apply_operands(&mut current, identity, &incoming.object_operands, deadline)?;
        if let Some(program) = one_program(&incoming.object_programs, identity)? {
            if program.expected() != &current {
                return Err(fail());
            }
            retained = program
                .retained_commands()
                .iter()
                .map(|ordinal| ordinal.and_then(|ordinal| retained.get(ordinal).copied().flatten()))
                .collect();
            current = program.replacement().clone();
        }
        collect(
            identity,
            original,
            &current,
            &retained,
            &mut operands,
            &mut programs,
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
fn one_program(
    programs: &[TeletextLocalObjectGlyphProgramOverlay],
    identity: TeletextObjectIdentity,
) -> Result<Option<&TeletextLocalObjectGlyphProgramOverlay>> {
    let mut selected = programs
        .iter()
        .filter(|program| program.identity() == identity);
    let value = selected.next();
    if selected.next().is_some() {
        return Err(fail());
    }
    Ok(value)
}
fn apply_operands(
    program: &mut TeletextEnhancementProgram,
    identity: TeletextObjectIdentity,
    operands: &[TeletextLocalObjectOperandOverlay],
    deadline: Instant,
) -> Result<()> {
    let mut commands = program.commands().to_vec();
    let mut seen = BTreeSet::new();
    for value in operands.iter().filter(|value| value.identity() == identity) {
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
    operands: &mut Vec<TeletextLocalObjectOperandOverlay>,
    programs: &mut Vec<TeletextLocalObjectGlyphProgramOverlay>,
) -> Result<()> {
    if replacement == original {
        return Ok(());
    }
    let structural = replacement.commands().len() != original.commands().len()
        || retained
            .iter()
            .enumerate()
            .any(|(ordinal, source)| *source != Some(ordinal))
        || replacement
            .commands()
            .iter()
            .zip(original.commands())
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
    if !structural {
        for (ordinal, (a, b)) in original
            .commands()
            .iter()
            .zip(replacement.commands())
            .enumerate()
        {
            push_operand(identity, ordinal, a, b, operands)?;
        }
        return Ok(());
    }
    let (program, scalar_operands) = make_program(
        identity,
        original,
        replacement,
        retained,
        ProgramPolicy::Legacy,
    )
    .or_else(|_| {
        make_program(
            identity,
            original,
            replacement,
            retained,
            ProgramPolicy::AllGlyphs,
        )
    })
    .or_else(|_| {
        make_program(
            identity,
            original,
            replacement,
            retained,
            ProgramPolicy::StyleCommands,
        )
    })
    .or_else(|_| {
        make_program(
            identity,
            original,
            replacement,
            retained,
            ProgramPolicy::InvocationChanges,
        )
    })
    .or_else(|_| {
        make_program(
            identity,
            original,
            replacement,
            retained,
            ProgramPolicy::InvocationStructure,
        )
    })?;
    operands.extend(scalar_operands);
    programs.push(program);
    Ok(())
}
fn make_program(
    identity: TeletextObjectIdentity,
    original: &TeletextEnhancementProgram,
    replacement: &TeletextEnhancementProgram,
    retained: &[Option<usize>],
    policy: ProgramPolicy,
) -> Result<(
    TeletextLocalObjectGlyphProgramOverlay,
    Vec<TeletextLocalObjectOperandOverlay>,
)> {
    let mut expected = original.commands().to_vec();
    let mut operands = Vec::new();
    for (command, source) in replacement.commands().iter().zip(retained) {
        let Some(source) = *source else { continue };
        let before = original.commands().get(source).ok_or_else(fail)?;
        if matches!(
            policy,
            ProgramPolicy::InvocationChanges | ProgramPolicy::InvocationStructure
        ) && matches!(before, TeletextEnhancementCommand::InvokeObject { .. })
        {
            continue;
        }
        if !glyph(before) || (matches!(policy, ProgramPolicy::Legacy) && !legacy_glyph(before)) {
            push_operand(identity, source, before, command, &mut operands)?;
            expected[source] = *command;
        }
    }
    let expected = TeletextEnhancementProgram::new(expected).map_err(|_| fail())?;
    let constructor = match policy {
        ProgramPolicy::Legacy => TeletextLocalObjectGlyphProgramOverlay::new,
        ProgramPolicy::AllGlyphs => TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs,
        ProgramPolicy::StyleCommands => {
            TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands
        }
        ProgramPolicy::InvocationChanges => {
            TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes
        }
        ProgramPolicy::InvocationStructure => {
            TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_structure
        }
    };
    let program = constructor(identity, expected, replacement.clone(), retained.to_vec())
        .map_err(|_| fail())?;
    Ok((program, operands))
}

fn push_operand(
    identity: TeletextObjectIdentity,
    ordinal: usize,
    before: &TeletextEnhancementCommand,
    after: &TeletextEnhancementCommand,
    output: &mut Vec<TeletextLocalObjectOperandOverlay>,
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
    output.push(
        TeletextLocalObjectOperandOverlay::new(identity, ordinal, *a, *expected, *replacement)
            .map_err(|_| fail())?,
    );
    Ok(())
}

#[cfg(test)]
#[path = "object_programs_tests.rs"]
mod tests;

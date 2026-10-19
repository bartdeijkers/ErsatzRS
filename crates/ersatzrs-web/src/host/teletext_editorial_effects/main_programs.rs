//! Main correspondence is separate from catalogued object identity and authority.
use super::{
    check_deadline, error, Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementProgram, TeletextLocalProgramOperandOverlay,
    TeletextMainLocalInvocationProgramOverlay, TeletextPageAddress,
    TeletextResolvedPageEnhancementDraft,
};
use std::{collections::BTreeMap, time::Instant};

fn fail(owner: TeletextPageAddress) -> super::TeletextEditorialEffectsError {
    error(
        Some(owner),
        None,
        TeletextEditorialEffectsFailure::Association,
    )
}

pub(super) fn check<'a>(
    before: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    after: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    incoming: TeletextEditorialEdits<'a>,
    cumulative: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<BTreeMap<TeletextPageAddress, &'a TeletextMainLocalInvocationProgramOverlay>> {
    let mut result = BTreeMap::new();
    for program in incoming.main_programs {
        check_deadline(deadline)?;
        if result.insert(program.owner(), program).is_some()
            || before
                .get(&program.owner())
                .and_then(|page| page.local_program.as_ref())
                .is_none()
        {
            return Err(fail(program.owner()));
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for program in cumulative.main_programs {
        check_deadline(deadline)?;
        if !seen.insert(program.owner())
            || after
                .get(&program.owner())
                .and_then(|page| page.local_program.as_ref())
                != Some(program.replacement())
        {
            return Err(fail(program.owner()));
        }
    }
    Ok(result)
}

pub(super) fn final_command(
    after: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    operand: &TeletextLocalProgramOperandOverlay,
    programs: &[TeletextMainLocalInvocationProgramOverlay],
) -> Option<TeletextEnhancementCommand> {
    let ordinal = match programs
        .iter()
        .find(|program| program.owner() == operand.owner())
    {
        Some(program) => program.replacement_command(operand.command())?,
        None => operand.command(),
    };
    after
        .get(&operand.owner())?
        .local_program
        .as_ref()?
        .commands()
        .get(ordinal)
        .copied()
}

pub(super) fn check_transition(
    owner: TeletextPageAddress,
    before: &TeletextEnhancementProgram,
    after: &TeletextEnhancementProgram,
    operands: &BTreeMap<(TeletextPageAddress, usize), &TeletextLocalProgramOperandOverlay>,
    program: &TeletextMainLocalInvocationProgramOverlay,
    deadline: Instant,
) -> Result<()> {
    if after != program.replacement()
        || before.commands().len() != program.expected().commands().len()
    {
        return Err(fail(owner));
    }
    for (ordinal, (old, expected)) in before
        .commands()
        .iter()
        .zip(program.expected().commands())
        .enumerate()
    {
        check_deadline(deadline)?;
        let transformed = operands.get(&(owner, ordinal)).map_or(*old, |operand| {
            TeletextEnhancementCommand::AtColumn {
                column: operand.column(),
                operation: operand.replacement(),
            }
        });
        if transformed != *expected {
            return Err(fail(owner));
        }
    }
    Ok(())
}

//! Main target intent never grants scalar, insertion, deletion or type authority.
use super::{
    check_deadline, error, Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextMainLocalInvocationProgramOverlay, TeletextMainResourceInvocationOverlay,
    TeletextPageAddress, TeletextResolvedPageEnhancementDraft,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

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
) -> Result<BTreeMap<(TeletextPageAddress, usize), &'a TeletextMainResourceInvocationOverlay>> {
    let mut result = BTreeMap::new();
    for change in incoming.main_resource_invocations {
        check_deadline(deadline)?;
        if result
            .insert((change.owner(), change.command()), change)
            .is_some()
            || command(before, change.owner(), change.command()) != Some(change.expected())
        {
            return Err(fail(change.owner()));
        }
    }
    let mut seen = BTreeSet::new();
    for change in cumulative.main_resource_invocations {
        check_deadline(deadline)?;
        let ordinal =
            final_ordinal(change, cumulative.main_programs).ok_or_else(|| fail(change.owner()))?;
        if !seen.insert((change.owner(), change.command()))
            || command(after, change.owner(), ordinal) != Some(change.replacement())
        {
            return Err(fail(change.owner()));
        }
    }
    Ok(result)
}
fn command(
    pages: &BTreeMap<TeletextPageAddress, TeletextResolvedPageEnhancementDraft>,
    owner: TeletextPageAddress,
    ordinal: usize,
) -> Option<TeletextEnhancementCommand> {
    pages
        .get(&owner)?
        .local_program
        .as_ref()?
        .commands()
        .get(ordinal)
        .copied()
}
fn final_ordinal(
    change: &TeletextMainResourceInvocationOverlay,
    programs: &[TeletextMainLocalInvocationProgramOverlay],
) -> Option<usize> {
    match programs
        .iter()
        .find(|program| program.owner() == change.owner())
    {
        Some(program) => program.replacement_command(change.command()),
        None => Some(change.command()),
    }
}

/// Structural Main overlays retain exact original resource-call anchors. Compose
/// separate checked targets only for comparison, without widening that contract.
pub(super) fn check_program_result(
    actual: &TeletextEnhancementProgram,
    program: &TeletextMainLocalInvocationProgramOverlay,
    changes: &[TeletextMainResourceInvocationOverlay],
    deadline: Instant,
) -> Result<()> {
    if actual.commands().len() != program.replacement().commands().len() {
        return Err(fail(program.owner()));
    }
    let mut replacements = BTreeMap::new();
    for change in changes
        .iter()
        .filter(|change| change.owner() == program.owner())
    {
        check_deadline(deadline)?;
        let ordinal = program
            .replacement_command(change.command())
            .ok_or_else(|| fail(program.owner()))?;
        if program.expected().commands().get(change.command()) != Some(&change.expected())
            || program.replacement().commands().get(ordinal) != Some(&change.expected())
            || replacements.insert(ordinal, change.replacement()).is_some()
        {
            return Err(fail(program.owner()));
        }
    }
    for (ordinal, (anchor, actual)) in program
        .replacement()
        .commands()
        .iter()
        .zip(actual.commands())
        .enumerate()
    {
        check_deadline(deadline)?;
        if *actual != replacements.get(&ordinal).copied().unwrap_or(*anchor) {
            return Err(fail(program.owner()));
        }
    }
    Ok(())
}

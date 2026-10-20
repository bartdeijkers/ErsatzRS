//! Root-relative Main targets remain separate from immutable structural anchors.
use super::{
    check_deadline, NativeTeletextEditorialImportError, QualifiedBaseline, Result,
    TeletextEditorialChanges,
};
use ersatzrs_domain::{
    TeletextEnhancementProgram, TeletextMainLocalInvocationProgramOverlay,
    TeletextMainResourceInvocationOverlay,
};
use std::{collections::BTreeSet, time::Instant};

fn invalid() -> NativeTeletextEditorialImportError {
    NativeTeletextEditorialImportError::Association
}

pub(super) fn fold(
    root: &QualifiedBaseline,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    cumulative: &mut TeletextEditorialChanges,
    deadline: Instant,
) -> Result<()> {
    let pages = root
        .original
        .view()
        .encoded()
        .ok_or_else(invalid)?
        .declared()
        .semantic()
        .page_enhancements();
    let mut seen = BTreeSet::new();
    for change in &incoming.main_resource_invocations {
        check_deadline(deadline)?;
        if !seen.insert((change.owner(), change.command())) {
            return Err(invalid());
        }
        let structural = previous
            .main_programs
            .iter()
            .find(|program| program.owner() == change.owner());
        let ordinal = structural
            .map_or(Some(change.command()), |program| {
                program.original_command(change.command())
            })
            .ok_or_else(invalid)?;
        let expected = *pages
            .get(&change.owner())
            .and_then(|page| page.local_program.as_ref())
            .and_then(|program| program.commands().get(ordinal))
            .ok_or_else(invalid)?;
        let prior = previous
            .main_resource_invocations
            .iter()
            .find(|prior| prior.owner() == change.owner() && prior.command() == ordinal);
        if change.expected() != prior.map_or(expected, |prior| prior.replacement()) {
            return Err(invalid());
        }
        cumulative
            .main_resource_invocations
            .retain(|prior| prior.owner() != change.owner() || prior.command() != ordinal);
        if expected != change.replacement() {
            cumulative.main_resource_invocations.push(
                TeletextMainResourceInvocationOverlay::new(
                    change.owner(),
                    ordinal,
                    expected,
                    change.replacement(),
                )
                .map_err(|_| invalid())?,
            );
        }
    }
    check_deadline(deadline)
}

/// TTI compares current calls. Only the temporary root-relative merge copy restores
/// authenticated resource anchors; the untouched incoming descriptor still proves
/// current-to-final effects and the V22 constructor remains closed.
pub(super) fn normalize_main_programs(
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    deadline: Instant,
) -> Result<Option<TeletextEditorialChanges>> {
    if previous.main_resource_invocations.is_empty() || incoming.main_programs.is_empty() {
        return Ok(None);
    }
    let mut normalized = incoming.clone();
    for program in &mut normalized.main_programs {
        check_deadline(deadline)?;
        let prior_structure = previous
            .main_programs
            .iter()
            .find(|value| value.owner() == program.owner());
        let mut expected = program.expected().commands().to_vec();
        let mut replacement = program.replacement().commands().to_vec();
        for call in previous
            .main_resource_invocations
            .iter()
            .filter(|call| call.owner() == program.owner())
        {
            check_deadline(deadline)?;
            let current = prior_structure
                .map_or(Some(call.command()), |prior| {
                    prior.replacement_command(call.command())
                })
                .ok_or_else(invalid)?;
            let final_ordinal = program.replacement_command(current).ok_or_else(invalid)?;
            if expected.get(current) != Some(&call.replacement())
                || replacement.get(final_ordinal) != Some(&call.replacement())
            {
                return Err(invalid());
            }
            expected[current] = call.expected();
            replacement[final_ordinal] = call.expected();
        }
        *program = TeletextMainLocalInvocationProgramOverlay::new(
            program.owner(),
            TeletextEnhancementProgram::new(expected).map_err(|_| invalid())?,
            TeletextEnhancementProgram::new(replacement).map_err(|_| invalid())?,
            program.retained_commands().to_vec(),
        )
        .map_err(|_| invalid())?;
    }
    check_deadline(deadline)?;
    Ok(Some(normalized))
}

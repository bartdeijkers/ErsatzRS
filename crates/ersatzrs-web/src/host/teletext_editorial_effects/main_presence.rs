//! Exact optional Main bodies; no missing/empty or original-writer aliases.
use super::{
    check_deadline, error, Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_domain::{
    TeletextMainProgramPresenceOverlay, TeletextPageAddress, TeletextResolvedPageEnhancementDraft,
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
) -> Result<BTreeMap<TeletextPageAddress, &'a TeletextMainProgramPresenceOverlay>> {
    let mut result = BTreeMap::new();
    for change in incoming.main_presence {
        check_deadline(deadline)?;
        let owner = change.owner();
        let old = before.get(&owner).ok_or_else(|| fail(owner))?;
        let new = after.get(&owner).ok_or_else(|| fail(owner))?;
        if result.insert(owner, change).is_some()
            || old.local_program.as_ref() != change.expected()
            || new.local_program.as_ref() != change.replacement()
        {
            return Err(fail(owner));
        }
    }
    let mut seen = BTreeSet::new();
    for change in cumulative.main_presence {
        check_deadline(deadline)?;
        if !seen.insert(change.owner())
            || after
                .get(&change.owner())
                .ok_or_else(|| fail(change.owner()))?
                .local_program
                .as_ref()
                != change.replacement()
        {
            return Err(fail(change.owner()));
        }
    }
    // A whole-body removal cannot silently consume a separately proposed Main
    // edit. Require every such incoming intent to exist in the actual result.
    for operand in incoming
        .operands
        .iter()
        .filter(|value| result.contains_key(&value.owner()))
    {
        check_deadline(deadline)?;
        if super::main_programs::final_command(after, operand, incoming.main_programs)
            != Some(ersatzrs_domain::TeletextEnhancementCommand::AtColumn {
                column: operand.column(),
                operation: operand.replacement(),
            })
        {
            return Err(fail(operand.owner()));
        }
    }
    for change in incoming
        .main_resource_invocations
        .iter()
        .filter(|value| result.contains_key(&value.owner()))
    {
        check_deadline(deadline)?;
        let ordinal = match incoming
            .main_programs
            .iter()
            .find(|program| program.owner() == change.owner())
        {
            Some(program) => program
                .replacement_command(change.command())
                .ok_or_else(|| fail(change.owner()))?,
            None => change.command(),
        };
        if after
            .get(&change.owner())
            .and_then(|page| page.local_program.as_ref())
            .and_then(|program| program.commands().get(ordinal))
            .copied()
            != Some(change.replacement())
        {
            return Err(fail(change.owner()));
        }
    }
    Ok(result)
}

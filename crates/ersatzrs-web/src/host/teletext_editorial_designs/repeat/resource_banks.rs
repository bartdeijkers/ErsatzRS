//! Retained explicit banks fold later qualified edits into complete definitions.
use super::{
    check_deadline, resource_additions, NativeTeletextEditorialImportError, Result,
    TeletextEditorialChanges,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition;
use ersatzrs_domain::{
    TeletextEnhancementObjectCatalog, TeletextObjectResourceAddition,
    TeletextResourceObjectDefinitionAddition as Addition,
    TeletextResourceObjectOperandOverlay as Operand,
    TeletextResourceObjectProgramOverlay as Program, TeletextServiceBudget,
};
use std::time::Instant;

pub(super) fn check_budget(
    changes: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
) -> Result<()> {
    let failure = || NativeTeletextEditorialImportError::Budget;
    if changes.object_resource_additions.len() > budget.max_records {
        return Err(failure());
    }
    let mut objects = budget
        .max_objects
        .checked_sub(changes.resource_additions.len())
        .ok_or_else(failure)?;
    let mut commands = budget.max_commands;
    for definition in changes
        .resource_additions
        .iter()
        .map(Addition::definition)
        .chain(
            changes
                .object_resource_additions
                .iter()
                .flat_map(|bank| bank.addition().definitions()),
        )
    {
        commands = commands
            .checked_sub(definition.program.commands().len())
            .ok_or_else(failure)?;
    }
    for bank in &changes.object_resource_additions {
        objects = objects
            .checked_sub(bank.addition().definitions().len())
            .ok_or_else(failure)?;
    }
    let _ = (objects, commands);
    Ok(())
}

pub(super) type MergedResources = (
    Vec<Operand>,
    Vec<Program>,
    Vec<Addition>,
    Vec<TeletextSavedObjectResourceAddition>,
);

pub(super) fn merge(
    root: &TeletextEnhancementObjectCatalog,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<MergedResources> {
    check_deadline(deadline)?;
    super::check_operand_budget(previous, budget)?;
    super::check_operand_budget(incoming, budget)?;
    if !incoming.object_resource_additions.is_empty() {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let is_bank = |identity: ersatzrs_domain::TeletextObjectIdentity| {
        previous
            .object_resource_additions
            .iter()
            .any(|bank| Some(bank.addition().resource()) == identity.resource())
    };
    for identity in incoming
        .resource_operands
        .iter()
        .map(|operand| operand.identity())
        .chain(incoming.resource_programs.iter().map(Program::identity))
    {
        check_deadline(deadline)?;
        if is_bank(identity)
            && !previous
                .object_resource_additions
                .iter()
                .flat_map(|bank| bank.addition().definitions())
                .any(|definition| definition.identity == identity)
        {
            return Err(NativeTeletextEditorialImportError::Association);
        }
    }
    let ordinary_previous = TeletextEditorialChanges {
        resource_operands: previous.resource_operands.clone(),
        resource_programs: previous.resource_programs.clone(),
        resource_additions: previous.resource_additions.clone(),
        ..Default::default()
    };
    let ordinary_incoming = TeletextEditorialChanges {
        resource_operands: incoming
            .resource_operands
            .iter()
            .filter(|change| !is_bank(change.identity()))
            .copied()
            .collect(),
        resource_programs: incoming
            .resource_programs
            .iter()
            .filter(|change| !is_bank(change.identity()))
            .cloned()
            .collect(),
        ..Default::default()
    };
    if !incoming.resource_additions.is_empty() {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let (operands, programs, additions) = resource_additions::merge(
        root,
        &ordinary_previous,
        &ordinary_incoming,
        budget,
        deadline,
    )?;
    let mut banks = Vec::with_capacity(previous.object_resource_additions.len());
    for bank in &previous.object_resource_additions {
        check_deadline(deadline)?;
        // Use only the existing exact definition fold: a bank is one record,
        // not one ordinary Add record per contained definition. Raw banks and
        // all nested definitions/commands were charged before cloning above.
        let definitions = bank
            .addition()
            .definitions()
            .iter()
            .map(|definition| {
                Addition::new(definition.clone(), budget)
                    .map_err(|_| NativeTeletextEditorialImportError::Association)
            })
            .collect::<Result<Vec<_>>>()?;
        let body = resource_additions::fold(&definitions, incoming, budget, deadline)?
            .into_iter()
            .map(|addition| addition.definition().clone())
            .collect();
        let addition =
            TeletextObjectResourceAddition::new(bank.addition().resource(), body, budget)
                .map_err(|_| NativeTeletextEditorialImportError::Association)?;
        banks.push(TeletextSavedObjectResourceAddition::new(
            bank.document_index(),
            addition,
            bank.transmission(),
        )?);
    }
    check_deadline(deadline)?;
    Ok((operands, programs, additions, banks))
}

#[cfg(test)]
#[path = "resource_banks_tests.rs"]
mod tests;

//! Later explicit definitions stay in the complete bank that owns their identity.
use super::{check_deadline, NativeTeletextEditorialImportError, Result};
use crate::host::teletext_editorial_tti::TeletextEditorialChanges;
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition;
use ersatzrs_domain::{
    TeletextObjectResourceAddition, TeletextResourceObjectDefinitionAddition, TeletextServiceBudget,
};
use std::time::Instant;

fn preflight(
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    super::super::repeat::check_operand_budget(previous, budget)?;
    let mut objects = budget.max_objects;
    let mut commands = budget.max_commands;
    let mut structure = budget.max_structure_items;
    for definition in previous
        .resource_additions
        .iter()
        .chain(&incoming.resource_additions)
        .map(TeletextResourceObjectDefinitionAddition::definition)
        .chain(
            previous
                .object_resource_additions
                .iter()
                .chain(&incoming.object_resource_additions)
                .flat_map(|bank| bank.addition().definitions()),
        )
    {
        check_deadline(deadline)?;
        objects = objects
            .checked_sub(1)
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        commands = commands
            .checked_sub(definition.program.commands().len())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        structure = structure
            .checked_sub(1)
            .and_then(|count| count.checked_sub(definition.program.commands().len()))
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
    }
    let original_additions = incoming
        .resource_additions
        .iter()
        .filter(|addition| {
            !previous
                .object_resource_additions
                .iter()
                .any(|bank| Some(bank.addition().resource()) == addition.identity().resource())
        })
        .count();
    let records = previous
        .object_programs
        .len()
        .checked_add(previous.resource_programs.len())
        .and_then(|count| count.checked_add(previous.main_programs.len()))
        .and_then(|count| count.checked_add(previous.resource_additions.len()))
        .and_then(|count| count.checked_add(previous.object_resource_additions.len()))
        .and_then(|count| count.checked_add(incoming.object_resource_additions.len()))
        .and_then(|count| count.checked_add(original_additions));
    if records.is_none_or(|count| count > budget.max_records) {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    Ok(())
}

pub(super) fn append(
    cumulative: &mut TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<()> {
    // Charge the complete old/new definition set before copying any bank body.
    // Existing full mixed-overlay quotas are checked before and after this fold.
    preflight(cumulative, incoming, budget, deadline)?;
    for bank in &mut cumulative.object_resource_additions {
        check_deadline(deadline)?;
        let resource = bank.addition().resource();
        let additions = || {
            incoming
                .resource_additions
                .iter()
                .filter(|addition| addition.identity().resource() == Some(resource))
        };
        if additions().next().is_none() {
            continue;
        }
        let mut definitions = bank.addition().definitions().to_vec();
        definitions.extend(additions().map(|addition| addition.definition().clone()));
        let addition = TeletextObjectResourceAddition::new(resource, definitions, budget)
            .map_err(|_| NativeTeletextEditorialImportError::Association)?;
        *bank = TeletextSavedObjectResourceAddition::new(
            bank.document_index(),
            addition,
            bank.transmission(),
        )?;
    }
    for addition in &incoming.resource_additions {
        check_deadline(deadline)?;
        if !cumulative
            .object_resource_additions
            .iter()
            .any(|bank| Some(bank.addition().resource()) == addition.identity().resource())
        {
            cumulative.resource_additions.push(addition.clone());
        }
    }
    cumulative
        .object_resource_additions
        .extend_from_slice(&incoming.object_resource_additions);
    super::super::repeat::check_operand_budget(cumulative, budget)?;
    check_deadline(deadline)
}

#[cfg(test)]
#[path = "retained_banks_tests.rs"]
mod tests;

//! Later editor changes to a root-added identity stay inside its complete addition.
use super::{
    check_deadline, check_operand_budget, resource_operands, resource_programs,
    NativeTeletextEditorialImportError, Result, TeletextEditorialChanges,
};
use ersatzrs_domain::{
    TeletextEnhancementObjectCatalog, TeletextResourceObjectDefinitionAddition as Addition,
    TeletextResourceObjectOperandOverlay as Operand,
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
) -> Result<(Vec<Operand>, Vec<Program>, Vec<Addition>)> {
    check_deadline(deadline)?;
    check_operand_budget(previous, budget)?;
    check_operand_budget(incoming, budget)?;
    // Only the explicit host Add operation creates definitions. Ordinary TTI
    // reimport can edit an already retained addition, never infer a new one.
    if !incoming.resource_additions.is_empty() {
        return Err(fail());
    }
    let mut identities = BTreeSet::new();
    for addition in &previous.resource_additions {
        check_deadline(deadline)?;
        if !identities.insert(addition.identity())
            || root.definition(addition.identity()).is_some()
            || previous
                .resource_operands
                .iter()
                .any(|change| change.identity() == addition.identity())
            || previous
                .resource_programs
                .iter()
                .any(|change| change.identity() == addition.identity())
        {
            return Err(fail());
        }
    }
    for changes in [previous, incoming] {
        for identity in changes
            .resource_operands
            .iter()
            .map(|operand| operand.identity())
            .chain(changes.resource_programs.iter().map(Program::identity))
        {
            check_deadline(deadline)?;
            if !identities.contains(&identity) && root.definition(identity).is_none() {
                return Err(fail());
            }
        }
    }
    let ordinary = TeletextEditorialChanges {
        object_resource_additions: Vec::new(),
        base_text_edits: Vec::new(),
        resource_operands: incoming
            .resource_operands
            .iter()
            .filter(|change| !identities.contains(&change.identity()))
            .copied()
            .collect(),
        resource_programs: incoming
            .resource_programs
            .iter()
            .filter(|change| !identities.contains(&change.identity()))
            .cloned()
            .collect(),
        ..TeletextEditorialChanges::default()
    };
    let (operands, programs) =
        if previous.resource_programs.is_empty() && ordinary.resource_programs.is_empty() {
            (
                resource_operands::merge(
                    &previous.resource_operands,
                    &ordinary.resource_operands,
                    budget,
                    deadline,
                )?,
                vec![],
            )
        } else {
            resource_programs::merge(root, previous, &ordinary, budget, deadline)?
        };
    let additions = fold(&previous.resource_additions, incoming, budget, deadline)?;
    let combined = TeletextEditorialChanges {
        object_resource_additions: Vec::new(),
        base_text_edits: Vec::new(),
        resource_operands: operands,
        resource_programs: programs,
        resource_additions: additions,
        ..TeletextEditorialChanges::default()
    };
    check_operand_budget(&combined, budget)?;
    check_deadline(deadline)?;
    Ok((
        combined.resource_operands,
        combined.resource_programs,
        combined.resource_additions,
    ))
}

pub(super) fn fold(
    previous: &[Addition],
    incoming: &TeletextEditorialChanges,
    budget: TeletextServiceBudget,
    deadline: Instant,
) -> Result<Vec<Addition>> {
    let mut additions = Vec::with_capacity(previous.len());
    for addition in previous {
        check_deadline(deadline)?;
        let mut definition = addition.definition().clone();
        resource_programs::apply(
            &mut definition.program,
            addition.identity(),
            &incoming.resource_operands,
            deadline,
        )?;
        if let Some(change) =
            resource_programs::one(&incoming.resource_programs, addition.identity())?
        {
            if change.expected() != &definition.program {
                return Err(fail());
            }
            definition.program = change.replacement().clone();
        }
        additions.push(Addition::new(definition, budget).map_err(|_| fail())?);
    }
    Ok(additions)
}

#[cfg(test)]
#[path = "resource_additions_tests.rs"]
mod tests;

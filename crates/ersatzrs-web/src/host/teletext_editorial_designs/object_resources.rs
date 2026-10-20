//! Explicit bank creation is bound to an immutable authored producer ordinal.
use super::super::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy;
use super::{check_deadline, NativeTeletextEditorialImportError, QualifiedBaseline, Result};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition;
use ersatzrs_domain::TeletextPageContract;
use std::{collections::BTreeSet, time::Instant};

pub(super) fn authorize(
    baseline: &QualifiedBaseline,
    additions: &[TeletextSavedObjectResourceAddition],
    policy: &TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if additions.is_empty() {
        return Ok(());
    }
    let budget = policy.composition.service;
    if additions.len() > budget.max_records
        || additions.len() > policy.composition.declarations.max_resources
    {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let original = baseline.original.view();
    let service = original
        .encoded()
        .ok_or(NativeTeletextEditorialImportError::Association)?;
    let mut records = budget.max_records;
    let mut objects = budget.max_objects;
    let mut commands = budget.max_commands;
    let mut owners = BTreeSet::new();
    for value in additions {
        check_deadline(deadline)?;
        records = records
            .checked_sub(1)
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        let resource = value.addition().resource();
        let Some(source) = original.sources().get(value.document_index()) else {
            return Err(NativeTeletextEditorialImportError::Association);
        };
        if !matches!(
            source.contract(),
            TeletextPageContract::V2 | TeletextPageContract::V3
        ) || value.document_index() >= original.producers().len()
            || !owners.insert((resource.page(), resource.logical_subpage()))
            || value.transmission().resource != resource
            || service.declared().resource_inventory().iter().any(|old| {
                old.page() == resource.page() && old.logical_subpage() == resource.logical_subpage()
            })
        {
            return Err(NativeTeletextEditorialImportError::Association);
        }
        objects = objects
            .checked_sub(value.addition().definitions().len())
            .ok_or(NativeTeletextEditorialImportError::Budget)?;
        for definition in value.addition().definitions() {
            check_deadline(deadline)?;
            commands = commands
                .checked_sub(definition.program.commands().len())
                .ok_or(NativeTeletextEditorialImportError::Budget)?;
        }
    }
    Ok(())
}

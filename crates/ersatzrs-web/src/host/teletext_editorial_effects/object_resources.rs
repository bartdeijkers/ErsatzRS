//! Exact new-bank intent; uploaded inventory and packet words grant no authority.
use super::{
    check_deadline, error, Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition;
use ersatzrs_domain::{TeletextDeclaredService, TeletextEnhancementObjectCatalog};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredPage;
use std::{collections::BTreeSet, time::Instant};

fn association() -> super::TeletextEditorialEffectsError {
    error(None, None, TeletextEditorialEffectsFailure::Association)
}
fn structure() -> super::TeletextEditorialEffectsError {
    error(
        None,
        None,
        TeletextEditorialEffectsFailure::ProtectedStructure,
    )
}

pub(super) fn is_added_page(
    page: &EncodedTeletextAuthoredPage,
    additions: &[TeletextSavedObjectResourceAddition],
) -> bool {
    matches!(page, EncodedTeletextAuthoredPage::Object(_))
        && page.resource_header().is_some_and(|header| {
            additions
                .iter()
                .any(|addition| addition.addition().resource() == header.identity())
        })
}

fn check_final_banks(
    after: &TeletextDeclaredService,
    banks: &[TeletextSavedObjectResourceAddition],
    deadline: Instant,
) -> Result<()> {
    let mut final_banks = BTreeSet::new();
    for bank in banks {
        check_deadline(deadline)?;
        let resource = bank.addition().resource();
        if bank.transmission().resource != resource
            || !final_banks.insert((resource.page(), resource.logical_subpage()))
            || !after.resource_inventory().contains(&resource)
        {
            return Err(association());
        }
        let mut actual = after
            .semantic()
            .objects()
            .definitions()
            .iter()
            .filter(|definition| definition.identity.resource() == Some(resource));
        for expected in bank.addition().definitions() {
            if actual.next() != Some(expected) {
                return Err(association());
            }
        }
        if actual.next().is_some() {
            return Err(association());
        }
    }
    check_deadline(deadline)
}

pub(super) fn check_declarations(
    before: &TeletextDeclaredService,
    after: &TeletextDeclaredService,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<()> {
    check_final_banks(after, cumulative.object_resource_additions, deadline)?;
    if incoming.object_resource_additions.is_empty() {
        return super::definition_additions::check_declarations(
            before,
            after,
            incoming.resource_additions,
            deadline,
        );
    }
    let mut added = BTreeSet::new();
    for bank in incoming.object_resource_additions {
        check_deadline(deadline)?;
        let resource = bank.addition().resource();
        if !added.insert((resource.page(), resource.logical_subpage()))
            || before.resource_inventory().iter().any(|old| {
                old.page() == resource.page() && old.logical_subpage() == resource.logical_subpage()
            })
            || !cumulative.object_resource_additions.contains(bank)
        {
            return Err(association());
        }
    }
    if before.resource_inventory().len().checked_add(added.len())
        != Some(after.resource_inventory().len())
        || !before.resource_inventory().iter().eq(after
            .resource_inventory()
            .iter()
            .filter(|resource| !added.contains(&(resource.page(), resource.logical_subpage()))))
    {
        return Err(structure());
    }
    // Original-bank Add remains confined to original banks, independently of new-bank permission.
    let mut populated = BTreeSet::new();
    for addition in incoming.resource_additions {
        check_deadline(deadline)?;
        let resource = addition.identity().resource().ok_or_else(association)?;
        if !before.resource_inventory().contains(&resource)
            || (!before.semantic().object_companions().contains(&resource)
                && !before
                    .semantic()
                    .objects()
                    .definitions()
                    .iter()
                    .any(|definition| definition.identity.resource() == Some(resource)))
        {
            return Err(structure());
        }
        populated.insert(resource);
    }
    for (old, new) in [
        (before.empty_object_pages(), after.empty_object_pages()),
        (
            before.semantic().object_companions(),
            after.semantic().object_companions(),
        ),
    ] {
        if !old
            .iter()
            .filter(|resource| !populated.contains(resource))
            .eq(new
                .iter()
                .filter(|resource| !added.contains(&(resource.page(), resource.logical_subpage()))))
        {
            return Err(structure());
        }
        for bank in incoming.object_resource_additions {
            if new.contains(&bank.addition().resource()) != bank.addition().definitions().is_empty()
            {
                return Err(structure());
            }
        }
    }
    check_deadline(deadline)
}

pub(super) fn check_catalogue_incoming(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    incoming: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<()> {
    let is_new = |definition: &ersatzrs_domain::TeletextEnhancementObjectDefinition| {
        incoming
            .object_resource_additions
            .iter()
            .any(|bank| definition.identity.resource() == Some(bank.addition().resource()))
    };
    let mut retained = after
        .definitions()
        .iter()
        .filter(|definition| !is_new(definition));
    for old in before.definitions() {
        check_deadline(deadline)?;
        let new = retained.next().ok_or_else(structure)?;
        if old.identity != new.identity || old.object_type != new.object_type {
            return Err(structure());
        }
    }
    let mut ids = BTreeSet::new();
    for addition in incoming.resource_additions {
        check_deadline(deadline)?;
        if !ids.insert(addition.identity())
            || before.definition(addition.identity()).is_some()
            || retained.next() != Some(addition.definition())
        {
            return Err(association());
        }
    }
    if retained.next().is_some() {
        return Err(structure());
    }
    for bank in incoming.object_resource_additions {
        for definition in bank.addition().definitions() {
            check_deadline(deadline)?;
            if before.definition(definition.identity).is_some()
                || after.definition(definition.identity) != Some(definition)
                || incoming
                    .resource_operands
                    .iter()
                    .any(|change| change.identity() == definition.identity)
                || incoming
                    .resource_programs
                    .iter()
                    .any(|change| change.identity() == definition.identity)
            {
                return Err(association());
            }
        }
    }
    Ok(())
}

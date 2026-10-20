//! Exact appended catalogue intent never changes an original declaration.
use super::{
    check_deadline, error, Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_app::teletext_design_documents::TeletextSavedObjectResourceAddition;
use ersatzrs_domain::{
    TeletextDeclaredService, TeletextEnhancementObjectCatalog,
    TeletextResourceObjectDefinitionAddition,
};
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

pub(super) fn check_declarations(
    before: &TeletextDeclaredService,
    after: &TeletextDeclaredService,
    incoming: &[TeletextResourceObjectDefinitionAddition],
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    let mut identities = BTreeSet::new();
    let mut resources = BTreeSet::new();
    for addition in incoming {
        check_deadline(deadline)?;
        let identity = addition.identity();
        let resource = identity.resource().ok_or_else(association)?;
        if !identities.insert(identity)
            || before.semantic().objects().definition(identity).is_some()
        {
            return Err(association());
        }
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
        resources.insert(resource);
    }
    let empty = before
        .empty_object_pages()
        .iter()
        .filter(|identity| !resources.contains(identity));
    let companions = before
        .semantic()
        .object_companions()
        .iter()
        .filter(|identity| !resources.contains(identity));
    if !empty.eq(after.empty_object_pages().iter())
        || !companions.eq(after.semantic().object_companions().iter())
    {
        return Err(structure());
    }
    check_deadline(deadline)
}

pub(super) fn check_catalogues(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if incoming.object_resource_additions.is_empty() {
        check_retained_incoming(
            before,
            after,
            incoming.resource_additions,
            cumulative.object_resource_additions,
            deadline,
        )?;
    } else {
        super::object_resources::check_catalogue_incoming(before, after, incoming, deadline)?;
    }
    let mut final_ids = BTreeSet::new();
    for addition in cumulative.resource_additions {
        check_deadline(deadline)?;
        if !final_ids.insert(addition.identity())
            || after.definition(addition.identity()) != Some(addition.definition())
            || overlaps(cumulative, addition.identity())
        {
            return Err(association());
        }
    }
    for bank in cumulative.object_resource_additions {
        for definition in bank.addition().definitions() {
            check_deadline(deadline)?;
            if !final_ids.insert(definition.identity)
                || after.definition(definition.identity) != Some(definition)
                || overlaps(cumulative, definition.identity)
            {
                return Err(association());
            }
        }
    }
    for addition in incoming.resource_additions {
        check_deadline(deadline)?;
        if !final_ids.contains(&addition.identity()) || overlaps(incoming, addition.identity()) {
            return Err(association());
        }
    }
    Ok(())
}

pub(super) fn check_incoming(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    additions: &[TeletextResourceObjectDefinitionAddition],
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    if before.definitions().len().checked_add(additions.len()) != Some(after.definitions().len()) {
        return Err(structure());
    }
    for (old, new) in before.definitions().iter().zip(after.definitions()) {
        check_deadline(deadline)?;
        if old.identity != new.identity || old.object_type != new.object_type {
            return Err(structure());
        }
    }
    let mut incoming_ids = BTreeSet::new();
    for (addition, definition) in additions
        .iter()
        .zip(&after.definitions()[before.definitions().len()..])
    {
        check_deadline(deadline)?;
        if !incoming_ids.insert(addition.identity())
            || before.definition(addition.identity()).is_some()
            || definition != addition.definition()
        {
            return Err(association());
        }
    }
    Ok(())
}

/// Ordinary additions keep their original global-tail contract. Only a delta
/// owned by an explicitly retained complete bank may precede old ordinary Adds.
pub(super) fn check_retained_incoming(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    additions: &[TeletextResourceObjectDefinitionAddition],
    banks: &[TeletextSavedObjectResourceAddition],
    deadline: Instant,
) -> Result<()> {
    let bank_owned = |addition: &TeletextResourceObjectDefinitionAddition| {
        banks
            .iter()
            .any(|bank| Some(bank.addition().resource()) == addition.identity().resource())
    };
    if !additions.iter().any(bank_owned) {
        return check_incoming(before, after, additions, deadline);
    }
    if before.definitions().len().checked_add(additions.len()) != Some(after.definitions().len()) {
        return Err(structure());
    }
    let mut incoming_ids = BTreeSet::new();
    let mut bank_ids = BTreeSet::new();
    for addition in additions {
        check_deadline(deadline)?;
        if !incoming_ids.insert(addition.identity())
            || before.definition(addition.identity()).is_some()
            || after.definition(addition.identity()) != Some(addition.definition())
        {
            return Err(association());
        }
        if bank_owned(addition) {
            bank_ids.insert(addition.identity());
        }
    }
    let mut retained = after
        .definitions()
        .iter()
        .filter(|definition| !bank_ids.contains(&definition.identity));
    for old in before.definitions() {
        check_deadline(deadline)?;
        let new = retained.next().ok_or_else(structure)?;
        if old.identity != new.identity || old.object_type != new.object_type {
            return Err(structure());
        }
    }
    for addition in additions.iter().filter(|addition| !bank_owned(addition)) {
        check_deadline(deadline)?;
        if retained.next() != Some(addition.definition()) {
            return Err(association());
        }
    }
    if retained.next().is_some() {
        return Err(structure());
    }
    for bank in banks {
        check_bank_tail(before, after, additions, bank, deadline)?;
    }
    check_deadline(deadline)
}

fn check_bank_tail(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    additions: &[TeletextResourceObjectDefinitionAddition],
    bank: &TeletextSavedObjectResourceAddition,
    deadline: Instant,
) -> Result<()> {
    let resource = Some(bank.addition().resource());
    let mut actual = after
        .definitions()
        .iter()
        .filter(|definition| definition.identity.resource() == resource);
    for old in before
        .definitions()
        .iter()
        .filter(|definition| definition.identity.resource() == resource)
    {
        check_deadline(deadline)?;
        let new = actual.next().ok_or_else(structure)?;
        if old.identity != new.identity || old.object_type != new.object_type {
            return Err(structure());
        }
    }
    for addition in additions
        .iter()
        .filter(|addition| addition.identity().resource() == resource)
    {
        check_deadline(deadline)?;
        if actual.next() != Some(addition.definition()) {
            return Err(association());
        }
    }
    if actual.next().is_some() {
        return Err(structure());
    }
    check_deadline(deadline)
}

fn overlaps(
    edits: TeletextEditorialEdits<'_>,
    identity: ersatzrs_domain::TeletextObjectIdentity,
) -> bool {
    edits
        .resource_operands
        .iter()
        .any(|operand| operand.identity() == identity)
        || edits
            .resource_programs
            .iter()
            .any(|program| program.identity() == identity)
}

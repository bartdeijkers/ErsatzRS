//! Exact appended catalogue intent never changes an original declaration.
use super::{
    check_deadline, error, Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
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
    check_incoming(before, after, incoming.resource_additions, deadline)?;
    let mut final_ids = BTreeSet::new();
    for addition in cumulative.resource_additions {
        check_deadline(deadline)?;
        if !final_ids.insert(addition.identity())
            || after.definition(addition.identity()) != Some(addition.definition())
            || overlaps(cumulative, addition)
        {
            return Err(association());
        }
    }
    for addition in incoming.resource_additions {
        check_deadline(deadline)?;
        if !final_ids.contains(&addition.identity()) || overlaps(incoming, addition) {
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

fn overlaps(
    edits: TeletextEditorialEdits<'_>,
    addition: &TeletextResourceObjectDefinitionAddition,
) -> bool {
    edits
        .resource_operands
        .iter()
        .any(|operand| operand.identity() == addition.identity())
        || edits
            .resource_programs
            .iter()
            .any(|program| program.identity() == addition.identity())
}

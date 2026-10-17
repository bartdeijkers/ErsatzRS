//! Full original resource catalogue in an explicitly disposable Local namespace.
use super::{
    allocate_teletext_resource_objects, check_deadline,
    compile_teletext_local_program_with_objects, document, project_encoded_teletext_local_objects,
    Error, Instant, Result, TeletextEncodedDisplayRecord, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
    TeletextEnhancementProgram, TeletextObjectCatalogBudget, TeletextObjectIdentity,
    TeletextObjectType, TeletextPageAddress, TeletextPageNumber, TeletextResolvedObjectPageLinks,
    TeletextSubcode, TeletextTtiBudget, TeletextTtiDocument,
};
use ersatzrs_domain::TeletextObjectId;
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, EncodedTeletextLocalObjectProgram,
    TeletextLocalObjectProgramEncodingFailure,
};

const MAX_WORDS: usize = 208;

pub(in super::super::super) struct ObjectEditorContextFrame {
    pub document: TeletextTtiDocument,
    pub identity: TeletextObjectIdentity,
    pub object_type: TeletextObjectType,
    /// Original full identities; ordinal is the disposable Local object ID.
    pub aliases: Vec<TeletextObjectIdentity>,
    pub encoded: EncodedTeletextLocalObjectProgram,
}

fn owner() -> Result<TeletextPageAddress> {
    Ok(TeletextPageAddress {
        page: TeletextPageNumber::new(100).map_err(|_| Error::Association)?,
        subcode: TeletextSubcode::new(0).map_err(|_| Error::Association)?,
    })
}
fn alias(owner: TeletextPageAddress, ordinal: usize) -> Result<TeletextObjectIdentity> {
    Ok(TeletextObjectIdentity::local(
        owner,
        TeletextObjectId::new(u16::try_from(ordinal).map_err(|_| Error::Budget)?),
    ))
}

fn inventory(
    catalog: &TeletextEnhancementObjectCatalog,
    target: TeletextObjectIdentity,
    deadline: Instant,
) -> Result<(Vec<TeletextObjectIdentity>, usize)> {
    check_deadline(deadline)?;
    if target.resource().is_none() || catalog.definition(target).is_none() {
        return Err(Error::Association);
    }
    let mut count = 0_usize;
    let mut commands = 0_usize;
    for definition in catalog.definitions() {
        check_deadline(deadline)?;
        if definition.identity.resource().is_none() {
            continue;
        }
        count = count.checked_add(1).ok_or(Error::Budget)?;
        commands = commands
            .checked_add(definition.program.commands().len())
            .ok_or(Error::Budget)?;
        // Every definition consumes at least one word. Native compilation below
        // checks the complete 208-word wrapper before allocating packet bodies.
        if count > MAX_WORDS {
            return Err(Error::Budget);
        }
    }
    let mut aliases = Vec::new();
    aliases
        .try_reserve_exact(count)
        .map_err(|_| Error::Budget)?;
    aliases.push(target);
    for definition in catalog.definitions() {
        check_deadline(deadline)?;
        if definition.identity != target && definition.identity.resource().is_some() {
            aliases.push(definition.identity);
        }
    }
    Ok((aliases, commands))
}

pub(in super::super::super) fn build_context(
    service: &EncodedTeletextAuthoredService,
    target: TeletextObjectIdentity,
    max_units: usize,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<ObjectEditorContextFrame> {
    build_catalog_context(
        service.declared().semantic().objects(),
        target,
        max_units,
        tti,
        deadline,
    )
}

pub(in super::super::super) fn build_context_with_root(
    original: &EncodedTeletextAuthoredService,
    target: TeletextObjectIdentity,
    replacement: &TeletextEnhancementProgram,
    max_units: usize,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<ObjectEditorContextFrame> {
    build_catalog_context_with_root(
        original.declared().semantic().objects(),
        target,
        Some(replacement),
        max_units,
        tti,
        deadline,
    )
}

fn build_catalog_context(
    catalog: &TeletextEnhancementObjectCatalog,
    target: TeletextObjectIdentity,
    max_units: usize,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<ObjectEditorContextFrame> {
    build_catalog_context_with_root(catalog, target, None, max_units, tti, deadline)
}

fn build_catalog_context_with_root(
    catalog: &TeletextEnhancementObjectCatalog,
    target: TeletextObjectIdentity,
    replacement: Option<&TeletextEnhancementProgram>,
    max_units: usize,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<ObjectEditorContextFrame> {
    check_deadline(deadline)?;
    if max_units == 0 || tti.max_records < 7 {
        return Err(Error::Budget);
    }
    let (aliases, original_commands) = inventory(catalog, target, deadline)?;
    let root = catalog.definition(target).ok_or(Error::Association)?;
    let object_type = root.object_type;
    let commands = if let Some(replacement) = replacement {
        original_commands
            .checked_sub(root.program.commands().len())
            .and_then(|count| count.checked_add(replacement.commands().len()))
            .ok_or(Error::Budget)?
    } else {
        original_commands
    };
    let budget = TeletextObjectCatalogBudget {
        max_objects: aliases.len(),
        max_commands: commands.checked_add(1).ok_or(Error::Budget)?,
    };
    let owner = owner()?;
    let projected = project_catalog(catalog, &aliases, owner, budget, replacement, deadline)?;
    let (encoded, document) =
        encode_context(&projected, object_type, budget, max_units, tti, deadline)?;
    Ok(ObjectEditorContextFrame {
        document,
        identity: target,
        object_type,
        aliases,
        encoded,
    })
}

fn encode_context(
    projected: &TeletextEnhancementObjectCatalog,
    object_type: TeletextObjectType,
    budget: TeletextObjectCatalogBudget,
    max_units: usize,
    tti: TeletextTtiBudget,
    deadline: Instant,
) -> Result<(EncodedTeletextLocalObjectProgram, TeletextTtiDocument)> {
    let owner = owner()?;
    let allocations = allocate_teletext_resource_objects(projected, &[], budget.max_objects)
        .map_err(|_| Error::Association)?;
    let main = TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::InvokeObject {
        target: alias(owner, 0)?,
        object_type,
        offset: None,
    }])
    .map_err(|_| Error::Association)?;
    check_deadline(deadline)?;
    let encoded = compile_teletext_local_program_with_objects(
        owner,
        &main,
        projected,
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        budget,
    )
    .map_err(|error| match error.reason {
        TeletextLocalObjectProgramEncodingFailure::Capacity { .. }
        | TeletextLocalObjectProgramEncodingFailure::CommandBudget { .. }
        | TeletextLocalObjectProgramEncodingFailure::ObjectBudget { .. } => Error::Budget,
        _ => Error::Association,
    })?;
    if encoded.packet_bodies().len() > max_units
        || encoded
            .packet_bodies()
            .len()
            .checked_add(6)
            .is_none_or(|count| count > tti.max_records)
    {
        return Err(Error::Budget);
    }
    let records = project_encoded_teletext_local_objects(&encoded, max_units)?;
    let mut words = Vec::new();
    words
        .try_reserve_exact(records.len().checked_mul(13).ok_or(Error::Budget)?)
        .map_err(|_| Error::Budget)?;
    for record in records {
        check_deadline(deadline)?;
        let TeletextEncodedDisplayRecord::Triplets {
            row: 26,
            words: packet,
            ..
        } = record
        else {
            return Err(Error::Association);
        };
        words.extend(packet);
    }
    let document = document(&words, tti)?;
    check_deadline(deadline)?;
    Ok((encoded, document))
}

fn project_catalog(
    catalog: &TeletextEnhancementObjectCatalog,
    aliases: &[TeletextObjectIdentity],
    owner: TeletextPageAddress,
    budget: TeletextObjectCatalogBudget,
    replacement: Option<&TeletextEnhancementProgram>,
    deadline: Instant,
) -> Result<TeletextEnhancementObjectCatalog> {
    let mut definitions = Vec::new();
    definitions
        .try_reserve_exact(aliases.len())
        .map_err(|_| Error::Budget)?;
    for (ordinal, original) in aliases.iter().enumerate() {
        check_deadline(deadline)?;
        let definition = catalog.definition(*original).ok_or(Error::Association)?;
        let program = if ordinal == 0 {
            replacement.unwrap_or(&definition.program)
        } else {
            &definition.program
        };
        let mut commands = Vec::new();
        commands
            .try_reserve_exact(program.commands().len())
            .map_err(|_| Error::Budget)?;
        for command in program.commands() {
            check_deadline(deadline)?;
            commands.push(match *command {
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    offset,
                } => {
                    let index = aliases
                        .iter()
                        .position(|identity| *identity == target)
                        .ok_or(Error::Association)?;
                    TeletextEnhancementCommand::InvokeObject {
                        target: alias(owner, index)?,
                        object_type,
                        offset,
                    }
                }
                other => other,
            });
        }
        definitions.push(TeletextEnhancementObjectDefinition {
            identity: alias(owner, ordinal)?,
            object_type: definition.object_type,
            program: TeletextEnhancementProgram::new(commands).map_err(|_| Error::Association)?,
        });
    }
    check_deadline(deadline)?;
    TeletextEnhancementObjectCatalog::new(definitions, budget).map_err(|_| Error::Association)
}

#[cfg(test)]
mod tests;

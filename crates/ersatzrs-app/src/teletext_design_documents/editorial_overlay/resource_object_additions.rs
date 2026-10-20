//! Closed private V18 additions to already declared POP/GPOP resource owners.
use super::{
    invalid, limit, map_only, resource_object_operands::StoredResourceObjectIdentity,
    resource_object_programs, TeletextDesignDocumentBudget, TeletextDesignDocumentError,
    TeletextProfileDocument, TeletextSavedEditorialOverlay, TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{
    TeletextEnhancementObjectDefinition, TeletextEnhancementProgram, TeletextObjectType,
    TeletextResourceObjectDefinitionAddition,
};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

#[derive(Serialize)]
pub(in super::super) struct StoredDefinitionAddition {
    pub(super) identity: StoredResourceObjectIdentity,
    pub(super) object_type: TeletextObjectType,
    pub(super) program: TeletextEnhancementProgram,
}
impl<'de> Deserialize<'de> for StoredDefinitionAddition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            identity: StoredResourceObjectIdentity,
            object_type: TeletextObjectType,
            program: TeletextEnhancementProgram,
        }
        let fields: Fields = map_only(deserializer)?;
        Ok(Self {
            identity: fields.identity,
            object_type: fields.object_type,
            program: fields.program,
        })
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredDefinitionAdditions(
    pub(super) Option<Vec<StoredDefinitionAddition>>,
);
impl OptionalStoredDefinitionAdditions {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredDefinitionAdditions {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<StoredDefinitionAddition>::deserialize(deserializer).map(|values| Self(Some(values)))
    }
}
pub(super) struct BorrowedDefinitionAdditions<'a>(
    pub(super) &'a [TeletextResourceObjectDefinitionAddition],
);
impl Serialize for BorrowedDefinitionAdditions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            identity: StoredResourceObjectIdentity,
            object_type: TeletextObjectType,
            program: &'a TeletextEnhancementProgram,
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for addition in self.0 {
            sequence.serialize_element(&Fields {
                identity: StoredResourceObjectIdentity(addition.identity()),
                object_type: addition.definition().object_type,
                program: &addition.definition().program,
            })?;
        }
        sequence.end()
    }
}
pub(super) fn distinct(
    overlay: &TeletextSavedEditorialOverlay,
) -> Result<(), TeletextDesignDocumentError> {
    if overlay.resource_object_definition_additions.is_empty() {
        return Err(invalid());
    }
    let mut identities = BTreeSet::new();
    for addition in &overlay.resource_object_definition_additions {
        if !identities.insert(addition.identity())
            || overlay
                .resource_object_operands
                .iter()
                .any(|change| change.identity() == addition.identity())
            || overlay
                .resource_object_program_overlays
                .iter()
                .any(|change| change.identity() == addition.identity())
        {
            return Err(invalid());
        }
    }
    if overlay.schema_version == super::RESOURCE_OBJECT_POSITION_OVERLAY_VERSION
        || (matches!(
            overlay.schema_version,
            super::LOCAL_OBJECT_INVOCATION_OVERLAY_VERSION
                | super::LOCAL_OBJECT_INVOCATION_STRUCTURE_OVERLAY_VERSION
                | super::MAIN_LOCAL_INVOCATION_OVERLAY_VERSION
                | super::BASE_TEXT_EDIT_VERSION
                | super::BASE_TEXT_NATIVE_SPELLING_VERSION
        ) && overlay.resource_object_program_overlays.iter().any(
            ersatzrs_domain::TeletextResourceObjectProgramOverlay::is_original_position_intent,
        ))
    {
        resource_object_programs::distinct_original_positions(
            &overlay.resource_object_program_overlays,
            &overlay.resource_object_operands,
        )
    } else {
        resource_object_programs::distinct_targets(
            &overlay.resource_object_program_overlays,
            &overlay.resource_object_operands,
        )
    }
}
pub(super) fn check_version(
    value: &super::StoredOverlay,
) -> Result<(), TeletextDesignDocumentError> {
    if value.schema_version != super::RESOURCE_OBJECT_DEFINITION_ADDITION_VERSION
        || value
            .resource_object_definition_additions
            .0
            .as_ref()
            .is_none_or(Vec::is_empty)
        || value
            .local_program_operands
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .local_object_operands
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .drcs_bitmap_overlays
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .local_object_glyph_program_overlays
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .resource_object_operands
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        || value
            .resource_object_program_overlays
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn restore(
    mut overlay: TeletextSavedEditorialOverlay,
    additions: Vec<StoredDefinitionAddition>,
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    limit(
        additions.len(),
        budget.validation.service.max_objects,
        "editorial definition additions",
    )?;
    let additions = additions
        .into_iter()
        .map(|addition| {
            TeletextResourceObjectDefinitionAddition::new(
                TeletextEnhancementObjectDefinition {
                    identity: addition.identity.0,
                    object_type: addition.object_type,
                    program: addition.program,
                },
                budget.validation.service,
            )
            .map_err(|_| invalid())
        })
        .collect::<Result<Vec<_>, _>>()?;
    overlay.resource_object_definition_additions = additions;
    if overlay.schema_version != super::RESOURCE_OBJECT_POSITION_OVERLAY_VERSION {
        overlay.schema_version = super::RESOURCE_OBJECT_DEFINITION_ADDITION_VERSION;
    }
    distinct(&overlay)?;
    Ok(overlay)
}
pub(super) fn restore_stored(
    stored: super::StoredOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let provenance = TeletextSavedEditorialProvenance {
        export_receipt_id: stored.export_receipt_id,
        export_receipt_digest: stored.export_receipt_digest,
        original_design_id: stored.original_design_id,
        original_revision: stored.original_revision,
        original_digest: stored.original_digest,
    };
    super::validate_provenance(&provenance)?;
    // The same parent streaming preflight bounds these V18 vectors before DTO allocation.
    let pages = stored
        .pages
        .into_iter()
        .map(super::restore_page)
        .collect::<Result<Vec<_>, _>>()?;
    let operands = stored
        .local_program_operands
        .0
        .unwrap_or_default()
        .iter()
        .map(super::restore_operand)
        .collect::<Result<Vec<_>, _>>()?;
    let bitmaps = stored
        .drcs_bitmap_overlays
        .0
        .unwrap_or_default()
        .iter()
        .map(super::restore_bitmap)
        .collect::<Result<Vec<_>, _>>()?;
    let objects =
        super::object_operands::restore(stored.local_object_operands.0.unwrap_or_default())?;
    let mut value = TeletextSavedEditorialOverlay::new_resource_parts(
        provenance,
        pages,
        operands,
        objects,
        bitmaps,
        super::local_object_glyph_programs::restore(
            stored
                .local_object_glyph_program_overlays
                .0
                .unwrap_or_default(),
            super::ProgramPolicy::StyleCommands,
        )?,
        super::resource_object_operands::restore(
            stored.resource_object_operands.0.unwrap_or_default(),
        )?,
    )?;
    value.resource_object_program_overlays = resource_object_programs::restore_programs(
        stored
            .resource_object_program_overlays
            .0
            .unwrap_or_default(),
        budget,
        stored.schema_version,
    )?;
    if stored.schema_version == super::RESOURCE_OBJECT_POSITION_OVERLAY_VERSION {
        value.schema_version = super::RESOURCE_OBJECT_POSITION_OVERLAY_VERSION;
    }
    let value = restore(
        value,
        stored
            .resource_object_definition_additions
            .0
            .unwrap_or_default(),
        budget,
    )?;
    super::validate_association(&value, documents, budget)?;
    Ok(value)
}
pub(super) fn empty_body(
    provenance: TeletextSavedEditorialProvenance,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    TeletextSavedEditorialOverlay::new_resource_parts(
        provenance,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    )
}
pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    if overlay.resource_object_definition_additions.is_empty() {
        return Ok(());
    }
    distinct(overlay)?;
    limit(
        overlay.resource_object_definition_additions.len(),
        budget.validation.service.max_objects,
        "editorial definition additions",
    )?;
    let mut commands = 0usize;
    for addition in &overlay.resource_object_definition_additions {
        commands = commands
            .checked_add(addition.definition().program.commands().len())
            .ok_or_else(invalid)?;
        limit(
            commands,
            budget.validation.service.max_commands,
            "editorial addition commands",
        )?;
    }
    for addition in &overlay.resource_object_definition_additions {
        let resource = addition.identity().resource().ok_or_else(invalid)?;
        let mut owners = documents.iter().filter_map(|document| match document {
            TeletextProfileDocument::Authored(service)
                if service.resource_inventory().contains(&resource) =>
            {
                Some(service)
            }
            _ => None,
        });
        let owner = owners.next().ok_or_else(invalid)?;
        if owners.next().is_some()
            || owner
                .semantic()
                .objects()
                .definition(addition.identity())
                .is_some()
        {
            return Err(invalid());
        }
    }
    for document in documents {
        let TeletextProfileDocument::Authored(service) = document else {
            continue;
        };
        let additions = overlay
            .resource_object_definition_additions
            .iter()
            .filter(|addition| {
                addition
                    .identity()
                    .resource()
                    .is_some_and(|resource| service.resource_inventory().contains(&resource))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !additions.is_empty() {
            service
                .as_ref()
                .clone()
                .with_resource_object_definition_additions(&additions, budget.validation.service)
                .map_err(|_| invalid())?;
        }
    }
    Ok(())
}

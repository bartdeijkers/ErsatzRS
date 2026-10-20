//! Private V25 bank/lifecycle intent; structural storage is not replay authority.
use super::super::{TeletextSavedResourceTransmission, Transmission, MAX_BINDINGS};
use super::resource_object_additions::StoredDefinitionAddition;
use super::resource_object_operands::StoredResourceObjectIdentity;
use super::{
    invalid, limit, map_only, StoredOverlay, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextProfileDocument, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{TeletextEnhancementObjectDefinition, TeletextObjectResourceAddition};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::BTreeSet, fmt};

/// Complete new bank tied to one immutable original producer-document position.
///
/// The original array order is preserved by the private document format. Neither
/// this numeric association nor a lifecycle descriptor grants source, editor,
/// native replay, package, current-selection or publication authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextSavedObjectResourceAddition {
    document_index: usize,
    addition: TeletextObjectResourceAddition,
    transmission: TeletextSavedResourceTransmission,
}
impl fmt::Debug for TeletextSavedObjectResourceAddition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextSavedObjectResourceAddition")
            .field("document_index", &self.document_index)
            .field("resource", &self.addition.resource())
            .field("definitions", &self.addition.definitions().len())
            .finish_non_exhaustive()
    }
}
impl TeletextSavedObjectResourceAddition {
    /// Associate typed bank and exact explicit lifecycle without inventing values.
    /// # Errors
    /// Rejects an out-of-range source index or a descriptor for another bank.
    /// Actual original membership and combined limits are checked on attachment.
    pub fn new(
        document_index: usize,
        addition: TeletextObjectResourceAddition,
        transmission: TeletextSavedResourceTransmission,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if document_index >= MAX_BINDINGS || transmission.resource != addition.resource() {
            return Err(invalid());
        }
        Ok(Self {
            document_index,
            addition,
            transmission,
        })
    }
    #[must_use]
    pub const fn document_index(&self) -> usize {
        self.document_index
    }
    #[must_use]
    pub const fn addition(&self) -> &TeletextObjectResourceAddition {
        &self.addition
    }
    #[must_use]
    pub const fn transmission(&self) -> TeletextSavedResourceTransmission {
        self.transmission
    }
}

impl TeletextSavedEditorialOverlay {
    /// Capture new banks with no fabricated original-resource edit authority.
    /// # Errors
    /// Rejects invalid provenance, an empty batch and repeated raw page/S1 owners.
    pub fn new_with_object_resource_additions(
        provenance: TeletextSavedEditorialProvenance,
        additions: Vec<TeletextSavedObjectResourceAddition>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        Self::new_parts(provenance, vec![], vec![], vec![], vec![])?
            .with_object_resource_additions(additions)
    }
    /// Compose bank sets with retained V1–24 roles, without changing their meaning.
    /// # Errors
    /// Rejects empty/repeated attachment and duplicate bank owners. Original source
    /// association is checked when appending/restoring; publication stays separate.
    pub fn with_object_resource_additions(
        mut self,
        additions: Vec<TeletextSavedObjectResourceAddition>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if additions.is_empty() || !self.object_resource_additions.is_empty() {
            return Err(invalid());
        }
        distinct(&additions)?;
        self.object_resource_additions = additions;
        Ok(self)
    }
    /// Stored complete new-bank instructions, never implicit replay authorization.
    #[must_use]
    pub fn object_resource_additions(&self) -> &[TeletextSavedObjectResourceAddition] {
        &self.object_resource_additions
    }
}

fn distinct(
    additions: &[TeletextSavedObjectResourceAddition],
) -> Result<(), TeletextDesignDocumentError> {
    let mut owners = BTreeSet::new();
    for value in additions {
        let resource = value.addition.resource();
        // Cross-producer duplicate ownership is also ambiguous on one service.
        if !owners.insert((resource.page(), resource.logical_subpage())) {
            return Err(invalid());
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub(in super::super) struct StoredObjectResourceAddition {
    document_index: usize,
    resource: ersatzrs_domain::TeletextResourceIdentity,
    definitions: Vec<StoredDefinitionAddition>,
    transmission: Transmission,
}
impl<'de> Deserialize<'de> for StoredObjectResourceAddition {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            document_index: usize,
            resource: ersatzrs_domain::TeletextResourceIdentity,
            definitions: Vec<StoredDefinitionAddition>,
            transmission: Transmission,
        }
        let value: Fields = map_only(deserializer)?;
        Ok(Self {
            document_index: value.document_index,
            resource: value.resource,
            definitions: value.definitions,
            transmission: value.transmission,
        })
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredObjectResources(
    pub(super) Option<Vec<StoredObjectResourceAddition>>,
);
impl OptionalStoredObjectResources {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredObjectResources {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredObjectResourceAddition>::deserialize(d).map(|v| Self(Some(v)))
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalPriorVersion(pub(super) Option<u16>);
impl OptionalPriorVersion {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalPriorVersion {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        u16::deserialize(d).map(|v| Self(Some(v)))
    }
}
pub(super) struct BorrowedObjectResources<'a>(pub(super) &'a [TeletextSavedObjectResourceAddition]);
impl Serialize for BorrowedObjectResources<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            document_index: usize,
            resource: ersatzrs_domain::TeletextResourceIdentity,
            definitions: BorrowedBankDefinitions<'a>,
            transmission: &'a TeletextSavedResourceTransmission,
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for value in self.0 {
            sequence.serialize_element(&Fields {
                document_index: value.document_index,
                resource: value.addition.resource(),
                definitions: BorrowedBankDefinitions(value.addition.definitions()),
                transmission: &value.transmission,
            })?;
        }
        sequence.end()
    }
}
struct BorrowedBankDefinitions<'a>(&'a [TeletextEnhancementObjectDefinition]);
impl Serialize for BorrowedBankDefinitions<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            identity: StoredResourceObjectIdentity,
            object_type: ersatzrs_domain::TeletextObjectType,
            program: &'a ersatzrs_domain::TeletextEnhancementProgram,
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for definition in self.0 {
            sequence.serialize_element(&Fields {
                identity: StoredResourceObjectIdentity(definition.identity),
                object_type: definition.object_type,
                program: &definition.program,
            })?;
        }
        sequence.end()
    }
}

pub(super) fn check_version(value: &StoredOverlay) -> Result<(), TeletextDesignDocumentError> {
    if matches!(
        value.schema_version,
        super::OBJECT_RESOURCE_ADDITION_VERSION
            | super::OBJECT_SUBPAGE_ANNOUNCEMENT_VERSION
            | super::OBJECT_PAGE_SELECTION_VERSION
            | super::MAIN_RESOURCE_INVOCATION_VERSION
            | super::MAIN_PROGRAM_PRESENCE_VERSION
    ) {
        if !matches!(value.prior_schema_version.0, Some(1..=24))
            || (value.schema_version == super::OBJECT_RESOURCE_ADDITION_VERSION
                && value
                    .object_resource_additions
                    .0
                    .as_ref()
                    .is_none_or(Vec::is_empty))
            || value
                .object_resource_additions
                .0
                .as_ref()
                .is_some_and(Vec::is_empty)
        {
            return Err(invalid());
        }
    } else if !value.object_resource_additions.is_absent()
        || !value.prior_schema_version.is_absent()
    {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn restore(
    mut stored: StoredOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let additions = stored
        .object_resource_additions
        .0
        .take()
        .unwrap_or_default();
    let announcements = super::object_subpage_announcements::restore(
        stored
            .object_subpage_announcements
            .0
            .take()
            .unwrap_or_default(),
    )?;
    let selections = super::object_page_selections::restore(
        stored.object_page_selections.0.take().unwrap_or_default(),
    )?;
    let invocations = super::main_resource_invocations::restore(
        stored
            .main_resource_invocations
            .0
            .take()
            .unwrap_or_default(),
    )?;
    let presence = super::main_program_presence::restore(
        stored
            .main_program_presence_overlays
            .0
            .take()
            .unwrap_or_default(),
    )?;
    stored.schema_version = stored.prior_schema_version.0.take().ok_or_else(invalid)?;
    let additions = additions
        .into_iter()
        .map(|value| {
            let definitions = value
                .definitions
                .into_iter()
                .map(|definition| TeletextEnhancementObjectDefinition {
                    identity: definition.identity.0,
                    object_type: definition.object_type,
                    program: definition.program,
                })
                .collect();
            let addition = TeletextObjectResourceAddition::new(
                value.resource,
                definitions,
                budget.validation.service,
            )
            .map_err(|_| invalid())?;
            TeletextSavedObjectResourceAddition::new(
                value.document_index,
                addition,
                TeletextSavedResourceTransmission {
                    resource: value.transmission.resource,
                    counter: value.transmission.counter,
                    update: value.transmission.update,
                    last_subpage: value.transmission.last_subpage,
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    // Recursion is exactly one envelope: prior versions cannot carry V25 fields.
    let prior = if stored.schema_version == super::OVERLAY_VERSION && stored.pages.is_empty() {
        // Only the new envelope may have no prior edit. V1 itself stays nonempty.
        stored.check_version()?;
        TeletextSavedEditorialOverlay::new_parts(
            TeletextSavedEditorialProvenance {
                export_receipt_id: stored.export_receipt_id,
                export_receipt_digest: stored.export_receipt_digest,
                original_design_id: stored.original_design_id,
                original_revision: stored.original_revision,
                original_digest: stored.original_digest,
            },
            vec![],
            vec![],
            vec![],
            vec![],
        )?
    } else {
        stored.restore_parts(documents, budget)?
    };
    let overlay = if additions.is_empty() {
        prior
    } else {
        prior.with_object_resource_additions(additions)?
    };
    let overlay = if announcements.is_empty() {
        overlay
    } else {
        overlay.with_object_subpage_announcements(announcements)?
    };
    let overlay = if selections.is_empty() {
        overlay
    } else {
        overlay.with_object_page_selections(selections)?
    };
    let overlay = attach_main_intents(overlay, invocations, presence)?;
    validate_association(&overlay, documents, budget)?;
    Ok(overlay)
}

fn attach_main_intents(
    overlay: TeletextSavedEditorialOverlay,
    invocations: Vec<ersatzrs_domain::TeletextMainResourceInvocationOverlay>,
    presence: Vec<ersatzrs_domain::TeletextMainProgramPresenceOverlay>,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let overlay = if invocations.is_empty() {
        overlay
    } else {
        overlay.with_main_resource_invocations(invocations)?
    };
    let overlay = if presence.is_empty() {
        overlay
    } else {
        overlay.with_main_program_presence_overlays(presence)?
    };
    Ok(overlay)
}

pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    let additions = overlay.object_resource_additions();
    if additions.is_empty() {
        return Ok(());
    }
    distinct(additions)?;
    limit(
        additions.len(),
        budget
            .max_resource_transmissions
            .min(budget.validation.service.max_records)
            .min(budget.validation.declarations.max_resources),
        "editorial object resources",
    )?;
    let mut definitions = 0usize;
    let mut commands = 0usize;
    for value in additions {
        let TeletextProfileDocument::Authored(_) =
            documents.get(value.document_index).ok_or_else(invalid)?
        else {
            return Err(invalid());
        };
        let resource = value.addition.resource();
        if documents.iter().any(|document| matches!(document, TeletextProfileDocument::Authored(service)
            if service.resource_inventory().iter().any(|old| old.page() == resource.page() && old.logical_subpage() == resource.logical_subpage()))) {
            return Err(invalid());
        }
        definitions = definitions
            .checked_add(value.addition.definitions().len())
            .ok_or_else(invalid)?;
        limit(
            definitions,
            budget.validation.service.max_objects,
            "editorial object programs",
        )?;
        for definition in value.addition.definitions() {
            commands = commands
                .checked_add(definition.program.commands().len())
                .ok_or_else(invalid)?;
            limit(
                commands,
                budget.validation.service.max_commands,
                "editorial operands",
            )?;
        }
    }
    // This proves only original owner association and bounded shape. The native
    // host must validate the combined edited graph, lifecycle and shared effects.
    Ok(())
}

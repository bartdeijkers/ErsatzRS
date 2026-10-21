//! V28 exact-original Main resource calls, separate from the V22 Local role.
use super::{
    invalid, map_only, StoredOverlay, TeletextDesignDocumentError, TeletextProfileDocument,
    TeletextSavedEditorialOverlay, TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextMainResourceInvocationOverlay, TeletextObjectIdentity,
    TeletextObjectType, TeletextPageAddress,
};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

impl TeletextSavedEditorialOverlay {
    /// Capture resource-call retargets without changing the V22 Main Local role.
    /// # Errors
    /// Rejects invalid provenance, empty intent or repeated original slots.
    pub fn new_with_main_resource_invocations(
        provenance: TeletextSavedEditorialProvenance,
        invocations: Vec<TeletextMainResourceInvocationOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        Self::new_parts(provenance, vec![], vec![], vec![], vec![])?
            .with_main_resource_invocations(invocations)
    }

    /// Compose root-relative Main retargets with retained editorial roles.
    /// Neither attachment nor source association grants native replay authority.
    /// # Errors
    /// Rejects an empty/repeated attachment or duplicate owner/command pairs.
    pub fn with_main_resource_invocations(
        mut self,
        invocations: Vec<TeletextMainResourceInvocationOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if invocations.is_empty() || !self.main_resource_invocations.is_empty() {
            return Err(invalid());
        }
        distinct(&invocations)?;
        self.main_resource_invocations = invocations;
        Ok(self)
    }

    /// Exact original Main ordinals; the host maps final emitted positions.
    #[must_use]
    pub fn main_resource_invocations(&self) -> &[TeletextMainResourceInvocationOverlay] {
        &self.main_resource_invocations
    }
}

fn distinct(
    values: &[TeletextMainResourceInvocationOverlay],
) -> Result<(), TeletextDesignDocumentError> {
    let mut keys = BTreeSet::new();
    for value in values {
        if !keys.insert((value.owner(), value.command())) {
            return Err(invalid());
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub(in super::super) struct StoredMainResourceInvocation {
    owner: super::super::Entry,
    command: usize,
    expected: TeletextEnhancementCommand,
    replacement: TeletextEnhancementCommand,
}
impl<'de> Deserialize<'de> for StoredMainResourceInvocation {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: super::super::Entry,
            command: usize,
            expected: TeletextEnhancementCommand,
            replacement: TeletextEnhancementCommand,
        }
        let value: Fields = map_only(d)?;
        Ok(Self {
            owner: value.owner,
            command: value.command,
            expected: value.expected,
            replacement: value.replacement,
        })
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredInvocations(pub(super) Option<Vec<StoredMainResourceInvocation>>);
impl OptionalStoredInvocations {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredInvocations {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredMainResourceInvocation>::deserialize(d).map(|values| Self(Some(values)))
    }
}
pub(super) struct BorrowedInvocations<'a>(pub(super) &'a [TeletextMainResourceInvocationOverlay]);
impl Serialize for BorrowedInvocations<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for value in self.0 {
            sequence.serialize_element(&StoredMainResourceInvocation {
                owner: super::super::Entry {
                    page: value.owner().page,
                    subcode: value.owner().subcode,
                },
                command: value.command(),
                expected: value.expected(),
                replacement: value.replacement(),
            })?;
        }
        sequence.end()
    }
}
pub(super) fn check_version(value: &StoredOverlay) -> Result<(), TeletextDesignDocumentError> {
    if value.schema_version == super::MAIN_RESOURCE_INVOCATION_VERSION {
        if value
            .main_resource_invocations
            .0
            .as_ref()
            .is_none_or(Vec::is_empty)
        {
            return Err(invalid());
        }
    } else if matches!(
        value.schema_version,
        super::MAIN_PROGRAM_PRESENCE_VERSION
            | super::DRCS_PAGE_SELECTION_VERSION
            | super::RETAINED_ROLE_ATTACHMENT_VERSION
    ) {
        if value
            .main_resource_invocations
            .0
            .as_ref()
            .is_some_and(Vec::is_empty)
        {
            return Err(invalid());
        }
    } else if !value.main_resource_invocations.is_absent() {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn restore(
    values: Vec<StoredMainResourceInvocation>,
) -> Result<Vec<TeletextMainResourceInvocationOverlay>, TeletextDesignDocumentError> {
    values
        .into_iter()
        .map(|value| {
            TeletextMainResourceInvocationOverlay::new(
                TeletextPageAddress {
                    page: value.owner.page,
                    subcode: value.owner.subcode,
                },
                value.command,
                value.expected,
                value.replacement,
            )
            .map_err(|_| invalid())
        })
        .collect()
}

pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
) -> Result<(), TeletextDesignDocumentError> {
    distinct(overlay.main_resource_invocations())?;
    for value in overlay.main_resource_invocations() {
        let document = super::unique_original_owner(value.owner(), documents)?;
        let TeletextProfileDocument::Authored(service) = document else {
            return Err(invalid());
        };
        let original = service
            .semantic()
            .page_enhancements()
            .get(&value.owner())
            .and_then(|page| page.local_program.as_ref())
            .ok_or_else(invalid)?;
        if original.commands().get(value.command()) != Some(&value.expected()) {
            return Err(invalid());
        }
        let index = documents
            .iter()
            .position(|candidate| std::ptr::eq(candidate, document))
            .ok_or_else(invalid)?;
        for command in [value.expected(), value.replacement()] {
            let TeletextEnhancementCommand::InvokeObject {
                target,
                object_type,
                ..
            } = command
            else {
                return Err(invalid());
            };
            target_owner(overlay, documents, index, target, object_type)?;
        }
    }
    Ok(())
}

pub(super) fn target_owner(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    owner: usize,
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
) -> Result<(), TeletextDesignDocumentError> {
    let mut matches = 0usize;
    for (index, document) in documents.iter().enumerate() {
        let TeletextProfileDocument::Authored(service) = document else {
            continue;
        };
        let definitions = service
            .semantic()
            .objects()
            .definitions()
            .iter()
            .chain(
                overlay
                    .resource_object_definition_additions()
                    .iter()
                    .filter(|addition| {
                        addition.identity().resource().is_some_and(|resource| {
                            service.resource_inventory().contains(&resource)
                        })
                    })
                    .map(ersatzrs_domain::TeletextResourceObjectDefinitionAddition::definition),
            )
            .chain(
                overlay
                    .object_resource_additions()
                    .iter()
                    .filter(|bank| bank.document_index() == index)
                    .flat_map(|bank| bank.addition().definitions()),
            );
        for definition in definitions.filter(|definition| definition.identity == target) {
            if index != owner || definition.object_type != object_type {
                return Err(invalid());
            }
            matches += 1;
        }
    }
    if matches != 1 {
        return Err(invalid());
    }
    Ok(())
}

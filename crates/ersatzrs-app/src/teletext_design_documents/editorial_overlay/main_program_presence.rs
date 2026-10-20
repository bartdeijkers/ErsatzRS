//! Private V29 optional Main presence, preserving explicit null and original bytes.
use super::{
    invalid, limit, map_only, StoredOverlay, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextProfileDocument, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementProgram, TeletextMainProgramPresenceOverlay,
    TeletextPageAddress,
};
use serde::{
    de::{MapAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{collections::BTreeSet, fmt};

impl TeletextSavedEditorialOverlay {
    /// Capture a distinct absent/present Main transition, never a legacy body edit.
    /// # Errors
    /// Rejects invalid provenance, empty/repeated intent and duplicate owners.
    pub fn new_with_main_program_presence_overlays(
        provenance: TeletextSavedEditorialProvenance,
        values: Vec<TeletextMainProgramPresenceOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        Self::new_parts(provenance, vec![], vec![], vec![], vec![])?
            .with_main_program_presence_overlays(values)
    }
    /// Compose exact optional original Main presence without downgrading siblings.
    /// # Errors
    /// Rejects empty/repeated attachment and duplicate exact owners.
    pub fn with_main_program_presence_overlays(
        mut self,
        values: Vec<TeletextMainProgramPresenceOverlay>,
    ) -> Result<Self, TeletextDesignDocumentError> {
        if values.is_empty() || !self.main_program_presence_overlays.is_empty() {
            return Err(invalid());
        }
        distinct(&values)?;
        self.main_program_presence_overlays = values;
        Ok(self)
    }
    /// Return exact absent/present Main transitions in this saved overlay.
    #[must_use]
    pub fn main_program_presence_overlays(&self) -> &[TeletextMainProgramPresenceOverlay] {
        &self.main_program_presence_overlays
    }
}

fn distinct(
    values: &[TeletextMainProgramPresenceOverlay],
) -> Result<(), TeletextDesignDocumentError> {
    let mut owners = BTreeSet::new();
    for value in values {
        if !owners.insert(value.owner()) {
            return Err(invalid());
        }
    }
    Ok(())
}

#[derive(Serialize)]
pub(in super::super) struct StoredMainPresence {
    owner: super::super::Entry,
    expected: RequiredProgram,
    replacement: RequiredProgram,
}
/// `deserialize_any` makes a missing field fail; explicit JSON null remains None.
#[derive(Serialize)]
#[serde(transparent)]
struct RequiredProgram(Option<TeletextEnhancementProgram>);
impl<'de> Deserialize<'de> for RequiredProgram {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ProgramVisitor;
        impl<'de> Visitor<'de> for ProgramVisitor {
            type Value = RequiredProgram;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an explicit null or a Main program object")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(RequiredProgram(None))
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                TeletextEnhancementProgram::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map),
                )
                .map(|program| RequiredProgram(Some(program)))
            }
        }
        d.deserialize_any(ProgramVisitor)
    }
}
impl<'de> Deserialize<'de> for StoredMainPresence {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: super::super::Entry,
            expected: RequiredProgram,
            replacement: RequiredProgram,
        }
        let value: Fields = map_only(d)?;
        Ok(Self {
            owner: value.owner,
            expected: value.expected,
            replacement: value.replacement,
        })
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredPresence(pub(super) Option<Vec<StoredMainPresence>>);
impl OptionalStoredPresence {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredPresence {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredMainPresence>::deserialize(d).map(|values| Self(Some(values)))
    }
}
pub(super) struct BorrowedPresence<'a>(pub(super) &'a [TeletextMainProgramPresenceOverlay]);
impl Serialize for BorrowedPresence<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            owner: super::super::Entry,
            expected: Option<&'a TeletextEnhancementProgram>,
            replacement: Option<&'a TeletextEnhancementProgram>,
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for value in self.0 {
            sequence.serialize_element(&Fields {
                owner: super::super::Entry {
                    page: value.owner().page,
                    subcode: value.owner().subcode,
                },
                expected: value.expected(),
                replacement: value.replacement(),
            })?;
        }
        sequence.end()
    }
}
pub(super) fn check_version(value: &StoredOverlay) -> Result<(), TeletextDesignDocumentError> {
    if value.schema_version == super::MAIN_PROGRAM_PRESENCE_VERSION {
        if value
            .main_program_presence_overlays
            .0
            .as_ref()
            .is_none_or(Vec::is_empty)
        {
            return Err(invalid());
        }
    } else if !value.main_program_presence_overlays.is_absent() {
        return Err(invalid());
    }
    Ok(())
}
pub(super) fn restore(
    values: Vec<StoredMainPresence>,
) -> Result<Vec<TeletextMainProgramPresenceOverlay>, TeletextDesignDocumentError> {
    values
        .into_iter()
        .map(|value| {
            TeletextMainProgramPresenceOverlay::new(
                TeletextPageAddress {
                    page: value.owner.page,
                    subcode: value.owner.subcode,
                },
                value.expected.0,
                value.replacement.0,
            )
            .map_err(|_| invalid())
        })
        .collect()
}
pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    let values = overlay.main_program_presence_overlays();
    limit(
        values.len(),
        budget.validation.service.max_records,
        "editorial Main presence",
    )?;
    distinct(values)?;
    for value in values {
        let document = super::unique_original_owner(value.owner(), documents)?;
        let TeletextProfileDocument::Authored(service) = document else {
            return Err(invalid());
        };
        let metadata = service
            .semantic()
            .page_enhancements()
            .get(&value.owner())
            .ok_or_else(invalid)?;
        if metadata.local_program.as_ref() != value.expected() {
            return Err(invalid());
        }
        let index = documents
            .iter()
            .position(|candidate| std::ptr::eq(candidate, document))
            .ok_or_else(invalid)?;
        for program in [value.expected(), value.replacement()]
            .into_iter()
            .flatten()
        {
            for command in program.commands() {
                if let TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    ..
                } = command
                {
                    if target.owner().is_some_and(|owner| owner != value.owner()) {
                        return Err(invalid());
                    }
                    super::main_resource_invocations::target_owner(
                        overlay,
                        documents,
                        index,
                        *target,
                        *object_type,
                    )?;
                }
            }
        }
    }
    Ok(())
}

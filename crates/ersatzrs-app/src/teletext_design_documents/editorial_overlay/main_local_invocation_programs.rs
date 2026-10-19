//! Closed private V22 Main call structure and exact-original attachment.
use super::{
    invalid, limit, map_only, StoredOverlay, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextProfileDocument, TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementProgram,
    TeletextMainLocalInvocationProgramOverlay, TeletextPageAddress,
};
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

#[derive(Serialize)]
pub(in super::super) struct StoredMainProgram {
    owner: super::super::Entry,
    expected: TeletextEnhancementProgram,
    replacement: TeletextEnhancementProgram,
    retained_commands: Vec<Option<usize>>,
}
impl<'de> Deserialize<'de> for StoredMainProgram {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: super::super::Entry,
            expected: TeletextEnhancementProgram,
            replacement: TeletextEnhancementProgram,
            retained_commands: Vec<Option<usize>>,
        }
        let fields: Fields = map_only(deserializer)?;
        Ok(Self {
            owner: fields.owner,
            expected: fields.expected,
            replacement: fields.replacement,
            retained_commands: fields.retained_commands,
        })
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredMainPrograms(pub(super) Option<Vec<StoredMainProgram>>);
impl OptionalStoredMainPrograms {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredMainPrograms {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredMainProgram>::deserialize(d).map(|value| Self(Some(value)))
    }
}
pub(super) struct BorrowedMainPrograms<'a>(
    pub(super) &'a [TeletextMainLocalInvocationProgramOverlay],
);
impl Serialize for BorrowedMainPrograms<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            owner: super::super::Entry,
            expected: &'a TeletextEnhancementProgram,
            replacement: &'a TeletextEnhancementProgram,
            retained_commands: &'a [Option<usize>],
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for program in self.0 {
            sequence.serialize_element(&Fields {
                owner: super::super::Entry {
                    page: program.owner().page,
                    subcode: program.owner().subcode,
                },
                expected: program.expected(),
                replacement: program.replacement(),
                retained_commands: program.retained_commands(),
            })?;
        }
        sequence.end()
    }
}
pub(super) fn install(
    mut value: TeletextSavedEditorialOverlay,
    programs: Vec<TeletextMainLocalInvocationProgramOverlay>,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    if programs.is_empty() {
        return Err(invalid());
    }
    let mut owners = BTreeSet::new();
    for program in value
        .main_local_invocation_program_overlays
        .iter()
        .chain(&programs)
    {
        if !owners.insert(program.owner()) {
            return Err(invalid());
        }
    }
    value
        .main_local_invocation_program_overlays
        .extend(programs);
    value.schema_version = super::MAIN_LOCAL_INVOCATION_OVERLAY_VERSION;
    Ok(value)
}
pub(super) fn check_version(value: &StoredOverlay) -> Result<(), TeletextDesignDocumentError> {
    if value.schema_version != super::MAIN_LOCAL_INVOCATION_OVERLAY_VERSION {
        return if value.main_local_invocation_program_overlays.is_absent() {
            Ok(())
        } else {
            Err(invalid())
        };
    }
    if value
        .main_local_invocation_program_overlays
        .0
        .as_ref()
        .is_none_or(Vec::is_empty)
        || [
            value
                .local_program_operands
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .local_object_operands
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .local_object_glyph_program_overlays
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .drcs_bitmap_overlays
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .resource_object_operands
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .resource_object_program_overlays
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
            value
                .resource_object_definition_additions
                .0
                .as_ref()
                .is_some_and(Vec::is_empty),
        ]
        .into_iter()
        .any(|empty| empty)
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
    let programs = stored
        .main_local_invocation_program_overlays
        .0
        .take()
        .ok_or_else(invalid)?;
    let programs = programs
        .into_iter()
        .map(|program| {
            TeletextMainLocalInvocationProgramOverlay::new(
                TeletextPageAddress {
                    page: program.owner.page,
                    subcode: program.owner.subcode,
                },
                program.expected,
                program.replacement,
                program.retained_commands,
            )
            .map_err(|_| invalid())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let siblings =
        super::local_object_invocation_programs::restore_siblings(stored, documents, budget)?;
    let value = install(siblings, programs)?;
    super::validate_association(&value, documents, budget)?;
    Ok(value)
}
pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    limit(
        overlay.main_local_invocation_program_overlays.len(),
        budget.validation.service.max_records,
        "editorial Main programs",
    )?;
    let mut owners = BTreeSet::new();
    for program in &overlay.main_local_invocation_program_overlays {
        if !owners.insert(program.owner()) {
            return Err(invalid());
        }
        let TeletextProfileDocument::Authored(service) =
            super::unique_original_owner(program.owner(), documents)?
        else {
            return Err(invalid());
        };
        let original = service
            .semantic()
            .page_enhancements()
            .get(&program.owner())
            .and_then(|page| page.local_program.as_ref())
            .ok_or_else(invalid)?;
        if original.commands().len() != program.expected().commands().len() {
            return Err(invalid());
        }
        for (ordinal, (before, expected)) in original
            .commands()
            .iter()
            .zip(program.expected().commands())
            .enumerate()
        {
            let patched = overlay
                .local_program_operands
                .iter()
                .find(|operand| operand.owner() == program.owner() && operand.command() == ordinal)
                .map_or(*before, |operand| Command::AtColumn {
                    column: operand.column(),
                    operation: operand.replacement(),
                });
            if patched != *expected {
                return Err(invalid());
            }
        }
        for command in program
            .expected()
            .commands()
            .iter()
            .chain(program.replacement().commands())
        {
            if let Command::InvokeObject {
                target,
                object_type,
                ..
            } = command
            {
                if target.owner() == Some(program.owner())
                    && service
                        .semantic()
                        .objects()
                        .definition(*target)
                        .is_none_or(|definition| definition.object_type != *object_type)
                {
                    return Err(invalid());
                }
            }
        }
    }
    Ok(())
}

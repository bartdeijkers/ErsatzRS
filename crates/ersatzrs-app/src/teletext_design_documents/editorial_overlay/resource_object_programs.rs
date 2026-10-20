//! Closed versioned resource programs, bound to exact immutable originals.
use super::{
    failure, invalid, limit, map_only, resource_object_operands, TeletextDesignDocumentBudget,
    TeletextDesignDocumentError, TeletextDesignDocumentFailure, TeletextProfileDocument,
    TeletextSavedEditorialOverlay, TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{
    TeletextDrcsBitmapOverlay, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectOperandOverlay, TeletextLocalProgramOperandOverlay,
    TeletextPhysicalBaseOverlay, TeletextResourceObjectOperandOverlay,
    TeletextResourceObjectProgramOverlay,
};
use resource_object_operands::StoredResourceObjectIdentity;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

#[derive(Serialize)]
pub(in super::super) struct StoredResourceProgram {
    identity: StoredResourceObjectIdentity,
    expected: TeletextEnhancementProgram,
    replacement: TeletextEnhancementProgram,
    retained_commands: Vec<Option<usize>>,
}
impl<'de> Deserialize<'de> for StoredResourceProgram {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            identity: StoredResourceObjectIdentity,
            expected: TeletextEnhancementProgram,
            replacement: TeletextEnhancementProgram,
            retained_commands: Vec<Option<usize>>,
        }
        let value: Fields = map_only(d)?;
        Ok(Self {
            identity: value.identity,
            expected: value.expected,
            replacement: value.replacement,
            retained_commands: value.retained_commands,
        })
    }
}
#[derive(Default, Serialize)]
pub(super) struct OptionalStoredResourcePrograms(pub(super) Option<Vec<StoredResourceProgram>>);
impl OptionalStoredResourcePrograms {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredResourcePrograms {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredResourceProgram>::deserialize(d).map(|values| Self(Some(values)))
    }
}
pub(super) struct BorrowedResourcePrograms<'a>(
    pub(super) &'a [TeletextResourceObjectProgramOverlay],
);
impl Serialize for BorrowedResourcePrograms<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            identity: StoredResourceObjectIdentity,
            expected: &'a TeletextEnhancementProgram,
            replacement: &'a TeletextEnhancementProgram,
            retained_commands: &'a [Option<usize>],
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for program in self.0 {
            sequence.serialize_element(&Fields {
                identity: StoredResourceObjectIdentity(program.identity()),
                expected: program.expected(),
                replacement: program.replacement(),
                retained_commands: program.retained_commands(),
            })?;
        }
        sequence.end()
    }
}

pub(super) fn check_version(
    value: &super::StoredOverlay,
) -> Result<(), TeletextDesignDocumentError> {
    if !matches!(
        value.schema_version,
        super::RESOURCE_OBJECT_PROGRAM_OVERLAY_VERSION
            | super::RESOURCE_OBJECT_INVOCATION_OVERLAY_VERSION
            | super::RESOURCE_OBJECT_INVOCATION_STRUCTURE_OVERLAY_VERSION
            | super::RESOURCE_OBJECT_POSITION_OVERLAY_VERSION
    ) || value
        .resource_object_program_overlays
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
    {
        return Err(invalid());
    }
    Ok(())
}
fn structural(program: &TeletextResourceObjectProgramOverlay) -> bool {
    let remapped = program
        .retained_commands()
        .iter()
        .enumerate()
        .any(|(ordinal, retained)| *retained != Some(ordinal));
    let repositioned = program
        .expected()
        .commands()
        .iter()
        .zip(program.replacement().commands())
        .any(|(before, after)| {
            matches!((before, after), (
                TeletextEnhancementCommand::AtColumn { column: before, .. },
                TeletextEnhancementCommand::AtColumn { column: after, .. }
            ) if before != after)
        });
    program.expected().commands().len() != program.replacement().commands().len()
        || remapped
        || repositioned
}

pub(super) fn distinct(
    programs: &[TeletextResourceObjectProgramOverlay],
    operands: &[TeletextResourceObjectOperandOverlay],
) -> Result<(), TeletextDesignDocumentError> {
    if programs.iter().any(|program| {
        program.has_invocation_changes() || program.has_invocation_structure_changes()
    }) || !programs.iter().any(structural)
    {
        return Err(invalid());
    }
    distinct_targets(programs, operands)
}

pub(super) fn distinct_invocations(
    programs: &[TeletextResourceObjectProgramOverlay],
    operands: &[TeletextResourceObjectOperandOverlay],
) -> Result<(), TeletextDesignDocumentError> {
    if programs
        .iter()
        .any(TeletextResourceObjectProgramOverlay::has_invocation_structure_changes)
        || !programs
            .iter()
            .any(TeletextResourceObjectProgramOverlay::has_invocation_changes)
    {
        return Err(invalid());
    }
    distinct_targets(programs, operands)
}

pub(super) fn distinct_invocation_structure(
    programs: &[TeletextResourceObjectProgramOverlay],
    operands: &[TeletextResourceObjectOperandOverlay],
) -> Result<(), TeletextDesignDocumentError> {
    if !programs
        .iter()
        .any(TeletextResourceObjectProgramOverlay::has_invocation_structure_changes)
    {
        return Err(invalid());
    }
    distinct_targets(programs, operands)
}

pub(super) fn distinct_targets(
    programs: &[TeletextResourceObjectProgramOverlay],
    operands: &[TeletextResourceObjectOperandOverlay],
) -> Result<(), TeletextDesignDocumentError> {
    distinct_targets_for_role(programs, operands, false)
}

pub(super) fn distinct_original_positions(
    programs: &[TeletextResourceObjectProgramOverlay],
    operands: &[TeletextResourceObjectOperandOverlay],
) -> Result<(), TeletextDesignDocumentError> {
    if !programs
        .iter()
        .any(TeletextResourceObjectProgramOverlay::is_original_position_intent)
    {
        return Err(invalid());
    }
    distinct_targets_for_role(programs, operands, true)
}

fn distinct_targets_for_role(
    programs: &[TeletextResourceObjectProgramOverlay],
    operands: &[TeletextResourceObjectOperandOverlay],
    original_positions: bool,
) -> Result<(), TeletextDesignDocumentError> {
    let mut identities = BTreeSet::new();
    for program in programs {
        if program.is_position_restoration()
            || (program.has_inserted_positions()
                && !(original_positions && program.is_original_position_intent()))
            || !identities.insert(program.identity())
            || operands.iter().any(|operand| {
                operand.identity() == program.identity()
                    && (operand.expected().glyph_family().is_some()
                        || program.replacement_command(operand.command()).is_none())
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(super) fn restore_overlay(
    provenance: TeletextSavedEditorialProvenance,
    pages: Vec<TeletextPhysicalBaseOverlay>,
    operands: Vec<TeletextLocalProgramOperandOverlay>,
    objects: Vec<TeletextLocalObjectOperandOverlay>,
    bitmaps: Vec<TeletextDrcsBitmapOverlay>,
    local_programs: Vec<super::local_object_glyph_programs::StoredGlyphProgram>,
    resource_changes: (
        Vec<resource_object_operands::StoredResourceObjectOperand>,
        Vec<StoredResourceProgram>,
        TeletextDesignDocumentBudget,
        u16,
    ),
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let (resources, programs, budget, version) = resource_changes;
    let local_programs = super::local_object_glyph_programs::restore(
        local_programs,
        super::ProgramPolicy::StyleCommands,
    )?;
    let resources = resource_object_operands::restore(resources)?;
    let programs = restore_programs(programs, budget, version)?;
    let constructor = match version {
        super::RESOURCE_OBJECT_POSITION_OVERLAY_VERSION => {
            TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays
        }
        super::RESOURCE_OBJECT_INVOCATION_STRUCTURE_OVERLAY_VERSION => {
            TeletextSavedEditorialOverlay::new_with_resource_object_invocation_structure_overlays
        }
        super::RESOURCE_OBJECT_INVOCATION_OVERLAY_VERSION => {
            TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays
        }
        _ => TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays,
    };
    constructor(
        provenance,
        pages,
        operands,
        objects,
        bitmaps,
        local_programs,
        (resources, programs),
    )
}

pub(super) fn restore_programs(
    programs: Vec<StoredResourceProgram>,
    budget: TeletextDesignDocumentBudget,
    version: u16,
) -> Result<Vec<TeletextResourceObjectProgramOverlay>, TeletextDesignDocumentError> {
    programs
        .into_iter()
        .map(|program| {
            let constructor = if version == super::RESOURCE_OBJECT_POSITION_OVERLAY_VERSION
                && inserted_position(&program)
            {
                TeletextResourceObjectProgramOverlay::new_with_original_positions
            } else if version >= super::RESOURCE_OBJECT_INVOCATION_STRUCTURE_OVERLAY_VERSION
                && invocation_structure(&program)
            {
                TeletextResourceObjectProgramOverlay::new_with_invocation_structure
            } else if version >= super::RESOURCE_OBJECT_INVOCATION_OVERLAY_VERSION
                && changed_invocation(&program)
            {
                TeletextResourceObjectProgramOverlay::new_with_invocation_changes
            } else {
                TeletextResourceObjectProgramOverlay::new
            };
            constructor(
                program.identity.0,
                program.expected,
                program.replacement,
                program.retained_commands,
                budget.validation.service,
            )
            .map_err(|_| invalid())
        })
        .collect::<Result<Vec<_>, _>>()
}

fn inserted_position(program: &StoredResourceProgram) -> bool {
    program
        .replacement
        .commands()
        .iter()
        .zip(&program.retained_commands)
        .any(|(command, original)| {
            original.is_none() && matches!(command, TeletextEnhancementCommand::SetPosition { .. })
        })
}

fn invocation_structure(program: &StoredResourceProgram) -> bool {
    let mut originals = program.expected.commands().iter().enumerate();
    for (after, original) in program
        .replacement
        .commands()
        .iter()
        .zip(&program.retained_commands)
    {
        if let Some(ordinal) = original {
            for (index, command) in originals.by_ref() {
                if index == *ordinal {
                    break;
                }
                if matches!(command, TeletextEnhancementCommand::InvokeObject { .. }) {
                    return true;
                }
            }
        } else if matches!(after, TeletextEnhancementCommand::InvokeObject { .. }) {
            return true;
        }
    }
    originals.any(|(_, command)| matches!(command, TeletextEnhancementCommand::InvokeObject { .. }))
}

fn changed_invocation(program: &StoredResourceProgram) -> bool {
    program
        .replacement
        .commands()
        .iter()
        .zip(&program.retained_commands)
        .any(|(after, original)| {
            original.is_some_and(|ordinal| {
                program
                    .expected
                    .commands()
                    .get(ordinal)
                    .is_some_and(|before| {
                        before != after
                            && matches!(
                                (before, after),
                                (
                                    TeletextEnhancementCommand::InvokeObject { .. },
                                    TeletextEnhancementCommand::InvokeObject { .. }
                                )
                            )
                    })
            })
        })
}
fn add(before: usize, amount: usize) -> Result<usize, TeletextDesignDocumentError> {
    before
        .checked_add(amount)
        .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))
}
pub(super) fn reserve_budgets(
    overlay: &TeletextSavedEditorialOverlay,
    mut items: usize,
    budget: TeletextDesignDocumentBudget,
) -> Result<(usize, TeletextDesignDocumentBudget), TeletextDesignDocumentError> {
    let programs = &overlay.resource_object_program_overlays;
    limit(
        add(
            add(
                programs.len(),
                overlay.resource_object_definition_additions.len(),
            )?,
            overlay.local_object_glyph_program_overlays.len(),
        )?,
        budget.validation.service.max_objects,
        "editorial object programs",
    )?;
    let mut commands = add(
        overlay.resource_object_operands.len(),
        overlay.main_resource_invocations().len(),
    )?;
    for presence in overlay.main_program_presence_overlays() {
        items = add(items, 1)?;
        for program in [presence.expected(), presence.replacement()]
            .into_iter()
            .flatten()
        {
            commands = add(commands, program.commands().len())?;
            items = add(items, program.commands().len())?;
        }
        limit(
            items,
            budget.validation.service.max_structure_items,
            "editorial structure items",
        )?;
    }
    limit(
        overlay.main_local_invocation_program_overlays.len(),
        budget.validation.service.max_records,
        "editorial Main programs",
    )?;
    for program in &overlay.main_local_invocation_program_overlays {
        let count = add(
            program.expected().commands().len(),
            program.replacement().commands().len(),
        )?;
        commands = add(commands, count)?;
        items = add(
            items,
            add(1, add(count, program.retained_commands().len())?)?,
        )?;
        limit(
            items,
            budget.validation.service.max_structure_items,
            "editorial structure items",
        )?;
    }

    for addition in &overlay.resource_object_definition_additions {
        let count = addition.definition().program.commands().len();
        commands = add(commands, count)?;
        items = add(items, add(1, count)?)?;
        limit(
            items,
            budget.validation.service.max_structure_items,
            "editorial structure items",
        )?;
    }
    for program in programs {
        let count = add(
            program.expected().commands().len(),
            program.replacement().commands().len(),
        )?;
        commands = add(commands, count)?;
        items = add(
            items,
            add(1, add(count, program.retained_commands().len())?)?,
        )?;
        limit(
            items,
            budget.validation.service.max_structure_items,
            "editorial structure items",
        )?;
    }
    let mut remaining = budget;
    limit(
        commands,
        budget.validation.service.max_commands,
        "editorial operands",
    )?;
    remaining.validation.service.max_commands -= commands;
    remaining.validation.service.max_objects -= add(
        programs.len(),
        overlay.resource_object_definition_additions.len(),
    )?;
    Ok((items, remaining))
}

pub(super) fn validate_association(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    if overlay.resource_object_program_overlays.is_empty() {
        return if overlay.resource_object_definition_additions.is_empty()
            && overlay.main_local_invocation_program_overlays.is_empty()
        {
            Ok(())
        } else {
            validate_result_budgets(overlay, documents, budget)
        };
    }
    for program in &overlay.resource_object_program_overlays {
        let mut originals = documents.iter().filter_map(|document| {
            let TeletextProfileDocument::Authored(service) = document else {
                return None;
            };
            service.semantic().objects().definition(program.identity())
        });
        let original = originals.next().ok_or_else(invalid)?;
        if originals.next().is_some()
            || original.program.commands().len() != program.expected().commands().len()
        {
            return Err(invalid());
        }
        for (ordinal, (before, expected)) in original
            .program
            .commands()
            .iter()
            .zip(program.expected().commands())
            .enumerate()
        {
            let patched = overlay
                .resource_object_operands
                .iter()
                .find(|operand| {
                    operand.identity() == program.identity() && operand.command() == ordinal
                })
                .map_or(*before, |operand| TeletextEnhancementCommand::AtColumn {
                    column: operand.column(),
                    operation: operand.replacement(),
                });
            if patched != *expected {
                return Err(invalid());
            }
        }
    }
    validate_result_budgets(overlay, documents, budget)
}

fn validate_result_budgets(
    overlay: &TeletextSavedEditorialOverlay,
    documents: &[TeletextProfileDocument],
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    for document in documents {
        let TeletextProfileDocument::Authored(service) = document else {
            continue;
        };
        let mut original = 0;
        let mut candidate = 0;
        for page in service.semantic().page_enhancements().values() {
            if let Some(program) = &page.local_program {
                original = add(original, program.commands().len())?;
                let replacement = overlay
                    .main_local_invocation_program_overlays
                    .iter()
                    .find(|intent| intent.owner() == page.address)
                    .map_or(
                        program,
                        ersatzrs_domain::TeletextMainLocalInvocationProgramOverlay::replacement,
                    );
                candidate = add(candidate, replacement.commands().len())?;
            }
        }
        for addition in &overlay.resource_object_definition_additions {
            if addition
                .identity()
                .resource()
                .is_some_and(|resource| service.resource_inventory().contains(&resource))
            {
                candidate = add(candidate, addition.definition().program.commands().len())?;
            }
        }
        for definition in service.semantic().objects().definitions() {
            original = add(original, definition.program.commands().len())?;
            let resource = overlay
                .resource_object_program_overlays
                .iter()
                .find(|program| program.identity() == definition.identity)
                .map(ersatzrs_domain::TeletextResourceObjectProgramOverlay::replacement);
            let local = overlay
                .local_object_glyph_program_overlays
                .iter()
                .find(|program| program.identity() == definition.identity)
                .map(ersatzrs_domain::TeletextLocalObjectGlyphProgramOverlay::replacement);
            candidate = add(
                candidate,
                resource
                    .or(local)
                    .unwrap_or(&definition.program)
                    .commands()
                    .len(),
            )?;
        }
        limit(
            original,
            budget.validation.service.max_commands,
            "editorial original commands",
        )?;
        limit(
            candidate,
            budget.validation.service.max_commands,
            "editorial result commands",
        )?;
    }
    Ok(())
}

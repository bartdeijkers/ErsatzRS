//! Closed V11–V13 structure and exact-root association for retained Local programs.

use super::{
    failure, invalid, limit, map_only, object_operands, unique_original_owner,
    TeletextDesignDocumentBudget, TeletextDesignDocumentError, TeletextDesignDocumentFailure,
    TeletextProfileDocument, TeletextSavedEditorialOverlay, TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{
    TeletextDrcsBitmapOverlay, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay, TeletextLocalObjectOperandOverlay,
    TeletextLocalProgramOperandOverlay, TeletextPhysicalBaseOverlay,
};
use object_operands::StoredLocalIdentity;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub(super) enum ProgramPolicy {
    Legacy,
    AllGlyphs,
    StyleCommands,
    InvocationChanges,
    InvocationStructure,
}

#[derive(Serialize)]
pub(in super::super) struct StoredGlyphProgram {
    identity: StoredLocalIdentity,
    expected: TeletextEnhancementProgram,
    replacement: TeletextEnhancementProgram,
    retained_commands: Vec<Option<usize>>,
}
impl<'de> Deserialize<'de> for StoredGlyphProgram {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            identity: StoredLocalIdentity,
            expected: TeletextEnhancementProgram,
            replacement: TeletextEnhancementProgram,
            retained_commands: Vec<Option<usize>>,
        }
        let fields: Fields = map_only(d)?;
        Ok(Self {
            identity: fields.identity,
            expected: fields.expected,
            replacement: fields.replacement,
            retained_commands: fields.retained_commands,
        })
    }
}

#[derive(Default, Serialize)]
pub(super) struct OptionalStoredGlyphPrograms(pub(super) Option<Vec<StoredGlyphProgram>>);
impl OptionalStoredGlyphPrograms {
    pub(super) fn is_absent(&self) -> bool {
        self.0.is_none()
    }
}
impl<'de> Deserialize<'de> for OptionalStoredGlyphPrograms {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<StoredGlyphProgram>::deserialize(d).map(|value| Self(Some(value)))
    }
}

pub(super) fn restore(
    programs: Vec<StoredGlyphProgram>,
    policy: ProgramPolicy,
) -> Result<Vec<TeletextLocalObjectGlyphProgramOverlay>, TeletextDesignDocumentError> {
    programs
        .into_iter()
        .map(|program| {
            let constructor = match policy {
                ProgramPolicy::Legacy => TeletextLocalObjectGlyphProgramOverlay::new,
                ProgramPolicy::AllGlyphs => {
                    TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs
                }
                ProgramPolicy::InvocationStructure if changed_invocation_structure(&program) => {
                    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_structure
                }
                ProgramPolicy::InvocationChanges | ProgramPolicy::InvocationStructure
                    if changed_invocation(&program) =>
                {
                    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes
                }
                ProgramPolicy::StyleCommands
                | ProgramPolicy::InvocationChanges
                | ProgramPolicy::InvocationStructure => {
                    TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands
                }
            };
            constructor(
                program.identity.0,
                program.expected,
                program.replacement,
                program.retained_commands,
            )
            .map_err(|_| invalid())
        })
        .collect()
}

/// Restore the unchanged V11–V13 constructor choice after parent preflight.
pub(super) fn restore_overlay(
    provenance: TeletextSavedEditorialProvenance,
    pages: Vec<TeletextPhysicalBaseOverlay>,
    operands: Vec<TeletextLocalProgramOperandOverlay>,
    objects: Vec<TeletextLocalObjectOperandOverlay>,
    bitmaps: Vec<TeletextDrcsBitmapOverlay>,
    programs: Vec<StoredGlyphProgram>,
    policy: ProgramPolicy,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let constructor = match policy {
        ProgramPolicy::InvocationStructure => {
            TeletextSavedEditorialOverlay::new_with_local_object_invocation_structure_programs
        }
        ProgramPolicy::InvocationChanges => {
            TeletextSavedEditorialOverlay::new_with_local_object_invocation_programs
        }
        ProgramPolicy::StyleCommands => {
            TeletextSavedEditorialOverlay::new_with_local_object_style_programs
        }
        ProgramPolicy::AllGlyphs => {
            TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs
        }
        ProgramPolicy::Legacy => {
            TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs
        }
    };
    constructor(
        provenance,
        pages,
        operands,
        objects,
        bitmaps,
        restore(programs, policy)?,
    )
}

fn structural(program: &TeletextLocalObjectGlyphProgramOverlay) -> bool {
    program.expected().commands().len() != program.replacement().commands().len()
        || program
            .retained_commands()
            .iter()
            .enumerate()
            .any(|(ordinal, original)| *original != Some(ordinal))
        || program
            .expected()
            .commands()
            .iter()
            .zip(program.replacement().commands())
            .any(|(before, after)| {
                matches!((before, after),
                    (TeletextEnhancementCommand::AtColumn { column: a, .. },
                     TeletextEnhancementCommand::AtColumn { column: b, .. }) if a != b)
            })
}

pub(super) fn distinct(
    programs: &[TeletextLocalObjectGlyphProgramOverlay],
    operands: &[TeletextLocalObjectOperandOverlay],
    policy: ProgramPolicy,
) -> Result<(), TeletextDesignDocumentError> {
    let compatible = match policy {
        ProgramPolicy::Legacy => programs
            .iter()
            .all(TeletextLocalObjectGlyphProgramOverlay::is_legacy_compatible),
        ProgramPolicy::AllGlyphs => {
            programs
                .iter()
                .all(TeletextLocalObjectGlyphProgramOverlay::is_all_glyphs_compatible)
                && programs
                    .iter()
                    .any(|program| !program.is_legacy_compatible())
        }
        ProgramPolicy::StyleCommands => {
            programs
                .iter()
                .all(TeletextLocalObjectGlyphProgramOverlay::is_style_commands_compatible)
                && programs
                    .iter()
                    .any(|program| !program.is_all_glyphs_compatible())
        }
        ProgramPolicy::InvocationChanges => {
            programs.iter().all(|program| {
                !program.has_invocation_structure_changes()
                    && (program.has_invocation_changes() || program.is_style_commands_compatible())
            }) && programs
                .iter()
                .any(TeletextLocalObjectGlyphProgramOverlay::has_invocation_changes)
        }
        ProgramPolicy::InvocationStructure => {
            programs.iter().all(|program| {
                program.has_invocation_structure_changes()
                    || program.has_invocation_changes()
                    || program.is_style_commands_compatible()
            }) && programs
                .iter()
                .any(TeletextLocalObjectGlyphProgramOverlay::has_invocation_structure_changes)
        }
    };
    if (!matches!(
        policy,
        ProgramPolicy::InvocationChanges | ProgramPolicy::InvocationStructure
    ) && !programs.iter().any(structural))
        || !compatible
    {
        return Err(invalid());
    }
    let mut identities = BTreeSet::new();
    for program in programs {
        if !identities.insert(program.identity())
            || operands.iter().any(|operand| {
                operand.identity() == program.identity()
                    && match policy {
                        ProgramPolicy::Legacy => object_operands::glyph(operand.expected()),
                        ProgramPolicy::AllGlyphs
                        | ProgramPolicy::StyleCommands
                        | ProgramPolicy::InvocationChanges
                        | ProgramPolicy::InvocationStructure => {
                            operand.expected().glyph_family().is_some()
                        }
                    }
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn changed_invocation(program: &StoredGlyphProgram) -> bool {
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

fn changed_invocation_structure(program: &StoredGlyphProgram) -> bool {
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

pub(super) fn validate_budgets(
    overlay: &TeletextSavedEditorialOverlay,
    mut items: usize,
    budget: TeletextDesignDocumentBudget,
) -> Result<(), TeletextDesignDocumentError> {
    let mut commands = add(
        overlay.local_program_operands.len(),
        overlay.local_object_operands.len(),
    )?;
    limit(
        overlay.local_object_glyph_program_overlays.len(),
        budget.validation.service.max_objects,
        "editorial glyph programs",
    )?;
    for program in &overlay.local_object_glyph_program_overlays {
        let program_commands = add(
            program.expected().commands().len(),
            program.replacement().commands().len(),
        )?;
        commands = add(commands, program_commands)?;
        items = add(
            items,
            add(1, add(program_commands, program.retained_commands().len())?)?,
        )?;
        limit(
            commands,
            budget.validation.service.max_commands,
            "editorial operands",
        )?;
        limit(
            items,
            budget.validation.service.max_structure_items,
            "editorial structure items",
        )?;
    }
    Ok(())
}

fn add(before: usize, amount: usize) -> Result<usize, TeletextDesignDocumentError> {
    before
        .checked_add(amount)
        .ok_or_else(|| failure(TeletextDesignDocumentFailure::Storage))
}

pub(super) fn validate_association(
    programs: &[TeletextLocalObjectGlyphProgramOverlay],
    operands: &[TeletextLocalObjectOperandOverlay],
    documents: &[TeletextProfileDocument],
) -> Result<(), TeletextDesignDocumentError> {
    for program in programs {
        let TeletextProfileDocument::Authored(service) =
            unique_original_owner(program.owner(), documents)?
        else {
            return Err(invalid());
        };
        let original = service
            .semantic()
            .objects()
            .definition(program.identity())
            .ok_or_else(invalid)?;
        if original.program.commands().len() != program.expected().commands().len() {
            return Err(invalid());
        }
        // Scalar operands were independently matched to immutable originals.
        // Compare without cloning or accepting an uploaded expected skeleton.
        for (ordinal, (before, expected)) in original
            .program
            .commands()
            .iter()
            .zip(program.expected().commands())
            .enumerate()
        {
            let patched = operands
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
    Ok(())
}

pub(super) struct BorrowedGlyphPrograms<'a>(
    pub(super) &'a [TeletextLocalObjectGlyphProgramOverlay],
);
impl Serialize for BorrowedGlyphPrograms<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields<'a> {
            identity: StoredLocalIdentity,
            expected: &'a TeletextEnhancementProgram,
            replacement: &'a TeletextEnhancementProgram,
            retained_commands: &'a [Option<usize>],
        }
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for program in self.0 {
            sequence.serialize_element(&Fields {
                identity: StoredLocalIdentity(program.identity()),
                expected: program.expected(),
                replacement: program.replacement(),
                retained_commands: program.retained_commands(),
            })?;
        }
        sequence.end()
    }
}

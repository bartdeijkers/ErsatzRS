//! Closed V11 structure and exact-root association for retained Local glyph programs.

use super::{
    failure, invalid, limit, map_only, object_operands, unique_original_owner,
    TeletextDesignDocumentBudget, TeletextDesignDocumentError, TeletextDesignDocumentFailure,
    TeletextProfileDocument, TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementProgram, TeletextLocalObjectGlyphProgramOverlay,
    TeletextLocalObjectOperandOverlay,
};
use object_operands::StoredLocalIdentity;
use serde::{ser::SerializeSeq, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;

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
) -> Result<Vec<TeletextLocalObjectGlyphProgramOverlay>, TeletextDesignDocumentError> {
    programs
        .into_iter()
        .map(|program| {
            TeletextLocalObjectGlyphProgramOverlay::new(
                program.identity.0,
                program.expected,
                program.replacement,
                program.retained_commands,
            )
            .map_err(|_| invalid())
        })
        .collect()
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
) -> Result<(), TeletextDesignDocumentError> {
    if !programs.iter().any(structural) {
        return Err(invalid());
    }
    let mut identities = BTreeSet::new();
    for program in programs {
        if !identities.insert(program.identity())
            || operands.iter().any(|operand| {
                operand.identity() == program.identity()
                    && object_operands::glyph(operand.expected())
            })
        {
            return Err(invalid());
        }
    }
    Ok(())
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

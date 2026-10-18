//! Preserve V11 scalar anchors where possible; V12 owns genuine wide structure.
use super::{fail, program::Candidate, Location, Result, TeletextEditorialChanges};
use ersatzrs_domain::{
    TeletextColumnGlyphFamily, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay, TeletextLocalObjectOperandOverlay,
};

pub(super) fn append(
    candidate: Candidate,
    changes: &mut TeletextEditorialChanges,
    location: Location,
) -> Result<()> {
    if !candidate.structural {
        for (ordinal, (before, after)) in candidate
            .expected
            .commands()
            .iter()
            .zip(&candidate.replacement)
            .enumerate()
        {
            if let Some(operand) = scalar(candidate.identity, ordinal, before, after, location)? {
                changes.object_operands.push(operand);
            }
        }
        return Ok(());
    }
    let replacement =
        TeletextEnhancementProgram::new(candidate.replacement).map_err(|_| fail(location))?;
    let mut expected = candidate.expected.commands().to_vec();
    let mut anchors = Vec::new();
    for (after, source) in replacement.commands().iter().zip(&candidate.retained) {
        let Some(ordinal) = source else { continue };
        let before = &candidate.expected.commands()[*ordinal];
        if wide(before) {
            if let Some(operand) = scalar(candidate.identity, *ordinal, before, after, location)
                .ok()
                .flatten()
            {
                expected[*ordinal] = *after;
                anchors.push(operand);
            }
        }
    }
    let expected = TeletextEnhancementProgram::new(expected).map_err(|_| fail(location))?;
    if let Ok(legacy) = TeletextLocalObjectGlyphProgramOverlay::new(
        candidate.identity,
        expected,
        replacement.clone(),
        candidate.retained.clone(),
    ) {
        changes.object_operands.extend(anchors);
        changes.object_programs.push(legacy);
    } else {
        let program = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
            candidate.identity,
            candidate.expected.clone(),
            replacement.clone(),
            candidate.retained.clone(),
        )
        .or_else(|_| {
            TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
                candidate.identity,
                candidate.expected.clone(),
                replacement.clone(),
                candidate.retained.clone(),
            )
        })
        .or_else(|_| {
            TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
                candidate.identity,
                candidate.expected,
                replacement,
                candidate.retained,
            )
        })
        .map_err(|_| fail(location))?;
        changes.object_programs.push(program);
    }
    Ok(())
}
fn wide(command: &TeletextEnhancementCommand) -> bool {
    matches!(command, TeletextEnhancementCommand::AtColumn { operation, .. }
        if operation.glyph_family().is_some_and(|family| !matches!(family, TeletextColumnGlyphFamily::G0 | TeletextColumnGlyphFamily::G1)))
}
fn scalar(
    identity: ersatzrs_domain::TeletextObjectIdentity,
    ordinal: usize,
    before: &TeletextEnhancementCommand,
    after: &TeletextEnhancementCommand,
    location: Location,
) -> Result<Option<TeletextLocalObjectOperandOverlay>> {
    if before == after {
        return Ok(None);
    }
    let (
        TeletextEnhancementCommand::AtColumn {
            column: a,
            operation: expected,
        },
        TeletextEnhancementCommand::AtColumn {
            column: b,
            operation: replacement,
        },
    ) = (before, after)
    else {
        return Err(fail(location));
    };
    if a != b {
        return Err(fail(location));
    }
    Ok(Some(
        TeletextLocalObjectOperandOverlay::new(identity, ordinal, *a, *expected, *replacement)
            .map_err(|_| fail(location))?,
    ))
}

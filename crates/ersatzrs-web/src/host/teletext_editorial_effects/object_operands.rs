//! Exact original definition changes; every invocation still needs permission.

use super::{
    check_deadline, error,
    programs::{self, Program},
    Result, TeletextEditorialEdits, TeletextEditorialEffectsFailure,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextLocalObjectOperandOverlay, TeletextObjectIdentity,
    TeletextResourceObjectOperandOverlay,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

#[derive(Clone, Copy)]
enum Operand<'a> {
    Local(&'a TeletextLocalObjectOperandOverlay),
    Resource(&'a TeletextResourceObjectOperandOverlay),
}
impl Operand<'_> {
    fn identity(self) -> TeletextObjectIdentity {
        match self {
            Self::Local(value) => value.identity(),
            Self::Resource(value) => value.identity(),
        }
    }
    fn command(self) -> usize {
        match self {
            Self::Local(value) => value.command(),
            Self::Resource(value) => value.command(),
        }
    }
    fn column(self) -> u8 {
        match self {
            Self::Local(value) => value.column(),
            Self::Resource(value) => value.column(),
        }
    }
    fn expected(self) -> TeletextColumnOperation {
        match self {
            Self::Local(value) => value.expected(),
            Self::Resource(value) => value.expected(),
        }
    }
    fn replacement(self) -> TeletextColumnOperation {
        match self {
            Self::Local(value) => value.replacement(),
            Self::Resource(value) => value.replacement(),
        }
    }
}
fn operands(edits: TeletextEditorialEdits<'_>) -> impl Iterator<Item = Operand<'_>> {
    edits
        .object_operands
        .iter()
        .map(Operand::Local)
        .chain(edits.resource_operands.iter().map(Operand::Resource))
}

pub(super) fn check(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<()> {
    super::definition_additions::check_catalogues(before, after, incoming, cumulative, deadline)?;
    let incoming_programs = programs::all(incoming);
    let cumulative_programs = programs::all(cumulative);
    if incoming.object_programs.is_empty()
        && incoming.resource_programs.is_empty()
        && cumulative.object_programs.is_empty()
        && cumulative.resource_programs.is_empty()
        && incoming.resource_additions.is_empty()
        && cumulative.resource_additions.is_empty()
        && incoming.object_resource_additions.is_empty()
        && cumulative.object_resource_additions.is_empty()
    {
        return check_scalar_operands(before, after, incoming, cumulative, deadline);
    }
    let association = || error(None, None, TeletextEditorialEffectsFailure::Association);
    let structure = || {
        error(
            None,
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        )
    };
    let mut changes = BTreeMap::new();
    for change in operands(incoming) {
        check_deadline(deadline)?;
        if changes
            .insert((change.identity(), change.command()), change)
            .is_some()
            || command(before, change)
                != Some(TeletextEnhancementCommand::AtColumn {
                    column: change.column(),
                    operation: change.expected(),
                })
        {
            return Err(association());
        }
    }
    let mut programs = BTreeMap::new();
    for program in incoming_programs {
        check_deadline(deadline)?;
        if programs.insert(program.identity(), program).is_some()
            || before.definition(program.identity()).is_none()
        {
            return Err(association());
        }
    }
    let mut final_programs = BTreeMap::new();
    for program in cumulative_programs {
        check_deadline(deadline)?;
        if final_programs.insert(program.identity(), program).is_some()
            || after
                .definition(program.identity())
                .map(|definition| &definition.program)
                != Some(program.replacement())
        {
            return Err(association());
        }
    }
    check_final_operands(after, cumulative, &final_programs, deadline)?;
    for old in before.definitions() {
        let new = after.definition(old.identity).ok_or_else(structure)?;
        check_deadline(deadline)?;
        if old.identity != new.identity || old.object_type != new.object_type {
            return Err(structure());
        }
        let has_operands = changes
            .range((old.identity, 0)..=(old.identity, usize::MAX))
            .next()
            .is_some();
        if !has_operands && !programs.contains_key(&old.identity) {
            if old.program != new.program {
                return Err(structure());
            }
            continue;
        }
        let mut expected = old.program.commands().to_vec();
        for ((_, ordinal), change) in changes.range((old.identity, 0)..=(old.identity, usize::MAX))
        {
            check_deadline(deadline)?;
            let value = expected.get_mut(*ordinal).ok_or_else(association)?;
            *value = TeletextEnhancementCommand::AtColumn {
                column: change.column(),
                operation: change.replacement(),
            };
        }
        if let Some(program) = programs.get(&old.identity) {
            if program.expected().commands() != expected || program.replacement() != &new.program {
                return Err(structure());
            }
        } else if new.program.commands() != expected {
            return Err(structure());
        }
    }
    Ok(())
}

fn check_final_operands(
    after: &TeletextEnhancementObjectCatalog,
    cumulative: TeletextEditorialEdits<'_>,
    final_programs: &BTreeMap<TeletextObjectIdentity, Program<'_>>,
    deadline: Instant,
) -> Result<()> {
    let association = || error(None, None, TeletextEditorialEffectsFailure::Association);
    let mut targets = BTreeSet::new();
    for change in operands(cumulative) {
        check_deadline(deadline)?;
        let ordinal = match final_programs.get(&change.identity()) {
            Some(program) => program
                .replacement_command(change.command())
                .ok_or_else(association)?,
            None => change.command(),
        };
        let actual = after
            .definition(change.identity())
            .and_then(|definition| definition.program.commands().get(ordinal));
        if !targets.insert((change.identity(), change.command()))
            || actual
                != Some(&TeletextEnhancementCommand::AtColumn {
                    column: change.column(),
                    operation: change.replacement(),
                })
        {
            return Err(association());
        }
    }
    Ok(())
}

fn check_scalar_operands(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    incoming: TeletextEditorialEdits<'_>,
    cumulative: TeletextEditorialEdits<'_>,
    deadline: Instant,
) -> Result<()> {
    let association = || error(None, None, TeletextEditorialEffectsFailure::Association);
    let structure = || {
        error(
            None,
            None,
            TeletextEditorialEffectsFailure::ProtectedStructure,
        )
    };
    let mut changes = BTreeMap::new();
    for change in operands(incoming) {
        check_deadline(deadline)?;
        if changes
            .insert((change.identity(), change.command()), change)
            .is_some()
            || command(before, change)
                != Some(TeletextEnhancementCommand::AtColumn {
                    column: change.column(),
                    operation: change.expected(),
                })
        {
            return Err(association());
        }
    }
    let mut targets = BTreeSet::new();
    for change in operands(cumulative) {
        check_deadline(deadline)?;
        if !targets.insert((change.identity(), change.command()))
            || command(after, change)
                != Some(TeletextEnhancementCommand::AtColumn {
                    column: change.column(),
                    operation: change.replacement(),
                })
        {
            return Err(association());
        }
    }
    if before.definitions().len() != after.definitions().len() {
        return Err(structure());
    }
    for (old, new) in before.definitions().iter().zip(after.definitions()) {
        check_deadline(deadline)?;
        if old.identity != new.identity
            || old.object_type != new.object_type
            || old.program.commands().len() != new.program.commands().len()
        {
            return Err(structure());
        }
        for (ordinal, (old_command, new_command)) in old
            .program
            .commands()
            .iter()
            .zip(new.program.commands())
            .enumerate()
        {
            check_deadline(deadline)?;
            let expected = changes
                .get(&(old.identity, ordinal))
                .map_or(*old_command, |change| {
                    TeletextEnhancementCommand::AtColumn {
                        column: change.column(),
                        operation: change.replacement(),
                    }
                });
            if *new_command != expected {
                return Err(structure());
            }
        }
    }
    Ok(())
}

fn command(
    catalog: &TeletextEnhancementObjectCatalog,
    change: Operand<'_>,
) -> Option<TeletextEnhancementCommand> {
    catalog
        .definition(change.identity())?
        .program
        .commands()
        .get(change.command())
        .copied()
}

#[cfg(test)]
#[path = "object_operands_tests.rs"]
mod tests;

#[cfg(test)]
fn check_operands(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    incoming: &[TeletextLocalObjectOperandOverlay],
    cumulative: &[TeletextLocalObjectOperandOverlay],
    deadline: Instant,
) -> Result<()> {
    fn edits(object_operands: &[TeletextLocalObjectOperandOverlay]) -> TeletextEditorialEdits<'_> {
        TeletextEditorialEdits {
            base_text_edits: &[],
            rows: &[],
            operands: &[],
            object_operands,
            resource_operands: &[],
            resource_programs: &[],
            resource_additions: &[],
            object_resource_additions: &[],
            object_programs: &[],
            main_programs: &[],
            bitmaps: &[],
        }
    }
    check_scalar_operands(before, after, edits(incoming), edits(cumulative), deadline)
}

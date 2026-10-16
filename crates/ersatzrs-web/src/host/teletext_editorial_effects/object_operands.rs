//! Exact original definition changes; every invocation still needs permission.

use super::{check_deadline, error, Result, TeletextEditorialEffectsFailure};
use ersatzrs_domain::{
    TeletextEnhancementCommand, TeletextEnhancementObjectCatalog, TeletextLocalObjectOperandOverlay,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

pub(super) fn check(
    before: &TeletextEnhancementObjectCatalog,
    after: &TeletextEnhancementObjectCatalog,
    incoming: &[TeletextLocalObjectOperandOverlay],
    cumulative: &[TeletextLocalObjectOperandOverlay],
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
    for change in incoming {
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
    for change in cumulative {
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
    change: &TeletextLocalObjectOperandOverlay,
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

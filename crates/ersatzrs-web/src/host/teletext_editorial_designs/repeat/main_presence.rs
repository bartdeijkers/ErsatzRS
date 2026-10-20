//! Fold edits of a newly created Main into its exact optional root attachment.
use super::{
    check_deadline, NativeTeletextEditorialImportError, QualifiedBaseline, Result,
    TeletextEditorialChanges,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementProgram,
    TeletextMainProgramPresenceOverlay, TeletextPageAddress,
};
use std::{collections::BTreeSet, time::Instant};

type Folded = (
    TeletextEditorialChanges,
    TeletextEditorialChanges,
    Vec<TeletextMainProgramPresenceOverlay>,
);
fn invalid() -> NativeTeletextEditorialImportError {
    NativeTeletextEditorialImportError::Association
}

pub(super) fn fold(
    root: &QualifiedBaseline,
    previous: &TeletextEditorialChanges,
    incoming: &TeletextEditorialChanges,
    deadline: Instant,
) -> Result<Folded> {
    let pages = root
        .original
        .view()
        .encoded()
        .ok_or_else(invalid)?
        .declared()
        .semantic()
        .page_enhancements();
    let owners: BTreeSet<_> = previous
        .main_presence
        .iter()
        .chain(&incoming.main_presence)
        .map(TeletextMainProgramPresenceOverlay::owner)
        .collect();
    let mut a = previous.clone();
    let mut b = incoming.clone();
    let mut result = Vec::new();
    for owner in &owners {
        check_deadline(deadline)?;
        let original = pages.get(owner).ok_or_else(invalid)?.local_program.as_ref();
        let mut current = original.cloned();
        for changes in [previous, incoming] {
            let mut seen = BTreeSet::new();
            for operand in changes.operands.iter().filter(|v| v.owner() == *owner) {
                check_deadline(deadline)?;
                let mut commands = current.as_ref().ok_or_else(invalid)?.commands().to_vec();
                if !seen.insert(operand.command())
                    || commands.get(operand.command())
                        != Some(&Command::AtColumn {
                            column: operand.column(),
                            operation: operand.expected(),
                        })
                {
                    return Err(invalid());
                }
                commands[operand.command()] = Command::AtColumn {
                    column: operand.column(),
                    operation: operand.replacement(),
                };
                current = Some(TeletextEnhancementProgram::new(commands).map_err(|_| invalid())?);
            }
            let structural: Vec<_> = changes
                .main_programs
                .iter()
                .filter(|v| v.owner() == *owner)
                .collect();
            if structural.len() > 1 {
                return Err(invalid());
            }
            if let Some(program) = structural.first() {
                if current.as_ref() != Some(program.expected()) {
                    return Err(invalid());
                }
                current = Some(program.replacement().clone());
            }
            // The existing resource-consumer role retains its own ordinal and
            // selection qualification. It cannot be laundered into a new body.
            if changes
                .main_resource_invocations
                .iter()
                .any(|v| v.owner() == *owner)
            {
                return Err(invalid());
            }
            let presence: Vec<_> = changes
                .main_presence
                .iter()
                .filter(|v| v.owner() == *owner)
                .collect();
            if presence.len() > 1 {
                return Err(invalid());
            }
            if let Some(presence) = presence.first() {
                if current.as_ref() != presence.expected() {
                    return Err(invalid());
                }
                current = presence.replacement().cloned();
            }
        }
        if original != current.as_ref() {
            // None and Some(empty) remain distinct. A root-present arbitrary body
            // replacement is not granted the older structural role's authority.
            result.push(
                TeletextMainProgramPresenceOverlay::new(*owner, original.cloned(), current)
                    .map_err(|_| invalid())?,
            );
        }
    }
    for changes in [&mut a, &mut b] {
        strip(changes, &owners);
    }
    Ok((a, b, result))
}

fn strip(changes: &mut TeletextEditorialChanges, owners: &BTreeSet<TeletextPageAddress>) {
    changes.main_presence.clear();
    changes.operands.retain(|p| !owners.contains(&p.owner()));
    changes
        .main_programs
        .retain(|p| !owners.contains(&p.owner()));
}

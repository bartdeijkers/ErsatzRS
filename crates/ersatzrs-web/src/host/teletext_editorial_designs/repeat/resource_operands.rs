//! Root-relative resource definition edits, including repeat and exact undo.

use super::{
    check_deadline, BTreeMap, BTreeSet, Instant, NativeTeletextEditorialImportError, Result,
    TeletextServiceBudget,
};
use ersatzrs_domain::TeletextResourceObjectOperandOverlay;

pub(super) fn merge(
    previous: &[TeletextResourceObjectOperandOverlay],
    incoming: &[TeletextResourceObjectOperandOverlay],
    service: TeletextServiceBudget,
    deadline: Instant,
) -> Result<Vec<TeletextResourceObjectOperandOverlay>> {
    let maximum = service.max_commands.min(service.max_structure_items);
    if previous.len() > maximum || incoming.len() > maximum {
        return Err(NativeTeletextEditorialImportError::Budget);
    }
    let mut merged = BTreeMap::new();
    for operand in previous {
        check_deadline(deadline)?;
        if merged
            .insert((operand.identity(), operand.command()), *operand)
            .is_some()
        {
            return Err(NativeTeletextEditorialImportError::Association);
        }
    }
    let mut seen = BTreeSet::new();
    for operand in incoming {
        check_deadline(deadline)?;
        let key = (operand.identity(), operand.command());
        if !seen.insert(key) {
            return Err(NativeTeletextEditorialImportError::Association);
        }
        if let Some(previous) = merged.get(&key).copied() {
            if previous.column() != operand.column() || previous.replacement() != operand.expected()
            {
                return Err(NativeTeletextEditorialImportError::Association);
            }
            if operand.replacement() == previous.expected() {
                // Revert removes root-relative intent. The incoming transition
                // remains separately checked against latest-export permissions.
                merged.remove(&key);
            } else {
                let cumulative = TeletextResourceObjectOperandOverlay::new(
                    operand.identity(),
                    operand.command(),
                    operand.column(),
                    previous.expected(),
                    operand.replacement(),
                )
                .map_err(|_| NativeTeletextEditorialImportError::Association)?;
                merged.insert(key, cumulative);
            }
        } else {
            if merged.len() >= maximum {
                return Err(NativeTeletextEditorialImportError::Budget);
            }
            // Reconciliation authenticated this against the latest service;
            // attachment independently matches it to the immutable root.
            merged.insert(key, *operand);
        }
    }
    Ok(merged.into_values().collect())
}

#[cfg(test)]
#[path = "resource_operands_tests.rs"]
mod tests;

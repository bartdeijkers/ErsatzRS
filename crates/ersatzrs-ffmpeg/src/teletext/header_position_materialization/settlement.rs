//! Settle a blocked header candidate from its real lowerable Main prefix.
//!
//! Main words retain absolute witnesses. Already-invoked Local definitions use
//! provisional body-relative order, requiring exact final body/mapping checks
//! before full final-word C.2 or output can succeed.

use std::{collections::BTreeMap, sync::Arc};

use ersatzrs_domain::{
    TeletextAttributeTransmissionEvidence, TeletextAttributeTransmissionPosition,
    TeletextEnhancementCommand, TeletextEnhancementProgram, TeletextObjectCatalogBudget,
    TeletextSemanticService,
};

use super::super::compile_context::TeletextCompileContext;
#[cfg(test)]
use super::super::composed_main::pack_composition_program;
use super::super::composed_main::{
    pack_normalized_composition_program, TeletextComposedMainEncodingError,
};
use super::super::{
    EncodedTeletextLocalObjectProgram, TeletextMainComposition, TeletextMainCompositionBudget,
    TeletextMainCompositionError, TeletextMainCompositionFailure, TeletextMainCompositionSource,
    TeletextResourceObjectAllocations,
};

#[path = "settlement/local_definition.rs"]
mod local_definition;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::teletext) struct Witness {
    position: TeletextAttributeTransmissionPosition,
    bytes: [u8; 3],
    local: Option<Arc<local_definition::Body>>,
}

pub(in crate::teletext) fn prepare(
    mut composition: TeletextMainComposition,
    service: &TeletextSemanticService,
    allocations: &TeletextResourceObjectAllocations,
    budget: TeletextMainCompositionBudget,
    packing: (TeletextObjectCatalogBudget, usize),
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<(TeletextMainComposition, Vec<Witness>), TeletextComposedMainEncodingError> {
    let mut witnesses = BTreeMap::<usize, Witness>::new();
    let mut previous = None;
    loop {
        let (candidate, unsettled) =
            super::materialize_until_unsettled(composition, service, budget, context)?;
        composition = candidate;
        let Some(unsettled) = unsettled else { break };
        // Each successful pass consumes another original call. Never retry the
        // same unresolved boundary, renew a deadline or create a fresh quota.
        if previous.is_some_and(|before| unsettled.before <= before) {
            return Err(state_error(&composition).into());
        }
        previous = Some(unsettled.before);
        let prefix = TeletextEnhancementProgram::new(
            composition.program().commands()[..unsettled.before].to_vec(),
        )
        .map_err(|reason| TeletextMainCompositionError {
            owner: composition.owner(),
            location: None,
            reason: reason.into(),
        })?;
        let encoded = pack_normalized_composition_program(
            service,
            &composition,
            &prefix,
            allocations,
            packing,
            context,
        )?;
        // Preserve existing Main-only successes: an unrelated nested Local
        // pointer can move when the later setter moves its definition.
        // Only unresolved foreground needs provisional Local body constraints.
        for local in [false, true] {
            let discovered = if local {
                local_definition::witnesses(&composition, &encoded)?
            } else {
                prefix_witnesses(&composition, &encoded)?
            };
            if local && discovered.is_empty() {
                break;
            }
            for witness in discovered {
                if let Some(previous) = witnesses.get(&witness.position.event_ordinal) {
                    if previous != &witness {
                        return Err(state_error(&composition).into());
                    }
                } else {
                    witnesses.insert(witness.position.event_ordinal, witness);
                }
            }
            let positions: Vec<_> = witnesses.values().map(|witness| witness.position).collect();
            let evidence = TeletextAttributeTransmissionEvidence::new(
                composition.spatial_trace(),
                &positions,
                budget.collection.max_commands,
            )
            .map_err(|reason| TeletextMainCompositionError {
                owner: composition.owner(),
                location: None,
                reason: reason.into(),
            })?;
            composition = composition.with_transmission_context(
                service,
                &evidence,
                budget.collection.max_commands,
                context,
            )?;
            if super::foreground(&composition, unsettled.column).is_some() {
                break;
            }
        }
        if super::foreground(&composition, unsettled.column).is_none() {
            // A different executing instance cannot gain invented time order.
            // Final packing retains the original located lowering refusal.
            break;
        }
    }
    Ok((composition, witnesses.into_values().collect()))
}

fn prefix_witnesses(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
) -> Result<Vec<Witness>, TeletextMainCompositionError> {
    let mut witnesses = Vec::new();
    for (event_ordinal, event) in composition.spatial_trace().events().iter().enumerate() {
        if event.source().object.is_some() || event.invocation().is_some() {
            continue;
        }
        let source = TeletextMainCompositionSource::OriginalPage {
            command: event.source().command,
        };
        let mut positions = encoded.command_positions().iter().filter(|position| {
            composition.source_location(position.source()) == Some(source)
                && matches!(
                    composition
                        .program()
                        .commands()
                        .get(position.source().command),
                    Some(TeletextEnhancementCommand::AtColumn { .. })
                )
        });
        if let Some(position) = positions.next() {
            if positions.next().is_some() {
                return Err(state_error(composition));
            }
            witnesses.push(Witness {
                position: TeletextAttributeTransmissionPosition {
                    event_ordinal,
                    source: event.source(),
                    invocation: None,
                    word: position.semantic_word(),
                },
                bytes: triplet(encoded, position.semantic_word())
                    .ok_or_else(|| state_error(composition))?,
                local: None,
            });
        }
    }
    Ok(witnesses)
}

pub(in crate::teletext) fn verify(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
    witnesses: &[Witness],
) -> Result<(), TeletextMainCompositionError> {
    if witnesses.is_empty() {
        return Ok(());
    }
    let mut actual = prefix_witnesses(composition, encoded)?;
    actual.extend(local_definition::witnesses(composition, encoded)?);
    if witnesses.iter().any(|expected| !actual.contains(expected)) {
        return Err(state_error(composition));
    }
    Ok(())
}

fn triplet(encoded: &EncodedTeletextLocalObjectProgram, word: usize) -> Option<[u8; 3]> {
    let body = encoded.packet_bodies().get(word / 13)?;
    let offset = 1 + (word % 13) * 3;
    Some([body[offset], body[offset + 1], body[offset + 2]])
}

fn state_error(composition: &TeletextMainComposition) -> TeletextMainCompositionError {
    TeletextMainCompositionError {
        owner: composition.owner(),
        location: None,
        reason: TeletextMainCompositionFailure::SelectionState,
    }
}

#[cfg(test)]
mod tests;

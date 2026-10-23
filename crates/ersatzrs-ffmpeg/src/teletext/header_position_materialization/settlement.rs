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
            let mut positions: Vec<_> =
                witnesses.values().map(|witness| witness.position).collect();
            if context.resources_before_page {
                append_resource_positions(&composition, &encoded, &mut positions, context)?;
            }
            let page_positions = if local || context.resources_before_page {
                absolute_page_positions(&composition, &encoded, &witnesses)?
            } else {
                Vec::new()
            };
            let mut evidence = TeletextAttributeTransmissionEvidence::new_with_same_page_order(
                composition.spatial_trace(),
                &positions,
                &page_positions,
                budget.collection.max_commands,
            )
            .map_err(|reason| TeletextMainCompositionError {
                owner: composition.owner(),
                location: None,
                reason: reason.into(),
            })?;
            if context.resources_before_page {
                evidence = evidence.with_resources_before_page();
            }
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
            // Missing, foreign or ambiguous producers cannot gain time order.
            // Final packing retains the original located lowering refusal.
            break;
        }
    }
    Ok((composition, witnesses.into_values().collect()))
}

fn append_resource_positions(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
    positions: &mut Vec<TeletextAttributeTransmissionPosition>,
    context: &TeletextCompileContext<'_, '_>,
) -> Result<(), TeletextComposedMainEncodingError> {
    let Some(resources) = context.resources else {
        return Ok(());
    };
    let trace = composition.spatial_trace();
    for (event_ordinal, &event) in trace.events().iter().enumerate() {
        if event
            .source()
            .object
            .is_none_or(|target| target.resource().is_none())
            || !local_definition::root_in_prefix(composition, encoded, event.root_source())
        {
            continue;
        }
        let word = resources
            .word(trace, event, event_ordinal)
            .map_err(|reason| TeletextMainCompositionError {
                owner: composition.owner(),
                location: event.source().object.map(|identity| {
                    TeletextMainCompositionSource::Object {
                        identity,
                        command: event.source().command,
                    }
                }),
                reason: reason.into(),
            })?;
        if let Some(word) = word {
            positions.push(TeletextAttributeTransmissionPosition {
                event_ordinal,
                source: event.source(),
                invocation: event.invocation(),
                word,
            });
        }
    }
    Ok(())
}

fn absolute_page_positions(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
    witnesses: &BTreeMap<usize, Witness>,
) -> Result<Vec<TeletextAttributeTransmissionPosition>, TeletextMainCompositionError> {
    let mut result = Vec::new();
    let eligible =
        super::super::composed_main::same_page::eligible_events(composition.spatial_trace());
    for witness in witnesses.values() {
        if eligible.get(witness.position.event_ordinal) != Some(&true) {
            continue;
        }
        let source = witness.position.source;
        let expected = match source.object {
            Some(identity) => TeletextMainCompositionSource::Object {
                identity,
                command: source.command,
            },
            None => TeletextMainCompositionSource::OriginalPage {
                command: source.command,
            },
        };
        let mut matches = encoded.command_positions().iter().filter(|position| {
            position
                .retained_source()
                .and_then(|source| composition.source_location(source))
                == Some(expected)
        });
        let position = matches.next().ok_or_else(|| state_error(composition))?;
        if matches.next().is_some()
            || triplet(encoded, position.semantic_word()) != Some(witness.bytes)
        {
            return Err(state_error(composition));
        }
        result.push(TeletextAttributeTransmissionPosition {
            word: position.semantic_word(),
            ..witness.position
        });
    }
    Ok(result)
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

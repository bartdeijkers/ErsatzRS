//! Provisional Local body order, accepted only after exact final-body checking.

use std::{
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};

use ersatzrs_domain::{
    TeletextAttributeTransmissionPosition, TeletextEnhancementCommand, TeletextObjectIdentity,
    TeletextObjectScope, TeletextObjectType, TeletextSpatialRootSource,
};

use super::{
    state_error, triplet, EncodedTeletextLocalObjectProgram, TeletextMainComposition,
    TeletextMainCompositionError, TeletextMainCompositionSource, Witness,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Body {
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    words: Vec<[u8; 3]>,
    boundary: Boundary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Boundary {
    Following(TeletextObjectIdentity, TeletextObjectType),
    Final([u8; 3]),
}

pub(super) fn witnesses(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
) -> Result<Vec<Witness>, TeletextMainCompositionError> {
    let mut bodies = BTreeMap::<TeletextObjectIdentity, (usize, Arc<Body>)>::new();
    let mut witnesses = Vec::new();
    for (event_ordinal, event) in composition.spatial_trace().events().iter().enumerate() {
        let Some(target) = event.source().object else {
            continue;
        };
        if target.scope() != TeletextObjectScope::Local
            || event.invocation().is_none()
            || !root_in_prefix(composition, encoded, event.root_source())
        {
            continue;
        }
        let source = TeletextMainCompositionSource::Object {
            identity: target,
            command: event.source().command,
        };
        let mut positions = encoded.command_positions().iter().filter(|position| {
            position
                .retained_source()
                .and_then(|source| composition.source_location(source))
                == Some(source)
        });
        let Some(position) = positions.next() else {
            continue;
        };
        if positions.next().is_some() {
            return Err(state_error(composition));
        }
        if let Entry::Vacant(entry) = bodies.entry(target) {
            let (start, body) = body(composition, encoded, target)?;
            entry.insert((start, Arc::new(body)));
        }
        let (start, body) = bodies
            .get(&target)
            .ok_or_else(|| state_error(composition))?;
        let relative = position
            .semantic_word()
            .checked_sub(*start)
            .filter(|relative| *relative < body.words.len())
            .ok_or_else(|| state_error(composition))?;
        witnesses.push(Witness {
            position: TeletextAttributeTransmissionPosition {
                event_ordinal,
                source: event.source(),
                invocation: event.invocation(),
                // Only same-instance order is used provisionally. Final packing
                // must retain this exact body/mapping before output can succeed.
                word: relative,
            },
            bytes: triplet(encoded, position.semantic_word())
                .ok_or_else(|| state_error(composition))?,
            local: Some(Arc::clone(body)),
        });
    }
    Ok(witnesses)
}

fn root_in_prefix(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
    root: TeletextSpatialRootSource,
) -> bool {
    let TeletextSpatialRootSource::Local { command } = root else {
        return false;
    };
    encoded.command_positions().iter().any(|position| {
        position.source().object.is_none()
            && composition.source_location(position.source())
                == Some(TeletextMainCompositionSource::OriginalPage { command })
            && matches!(
                composition
                    .program()
                    .commands()
                    .get(position.source().command),
                Some(TeletextEnhancementCommand::InvokeObject { .. })
            )
    })
}

fn body(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
    target: TeletextObjectIdentity,
) -> Result<(usize, Body), TeletextMainCompositionError> {
    let mut definitions = encoded
        .definition_positions()
        .iter()
        .enumerate()
        .filter(|(_, definition)| definition.target() == target);
    let (index, definition) = definitions.next().ok_or_else(|| state_error(composition))?;
    if definitions.next().is_some() {
        return Err(state_error(composition));
    }
    let start = usize::from(definition.absolute_index()) + 1;
    let mut end = start;
    for position in encoded
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(target))
    {
        if position.first_word() != end || position.semantic_word() < end {
            return Err(state_error(composition));
        }
        end = position.semantic_word() + 1;
    }
    let boundary = if let Some(next) = encoded.definition_positions().get(index + 1) {
        if end != usize::from(next.absolute_index()) {
            return Err(state_error(composition));
        }
        Boundary::Following(next.target(), next.object_type())
    } else {
        Boundary::Final(triplet(encoded, end).ok_or_else(|| state_error(composition))?)
    };
    // Both indices come from the existing at-most16-packet native packing.
    // Markers can relocate; every body and compatibility word remains exact.
    let words = (start..end)
        .map(|word| triplet(encoded, word).ok_or_else(|| state_error(composition)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((
        start,
        Body {
            target,
            object_type: definition.object_type(),
            words,
            boundary,
        },
    ))
}

//! Eligibility and retained lineage only; callers supply actual native words.
use crate::teletext::{
    TeletextAugmentedOriginProgram, TeletextAugmentedOriginSource, TeletextMainComposition,
    TeletextMainCompositionSource,
};
use ersatzrs_domain::{
    TeletextEffectiveProgramSource, TeletextEnhancementCommand, TeletextEnhancementObjectCatalog,
    TeletextEnhancementProgram, TeletextEnhancementSource, TeletextObjectIdentity,
    TeletextObjectScope, TeletextSemanticService, TeletextSpatialRootSource, TeletextSpatialTrace,
};

use std::collections::BTreeMap;

type EventKey = (Option<TeletextObjectIdentity>, usize, usize);
type OriginalIndex = BTreeMap<EventKey, Option<TeletextEnhancementCommand>>;

pub(in crate::teletext) fn eligible_events(trace: &TeletextSpatialTrace) -> Vec<bool> {
    if !matches!(
        trace.original_program_source(),
        TeletextEffectiveProgramSource::Local
    ) {
        return vec![false; trace.events().len()];
    }
    let mut counts = BTreeMap::new();
    for call in trace.invocations() {
        if let TeletextSpatialRootSource::Local { command } = call.root_source() {
            *counts.entry((call.target(), command)).or_insert(0_usize) += 1;
        }
    }
    trace
        .events()
        .iter()
        .map(|event| {
            let TeletextSpatialRootSource::Local { command } = event.root_source() else {
                return false;
            };
            match (event.source().object, event.invocation()) {
                (None, None) => true,
                (Some(target), Some(id)) => {
                    target.scope() == TeletextObjectScope::Local
                        && target.owner() == Some(trace.owner())
                        && trace.invocations().get(id.get()).is_some_and(|call| {
                            call.id() == id
                                && call.target() == target
                                && call.root_source() == event.root_source()
                        })
                        && counts.get(&(target, command)) == Some(&1)
                }
                _ => false,
            }
        })
        .collect()
}

fn original_index(trace: &TeletextSpatialTrace) -> OriginalIndex {
    let eligible = eligible_events(trace);
    let mut result = BTreeMap::new();
    for (index, event) in trace.events().iter().enumerate() {
        if !eligible[index] {
            continue;
        }
        let TeletextSpatialRootSource::Local { command } = event.root_source() else {
            continue;
        };
        result
            .entry((event.source().object, event.source().command, command))
            .and_modify(|previous| *previous = None)
            .or_insert(Some(event.command()));
    }
    result
}

pub(super) fn original_evidence(
    trace: &TeletextSpatialTrace,
    positions: &[ersatzrs_domain::TeletextAttributeTransmissionPosition],
    maximum: usize,
) -> Result<
    ersatzrs_domain::TeletextAttributeTransmissionEvidence,
    ersatzrs_domain::TeletextAttributeTransmissionError,
> {
    let eligible = eligible_events(trace);
    let page_positions = positions
        .iter()
        .filter(|position| eligible.get(position.event_ordinal) == Some(&true))
        .copied()
        .collect::<Vec<_>>();
    ersatzrs_domain::TeletextAttributeTransmissionEvidence::new_with_same_page_order(
        trace,
        positions,
        &page_positions,
        maximum,
    )
}

pub(super) struct PhysicalSource<'a, 'b> {
    pub service: &'a TeletextSemanticService,
    pub composition: &'a TeletextMainComposition,
    pub program: (
        &'a TeletextEnhancementProgram,
        &'a TeletextEnhancementObjectCatalog,
    ),
    pub augmented: Option<&'a TeletextAugmentedOriginProgram<'b>>,
}

pub(super) fn physical_evidence(
    source: &PhysicalSource<'_, '_>,
    trace: &TeletextSpatialTrace,
    positions: &[ersatzrs_domain::TeletextAttributeTransmissionPosition],
    maximum: usize,
) -> Result<
    ersatzrs_domain::TeletextAttributeTransmissionEvidence,
    ersatzrs_domain::TeletextAttributeTransmissionError,
> {
    let eligible = eligible_events(trace);
    let originals = original_index(source.composition.spatial_trace());
    let direct = source.augmented.is_none()
        && source.program.0 == source.composition.program()
        && source.program.1 == source.service.objects();
    let page_positions = positions
        .iter()
        .filter(|position| {
            retained_physical(
                source,
                trace,
                position.event_ordinal,
                &eligible,
                &originals,
                direct,
            )
        })
        .copied()
        .collect::<Vec<_>>();
    ersatzrs_domain::TeletextAttributeTransmissionEvidence::new_with_same_page_order(
        trace,
        positions,
        &page_positions,
        maximum,
    )
}

fn retained_physical(
    source: &PhysicalSource<'_, '_>,
    trace: &TeletextSpatialTrace,
    ordinal: usize,
    eligible: &[bool],
    originals: &OriginalIndex,
    direct: bool,
) -> bool {
    let composition = source.composition;
    if eligible.get(ordinal) != Some(&true)
        || !matches!(
            composition.spatial_trace().original_program_source(),
            TeletextEffectiveProgramSource::Local
        )
    {
        return false;
    }
    let Some(event) = trace.events().get(ordinal) else {
        return false;
    };
    let TeletextSpatialRootSource::Local { command } = event.root_source() else {
        return false;
    };
    let map = |location| retained_source(composition, source.augmented, location, direct);
    let Some(root) = map(TeletextEnhancementSource {
        object: None,
        command,
    }) else {
        return false;
    };
    let Some(source) = map(event.source()) else {
        return false;
    };
    if root.object.is_some() {
        return false;
    }
    originals.get(&(source.object, source.command, root.command)) == Some(&Some(event.command()))
}

fn retained_source(
    composition: &TeletextMainComposition,
    augmented: Option<&TeletextAugmentedOriginProgram<'_>>,
    source: TeletextEnhancementSource,
    direct: bool,
) -> Option<TeletextEnhancementSource> {
    let original = if let Some(augmented) = augmented {
        match augmented.emitter_source(source)? {
            TeletextAugmentedOriginSource::RetainedMain {
                provenance: TeletextMainCompositionSource::OriginalPage { command },
                ..
            } => TeletextEnhancementSource {
                object: None,
                command,
            },
            TeletextAugmentedOriginSource::RetainedObject { identity, command } => {
                TeletextEnhancementSource {
                    object: Some(identity),
                    command,
                }
            }
            _ => return None,
        }
    } else {
        if !direct {
            return None;
        }
        match source.object {
            None => match composition.source_location(source)? {
                TeletextMainCompositionSource::OriginalPage { command } => {
                    TeletextEnhancementSource {
                        object: None,
                        command,
                    }
                }
                _ => return None,
            },
            Some(_) => source,
        }
    };
    if original.object.is_some_and(|target| {
        target.scope() != TeletextObjectScope::Local || target.owner() != Some(composition.owner())
    }) {
        return None;
    }
    Some(original)
}

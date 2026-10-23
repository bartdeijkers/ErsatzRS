//! Same-service composition and compatibility-aware shared main/definition packing.
//!
//! The result remains unpublished: packet fit and selected receiver-context
//! lowering do not resolve final display geometry, overlap or service lifecycle.

use ersatzrs_domain::{
    TeletextAttributeTransmissionError, TeletextAttributeTransmissionEvidence,
    TeletextAttributeTransmissionPosition, TeletextEnhancementCollectionBudget,
    TeletextEnhancementObjectCatalog, TeletextEnhancementProgram, TeletextEnhancementSource,
    TeletextObjectCatalogBudget, TeletextPageAddress, TeletextResolvedObjectPageLinks,
    TeletextSemanticService,
};

use super::compile_context::TeletextCompileContext;
use super::local_object_program::{
    compile_with_context, EncodedTeletextLocalObjectProgram,
    TeletextLocalObjectProgramEncodingError, TeletextLocalObjectProgramEncodingFailure,
};
use super::main_composition::{
    compose_teletext_main_program_with_context, replay_teletext_physical_display_with_context,
    TeletextMainComposition, TeletextMainCompositionBudget, TeletextMainCompositionError,
    TeletextMainCompositionFailure, TeletextMainCompositionSource, TeletextPhysicalDisplayReplay,
    TeletextPhysicalDisplayReplayError,
};
use super::resource_transmission::{
    ResourceTransmissionContext, TeletextResourceTransmissionError,
};
use super::TeletextResourceObjectAllocations;

#[path = "same_page_transmission.rs"]
pub(super) mod same_page;

/// Packing diagnostics remapped to authored/default/base or original object intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("teletext page {owner:?}, location {location:?}: {reason}")]
pub struct TeletextComposedMainPackingError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextMainCompositionSource>,
    pub reason: TeletextLocalObjectProgramEncodingFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextComposedMainEncodingError {
    #[error(transparent)]
    Composition(#[from] TeletextMainCompositionError),
    #[error(transparent)]
    Packing(#[from] TeletextComposedMainPackingError),
    #[error(transparent)]
    Resource(#[from] TeletextResourceTransmissionError),
}

/// Immutable semantic plan and its single physical main/local-definition sequence.
/// This is not a complete display, service inventory or publication capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextComposedMainProgram {
    composition: TeletextMainComposition,
    encoded: EncodedTeletextLocalObjectProgram,
}

impl EncodedTeletextComposedMainProgram {
    #[must_use]
    pub const fn composition(&self) -> &TeletextMainComposition {
        &self.composition
    }

    #[must_use]
    pub const fn encoded(&self) -> &EncodedTeletextLocalObjectProgram {
        &self.encoded
    }

    /// Consumes the preliminary packing while retaining its settled target model.
    #[must_use]
    pub(super) fn into_composition(self) -> TeletextMainComposition {
        self.composition
    }
}

/// Compose and pack one exact owner using the same immutable service throughout.
///
/// The main's initial header selector uses its original resolved header colour.
/// Body-row restoration preserves the enhanced AP after historical row-address
/// disturbances. Main and definitions share final sizing/pointer allocation;
/// definition states remain independently reset and outside Level 1.5 local data.
/// Original default roots retain their original target identities. They are never
/// rebound to a different selected resource to make this partial compiler succeed.
///
/// # Errors
/// Returns bounded composition or provenance-mapped packing errors without partial
/// output. Forward header positions can await a real column operation; a call
/// while the authored and emitted AP differ, later row-zero resets and complex
/// header-row restoration still need semantic normalization. Final object/base priority, attribute/geometry,
/// default realization and full service/publication acceptance remain open.
pub fn compile_teletext_composed_main_program(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    allocations: &TeletextResourceObjectAllocations,
    composition_budget: TeletextMainCompositionBudget,
    packing_budget: TeletextObjectCatalogBudget,
) -> Result<EncodedTeletextComposedMainProgram, TeletextComposedMainEncodingError> {
    compile_composed_with_context(
        service,
        owner,
        allocations,
        composition_budget,
        packing_budget,
        &mut TeletextCompileContext::new(None, None, None),
    )
}

pub(super) fn compile_composed_with_context(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    allocations: &TeletextResourceObjectAllocations,
    composition_budget: TeletextMainCompositionBudget,
    packing_budget: TeletextObjectCatalogBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<EncodedTeletextComposedMainProgram, TeletextComposedMainEncodingError> {
    compile_composed_with_policy(
        service,
        owner,
        allocations,
        composition_budget,
        packing_budget,
        context,
        None,
    )
}

/// Normalization-only bootstrap. The generated typed Main must subsequently
/// pass the existing whole-model comparisons; composed-only callers cannot opt in.
pub(super) fn compile_composed_with_header_positions(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    allocations: &TeletextResourceObjectAllocations,
    composition_budget: TeletextMainCompositionBudget,
    packing: (TeletextObjectCatalogBudget, usize),
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<EncodedTeletextComposedMainProgram, TeletextComposedMainEncodingError> {
    compile_composed_with_policy(
        service,
        owner,
        allocations,
        composition_budget,
        packing.0,
        context,
        Some(packing.1),
    )
}

fn compile_composed_with_policy(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    allocations: &TeletextResourceObjectAllocations,
    composition_budget: TeletextMainCompositionBudget,
    packing_budget: TeletextObjectCatalogBudget,
    context: &mut TeletextCompileContext<'_, '_>,
    header_positions: Option<usize>,
) -> Result<EncodedTeletextComposedMainProgram, TeletextComposedMainEncodingError> {
    if let Some(resources) = context.resources {
        resources.validate_catalog(service.objects(), allocations)?;
    }
    let composition =
        compose_teletext_main_program_with_context(service, owner, composition_budget, context)?;
    let (composition, prefix) = if let Some(work) = header_positions {
        super::header_position_materialization::settlement::prepare(
            composition,
            service,
            allocations,
            composition_budget,
            (packing_budget, work),
            context,
        )?
    } else {
        (composition, Vec::new())
    };
    let encoded = if let Some(work) = header_positions {
        pack_normalized_composition_program(
            service,
            &composition,
            composition.program(),
            allocations,
            (packing_budget, work),
            context,
        )?
    } else {
        pack_composition_program(
            service,
            &composition,
            composition.program(),
            allocations,
            packing_budget,
        )?
    };
    super::header_position_materialization::settlement::verify(&composition, &encoded, &prefix)?;
    let mut transmission = transmission_evidence(
        &composition,
        &encoded,
        composition_budget.collection.max_commands,
        context.resources,
    )?;
    if context.resources_before_page {
        transmission = transmission.with_resources_before_page();
    }
    let composition = composition.with_transmission_context(
        service,
        &transmission,
        composition_budget.collection.max_commands,
        context,
    )?;
    Ok(EncodedTeletextComposedMainProgram {
        composition,
        encoded,
    })
}

/// Identical packing policy for a lowerable Main prefix and the final Main.
/// Prefix-packed definitions never supply transmission authority.
pub(super) fn pack_composition_program(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    program: &TeletextEnhancementProgram,
    allocations: &TeletextResourceObjectAllocations,
    packing_budget: TeletextObjectCatalogBudget,
) -> Result<EncodedTeletextLocalObjectProgram, TeletextComposedMainPackingError> {
    pack_composition_catalog(
        service,
        composition,
        program,
        allocations,
        packing_budget,
        service.objects(),
        Some,
    )
}

/// Original-only normalization bootstrap. Keep all actual word ranges, but give
/// inserted writers no original transmission or provisional settlement authority.
pub(super) fn pack_normalized_composition_program(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    program: &TeletextEnhancementProgram,
    allocations: &TeletextResourceObjectAllocations,
    packing: (TeletextObjectCatalogBudget, usize),
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<EncodedTeletextLocalObjectProgram, TeletextComposedMainEncodingError> {
    let derived = context.object_headers(composition, service.objects(), packing.0, packing.1)?;
    let mut encoded = pack_composition_catalog(
        service,
        composition,
        program,
        allocations,
        packing.0,
        derived.catalog(),
        |source| derived.retained_source(source),
    )?;
    encoded.bind_retained_object_sources(|source| derived.retained_source(source));
    Ok(encoded)
}

fn pack_composition_catalog(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    program: &TeletextEnhancementProgram,
    allocations: &TeletextResourceObjectAllocations,
    packing_budget: TeletextObjectCatalogBudget,
    catalog: &TeletextEnhancementObjectCatalog,
    retained: impl Fn(TeletextEnhancementSource) -> Option<TeletextEnhancementSource>,
) -> Result<EncodedTeletextLocalObjectProgram, TeletextComposedMainPackingError> {
    let owner = composition.owner();
    let links = service.page_enhancements().get(&owner).map_or(
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        |page| page.resources.objects,
    );
    compile_with_context(
        owner,
        program,
        catalog,
        allocations,
        links,
        packing_budget,
        Some(composition.colour_context().row_colours()[0]),
    )
    .map_err(|error| TeletextComposedMainPackingError {
        owner: error.owner,
        location: error
            .location
            .and_then(retained)
            .and_then(|location| composition.source_location(location)),
        reason: error.reason,
    })
}

fn transmission_evidence(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
    maximum: usize,
    resources: Option<&ResourceTransmissionContext<'_>>,
) -> Result<TeletextAttributeTransmissionEvidence, TeletextMainCompositionError> {
    let original = composition.spatial_trace();
    let fail = |reason| TeletextMainCompositionError {
        owner: composition.owner(),
        location: None,
        reason,
    };
    // Both inputs have already passed their exact trusted/physical bounds. Fence
    // again before building the provenance index: no producer-sized allocation.
    if original.events().len() > maximum {
        return Err(fail(TeletextMainCompositionFailure::InputBudget {
            maximum,
        }));
    }
    if original
        .events()
        .len()
        .checked_mul(encoded.command_positions().len())
        .is_none()
        || original
            .events()
            .len()
            .checked_mul(std::mem::size_of::<TeletextAttributeTransmissionPosition>())
            .is_none_or(|bytes| bytes > isize::MAX as usize)
    {
        return Err(fail(
            ersatzrs_domain::TeletextAttributeTransmissionError::AllocationSize.into(),
        ));
    }
    let mut positions = Vec::new();
    for (event_ordinal, event) in original.events().iter().enumerate() {
        if let Some(resources) = resources.filter(|_| {
            event
                .source()
                .object
                .is_some_and(|target| target.resource().is_some())
        }) {
            if let Some(word) =
                resources
                    .word(original, *event, event_ordinal)
                    .map_err(|reason| TeletextMainCompositionError {
                        owner: composition.owner(),
                        location: event.source().object.map(|identity| {
                            TeletextMainCompositionSource::Object {
                                identity,
                                command: event.source().command,
                            }
                        }),
                        reason: reason.into(),
                    })?
            {
                positions.push(TeletextAttributeTransmissionPosition {
                    event_ordinal,
                    source: event.source(),
                    invocation: event.invocation(),
                    word,
                });
            }
            continue;
        }
        let expected = if let Some(identity) = event.source().object {
            TeletextMainCompositionSource::Object {
                identity,
                command: event.source().command,
            }
        } else {
            match original.original_program_source() {
                ersatzrs_domain::TeletextEffectiveProgramSource::Local => {
                    TeletextMainCompositionSource::OriginalPage {
                        command: event.source().command,
                    }
                }
                ersatzrs_domain::TeletextEffectiveProgramSource::Defaults { slots } => {
                    let Some(&slot) = slots.get(event.source().command) else {
                        return Err(fail(TeletextMainCompositionFailure::SelectionState));
                    };
                    TeletextMainCompositionSource::DefaultSlot { slot }
                }
                ersatzrs_domain::TeletextEffectiveProgramSource::BaseOnly => continue,
            }
        };
        // Only the original command's final emitted semantic word is evidence.
        // Synthetic BaseCell commands and unmapped resource bodies stay absent.
        let mut matches = encoded.command_positions().iter().filter(|position| {
            position
                .retained_source()
                .and_then(|source| composition.source_location(source))
                == Some(expected)
        });
        if let Some(position) = matches.next() {
            if matches.next().is_some() {
                return Err(fail(TeletextMainCompositionFailure::SelectionState));
            }
            positions.push(TeletextAttributeTransmissionPosition {
                event_ordinal,
                source: event.source(),
                invocation: event.invocation(),
                word: position.semantic_word(),
            });
        }
    }
    same_page::original_evidence(original, &positions, maximum)
        .map_err(|reason| fail(reason.into()))
}

#[cfg(test)]
#[path = "physical_order_tests.rs"]
mod physical_order_tests;

/// Final-word diagnostics retain actual supplied main/object ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("physical teletext display {owner:?} at {location:?}: {reason}")]
pub struct TeletextPhysicalDisplayTransmissionError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextEnhancementSource>,
    pub reason: TeletextAttributeTransmissionError,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPhysicalDisplayEncodingError {
    #[error(transparent)]
    PhysicalReplay(#[from] TeletextPhysicalDisplayReplayError),
    #[error(transparent)]
    Packing(#[from] TeletextLocalObjectProgramEncodingError),
    #[error(transparent)]
    Transmission(#[from] TeletextPhysicalDisplayTransmissionError),
    #[error(transparent)]
    Resource(#[from] TeletextResourceTransmissionError),
}

/// Actual supplied-program replay bound internally to its final packed words.
/// This diagnostic is not original-target equivalence or publication authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextPhysicalDisplayProgram {
    replay: TeletextPhysicalDisplayReplay,
    encoded: EncodedTeletextLocalObjectProgram,
}

impl EncodedTeletextPhysicalDisplayProgram {
    pub(super) fn bind_original_object_sources(
        &mut self,
        original: impl FnMut(TeletextEnhancementSource) -> Option<TeletextMainCompositionSource>,
    ) {
        self.encoded.bind_original_object_sources(original);
    }

    pub(super) fn bind_original_main_sources(
        &mut self,
        original: impl FnMut(TeletextEnhancementSource) -> Option<super::TeletextMainCompositionSource>,
    ) {
        self.encoded.bind_original_main_sources(original);
    }

    #[must_use]
    pub const fn replay(&self) -> &TeletextPhysicalDisplayReplay {
        &self.replay
    }

    #[must_use]
    pub const fn encoded(&self) -> &EncodedTeletextLocalObjectProgram {
        &self.encoded
    }
}

/// Replay and pack exactly the supplied physical program and object catalogue.
///
/// Reuses the original composition's Base and colour planes without composing or
/// inserting fallback again. Original service links and header colour remain
/// authoritative. Physical evidence comes only from this encoder's actual final
/// command positions, including inserted compatibility selectors. Unmapped
/// resource bodies gain no cross-page timing authority. Only exact retained Local
/// sources can opt into authenticated same-page transmission order.
/// Matching owners do not certify the composition/service generation pairing.
/// # Errors
/// Returns actual-source replay, packing or transmission diagnostics. Default
/// owners are not rebound or exempted from selected-link lowering. This does not
/// normalize the program, prove target equivalence or grant receiver readiness.
pub fn compile_teletext_physical_display_program(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    actual_program: &TeletextEnhancementProgram,
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
    replay_budget: TeletextEnhancementCollectionBudget,
    packing_budget: TeletextObjectCatalogBudget,
) -> Result<EncodedTeletextPhysicalDisplayProgram, TeletextPhysicalDisplayEncodingError> {
    compile_physical_with_context(
        service,
        composition,
        (actual_program, catalog),
        allocations,
        (replay_budget, packing_budget),
        None,
        &mut TeletextCompileContext::new(None, None, None),
    )
}

pub(super) fn compile_physical_with_context(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    program: (
        &TeletextEnhancementProgram,
        &TeletextEnhancementObjectCatalog,
    ),
    allocations: &TeletextResourceObjectAllocations,
    budgets: (
        TeletextEnhancementCollectionBudget,
        TeletextObjectCatalogBudget,
    ),
    augmented: Option<&super::TeletextAugmentedOriginProgram<'_>>,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<EncodedTeletextPhysicalDisplayProgram, TeletextPhysicalDisplayEncodingError> {
    let (replay_budget, packing_budget) = budgets;
    let (actual_program, catalog) = program;
    if let Some(resources) = context.resources {
        resources.validate_catalog(catalog, allocations)?;
    }
    let owner = composition.owner();
    let replay = replay_teletext_physical_display_with_context(
        service,
        composition,
        actual_program,
        catalog,
        replay_budget,
        context,
    )?;
    let links = service.page_enhancements().get(&owner).map_or(
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        |page| page.resources.objects,
    );
    let encoded = compile_with_context(
        owner,
        actual_program,
        catalog,
        allocations,
        links,
        packing_budget,
        Some(composition.colour_context().row_colours()[0]),
    )?;
    let mut evidence = physical_transmission_evidence(
        &replay,
        &encoded,
        replay_budget.max_commands,
        context.resources,
        &same_page::PhysicalSource {
            service,
            composition,
            program,
            augmented,
        },
    )?;
    if context.resources_before_page {
        evidence = evidence.with_resources_before_page();
    }
    let replay = replay.with_transmission_context(
        service,
        composition,
        &evidence,
        replay_budget.max_commands,
        context,
    )?;
    Ok(EncodedTeletextPhysicalDisplayProgram { replay, encoded })
}

fn physical_transmission_evidence(
    replay: &TeletextPhysicalDisplayReplay,
    encoded: &EncodedTeletextLocalObjectProgram,
    maximum: usize,
    resources: Option<&ResourceTransmissionContext<'_>>,
    retained: &same_page::PhysicalSource<'_, '_>,
) -> Result<TeletextAttributeTransmissionEvidence, TeletextPhysicalDisplayTransmissionError> {
    let trace = replay.spatial_trace();
    let fail = |reason| TeletextPhysicalDisplayTransmissionError {
        owner: replay.owner(),
        location: match reason {
            TeletextAttributeTransmissionError::Correspondence { event } => {
                trace.events().get(event).map(|event| event.source())
            }
            _ => None,
        },
        reason,
    };
    if trace.events().len() > maximum {
        return Err(fail(TeletextAttributeTransmissionError::Budget { maximum }));
    }
    if trace
        .events()
        .len()
        .checked_mul(encoded.command_positions().len())
        .is_none()
        || trace
            .events()
            .len()
            .checked_mul(std::mem::size_of::<TeletextAttributeTransmissionPosition>())
            .is_none_or(|bytes| isize::try_from(bytes).is_err())
    {
        return Err(fail(TeletextAttributeTransmissionError::AllocationSize));
    }
    // Final packing has bounded the word table to the shared 208-word sequence.
    // Each expanded event maps only to its own exact supplied source; repetitions
    // retain distinct invocation IDs while sharing the physical definition word.
    let mut positions = Vec::with_capacity(trace.events().len());
    for (event_ordinal, event) in trace.events().iter().enumerate() {
        if let Some(resources) = resources.filter(|_| {
            event
                .source()
                .object
                .is_some_and(|target| target.resource().is_some())
        }) {
            if let Some(word) = resources
                .word(trace, *event, event_ordinal)
                .map_err(&fail)?
            {
                positions.push(TeletextAttributeTransmissionPosition {
                    event_ordinal,
                    source: event.source(),
                    invocation: event.invocation(),
                    word,
                });
            }
            continue;
        }
        let mut matches = encoded
            .command_positions()
            .iter()
            .filter(|position| position.source() == event.source());
        if let Some(position) = matches.next() {
            if matches.next().is_some() {
                return Err(fail(TeletextAttributeTransmissionError::Correspondence {
                    event: event_ordinal,
                }));
            }
            positions.push(TeletextAttributeTransmissionPosition {
                event_ordinal,
                source: event.source(),
                invocation: event.invocation(),
                word: position.semantic_word(),
            });
        }
    }
    same_page::physical_evidence(retained, trace, &positions, maximum).map_err(fail)
}

#[cfg(test)]
#[path = "physical_packing_tests.rs"]
mod physical_packing_tests;

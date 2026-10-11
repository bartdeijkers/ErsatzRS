//! Same-service composition and compatibility-aware shared main/definition packing.
//!
//! The result remains unpublished: packet fit and selected receiver-context
//! lowering do not resolve final display geometry, overlap or service lifecycle.

use ersatzrs_domain::{
    TeletextAttributeTransmissionEvidence, TeletextAttributeTransmissionPosition,
    TeletextObjectCatalogBudget, TeletextPageAddress, TeletextResolvedObjectPageLinks,
    TeletextSemanticService,
};

use super::local_object_program::{
    compile_with_context, EncodedTeletextLocalObjectProgram,
    TeletextLocalObjectProgramEncodingFailure,
};
use super::main_composition::{
    compose_teletext_main_program, TeletextMainComposition, TeletextMainCompositionBudget,
    TeletextMainCompositionError, TeletextMainCompositionFailure, TeletextMainCompositionSource,
};
use super::TeletextResourceObjectAllocations;

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
/// output. Explicit row-zero positions and complex header-row restoration still
/// need semantic normalization. Final object/base priority, attribute/geometry,
/// default realization and full service/publication acceptance remain open.
pub fn compile_teletext_composed_main_program(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    allocations: &TeletextResourceObjectAllocations,
    composition_budget: TeletextMainCompositionBudget,
    packing_budget: TeletextObjectCatalogBudget,
) -> Result<EncodedTeletextComposedMainProgram, TeletextComposedMainEncodingError> {
    let composition = compose_teletext_main_program(service, owner, composition_budget)?;
    let links = service.page_enhancements().get(&owner).map_or(
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        |page| page.resources.objects,
    );
    let encoded = compile_with_context(
        owner,
        composition.program(),
        service.objects(),
        allocations,
        links,
        packing_budget,
        Some(composition.colour_context().row_colours()[0]),
    )
    .map_err(|error| TeletextComposedMainPackingError {
        owner: error.owner,
        location: error
            .location
            .and_then(|location| composition.source_location(location)),
        reason: error.reason,
    })?;
    let transmission = transmission_evidence(
        &composition,
        &encoded,
        composition_budget.collection.max_commands,
    )?;
    let composition = composition.with_transmission(
        service,
        &transmission,
        composition_budget.collection.max_commands,
    )?;
    Ok(EncodedTeletextComposedMainProgram {
        composition,
        encoded,
    })
}

fn transmission_evidence(
    composition: &TeletextMainComposition,
    encoded: &EncodedTeletextLocalObjectProgram,
    maximum: usize,
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
        let mut matches = encoded
            .command_positions()
            .iter()
            .filter(|position| composition.source_location(position.source()) == Some(expected));
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
    TeletextAttributeTransmissionEvidence::new(original, &positions, maximum)
        .map_err(|reason| fail(reason.into()))
}

#[cfg(test)]
#[path = "physical_order_tests.rs"]
mod physical_order_tests;

//! Owned engine normalization candidate; not whole-service admission or publication.
use super::augmented_origin_compilation::compile_augmented_with_resources;
use super::composed_main::compile_composed_with_resources;
use super::resource_transmission::ResourceTransmissionContext;
use super::{
    allocate_teletext_resource_objects, build_teletext_augmented_origin_program,
    compare_teletext_augmented_origin_glyphs, compare_teletext_physical_display,
    plan_teletext_base_compatibility_realization, plan_teletext_origin_carrier_placement,
    EncodedTeletextAugmentedOriginProgram, EncodedTeletextLocalObjectProgram,
    EncodedTeletextPhysicalDisplayProgram, TeletextAugmentedGlyphComparisonBudget,
    TeletextAugmentedGlyphComparisonError, TeletextAugmentedGlyphDiagnostic,
    TeletextAugmentedInvocationBudget, TeletextAugmentedOriginCompilationError,
    TeletextAugmentedOriginProgramBudget, TeletextAugmentedOriginProgramError,
    TeletextBaseCompatibilityRealizationBudget, TeletextBaseCompatibilityRealizationError,
    TeletextComposedMainEncodingError, TeletextMainComposition, TeletextMainCompositionBudget,
    TeletextObjectAllocationError, TeletextOriginCarrierPlacementBudget,
    TeletextOriginCarrierPlacementError, TeletextPhysicalComparisonBudget,
    TeletextPhysicalComparisonDiagnostic, TeletextPhysicalComparisonError,
};
use ersatzrs_domain::{
    TeletextDefaultObjectSelection, TeletextEnhancementCollectionBudget,
    TeletextObjectCatalogBudget, TeletextPageAddress, TeletextSemanticService,
};

/// Trusted per-stage bounds. Original and actual packing each use `packing`;
/// assembly's recipe/ID budgets explicitly include its two derivation passes.
/// These are bounded phase operations, not an aggregate CPU-time guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextNormalizedMainProgramBudget {
    pub composition: TeletextMainCompositionBudget,
    pub realization: TeletextBaseCompatibilityRealizationBudget,
    pub placement: TeletextOriginCarrierPlacementBudget,
    pub assembly: TeletextAugmentedOriginProgramBudget,
    pub packing: TeletextObjectCatalogBudget,
    pub replay: TeletextEnhancementCollectionBudget,
    pub ancestry: TeletextAugmentedInvocationBudget,
    pub glyph_comparison: TeletextAugmentedGlyphComparisonBudget,
    pub display_comparison: TeletextPhysicalComparisonBudget,
    /// Applied separately to original and augmented allocation input counts.
    pub max_resource_objects: usize,
}
/// Original settled target and checked physical candidate from the same call.
/// No borrowed plan or detached caller map survives. Pending histories remain
/// accessible on each owned model; these getters grant no readiness authority.
#[derive(Debug)]
pub struct EncodedTeletextNormalizedMainProgram {
    original: TeletextMainComposition,
    physical: EncodedTeletextPhysicalDisplayProgram,
}
impl EncodedTeletextNormalizedMainProgram {
    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.original.owner()
    }
    #[must_use]
    pub const fn original(&self) -> &TeletextMainComposition {
        &self.original
    }
    #[must_use]
    pub const fn physical(&self) -> &EncodedTeletextPhysicalDisplayProgram {
        &self.physical
    }
    #[must_use]
    pub const fn encoded(&self) -> &EncodedTeletextLocalObjectProgram {
        self.physical.encoded()
    }
    /// Consumes this result into its owned physical diagnostic, not a capability.
    #[must_use]
    pub fn into_physical(self) -> EncodedTeletextPhysicalDisplayProgram {
        self.physical
    }
}
#[derive(Debug, thiserror::Error)]
pub enum TeletextNormalizedMainProgramFailure {
    #[error("resource allocation failed: {0}")]
    Allocation(Box<TeletextObjectAllocationError>),
    #[error("original composition/packing failed: {0}")]
    Original(Box<TeletextComposedMainEncodingError>),
    #[error("compatibility planning failed: {0}")]
    Realization(Box<TeletextBaseCompatibilityRealizationError>),
    #[error("carrier placement failed: {0}")]
    Placement(Box<TeletextOriginCarrierPlacementError>),
    #[error("candidate assembly failed: {0}")]
    Assembly(Box<TeletextAugmentedOriginProgramError>),
    #[error("paired physical compilation failed: {0}")]
    Compilation(Box<TeletextAugmentedOriginCompilationError>),
    #[error("typed glyph comparison failed: {0}")]
    GlyphComparison(Box<TeletextAugmentedGlyphComparisonError>),
    #[error("full selected-model comparison failed: {0}")]
    DisplayComparison(Box<TeletextPhysicalComparisonError>),
    #[error("typed glyph correspondence remains unresolved: {0:?}")]
    GlyphMismatch(Box<TeletextAugmentedGlyphDiagnostic>),
    #[error("selected-model correspondence remains unresolved: {0:?}")]
    DisplayMismatch(Box<TeletextPhysicalComparisonDiagnostic>),
}
#[derive(Debug, thiserror::Error)]
#[error("teletext normalization candidate for {owner:?}: {reason}")]
pub struct TeletextNormalizedMainProgramError {
    pub owner: TeletextPageAddress,
    /// Wrapped diagnostics retain their exact original/actual source spaces.
    pub reason: TeletextNormalizedMainProgramFailure,
}

/// Compose, settle original C2, realize compatibility collisions, pack and check.
/// Both packing passes use this exact immutable service. Original Base fallback,
/// presentation and selected resource/default identities are preserved. Foreign
/// default owners receive no selected-link exemption or Local cloning workaround.
/// Comparisons reject current/retained relevant uncertainty, not the mere presence
/// of a superseded append-only pending history. No-seed pages still run both checks.
/// Ordinary encoding is unchanged; callers deliberately select this engine seam.
/// # Errors
/// Returns located existing stage diagnostics or the first owned finding from a
/// completed bounded comparison. No partial wire candidate escapes on failure.
/// This is not full standards/client/editor/lifecycle or publication acceptance.
pub fn compile_teletext_normalized_main_program(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    budget: TeletextNormalizedMainProgramBudget,
) -> Result<EncodedTeletextNormalizedMainProgram, TeletextNormalizedMainProgramError> {
    compile_teletext_normalized_main_program_with_defaults(service, owner, &[], &budget)
}

/// Same-service MOT defaults reserve identical resource pointers in both passes.
/// The raw list is bounded by the existing allocator before validation/scanning.
/// Callers inside this module derive this list from their immutable service
/// declarations; it is not a detached external allocation or source authority.
pub(super) fn compile_teletext_normalized_main_program_with_defaults(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
) -> Result<EncodedTeletextNormalizedMainProgram, TeletextNormalizedMainProgramError> {
    compile_normalized_with_resources(service, owner, defaults, budget, None)
}

pub(super) fn compile_normalized_with_resources(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    resources: Option<&ResourceTransmissionContext<'_>>,
) -> Result<EncodedTeletextNormalizedMainProgram, TeletextNormalizedMainProgramError> {
    let fail = |reason| TeletextNormalizedMainProgramError { owner, reason };
    let original_allocations = allocate_teletext_resource_objects(
        service.objects(),
        defaults,
        budget.max_resource_objects,
    )
    .map_err(|error| {
        fail(TeletextNormalizedMainProgramFailure::Allocation(Box::new(
            error,
        )))
    })?;
    let original = compile_composed_with_resources(
        service,
        owner,
        &original_allocations,
        budget.composition,
        budget.packing,
        resources,
    )
    .map_err(|error| {
        fail(TeletextNormalizedMainProgramFailure::Original(Box::new(
            error,
        )))
    })?
    .into_composition();
    let physical = {
        let plan = plan_teletext_base_compatibility_realization(&original, budget.realization)
            .map_err(|error| {
                fail(TeletextNormalizedMainProgramFailure::Realization(Box::new(
                    error,
                )))
            })?;
        let placement =
            plan_teletext_origin_carrier_placement(&plan, service.objects(), budget.placement)
                .map_err(|error| {
                    fail(TeletextNormalizedMainProgramFailure::Placement(Box::new(
                        error,
                    )))
                })?;
        let augmented = build_teletext_augmented_origin_program(&placement, budget.assembly)
            .map_err(|error| {
                fail(TeletextNormalizedMainProgramFailure::Assembly(Box::new(
                    error,
                )))
            })?;
        let allocations = allocate_teletext_resource_objects(
            augmented.catalog(),
            defaults,
            budget.max_resource_objects,
        )
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::Allocation(Box::new(
                error,
            )))
        })?;
        let compiled = compile_augmented_with_resources(
            service,
            &augmented,
            &allocations,
            budget.replay,
            budget.packing,
            budget.ancestry,
            resources,
        )
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::Compilation(Box::new(
                error,
            )))
        })?;
        check_candidate(&compiled, budget)?;
        compiled.into_physical()
    };
    Ok(EncodedTeletextNormalizedMainProgram { original, physical })
}
fn check_candidate(
    compiled: &EncodedTeletextAugmentedOriginProgram<'_>,
    budget: &TeletextNormalizedMainProgramBudget,
) -> Result<(), TeletextNormalizedMainProgramError> {
    let original = compiled.augmented().placement().plan().composition();
    let fail = |reason| TeletextNormalizedMainProgramError {
        owner: original.owner(),
        reason,
    };
    let glyphs = compare_teletext_augmented_origin_glyphs(compiled, budget.glyph_comparison)
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::GlyphComparison(
                Box::new(error),
            ))
        })?;
    if let Some(diagnostic) = glyphs.diagnostics().first() {
        return Err(fail(TeletextNormalizedMainProgramFailure::GlyphMismatch(
            Box::new(*diagnostic),
        )));
    }
    let display = compare_teletext_physical_display(
        original,
        compiled.physical().replay(),
        budget.display_comparison,
    )
    .map_err(|error| {
        fail(TeletextNormalizedMainProgramFailure::DisplayComparison(
            Box::new(error),
        ))
    })?;
    if let Some(diagnostic) = display.diagnostics().first() {
        return Err(fail(TeletextNormalizedMainProgramFailure::DisplayMismatch(
            Box::new(diagnostic.clone()),
        )));
    }
    Ok(())
}

//! Owned engine normalization candidate; not whole-service admission or publication.
use super::compile_context::TeletextCompileContext;
use super::composed_main::compile_composed_with_header_positions;
#[cfg(test)]
use super::resource_transmission::ResourceTransmissionContext;
use super::{
    allocate_teletext_resource_objects, EncodedTeletextAugmentedOriginProgram,
    EncodedTeletextLocalObjectProgram, EncodedTeletextPhysicalDisplayProgram,
    TeletextAugmentedGlyphComparisonBudget, TeletextAugmentedGlyphComparisonError,
    TeletextAugmentedGlyphDiagnostic, TeletextAugmentedInvocationBudget,
    TeletextAugmentedOriginCompilationError, TeletextAugmentedOriginProgramBudget,
    TeletextAugmentedOriginProgramError, TeletextBaseCompatibilityRealizationBudget,
    TeletextBaseCompatibilityRealizationError, TeletextComposedMainEncodingError,
    TeletextMainComposition, TeletextMainCompositionBudget, TeletextObjectAllocationError,
    TeletextOriginCarrierPlacementBudget, TeletextOriginCarrierPlacementError,
    TeletextPhysicalComparisonBudget, TeletextPhysicalComparisonDiagnostic,
    TeletextPhysicalComparisonError,
};
use ersatzrs_domain::{
    TeletextDefaultObjectSelection, TeletextEnhancementCollectionBudget,
    TeletextObjectCatalogBudget, TeletextObjectScope, TeletextPageAddress, TeletextSemanticService,
};
use std::time::Instant;

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
    #[error("normalization exceeded its original deadline")]
    TimedOut,
    #[error("resource allocation failed: {0}")]
    Allocation(Box<TeletextObjectAllocationError>),
    #[error("original composition/packing failed: {0}")]
    Original(Box<TeletextComposedMainEncodingError>),
    #[error("definition header-position derivation failed: {0}")]
    ObjectHeader(Box<super::TeletextMainCompositionError>),
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
    compile_normalized_with_context(
        service,
        owner,
        defaults,
        budget,
        &mut TeletextCompileContext::new(None, None, None),
    )
}

/// Cooperatively check the original deadline between bounded normalization stages.
/// An executing stage is not preempted; no deadline is renewed and no output is
/// returned after observed expiry. `None` preserves the standalone encoder path.
#[cfg(test)]
pub(super) fn compile_normalized_with_resources_before(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    resources: Option<&ResourceTransmissionContext<'_>>,
    deadline: Option<Instant>,
) -> Result<EncodedTeletextNormalizedMainProgram, TeletextNormalizedMainProgramError> {
    compile_normalized_with_context(
        service,
        owner,
        defaults,
        budget,
        &mut TeletextCompileContext::new(resources, deadline, None),
    )
}

pub(super) fn compile_normalized_with_context(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<EncodedTeletextNormalizedMainProgram, TeletextNormalizedMainProgramError> {
    let original = compose_normalization_original(service, owner, defaults, budget, context)?;
    compile_normalized_original_with_context(service, original, defaults, budget, context)
}

/// Complete-service compilation may supplement physical DRCS resources only
/// after settling this unchanged authored target against its actual resources.
pub(super) fn compose_normalization_original(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<TeletextMainComposition, TeletextNormalizedMainProgramError> {
    let fail = |reason| TeletextNormalizedMainProgramError { owner, reason };
    check_deadline(owner, context.deadline)?;
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
    check_deadline(owner, context.deadline)?;
    // This target has implicit MOT roots, not wire-level explicit calls. Only
    // the complete declared-service factory supplies their actual resource
    // transmission context and can subsequently realize distinct physical
    // wrappers. Packing them here would incorrectly require their owner to
    // equal the page's explicit X27 selection. No unvalidated wire escapes:
    // the augmented candidate still passes packing, ancestry and both complete
    // comparisons below. Standalone composed/normalized entry points stay strict.
    if context.declared_foreign_defaults
        && context.resources.is_some()
        && has_foreign_defaults(service, owner)
    {
        let original = super::main_composition::compose_teletext_main_program_with_context(
            service,
            owner,
            budget.composition,
            context,
        )
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::Original(Box::new(
                error.into(),
            )))
        })?;
        let original = super::resource_transmission::settle_native_with_context(
            original,
            service,
            budget.composition.collection.max_commands,
            context,
        )
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::Original(Box::new(
                error.into(),
            )))
        })?;
        check_deadline(owner, context.deadline)?;
        return Ok(original);
    }
    let original = compile_composed_with_header_positions(
        service,
        owner,
        &original_allocations,
        budget.composition,
        (budget.packing, budget.assembly.max_work),
        context,
    )
    .map_err(|error| {
        fail(TeletextNormalizedMainProgramFailure::Original(Box::new(
            error,
        )))
    })?
    .into_composition();
    check_deadline(owner, context.deadline)?;
    Ok(original)
}

pub(super) fn has_foreign_defaults(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
) -> bool {
    let Some(page) = service.page_enhancements().get(&owner) else {
        return false;
    };
    page.local_program.is_none()
        && page.resources.defaults.iter().flatten().any(|selection| {
            let selected = match selection.target.scope() {
                TeletextObjectScope::Public => page.resources.objects.public,
                TeletextObjectScope::Global => page.resources.objects.global,
                TeletextObjectScope::Local => return false,
            };
            selection
                .target
                .resource()
                .map(ersatzrs_domain::TeletextResourceIdentity::page)
                != selected
        })
}

/// The caller owns the validated, additive effective service and retains the
/// exact original composition. This private seam grants no authored authority.
pub(super) fn compile_normalized_original_with_context(
    service: &TeletextSemanticService,
    original: TeletextMainComposition,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<EncodedTeletextNormalizedMainProgram, TeletextNormalizedMainProgramError> {
    let owner = original.owner();
    let physical = compile_physical_with_context(service, &original, defaults, budget, context)?;
    check_deadline(owner, context.deadline)?;
    Ok(EncodedTeletextNormalizedMainProgram { original, physical })
}

fn compile_physical_with_context(
    service: &TeletextSemanticService,
    original: &TeletextMainComposition,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<EncodedTeletextPhysicalDisplayProgram, TeletextNormalizedMainProgramError> {
    let owner = original.owner();
    let fail = |reason| TeletextNormalizedMainProgramError { owner, reason };
    check_deadline(owner, context.deadline)?;
    let plan = context
        .realization(original, budget.realization)
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::Realization(Box::new(
                error,
            )))
        })?;
    check_deadline(owner, context.deadline)?;
    let placement = context
        .placement(&plan, service.objects(), budget.placement)
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::Placement(Box::new(
                error,
            )))
        })?;
    check_deadline(owner, context.deadline)?;
    let mut augmented = context
        .assembly(&placement, service, budget.assembly)
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::Assembly(Box::new(
                error,
            )))
        })?;
    check_deadline(owner, context.deadline)?;
    augmented
        .materialize_object_headers(budget.packing, budget.assembly.max_work, context)
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::ObjectHeader(
                Box::new(error),
            ))
        })?;
    check_deadline(owner, context.deadline)?;
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
    check_deadline(owner, context.deadline)?;
    let compiled = context
        .compile_augmented(
            service,
            &augmented,
            &allocations,
            budget.replay,
            budget.packing,
            budget.ancestry,
        )
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::Compilation(Box::new(
                error,
            )))
        })?;
    check_deadline(owner, context.deadline)?;
    check_candidate(&compiled, budget, context)?;
    check_deadline(owner, context.deadline)?;
    Ok(compiled.into_physical())
}
fn check_candidate(
    compiled: &EncodedTeletextAugmentedOriginProgram<'_>,
    budget: &TeletextNormalizedMainProgramBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<(), TeletextNormalizedMainProgramError> {
    let original = compiled.augmented().placement().plan().composition();
    let fail = |reason| TeletextNormalizedMainProgramError {
        owner: original.owner(),
        reason,
    };
    check_deadline(original.owner(), context.deadline)?;
    let glyphs = context
        .compare_glyphs(compiled, budget.glyph_comparison)
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::GlyphComparison(
                Box::new(error),
            ))
        })?;
    check_deadline(original.owner(), context.deadline)?;
    if let Some(diagnostic) = glyphs.diagnostics().first() {
        return Err(fail(TeletextNormalizedMainProgramFailure::GlyphMismatch(
            Box::new(*diagnostic),
        )));
    }
    let display = context
        .compare_display(compiled, budget.display_comparison)
        .map_err(|error| {
            fail(TeletextNormalizedMainProgramFailure::DisplayComparison(
                Box::new(error),
            ))
        })?;
    check_deadline(original.owner(), context.deadline)?;
    if let Some(diagnostic) = display.diagnostics().first() {
        return Err(fail(TeletextNormalizedMainProgramFailure::DisplayMismatch(
            Box::new(diagnostic.clone()),
        )));
    }
    Ok(())
}

fn check_deadline(
    owner: TeletextPageAddress,
    deadline: Option<Instant>,
) -> Result<(), TeletextNormalizedMainProgramError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(TeletextNormalizedMainProgramError {
            owner,
            reason: TeletextNormalizedMainProgramFailure::TimedOut,
        });
    }
    Ok(())
}

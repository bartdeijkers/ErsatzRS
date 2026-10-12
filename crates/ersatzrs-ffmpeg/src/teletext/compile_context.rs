//! One non-clone context for every original, physical and settlement pass.

use std::time::Instant;

use ersatzrs_domain::{
    resolve_teletext_display_resolution, resolve_teletext_display_resolution_with_usage,
    TeletextDisplayResolution, TeletextDisplayResolutionBudget, TeletextDisplayResolutionError,
    TeletextDisplayResolutionFailure, TeletextDisplayResolutionInputs,
    TeletextEnhancementCollectionBudget, TeletextEnhancementObjectCatalog,
    TeletextObjectCatalogBudget, TeletextSemanticService,
};

use super::augmented_origin_compilation::compile_augmented_with_context_and_usage;
use super::resource_transmission::ResourceTransmissionContext;
use super::{
    build_teletext_augmented_origin_program_with_usage, compare_teletext_augmented_origin_glyphs,
    compare_teletext_augmented_origin_glyphs_with_usage, compare_teletext_physical_display,
    compare_teletext_physical_display_with_usage,
    plan_teletext_base_compatibility_realization_with_usage,
    plan_teletext_origin_carrier_placement_with_usage, EncodedTeletextAugmentedOriginProgram,
    TeletextAugmentedGlyphComparison, TeletextAugmentedGlyphComparisonBudget,
    TeletextAugmentedGlyphComparisonError, TeletextAugmentedGlyphComparisonFailure,
    TeletextAugmentedInvocationBudget, TeletextAugmentedOriginCompilationError,
    TeletextAugmentedOriginCompilationFailure, TeletextAugmentedOriginProgram,
    TeletextAugmentedOriginProgramBudget, TeletextAugmentedOriginProgramError,
    TeletextAugmentedOriginProgramFailure, TeletextAuthoredServiceEncodingBudget,
    TeletextAuthoredServiceEncodingUsage, TeletextBaseCompatibilityRealizationBudget,
    TeletextBaseCompatibilityRealizationError, TeletextBaseCompatibilityRealizationFailure,
    TeletextBaseCompatibilityRealizationPlan, TeletextMainComposition,
    TeletextOriginCarrierPlacement, TeletextOriginCarrierPlacementBudget,
    TeletextOriginCarrierPlacementError, TeletextOriginCarrierPlacementFailure,
    TeletextPhysicalComparison, TeletextPhysicalComparisonBudget, TeletextPhysicalComparisonError,
    TeletextPhysicalComparisonFailure, TeletextPhysicalDisplayReplay,
    TeletextResourceObjectAllocations,
};

pub(super) struct TeletextCompileContext<'a, 'service> {
    pub resources: Option<&'a ResourceTransmissionContext<'service>>,
    pub deadline: Option<Instant>,
    maximum_display_updates: Option<usize>,
    remaining_display_updates: Option<usize>,
    maximum_glyph_comparison_work: Option<usize>,
    remaining_glyph_comparison_work: Option<usize>,
    maximum_display_comparison_updates: Option<usize>,
    remaining_display_comparison_updates: Option<usize>,
    realization: WorkCredit,
    placement: WorkCredit,
    assembly: WorkCredit,
    ancestry: WorkCredit,
}

// One non-clone credit per existing meter; these units are never summed.
struct WorkCredit {
    maximum: Option<usize>,
    remaining: Option<usize>,
}

impl WorkCredit {
    const fn new(maximum: Option<usize>) -> Self {
        Self {
            maximum,
            remaining: maximum,
        }
    }
    fn cap(&self, stage: usize) -> usize {
        self.remaining
            .map_or(stage, |remaining| stage.min(remaining))
    }
    fn debit(&mut self, admitted: usize) -> bool {
        let Some(remaining) = self.remaining else {
            return true;
        };
        let Some(next) = remaining.checked_sub(admitted) else {
            return false;
        };
        self.remaining = Some(next);
        true
    }
    fn used(&self) -> Option<usize> {
        self.maximum
            .zip(self.remaining)
            .map(|(maximum, remaining)| maximum - remaining)
    }
}

impl<'a, 'service> TeletextCompileContext<'a, 'service> {
    pub(super) const fn new(
        resources: Option<&'a ResourceTransmissionContext<'service>>,
        deadline: Option<Instant>,
        maximum_display_updates: Option<usize>,
    ) -> Self {
        Self {
            resources,
            deadline,
            maximum_display_updates,
            remaining_display_updates: maximum_display_updates,
            maximum_glyph_comparison_work: None,
            remaining_glyph_comparison_work: None,
            maximum_display_comparison_updates: None,
            remaining_display_comparison_updates: None,
            realization: WorkCredit::new(None),
            placement: WorkCredit::new(None),
            assembly: WorkCredit::new(None),
            ancestry: WorkCredit::new(None),
        }
    }

    pub(super) fn for_service(
        resources: Option<&'a ResourceTransmissionContext<'service>>,
        deadline: Option<Instant>,
        budget: &TeletextAuthoredServiceEncodingBudget,
    ) -> Self {
        let mut context = Self::new(resources, deadline, budget.max_display_updates);
        context.maximum_glyph_comparison_work = budget.max_glyph_comparison_work;
        context.remaining_glyph_comparison_work = budget.max_glyph_comparison_work;
        context.maximum_display_comparison_updates = budget.max_display_comparison_updates;
        context.remaining_display_comparison_updates = budget.max_display_comparison_updates;
        context.realization = WorkCredit::new(budget.max_realization_updates);
        context.placement = WorkCredit::new(budget.max_placement_work);
        context.assembly = WorkCredit::new(budget.max_assembly_work);
        context.ancestry = WorkCredit::new(budget.max_ancestry_work);
        context
    }

    pub(super) fn used_work(&self) -> TeletextAuthoredServiceEncodingUsage {
        let used = |maximum: Option<usize>, remaining: Option<usize>| {
            maximum
                .zip(remaining)
                .map(|(maximum, remaining)| maximum - remaining)
        };
        TeletextAuthoredServiceEncodingUsage {
            display_updates: used(self.maximum_display_updates, self.remaining_display_updates),
            glyph_comparison_work: used(
                self.maximum_glyph_comparison_work,
                self.remaining_glyph_comparison_work,
            ),
            display_comparison_updates: used(
                self.maximum_display_comparison_updates,
                self.remaining_display_comparison_updates,
            ),
            realization_updates: self.realization.used(),
            placement_work: self.placement.used(),
            assembly_work: self.assembly.used(),
            ancestry_work: self.ancestry.used(),
        }
    }

    pub(super) fn realization<'b>(
        &mut self,
        original: &'b TeletextMainComposition,
        mut budget: TeletextBaseCompatibilityRealizationBudget,
    ) -> Result<
        TeletextBaseCompatibilityRealizationPlan<'b>,
        TeletextBaseCompatibilityRealizationError,
    > {
        budget.max_updates = self.realization.cap(budget.max_updates);
        let (result, admitted) =
            plan_teletext_base_compatibility_realization_with_usage(original, budget);
        if !self.realization.debit(admitted) {
            return Err(TeletextBaseCompatibilityRealizationError {
                owner: original.owner(),
                origin: None,
                location: None,
                root_source: None,
                invocation: None,
                reason: TeletextBaseCompatibilityRealizationFailure::AllocationSize,
            });
        }
        result
    }

    pub(super) fn placement<'b>(
        &mut self,
        plan: &'b TeletextBaseCompatibilityRealizationPlan<'b>,
        catalog: &'b TeletextEnhancementObjectCatalog,
        mut budget: TeletextOriginCarrierPlacementBudget,
    ) -> Result<TeletextOriginCarrierPlacement<'b>, TeletextOriginCarrierPlacementError> {
        budget.max_work = self.placement.cap(budget.max_work);
        let (result, admitted) =
            plan_teletext_origin_carrier_placement_with_usage(plan, catalog, budget);
        if !self.placement.debit(admitted) {
            return Err(TeletextOriginCarrierPlacementError {
                owner: plan.composition().owner(),
                origin: None,
                location: None,
                root_source: None,
                invocation: None,
                reason: TeletextOriginCarrierPlacementFailure::AllocationSize,
            });
        }
        result
    }

    pub(super) fn assembly<'b>(
        &mut self,
        placement: &'b TeletextOriginCarrierPlacement<'b>,
        mut budget: TeletextAugmentedOriginProgramBudget,
    ) -> Result<TeletextAugmentedOriginProgram<'b>, TeletextAugmentedOriginProgramError> {
        budget.max_work = self.assembly.cap(budget.max_work);
        let (result, admitted) =
            build_teletext_augmented_origin_program_with_usage(placement, budget);
        if !self.assembly.debit(admitted) {
            return Err(TeletextAugmentedOriginProgramError {
                owner: placement.plan().composition().owner(),
                origin: None,
                location: None,
                root_source: None,
                invocation: None,
                reason: TeletextAugmentedOriginProgramFailure::AllocationSize,
            });
        }
        result
    }

    pub(super) fn compile_augmented<'b>(
        &mut self,
        service: &TeletextSemanticService,
        augmented: &'b TeletextAugmentedOriginProgram<'b>,
        allocations: &TeletextResourceObjectAllocations,
        replay: TeletextEnhancementCollectionBudget,
        packing: TeletextObjectCatalogBudget,
        mut budget: TeletextAugmentedInvocationBudget,
    ) -> Result<EncodedTeletextAugmentedOriginProgram<'b>, TeletextAugmentedOriginCompilationError>
    {
        budget.max_work = self.ancestry.cap(budget.max_work);
        let (result, admitted) = compile_augmented_with_context_and_usage(
            service,
            augmented,
            allocations,
            replay,
            packing,
            budget,
            self,
        );
        if !self.ancestry.debit(admitted) {
            return Err(TeletextAugmentedOriginCompilationError {
                owner: augmented.placement().plan().composition().owner(),
                location: None,
                root_source: None,
                actual: None,
                original: None,
                reason: TeletextAugmentedOriginCompilationFailure::AllocationSize,
            });
        }
        result
    }

    pub(super) fn compare_glyphs<'b>(
        &mut self,
        compiled: &'b EncodedTeletextAugmentedOriginProgram<'b>,
        mut budget: TeletextAugmentedGlyphComparisonBudget,
    ) -> Result<TeletextAugmentedGlyphComparison<'b>, TeletextAugmentedGlyphComparisonError> {
        let Some(remaining) = self.remaining_glyph_comparison_work else {
            return compare_teletext_augmented_origin_glyphs(compiled, budget);
        };
        budget.max_work = budget.max_work.min(remaining);
        let (result, admitted) =
            compare_teletext_augmented_origin_glyphs_with_usage(compiled, budget);
        let next = remaining.checked_sub(admitted).ok_or_else(|| {
            TeletextAugmentedGlyphComparisonError {
                owner: compiled
                    .augmented()
                    .placement()
                    .plan()
                    .composition()
                    .owner(),
                position: None,
                reason: TeletextAugmentedGlyphComparisonFailure::AllocationSize,
            }
        })?;
        self.remaining_glyph_comparison_work = Some(next);
        result
    }

    pub(super) fn compare_display<'b>(
        &mut self,
        original: &'b TeletextMainComposition,
        physical: &'b TeletextPhysicalDisplayReplay,
        mut budget: TeletextPhysicalComparisonBudget,
    ) -> Result<TeletextPhysicalComparison<'b>, TeletextPhysicalComparisonError> {
        let Some(remaining) = self.remaining_display_comparison_updates else {
            return compare_teletext_physical_display(original, physical, budget);
        };
        budget.max_updates = budget.max_updates.min(remaining);
        let (result, admitted) =
            compare_teletext_physical_display_with_usage(original, physical, budget);
        let next = remaining
            .checked_sub(admitted)
            .ok_or(TeletextPhysicalComparisonError {
                owner: original.owner(),
                position: None,
                reason: TeletextPhysicalComparisonFailure::AllocationSize,
            })?;
        self.remaining_display_comparison_updates = Some(next);
        result
    }

    pub(super) fn resolve_display(
        &mut self,
        inputs: TeletextDisplayResolutionInputs<'_>,
        mut budget: TeletextDisplayResolutionBudget,
    ) -> Result<TeletextDisplayResolution, TeletextDisplayResolutionError> {
        let Some(remaining) = self.remaining_display_updates else {
            return resolve_teletext_display_resolution(inputs, budget);
        };
        // Retain the stage's own ceiling. Every pass shares the smaller
        // remaining service credit; it is never reset for the next display.
        budget.max_updates = budget.max_updates.min(remaining);
        let (result, admitted) = resolve_teletext_display_resolution_with_usage(inputs, budget);
        let Some(next) = remaining.checked_sub(admitted) else {
            // The resolver's admitted meter cannot exceed its supplied cap.
            // Fail closed if a future implementation breaks that contract.
            return Err(TeletextDisplayResolutionError {
                owner: inputs.base.owner(),
                location: None,
                root_source: None,
                invocation: None,
                reason: TeletextDisplayResolutionFailure::AllocationSize,
            });
        };
        self.remaining_display_updates = Some(next);
        // Debit before returning either outcome: a failed stage cannot refund
        // earlier admitted updates or an accepted conservative reservation.
        result
    }
}

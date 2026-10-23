//! One non-clone context for every original, physical and settlement pass.

use std::time::Instant;

use ersatzrs_domain::{
    resolve_teletext_attribute_execution_with_transmission_and_usage,
    resolve_teletext_attribute_execution_with_usage, resolve_teletext_display_resolution,
    resolve_teletext_display_resolution_with_usage, resolve_teletext_drcs_program_with_usage,
    resolve_teletext_spatial_program_with_usage, resolve_teletext_spatial_trace_with_usage,
    TeletextAttributeExecution, TeletextAttributeExecutionError, TeletextAttributeExecutionFailure,
    TeletextAttributeTransmissionEvidence, TeletextBaseDisplay, TeletextCharsetPair,
    TeletextCollectedEnhancementReferences, TeletextColourReference, TeletextDisplayResolution,
    TeletextDisplayResolutionBudget, TeletextDisplayResolutionError,
    TeletextDisplayResolutionFailure, TeletextDisplayResolutionInputs, TeletextDrcsExecution,
    TeletextDrcsExecutionBudget, TeletextDrcsExecutionError, TeletextDrcsExecutionFailure,
    TeletextDrcsProgramExecutionError, TeletextEnhancementCollectionBudget,
    TeletextEnhancementCollectionError, TeletextEnhancementCollectionFailure,
    TeletextEnhancementObjectCatalog, TeletextEnhancementProgram, TeletextEnhancementSource,
    TeletextGlyphIntent, TeletextGlyphLayers, TeletextObjectCatalogBudget, TeletextPageAddress,
    TeletextSemanticService, TeletextSpatialTrace, TeletextSpatialTraceBudget,
    TeletextSpatialTraceError, TeletextSpatialTraceFailure, TeletextSpatialTraceUsage,
    TeletextSpatialTraceWorkBudget,
};

use super::augmented_origin_compilation::compile_augmented_with_context_and_usage;
use super::augmented_origin_program::{
    build_augmented_with_foreign_defaults_and_work_usage,
    build_augmented_with_service_and_work_usage,
};
use super::origin_carrier_placement::plan_teletext_origin_carrier_placement_with_context_and_usage;
use super::physical_comparison::compare_augmented_physical_with_usage;
use super::resource_transmission::ResourceTransmissionContext;
use super::{
    compare_teletext_augmented_origin_glyphs, compare_teletext_augmented_origin_glyphs_with_usage,
    plan_teletext_base_compatibility_realization_with_usage, EncodedTeletextAugmentedOriginProgram,
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
    TeletextPhysicalComparisonFailure, TeletextResourceObjectAllocations,
};

pub(super) struct TeletextCompileContext<'a, 'service> {
    pub resources: Option<&'a ResourceTransmissionContext<'service>>,
    pub deadline: Option<Instant>,
    // Only the whole declared-service factory can realize implicit MOT roots.
    // Standalone composed/normalized callers retain selected-link enforcement.
    pub declared_foreign_defaults: bool,
    // The declared-service producer acquires resources before display pages (B.7).
    pub resources_before_page: bool,
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
    collected_commands: WorkCredit,
    collected_drcs_references: WorkCredit,
    spatial_event_visits: WorkCredit,
    spatial_invocation_visits: WorkCredit,
    identity_checks: WorkCredit,
    attribute_cell_updates: WorkCredit,
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
            declared_foreign_defaults: false,
            resources_before_page: false,
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
            collected_commands: WorkCredit::new(None),
            collected_drcs_references: WorkCredit::new(None),
            spatial_event_visits: WorkCredit::new(None),
            spatial_invocation_visits: WorkCredit::new(None),
            identity_checks: WorkCredit::new(None),
            attribute_cell_updates: WorkCredit::new(None),
        }
    }

    pub(super) fn for_service(
        resources: Option<&'a ResourceTransmissionContext<'service>>,
        deadline: Option<Instant>,
        budget: &TeletextAuthoredServiceEncodingBudget,
    ) -> Self {
        let mut context = Self::new(resources, deadline, budget.max_display_updates);
        context.resources_before_page = resources.is_some();
        context.maximum_glyph_comparison_work = budget.max_glyph_comparison_work;
        context.remaining_glyph_comparison_work = budget.max_glyph_comparison_work;
        context.maximum_display_comparison_updates = budget.max_display_comparison_updates;
        context.remaining_display_comparison_updates = budget.max_display_comparison_updates;
        context.realization = WorkCredit::new(budget.max_realization_updates);
        context.placement = WorkCredit::new(budget.max_placement_work);
        context.assembly = WorkCredit::new(budget.max_assembly_work);
        context.ancestry = WorkCredit::new(budget.max_ancestry_work);
        context.collected_commands = WorkCredit::new(budget.max_collected_commands);
        context.collected_drcs_references = WorkCredit::new(budget.max_collected_drcs_references);
        context.spatial_event_visits = WorkCredit::new(budget.max_spatial_event_visits);
        context.spatial_invocation_visits = WorkCredit::new(budget.max_spatial_invocation_visits);
        context.identity_checks = WorkCredit::new(budget.max_identity_checks);
        context.attribute_cell_updates = WorkCredit::new(budget.max_attribute_cell_updates);
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
            collected_commands: self.collected_commands.used(),
            collected_drcs_references: self.collected_drcs_references.used(),
            spatial_event_visits: self.spatial_event_visits.used(),
            spatial_invocation_visits: self.spatial_invocation_visits.used(),
            identity_checks: self.identity_checks.used(),
            attribute_cell_updates: self.attribute_cell_updates.used(),
        }
    }

    pub(super) fn resolve_attributes(
        &mut self,
        base: &TeletextBaseDisplay,
        original: &TeletextSpatialTrace,
        layers: &TeletextGlyphLayers,
        row_colours: &[TeletextColourReference; 25],
        max_commands: usize,
        transmission: Option<&TeletextAttributeTransmissionEvidence>,
    ) -> Result<TeletextAttributeExecution, TeletextAttributeExecutionError> {
        // The domain retains its derived stage ceiling. Remaining service
        // credit only shortens that ceiling across original and settled passes.
        let remaining = self.attribute_cell_updates.remaining;
        let (result, admitted) = match transmission {
            Some(transmission) => resolve_teletext_attribute_execution_with_transmission_and_usage(
                base,
                original,
                layers,
                row_colours,
                max_commands,
                transmission,
                remaining,
            ),
            None => resolve_teletext_attribute_execution_with_usage(
                base,
                original,
                layers,
                row_colours,
                max_commands,
                remaining,
            ),
        };
        if !self.attribute_cell_updates.debit(admitted) {
            return Err(TeletextAttributeExecutionError {
                owner: base.owner(),
                location: None,
                root_source: None,
                invocation: None,
                reason: TeletextAttributeExecutionFailure::WorkBudget,
            });
        }
        result
    }

    pub(super) fn collect_display_references(
        &mut self,
        catalog: &TeletextEnhancementObjectCatalog,
        owner: TeletextPageAddress,
        program: &TeletextEnhancementProgram,
        primary_pair: TeletextCharsetPair,
        mut budget: TeletextEnhancementCollectionBudget,
    ) -> Result<TeletextCollectedEnhancementReferences, TeletextEnhancementCollectionError> {
        budget.max_commands = self.collected_commands.cap(budget.max_commands);
        budget.max_drcs_references = self
            .collected_drcs_references
            .cap(budget.max_drcs_references);
        let (result, admitted) =
            catalog.collect_display_references_with_usage(owner, program, primary_pair, budget);
        let commands_ok = self.collected_commands.debit(admitted.commands);
        let references_ok = self
            .collected_drcs_references
            .debit(admitted.drcs_references);
        if !commands_ok || !references_ok {
            return Err(TeletextEnhancementCollectionError {
                display: owner,
                source_location: TeletextEnhancementSource {
                    object: None,
                    command: 0,
                },
                reason: if commands_ok {
                    TeletextEnhancementCollectionFailure::ReferenceBudget {
                        maximum: budget.max_drcs_references,
                    }
                } else {
                    TeletextEnhancementCollectionFailure::CommandBudget {
                        maximum: budget.max_commands,
                    }
                },
            });
        }
        result
    }

    pub(super) fn resolve_drcs_program(
        &mut self,
        service: &TeletextSemanticService,
        program: &TeletextEnhancementProgram,
        catalog: &TeletextEnhancementObjectCatalog,
        intent: &TeletextGlyphIntent,
        mut collection_budget: TeletextEnhancementCollectionBudget,
        drcs_budget: TeletextDrcsExecutionBudget,
    ) -> Result<TeletextDrcsExecution, TeletextDrcsProgramExecutionError> {
        collection_budget.max_commands =
            self.collected_commands.cap(collection_budget.max_commands);
        collection_budget.max_drcs_references = self
            .collected_drcs_references
            .cap(collection_budget.max_drcs_references);
        let (result, admitted) = resolve_teletext_drcs_program_with_usage(
            service,
            program,
            catalog,
            intent,
            collection_budget,
            drcs_budget,
        );
        let commands_ok = self.collected_commands.debit(admitted.commands);
        let references_ok = self
            .collected_drcs_references
            .debit(admitted.drcs_references);
        if !commands_ok || !references_ok {
            return Err(TeletextDrcsExecutionError {
                owner: intent.owner(),
                position: None,
                location: None,
                root_source: None,
                invocation: None,
                reason: TeletextDrcsExecutionFailure::WorkSize,
            }
            .into());
        }
        result
    }

    pub(super) fn resolve_spatial_trace(
        &mut self,
        service: &TeletextSemanticService,
        owner: TeletextPageAddress,
        budget: TeletextSpatialTraceBudget,
    ) -> Result<TeletextSpatialTrace, TeletextSpatialTraceError> {
        let (result, admitted) = resolve_teletext_spatial_trace_with_usage(
            service,
            owner,
            budget,
            self.spatial_work_budget(),
        );
        self.debit_spatial(owner, admitted)?;
        result
    }

    pub(super) fn resolve_spatial_program(
        &mut self,
        owner: TeletextPageAddress,
        program: &TeletextEnhancementProgram,
        catalog: &TeletextEnhancementObjectCatalog,
        budget: TeletextSpatialTraceBudget,
    ) -> Result<TeletextSpatialTrace, TeletextSpatialTraceError> {
        let (result, admitted) = resolve_teletext_spatial_program_with_usage(
            owner,
            program,
            catalog,
            budget,
            self.spatial_work_budget(),
        );
        self.debit_spatial(owner, admitted)?;
        result
    }

    fn spatial_work_budget(&self) -> TeletextSpatialTraceWorkBudget {
        // The domain shares these remaining credits across its count and emit
        // passes while retaining each pass's independent event/call ceilings.
        TeletextSpatialTraceWorkBudget {
            max_event_visits: self.spatial_event_visits.remaining,
            max_invocation_visits: self.spatial_invocation_visits.remaining,
        }
    }

    fn debit_spatial(
        &mut self,
        owner: TeletextPageAddress,
        admitted: TeletextSpatialTraceUsage,
    ) -> Result<(), TeletextSpatialTraceError> {
        let events_ok = self.spatial_event_visits.debit(admitted.event_visits);
        let invocations_ok = self
            .spatial_invocation_visits
            .debit(admitted.invocation_visits);
        if !events_ok || !invocations_ok {
            return Err(TeletextSpatialTraceError {
                owner,
                location: None,
                root_source: None,
                invocation: None,
                reason: TeletextSpatialTraceFailure::SelectionState,
            });
        }
        Ok(())
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
        let (result, admitted) = plan_teletext_origin_carrier_placement_with_context_and_usage(
            plan, catalog, budget, self,
        );
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

    /// A complete-service demand pass shares assembly credit with both physical
    /// derivation passes. Rejected scans never run or renew the original quota.
    pub(super) fn admit_national_drcs_scan(
        &mut self,
        owner: TeletextPageAddress,
        service: &TeletextSemanticService,
        maximum: usize,
    ) -> Result<usize, TeletextAugmentedOriginProgramError> {
        let reason = match super::national_drcs_carrier::scan_work(service) {
            Some(work) if work <= self.assembly.cap(maximum) && self.assembly.debit(work) => {
                return Ok(work)
            }
            Some(_) => TeletextAugmentedOriginProgramFailure::WorkBudget {
                maximum: self.assembly.cap(maximum),
            },
            None => TeletextAugmentedOriginProgramFailure::AllocationSize,
        };
        Err(TeletextAugmentedOriginProgramError {
            owner,
            origin: None,
            location: None,
            root_source: None,
            invocation: None,
            reason,
        })
    }

    /// Definition header derivation shares existing assembly credit across the
    /// original bootstrap, prefix settlements and final physical catalogue.
    pub(super) fn object_headers(
        &mut self,
        original: &TeletextMainComposition,
        catalog: &TeletextEnhancementObjectCatalog,
        budget: TeletextObjectCatalogBudget,
        maximum: usize,
    ) -> Result<
        super::object_header_positions::DerivedObjectHeaderPositions,
        super::TeletextMainCompositionError,
    > {
        use super::object_header_positions::DerivationError;
        use super::TeletextMainCompositionFailure as Failure;
        let (result, admitted) = super::object_header_positions::derive(
            original,
            catalog,
            budget,
            self.assembly.cap(maximum),
        );
        let fail = |reason| super::TeletextMainCompositionError {
            owner: original.owner(),
            location: None,
            reason,
        };
        if !self.assembly.debit(admitted) {
            return Err(fail(Failure::ObjectHeaderStorage));
        }
        result.map_err(|error| {
            fail(match error {
                DerivationError::WorkBudget { maximum } => {
                    Failure::ObjectHeaderWorkBudget { maximum }
                }
                DerivationError::Catalog(error) => Failure::ObjectCatalog(error),
                DerivationError::Program(error) => Failure::Program(error),
                DerivationError::AllocationSize => Failure::ObjectHeaderStorage,
            })
        })
    }

    pub(super) fn assembly<'b>(
        &mut self,
        placement: &'b TeletextOriginCarrierPlacement<'b>,
        service: &TeletextSemanticService,
        mut budget: TeletextAugmentedOriginProgramBudget,
    ) -> Result<TeletextAugmentedOriginProgram<'b>, TeletextAugmentedOriginProgramError> {
        budget.max_work = self.assembly.cap(budget.max_work);
        budget.max_identity_checks = self.identity_checks.cap(budget.max_identity_checks);
        let (result, admitted) = if self.declared_foreign_defaults
            && super::normalized_main_program::has_foreign_defaults(
                service,
                placement.plan().composition().owner(),
            ) {
            build_augmented_with_foreign_defaults_and_work_usage(placement, service, budget)
        } else {
            build_augmented_with_service_and_work_usage(placement, service, budget)
        };
        let assembly_ok = self.assembly.debit(admitted.work);
        let identity_ok = self.identity_checks.debit(admitted.identity_checks);
        if !assembly_ok || !identity_ok {
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
        compiled: &'b EncodedTeletextAugmentedOriginProgram<'_>,
        mut budget: TeletextPhysicalComparisonBudget,
    ) -> Result<TeletextPhysicalComparison<'b>, TeletextPhysicalComparisonError> {
        if let Some(remaining) = self.remaining_display_comparison_updates {
            budget.max_updates = budget.max_updates.min(remaining);
        }
        let (result, admitted) = compare_augmented_physical_with_usage(compiled, budget);
        if let Some(remaining) = self.remaining_display_comparison_updates {
            self.remaining_display_comparison_updates = Some(
                remaining
                    .checked_sub(admitted)
                    .ok_or(TeletextPhysicalComparisonError {
                        owner: compiled
                            .augmented()
                            .placement()
                            .plan()
                            .composition()
                            .owner(),
                        position: None,
                        reason: TeletextPhysicalComparisonFailure::AllocationSize,
                    })?,
            );
        }
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

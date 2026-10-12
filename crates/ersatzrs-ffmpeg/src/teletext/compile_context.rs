//! One non-clone context for every original, physical and settlement pass.

use std::time::Instant;

use ersatzrs_domain::{
    resolve_teletext_display_resolution, resolve_teletext_display_resolution_with_usage,
    TeletextDisplayResolution, TeletextDisplayResolutionBudget, TeletextDisplayResolutionError,
    TeletextDisplayResolutionFailure, TeletextDisplayResolutionInputs,
};

use super::resource_transmission::ResourceTransmissionContext;
use super::{
    compare_teletext_augmented_origin_glyphs, compare_teletext_augmented_origin_glyphs_with_usage,
    compare_teletext_physical_display, compare_teletext_physical_display_with_usage,
    EncodedTeletextAugmentedOriginProgram, TeletextAugmentedGlyphComparison,
    TeletextAugmentedGlyphComparisonBudget, TeletextAugmentedGlyphComparisonError,
    TeletextAugmentedGlyphComparisonFailure, TeletextAuthoredServiceEncodingBudget,
    TeletextAuthoredServiceEncodingUsage, TeletextMainComposition, TeletextPhysicalComparison,
    TeletextPhysicalComparisonBudget, TeletextPhysicalComparisonError,
    TeletextPhysicalComparisonFailure, TeletextPhysicalDisplayReplay,
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
        }
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

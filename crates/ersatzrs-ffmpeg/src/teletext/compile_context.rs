//! One non-clone context for every original, physical and settlement pass.

use std::time::Instant;

use ersatzrs_domain::{
    resolve_teletext_display_resolution, resolve_teletext_display_resolution_with_usage,
    TeletextDisplayResolution, TeletextDisplayResolutionBudget, TeletextDisplayResolutionError,
    TeletextDisplayResolutionFailure, TeletextDisplayResolutionInputs,
};

use super::resource_transmission::ResourceTransmissionContext;

pub(super) struct TeletextCompileContext<'a, 'service> {
    pub resources: Option<&'a ResourceTransmissionContext<'service>>,
    pub deadline: Option<Instant>,
    maximum_display_updates: Option<usize>,
    remaining_display_updates: Option<usize>,
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
        }
    }

    pub(super) fn used_updates(&self) -> Option<usize> {
        self.maximum_display_updates
            .zip(self.remaining_display_updates)
            .map(|(maximum, remaining)| maximum - remaining)
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

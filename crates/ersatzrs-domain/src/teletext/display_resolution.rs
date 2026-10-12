//! Original C.3 display realization with compact writer and footprint pools.
//!
//! Attribute values retain requested semantics; effective size and fragments
//! alone describe occupied cells. Exact transmission ties, discretionary edge
//! behavior and unproved inherited non-origin Adaptive sizing stay explicit.
//! Matching inputs do not certify a generation, raster or publication.

use super::display_resolution_values::{
    State, TeletextDisplayOrigin, TeletextDisplayPending, TeletextDisplayResolution,
    TeletextDisplayWriter,
};
use super::{
    resolve_teletext_base_footprints, TeletextAttributeExecution, TeletextBaseDisplay,
    TeletextBaseFootprints, TeletextCandidatePlacement, TeletextColourReference,
    TeletextDisplayGeometry, TeletextDisplayGeometryFailure, TeletextDrcsExecution,
    TeletextEnhancementSource, TeletextGlyphIntent, TeletextGlyphLayers, TeletextObjectType,
    TeletextPageAddress, TeletextSidePanels, TeletextSpatialEvent, TeletextSpatialInvocationId,
    TeletextSpatialRootSource, TeletextSpatialTrace,
};

#[derive(Debug, Clone, Copy)]
pub struct TeletextDisplayResolutionInputs<'a> {
    pub base: &'a TeletextBaseDisplay,
    pub base_footprints: &'a TeletextBaseFootprints,
    pub original: &'a TeletextSpatialTrace,
    pub layers: &'a TeletextGlyphLayers,
    pub attributes: &'a TeletextAttributeExecution,
    pub intent: &'a TeletextGlyphIntent,
    pub drcs: &'a TeletextDrcsExecution,
    pub geometry: &'a TeletextDisplayGeometry,
    pub row_colours: &'a [TeletextColourReference; 25],
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextDisplayResolutionBudget {
    pub max_candidates: usize,
    pub max_commands: usize,
    pub max_writers: usize,
    pub max_origins: usize,
    pub max_updates: usize,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("teletext display resolution for {owner:?} at {location:?}: {reason}")]
pub struct TeletextDisplayResolutionError {
    pub owner: TeletextPageAddress,
    pub location: Option<TeletextEnhancementSource>,
    pub root_source: Option<TeletextSpatialRootSource>,
    pub invocation: Option<TeletextSpatialInvocationId>,
    pub reason: TeletextDisplayResolutionFailure,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextDisplayResolutionFailure {
    #[error("input owner {other:?} differs from base")]
    OwnerMismatch { other: TeletextPageAddress },
    #[error("candidate count {count} exceeds {maximum}")]
    CandidateBudget { count: usize, maximum: usize },
    #[error("command count {count} exceeds {maximum}")]
    CommandBudget { count: usize, maximum: usize },
    #[error("writer count {count} exceeds {maximum}")]
    WriterBudget { count: usize, maximum: usize },
    #[error("origin count {count} exceeds {maximum}")]
    OriginBudget { count: usize, maximum: usize },
    #[error("display update budget {maximum} exhausted")]
    UpdateBudget { maximum: usize },
    #[error("candidate {candidate} does not match its original inputs")]
    CandidateMismatch { candidate: usize },
    #[error("original input correspondence failed: {0}")]
    OriginalSource(TeletextDisplayGeometryFailure),
    #[error("base footprints do not match the original base")]
    BaseFootprintMismatch,
    #[error("bounded display allocation arithmetic exceeds storage range")]
    AllocationSize,
    #[error("same-type object addressed-cell overlap")]
    ObjectOverlap {
        first: TeletextSpatialInvocationId,
        second: TeletextSpatialInvocationId,
    },
    #[error("adaptive instances interleave while redefining attributes")]
    AdaptiveInterleaving {
        first: TeletextSpatialInvocationId,
        second: TeletextSpatialInvocationId,
    },
}

impl TeletextDisplayResolutionInputs<'_> {
    pub(super) fn error(
        self,
        reason: TeletextDisplayResolutionFailure,
    ) -> TeletextDisplayResolutionError {
        TeletextDisplayResolutionError {
            owner: self.base.owner(),
            location: None,
            root_source: None,
            invocation: None,
            reason,
        }
    }
    pub(super) fn candidate_error(
        self,
        index: usize,
        reason: TeletextDisplayResolutionFailure,
    ) -> TeletextDisplayResolutionError {
        let candidate = &self.layers.candidates()[index];
        TeletextDisplayResolutionError {
            owner: self.base.owner(),
            location: candidate.source(),
            root_source: candidate.root_source(),
            invocation: candidate.invocation(),
            reason,
        }
    }
}

/// Resolve original writers, visible glyph origins and their occupied parts.
///
/// This consumes one immutable original model. The old attribute projection is
/// retained for correspondence and diagnostic provenance, not used as a final
/// C.3 plane. Pending glyph/size/physical-order outcomes are not renderable wins.
/// # Errors
/// Rejects raw budgets before indexing or allocating, inconsistent original
/// candidate inputs, known forbidden A.7 overlaps and exhausted update/storage
/// bounds. No partial result escapes and no input is mutated.
pub fn resolve_teletext_display_resolution(
    inputs: TeletextDisplayResolutionInputs<'_>,
    budget: TeletextDisplayResolutionBudget,
) -> Result<TeletextDisplayResolution, TeletextDisplayResolutionError> {
    resolve_teletext_display_resolution_with_usage(inputs, budget).0
}

/// Resolve the display and report admitted display-update work, including on error.
///
/// The counter includes conservative bulk reservations admitted by the update
/// budget. It measures neither executed CPU work nor allocated memory. A denied
/// or overflowing reservation adds nothing. Preflight or builder-construction
/// failures report zero; later failures retain all previously admitted charges.
/// The result has the same semantics and diagnostic provenance as
/// [`resolve_teletext_display_resolution`]; no partial display escapes.
/// # Errors
/// Returns the original resolution error together with its admitted work count.
pub fn resolve_teletext_display_resolution_with_usage(
    inputs: TeletextDisplayResolutionInputs<'_>,
    budget: TeletextDisplayResolutionBudget,
) -> (
    Result<TeletextDisplayResolution, TeletextDisplayResolutionError>,
    usize,
) {
    if let Err(error) = preflight(inputs, budget) {
        return (Err(error), 0);
    }
    let mut builder = match super::display_resolution_writers::Builder::new(inputs, budget) {
        Ok(builder) => builder,
        Err(error) => return (Err(error), 0),
    };
    let result = builder
        .shared()
        .and_then(|()| builder.objects(TeletextObjectType::Adaptive))
        .and_then(|()| builder.objects(TeletextObjectType::Passive));
    let updates = builder.admitted_updates();
    (result.map(|()| builder.finish()), updates)
}

fn preflight(
    inputs: TeletextDisplayResolutionInputs<'_>,
    budget: TeletextDisplayResolutionBudget,
) -> Result<(), TeletextDisplayResolutionError> {
    let candidates = inputs.layers.candidates().len();
    let events = inputs.original.events().len();
    for (count, maximum, reason) in [
        (
            candidates,
            budget.max_candidates,
            TeletextDisplayResolutionFailure::CandidateBudget {
                count: candidates,
                maximum: budget.max_candidates,
            },
        ),
        (
            events,
            budget.max_commands,
            TeletextDisplayResolutionFailure::CommandBudget {
                count: events,
                maximum: budget.max_commands,
            },
        ),
        (
            candidates,
            budget.max_origins,
            TeletextDisplayResolutionFailure::OriginBudget {
                count: candidates,
                maximum: budget.max_origins,
            },
        ),
    ] {
        if count > maximum {
            if !matches!(
                reason,
                TeletextDisplayResolutionFailure::CommandBudget { .. }
            ) && maximum < candidates
            {
                return Err(inputs.candidate_error(maximum, reason));
            }
            if let Some(event) = inputs.original.events().get(maximum) {
                return Err(event_error(inputs, event, reason));
            }
            return Err(inputs.error(reason));
        }
    }
    for other in [
        inputs.base_footprints.owner(),
        inputs.original.owner(),
        inputs.layers.owner(),
        inputs.attributes.owner(),
        inputs.intent.owner(),
        inputs.drcs.owner(),
        inputs.geometry.owner(),
    ] {
        if other != inputs.base.owner() {
            return Err(inputs.error(TeletextDisplayResolutionFailure::OwnerMismatch { other }));
        }
    }
    let writer_count = super::display_resolution_writers::writer_count(inputs);
    if writer_count > budget.max_writers {
        return Err(
            inputs.error(TeletextDisplayResolutionFailure::WriterBudget {
                count: writer_count,
                maximum: budget.max_writers,
            }),
        );
    }
    if budget.max_updates == 0 {
        return Err(inputs.error(TeletextDisplayResolutionFailure::UpdateBudget { maximum: 0 }));
    }
    // Bound all scratch and output arithmetic before correspondence allocation.
    let total = events
        .checked_mul(72 * 25 * 16)
        .and_then(|n| n.checked_add(candidates.checked_mul(72 * 25 * 16)?))
        .and_then(|n| n.checked_add(1000 * 72 * 16));
    let storage = resolution_storage(inputs);
    if total.is_none() || storage.is_none_or(|bytes| bytes > isize::MAX as usize) {
        return Err(inputs.error(TeletextDisplayResolutionFailure::AllocationSize));
    }
    validate_projection(inputs)
}

fn validate_projection(
    inputs: TeletextDisplayResolutionInputs<'_>,
) -> Result<(), TeletextDisplayResolutionError> {
    let candidates = inputs.layers.candidates().len();
    super::display_geometry::validate_original_glyph_inputs(
        inputs.base,
        inputs.original,
        inputs.layers,
    )
    .map_err(|error| TeletextDisplayResolutionError {
        owner: error.owner,
        location: error.location,
        root_source: error.root_source,
        invocation: error.invocation,
        reason: TeletextDisplayResolutionFailure::OriginalSource(error.reason),
    })?;
    if inputs.geometry.records().len() != candidates
        || inputs.intent.records().len() != candidates
        || inputs.attributes.records().len() != candidates
    {
        return Err(
            inputs.error(TeletextDisplayResolutionFailure::CandidateMismatch { candidate: 0 })
        );
    }
    let panels = inputs
        .base
        .presentation()
        .map(|presentation| presentation.panels);
    let widths = panels.map_or((0, 0), TeletextSidePanels::level25_widths);
    if inputs.geometry.panels() != panels
        || (inputs.geometry.left_width(), inputs.geometry.right_width()) != widths
    {
        return Err(
            inputs.error(TeletextDisplayResolutionFailure::CandidateMismatch { candidate: 0 })
        );
    }
    for (index, candidate) in inputs.layers.candidates().iter().enumerate() {
        let expected =
            super::display_resolution_writers::visual(inputs.geometry, candidate.position());
        let actual = match inputs.geometry.records()[index].placement() {
            TeletextCandidatePlacement::Placed { row, column } => Some((row, column)),
            TeletextCandidatePlacement::Unplaced(_) => None,
        };
        if actual != expected
            || inputs.geometry.records()[index].candidate() != candidate
            || inputs.intent.records()[index].context().candidate() != candidate
            || inputs.attributes.records()[index].candidate() != candidate
        {
            return Err(inputs.candidate_error(
                index,
                TeletextDisplayResolutionFailure::CandidateMismatch { candidate: index },
            ));
        }
    }
    if inputs.geometry.object_regions().len() != inputs.original.invocations().len()
        || inputs
            .geometry
            .object_regions()
            .iter()
            .zip(inputs.original.invocations())
            .any(|(region, instance)| region.instance() != instance)
    {
        return Err(
            inputs.error(TeletextDisplayResolutionFailure::CandidateMismatch { candidate: 0 })
        );
    }
    for record in inputs.drcs.records() {
        if inputs.layers.candidates().get(record.candidate_index()) != Some(record.candidate()) {
            return Err(
                inputs.error(TeletextDisplayResolutionFailure::CandidateMismatch {
                    candidate: record.candidate_index(),
                }),
            );
        }
    }
    let expected = resolve_teletext_base_footprints(inputs.base, inputs.row_colours)
        .map_err(|_| inputs.error(TeletextDisplayResolutionFailure::BaseFootprintMismatch))?;
    if expected != *inputs.base_footprints {
        return Err(inputs.error(TeletextDisplayResolutionFailure::BaseFootprintMismatch));
    }
    super::display_resolution_writers::validate_objects(inputs)
}

pub(super) fn event_error(
    inputs: TeletextDisplayResolutionInputs<'_>,
    event: &TeletextSpatialEvent,
    reason: TeletextDisplayResolutionFailure,
) -> TeletextDisplayResolutionError {
    TeletextDisplayResolutionError {
        owner: inputs.base.owner(),
        location: Some(event.source()),
        root_source: Some(event.root_source()),
        invocation: event.invocation(),
        reason,
    }
}

fn resolution_storage(inputs: TeletextDisplayResolutionInputs<'_>) -> Option<usize> {
    let candidates = inputs.layers.candidates().len();
    let events = inputs.original.events().len();
    let plane = 25usize.checked_mul(usize::from(inputs.geometry.total_width()))?;
    // One original command can repaint/defer at most one 72-column area.
    // No vector reserves these upper bounds; they fence growth arithmetic only.
    let replay = events.checked_mul(72)?;
    let origins = candidates.checked_add(replay)?;
    let states = candidates
        .checked_mul(4)?
        .checked_add(replay.checked_mul(4)?)?
        .checked_add(plane)?;
    let pending = origins
        .checked_mul(3)?
        .checked_add(replay.checked_mul(10)?)?
        .checked_add(events.checked_mul(20)?)?
        .checked_add(inputs.original.invocations().len())?;
    plane
        .checked_mul(std::mem::size_of::<Option<super::display_resolution_values::Cell>>() * 3)?
        .checked_add(origins.checked_mul(std::mem::size_of::<TeletextDisplayOrigin>())?)?
        .checked_add(states.checked_mul(std::mem::size_of::<State>())?)?
        .checked_add(pending.checked_mul(std::mem::size_of::<TeletextDisplayPending>())?)?
        .checked_add(
            super::display_resolution_writers::writer_count(inputs)
                .checked_mul(std::mem::size_of::<TeletextDisplayWriter>())?,
        )?
        .checked_add(events.checked_mul(128)?)?
        .checked_add(candidates.checked_mul(
            std::mem::size_of::<Option<State>>()
                + std::mem::size_of::<
                    Option<[Option<super::display_resolution_values::Selection>; 10]>,
                >()
                + 32,
        )?)
}

//! Candidate-aligned pre-geometry observations, not C.3 execution or footprints.

use std::mem::size_of;

use super::attribute_execution::area;
use super::display_geometry::validate_original_glyph_inputs;
use super::{
    TeletextAttributeExecutionError, TeletextAttributeExecutionFailure, TeletextBaseDisplay,
    TeletextDisplayGeometryFailure, TeletextExecutedAttributes, TeletextGlyphCandidate,
    TeletextGlyphLayers, TeletextGlyphPriority, TeletextSpatialTrace, TELETEXT_COLUMNS,
    TELETEXT_ROWS,
};

const CELLS: usize = TELETEXT_ROWS * 72;
const BASE_CANDIDATES: usize = TELETEXT_ROWS * TELETEXT_COLUMNS - 8;

/// A raw candidate is outside the broad logical attribute plane. Actual panel
/// enable/width and occupied footprint validity remain separate obligations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextGlyphAttributeObservationReason {
    Unaddressable,
}

/// One original candidate's pre-geometry attribute observation, including losers.
///
/// Non-Passive candidates share the after-Adaptive, before-Passive projection;
/// this is neither their individual rank's render stage nor Adaptive instance
/// history. Passive candidates retain their own fresh private invocation state
/// at the original glyph event, even outside the logical plane. Original event
/// and control writers remain authoritative for future C.3 replay, lower-row
/// suppression and footprint validation. None of these observations grant
/// placement, rendered validity or publication readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextExecutedGlyphAttributes {
    candidate_index: usize,
    candidate: TeletextGlyphCandidate,
    attributes: Option<TeletextExecutedAttributes>,
    reason: Option<TeletextGlyphAttributeObservationReason>,
}
impl TeletextExecutedGlyphAttributes {
    #[must_use]
    pub const fn candidate_index(&self) -> usize {
        self.candidate_index
    }
    #[must_use]
    pub const fn candidate(&self) -> &TeletextGlyphCandidate {
        &self.candidate
    }
    /// Pending handles refer to the containing execution's immutable registry.
    /// Unaddressable Passive candidates still retain their private attributes.
    #[must_use]
    pub const fn attributes(&self) -> Option<TeletextExecutedAttributes> {
        self.attributes
    }
    #[must_use]
    pub const fn reason(&self) -> Option<TeletextGlyphAttributeObservationReason> {
        self.reason
    }
}

pub(super) fn preflight_observations(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    layers: &TeletextGlyphLayers,
    max_commands: usize,
) -> Result<(), TeletextAttributeExecutionError> {
    let fail = |reason| TeletextAttributeExecutionError {
        owner: base.owner(),
        location: None,
        root_source: None,
        invocation: None,
        reason,
    };
    let maximum = BASE_CANDIDATES
        .checked_add(max_commands)
        .ok_or_else(|| fail(TeletextAttributeExecutionFailure::AllocationSize))?;
    let count = layers.candidates().len();
    if count > maximum {
        let candidate = &layers.candidates()[maximum];
        return Err(TeletextAttributeExecutionError {
            owner: base.owner(),
            location: candidate.source(),
            root_source: candidate.root_source(),
            invocation: candidate.invocation(),
            reason: TeletextAttributeExecutionFailure::CandidateBudget { count, maximum },
        });
    }
    // Every actual invocation has one original call event. Fence this raw count
    // before the shared correspondence helper allocates its seen-call scratch.
    if original.invocations().len() > original.events().len() {
        return Err(fail(TeletextAttributeExecutionFailure::OriginalSource(
            TeletextDisplayGeometryFailure::EventMismatch {
                event: original.events().len(),
            },
        )));
    }
    // Global projection plus both retained stage snapshots, private per-event
    // captures, candidate records and the shared validator's three bit vectors.
    // This is a checked allocation/work policy, not an arbitrary-input RSS claim.
    let storage = CELLS
        .checked_mul(size_of::<Option<TeletextExecutedAttributes>>() * 3)
        .and_then(|bytes| {
            original
                .events()
                .len()
                .checked_mul(size_of::<Option<TeletextExecutedAttributes>>())
                .and_then(|captures| bytes.checked_add(captures))
        })
        .and_then(|bytes| {
            count
                .checked_mul(size_of::<TeletextExecutedGlyphAttributes>())
                .and_then(|records| bytes.checked_add(records))
        })
        .and_then(|bytes| bytes.checked_add(original.events().len()))
        .and_then(|bytes| bytes.checked_add(original.invocations().len()))
        .and_then(|bytes| bytes.checked_add(TELETEXT_ROWS * TELETEXT_COLUMNS));
    let work = original
        .events()
        .len()
        .checked_mul(3)
        .and_then(|events| events.checked_add(count.checked_mul(2)?))
        .and_then(|work| work.checked_add(CELLS * 3))
        .and_then(|work| work.checked_add(original.invocations().len()));
    if storage.is_none_or(|bytes| bytes > isize::MAX as usize) || work.is_none() {
        return Err(fail(TeletextAttributeExecutionFailure::AllocationSize));
    }
    validate_original_glyph_inputs(base, original, layers).map_err(|error| {
        TeletextAttributeExecutionError {
            owner: error.owner,
            location: error.location,
            root_source: error.root_source,
            invocation: error.invocation,
            reason: match error.reason {
                TeletextDisplayGeometryFailure::CandidateMismatch { candidate } => {
                    TeletextAttributeExecutionFailure::CandidateMismatch { candidate }
                }
                other => TeletextAttributeExecutionFailure::OriginalSource(other),
            },
        }
    })
}

pub(super) fn observe_candidates(
    layers: &TeletextGlyphLayers,
    shared: &[Option<TeletextExecutedAttributes>],
    private: &[Option<TeletextExecutedAttributes>],
) -> Result<Vec<TeletextExecutedGlyphAttributes>, TeletextAttributeExecutionError> {
    let mut records = Vec::with_capacity(layers.candidates().len());
    for (candidate_index, &candidate) in layers.candidates().iter().enumerate() {
        let cell = area(candidate.position());
        let attributes = if candidate.priority() == TeletextGlyphPriority::Passive {
            candidate
                .event_ordinal()
                .and_then(|ordinal| private.get(ordinal))
                .copied()
                .flatten()
        } else {
            cell.and_then(|(row, column, _)| shared[row * 72 + column])
        };
        // Full correspondence was checked before execution. A Passive original
        // glyph must have been captured, regardless of placement/winner status.
        if attributes.is_none() && candidate.priority() == TeletextGlyphPriority::Passive {
            return Err(TeletextAttributeExecutionError {
                owner: layers.owner(),
                location: candidate.source(),
                root_source: candidate.root_source(),
                invocation: candidate.invocation(),
                reason: TeletextAttributeExecutionFailure::CandidateMismatch {
                    candidate: candidate_index,
                },
            });
        }
        records.push(TeletextExecutedGlyphAttributes {
            candidate_index,
            candidate,
            attributes,
            reason: cell
                .is_none()
                .then_some(TeletextGlyphAttributeObservationReason::Unaddressable),
        });
    }
    Ok(records)
}

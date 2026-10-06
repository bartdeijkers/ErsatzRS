//! Reconcile an externally timed candidate without moving its programme clocks.
//!
//! This policy consumes an accepted baseline. It does not read the source again,
//! publish a snapshot, or claim that a live producer has adopted the candidate.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use ersatzrs_core::scheduling::{PlayoutBuildMode, PlayoutPreservation};

use super::ExternalJsonSchedule;

/// Redacted refusal reasons: never include external media paths or Plex keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AdaptationError {
    #[error("external JSON programme preservation requires an accepted planning baseline")]
    MissingBaseline,
    #[error("external JSON Continue requires programme preservation")]
    InvalidPolicy,
    #[error("external JSON planning contains invalid or non-contiguous programme intervals")]
    InvalidTimeline,
    #[error("external JSON Continue cannot replace accepted programmes")]
    ChangedPrefix,
    #[error("external JSON Continue cannot shorten the accepted planning horizon")]
    ShortenedHorizon,
    #[error("external JSON Continue requires new programmes to join the accepted tail")]
    DisconnectedTail,
    #[error("external JSON guide visibility changes cannot preserve the accepted programmes")]
    ChangedGuideVisibility,
    #[error("external JSON planning has no safe shared programme boundary")]
    NoSafeBoundary,
}

/// A candidate ready for guarded persistence and producer publication.
pub struct Adaptation {
    pub schedule: ExternalJsonSchedule,
    pub changed: bool,
    /// First replacement/removal's external boundary; absent for a no-op or an
    /// empty initial baseline.
    /// The caller must still publish before its producer-prefetch deadline.
    pub effective_from: Option<DateTime<Utc>>,
}

/// Apply the explicit operator policy to a validated external timeline.
///
/// Continue keeps the full accepted horizon and only appends an exact adjoining
/// suffix. Preserving Refresh/Reset keep complete old programmes through a shared
/// absolute boundary at or after `not_before`. Reset cannot reset an external
/// producer's rotation or clock, so it adopts the same supplied future as Refresh.
/// No item is trimmed or re-timed to manufacture a boundary.
///
/// # Errors
/// Refuses missing baselines, invalid policy/timelines, rewritten Continue
/// prefixes, guide visibility changes during preservation, or an unsafe seam.
pub fn reconcile(
    accepted: Option<&ExternalJsonSchedule>,
    candidate: ExternalJsonSchedule,
    mode: PlayoutBuildMode,
    preservation: PlayoutPreservation,
    not_before: DateTime<Utc>,
) -> Result<Adaptation, AdaptationError> {
    validate(&candidate)?;
    if preservation == PlayoutPreservation::EngineDefault
        || (mode == PlayoutBuildMode::Continue && preservation != PlayoutPreservation::KeepCurrent)
    {
        return Err(AdaptationError::InvalidPolicy);
    }
    if preservation == PlayoutPreservation::ReplaceCurrent {
        let effective_from = candidate.items.first().map(|item| item.start).or_else(|| {
            accepted
                .and_then(|old| old.items.first())
                .map(|item| item.start)
        });
        return Ok(outcome(accepted, candidate, effective_from));
    }
    let accepted = accepted.ok_or(AdaptationError::MissingBaseline)?;
    validate(accepted)?;
    if candidate == *accepted {
        return Ok(outcome(Some(accepted), candidate, None));
    }
    if candidate.guide_minimum_duration_seconds != accepted.guide_minimum_duration_seconds {
        return Err(AdaptationError::ChangedGuideVisibility);
    }
    if mode == PlayoutBuildMode::Continue {
        return append(accepted, candidate);
    }
    preserve(accepted, candidate, not_before)
}

fn append(
    accepted: &ExternalJsonSchedule,
    mut candidate: ExternalJsonSchedule,
) -> Result<Adaptation, AdaptationError> {
    let Some(tail) = accepted.items.last().map(|item| item.finish) else {
        let effective_from = candidate.items.first().map(|item| item.start);
        return Ok(outcome(Some(accepted), candidate, effective_from));
    };
    if candidate.items.last().is_none_or(|item| item.finish < tail) {
        return Err(AdaptationError::ShortenedHorizon);
    }
    let by_start: BTreeMap<_, _> = accepted
        .items
        .iter()
        .map(|item| (item.start, item))
        .collect();
    for item in candidate.items.iter().take_while(|item| item.start < tail) {
        if by_start.get(&item.start).is_none_or(|old| **old != *item) {
            return Err(AdaptationError::ChangedPrefix);
        }
    }
    let first_new = candidate.items.iter().position(|item| item.start >= tail);
    if first_new.is_some_and(|index| candidate.items[index].start != tail) {
        return Err(AdaptationError::DisconnectedTail);
    }
    let mut items = accepted.items.clone();
    if let Some(index) = first_new {
        items.extend(candidate.items.drain(index..));
    }
    candidate.items = items;
    Ok(outcome(Some(accepted), candidate, first_new.map(|_| tail)))
}

fn preserve(
    accepted: &ExternalJsonSchedule,
    mut candidate: ExternalJsonSchedule,
    not_before: DateTime<Utc>,
) -> Result<Adaptation, AdaptationError> {
    // The source reader may reserve the next item even before its declared start.
    // Include that complete item when the protected clock precedes the schedule.
    let floor = accepted
        .items
        .iter()
        .find(|item| item.finish > not_before)
        .map_or(not_before, |item| item.finish);
    if candidate.items.is_empty() {
        let items: Vec<_> = accepted
            .items
            .iter()
            .take_while(|item| item.finish <= floor)
            .cloned()
            .collect();
        let effective_from = (items.len() != accepted.items.len())
            .then(|| items.last().map(|item| item.finish))
            .flatten();
        candidate.items = items;
        return Ok(outcome(Some(accepted), candidate, effective_from));
    }
    let finishes: BTreeSet<_> = accepted.items.iter().map(|item| item.finish).collect();
    let tail = accepted.items.last().map(|item| item.finish);
    let Some(index) = candidate.items.iter().position(|item| {
        item.start >= floor
            && (finishes.contains(&item.start) || tail.is_none_or(|tail| item.start >= tail))
    }) else {
        return Err(AdaptationError::NoSafeBoundary);
    };
    let boundary = candidate.items[index].start;
    let mut items: Vec<_> = accepted
        .items
        .iter()
        .take_while(|item| item.finish <= boundary)
        .cloned()
        .collect();
    // A hole cannot be represented by the external reader, whose active_item
    // selects the next finish even before an item's declared start. Refuse it.
    if items.last().is_some_and(|item| item.finish != boundary) {
        return Err(AdaptationError::NoSafeBoundary);
    }
    items.extend(candidate.items.drain(index..));
    candidate.items = items;
    Ok(outcome(Some(accepted), candidate, Some(boundary)))
}

fn outcome(
    accepted: Option<&ExternalJsonSchedule>,
    schedule: ExternalJsonSchedule,
    boundary: Option<DateTime<Utc>>,
) -> Adaptation {
    let changed = accepted.is_none_or(|accepted| *accepted != schedule);
    Adaptation {
        schedule,
        changed,
        effective_from: boundary.filter(|_| changed),
    }
}

pub(super) fn validate(schedule: &ExternalJsonSchedule) -> Result<(), AdaptationError> {
    if schedule.items.iter().any(|item| {
        item.program.duration <= 0
            || item.finish.signed_duration_since(item.start)
                != chrono::Duration::milliseconds(i64::from(item.program.duration))
    }) || schedule
        .items
        .windows(2)
        .any(|pair| pair[0].finish != pair[1].start)
    {
        return Err(AdaptationError::InvalidTimeline);
    }
    Ok(())
}

#[cfg(test)]
#[path = "adaptation_tests.rs"]
mod tests;

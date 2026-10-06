use chrono::{DateTime, Utc};

use crate::scheduling::{PlayoutBuildMode, SchedulingError, SchedulingRequest};

/// Validate Scripted intervals in emission order, without silently sorting them.
/// Forward waits are permitted; non-positive intervals and overlaps are not.
///
/// # Errors
/// Returns an actionable error when an interval precedes the allowed cursor,
/// overlaps its predecessor, or has no positive duration.
pub fn validate_scripted_timeline(
    intervals: impl IntoIterator<Item = (DateTime<Utc>, DateTime<Utc>)>,
    earliest_start: Option<DateTime<Utc>>,
) -> Result<(), SchedulingError> {
    let mut previous_finish = earliest_start;
    for (index, (start, finish)) in intervals.into_iter().enumerate() {
        if finish <= start {
            return Err(SchedulingError::InvalidScriptedTimeline(format!(
                "item {} has a non-positive interval ({start} to {finish}); correct the script duration",
                index + 1
            )));
        }
        if let Some(boundary) = previous_finish {
            if start < boundary {
                return Err(SchedulingError::InvalidScriptedTimeline(format!(
                    "item {} starts at {start}, before the preceding finish/cursor {boundary}; correct backward waits or overlapping items",
                    index + 1
                )));
            }
        }
        previous_finish = Some(finish);
    }
    Ok(())
}

pub(super) fn continuation_start(
    request: &SchedulingRequest,
) -> Result<DateTime<Utc>, SchedulingError> {
    if request.mode != PlayoutBuildMode::Continue {
        return Ok(request.start);
    }
    let mut intervals = request
        .reference_data
        .existing_items
        .iter()
        .map(|item| (item.start, item.finish))
        .collect::<Vec<_>>();
    intervals.sort_unstable();
    validate_scripted_timeline(intervals.iter().copied(), None)?;
    let tail = intervals.last().map(|(_, finish)| *finish);
    let anchor = request
        .playout
        .anchor
        .as_ref()
        .map(|anchor| anchor.next_start);
    if let (Some(tail), Some(anchor)) = (tail, anchor) {
        if tail > request.start && anchor != tail {
            return Err(SchedulingError::InvalidScriptedTimeline(format!(
                "persisted tail {tail} disagrees with anchor {anchor}; inspect the published timeline and restore consistent continuation state"
            )));
        }
    }
    Ok(tail.or(anchor).unwrap_or(request.start).max(request.start))
}

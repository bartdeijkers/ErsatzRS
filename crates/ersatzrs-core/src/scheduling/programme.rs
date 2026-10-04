use chrono::{DateTime, Utc};
use ersatzrs_domain::PlayoutItem;

use super::{SchedulingError, SchedulingReferenceData};

/// Choose a programme at the host's safe horizon, including any published gap
/// leading to it. A producer can already own the entire gap or guide interval;
/// inserting new content into its middle would lose the new programme opening.
pub(super) fn preservation_clock(
    reference: &SchedulingReferenceData,
    now: DateTime<Utc>,
) -> DateTime<Utc> {
    let Some(floor) = reference
        .preservation_not_before
        .filter(|floor| *floor > now)
    else {
        return now;
    };
    let items = &reference.existing_items;
    if items
        .iter()
        .any(|item| item.start <= floor && item.finish > floor)
    {
        return floor;
    }
    if let Some(item) = items
        .iter()
        .filter(|item| {
            item.guide_start.is_some_and(|start| start <= floor)
                && item.guide_finish.is_some_and(|finish| finish > floor)
        })
        .max_by_key(|item| item.start)
    {
        return item.start;
    }
    items
        .iter()
        .filter(|item| item.start >= floor)
        .map(|item| item.start)
        .min()
        .unwrap_or(floor)
}

/// Resolve the published programme's end, including contiguous rolls and split
/// rows. Refuse ambiguous intervals before choosing a retention seam.
pub(super) fn preserved_programme_finish(
    items: &[PlayoutItem],
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, SchedulingError> {
    let mut items = items.iter().collect::<Vec<_>>();
    items.sort_by_key(|item| (item.start, item.finish));
    if items.iter().any(|item| item.finish <= item.start)
        || items.windows(2).any(|pair| pair[1].start < pair[0].finish)
    {
        return Err(SchedulingError::InvalidRebuild(
            "cannot preserve a programme in a timeline with overlapping or non-positive intervals"
                .into(),
        ));
    }
    let Some(index) = items
        .iter()
        .position(|item| item.start <= now && item.finish > now)
    else {
        return Ok(now);
    };
    let current = items[index];
    let mut finish = current
        .finish
        .max(current.guide_finish.unwrap_or(current.finish));
    for item in &items[index + 1..] {
        if item.start < finish
            || (item.start == finish
                && current.guide_group > 0
                && item.guide_group == current.guide_group
                && item.guide_start == current.guide_start)
        {
            finish = finish
                .max(item.finish)
                .max(item.guide_finish.unwrap_or(item.finish));
        } else {
            break;
        }
    }
    Ok(finish)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    #[test]
    fn prefetch_horizon_preserves_long_gap_or_unfilled_guide_interval() {
        let now = Utc.with_ymd_and_hms(2026, 4, 15, 12, 0, 0).unwrap();
        let floor = now + Duration::minutes(3);
        let mut reference = SchedulingReferenceData {
            preservation_not_before: Some(floor),
            existing_items: vec![
                PlayoutItem {
                    start: now,
                    finish: now + Duration::minutes(1),
                    ..Default::default()
                },
                PlayoutItem {
                    start: now + Duration::hours(2),
                    finish: now + Duration::hours(3),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let clock = preservation_clock(&reference, now);
        assert_eq!(clock, now + Duration::hours(2));
        assert_eq!(
            preserved_programme_finish(&reference.existing_items, clock).unwrap(),
            now + Duration::hours(3)
        );
        reference.existing_items[0].guide_start = Some(now);
        reference.existing_items[0].guide_finish = Some(now + Duration::hours(2));
        let clock = preservation_clock(&reference, now);
        assert_eq!(clock, now);
        assert_eq!(
            preserved_programme_finish(&reference.existing_items, clock).unwrap(),
            now + Duration::hours(2)
        );
        reference.preservation_not_before = None;
        assert_eq!(
            preservation_clock(&reference, floor),
            floor,
            "offline rebuilds retain their existing clock semantics"
        );
    }
}

//! Shared guide occurrence identity and graphics programme selection.
//!
//! Source: `$ERSATZTV_SRC/ErsatzTV.Application/Channels/Commands/RefreshChannelDataHandler.cs`.

use std::collections::BTreeMap;
use std::num::NonZeroU8;

use chrono::{DateTime, Duration, Utc};
use ersatzrs_domain::{FillerKind, PlayoutScheduleKind};
use jiff::{tz::TimeZone, Span, Timestamp};

use crate::xmltv::{project_programmes, XmltvBlockBehavior, XmltvGuideItem, XmltvProgramme};

/// A guide row retaining the source identity discarded by XML serialization.
#[derive(Debug, Clone)]
pub struct ProgrammeGuideItem {
    pub playout_id: i64,
    pub item_id: i64,
    pub media_item_id: Option<i64>,
    pub schedule_kind: PlayoutScheduleKind,
    pub in_point: Duration,
    pub out_point: Duration,
    pub guide: XmltvGuideItem,
}

/// Actual content segment within a full guide programme; filler is excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgrammeSegment {
    pub item_id: i64,
    pub media_item_id: Option<i64>,
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub in_point: Duration,
    pub out_point: Duration,
}

/// A broadcast occurrence, distinct from the media identity or title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgrammeOccurrence {
    pub playout_id: i64,
    pub first_item_id: i64,
    pub programme: XmltvProgramme,
    pub segments: Vec<ProgrammeSegment>,
}

/// Apply scheduler-specific guide grouping without changing any playout state.
#[must_use]
pub fn project_occurrences(
    items: &[ProgrammeGuideItem],
    behavior: XmltvBlockBehavior,
) -> Vec<ProgrammeOccurrence> {
    let mut channels: BTreeMap<(&str, i64), Vec<&ProgrammeGuideItem>> = BTreeMap::new();
    for item in items {
        channels
            .entry((&item.guide.channel_number, item.playout_id))
            .or_default()
            .push(item);
    }
    let mut result = Vec::new();
    for mut rows in channels.into_values() {
        rows.sort_by_key(|row| (row.guide.start, row.item_id));
        if matches!(
            rows[0].schedule_kind,
            PlayoutScheduleKind::Block | PlayoutScheduleKind::None
        ) {
            result.extend(block_occurrences(&rows, behavior));
        } else {
            result.extend(grouped_occurrences(&rows));
        }
    }
    result.sort_by(|a, b| {
        a.programme
            .channel_number
            .cmp(&b.programme.channel_number)
            .then(a.programme.start.cmp(&b.programme.start))
            .then(a.playout_id.cmp(&b.playout_id))
            .then(a.first_item_id.cmp(&b.first_item_id))
    });
    result
}

fn segment(row: &ProgrammeGuideItem) -> ProgrammeSegment {
    ProgrammeSegment {
        item_id: row.item_id,
        media_item_id: row.media_item_id,
        start: row.guide.start,
        finish: row.guide.finish,
        in_point: row.in_point,
        out_point: row.out_point,
    }
}

fn grouped_occurrences(rows: &[&ProgrammeGuideItem]) -> Vec<ProgrammeOccurrence> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < rows.len() {
        let first = rows[index];
        if !matches!(
            first.guide.filler_kind,
            FillerKind::None | FillerKind::PreRoll
        ) {
            index += 1;
            continue;
        }
        let Some(display_index) = (index..rows.len())
            .find(|&candidate| rows[candidate].guide.filler_kind == FillerKind::None)
        else {
            break;
        };
        let display = rows[display_index];
        let mut end = display_index;
        while end + 1 < rows.len() {
            let next = &rows[end + 1].guide;
            if next.guide_group != first.guide.guide_group
                && !matches!(
                    next.filler_kind,
                    FillerKind::GuideMode
                        | FillerKind::PostRoll
                        | FillerKind::Tail
                        | FillerKind::Fallback
                        | FillerKind::DecoDefault
                )
            {
                break;
            }
            end += 1;
        }
        result.push(ProgrammeOccurrence {
            playout_id: first.playout_id,
            first_item_id: first.item_id,
            programme: XmltvProgramme {
                channel_number: first.guide.channel_number.clone(),
                start: first.guide.start,
                stop: display.guide.guide_finish.unwrap_or(rows[end].guide.finish),
                custom_title: first.guide.custom_title.clone(),
                title: display.guide.title.clone(),
                content: display.guide.content.clone(),
            },
            segments: rows[display_index..=end]
                .iter()
                .filter(|row| row.guide.filler_kind == FillerKind::None)
                .map(|row| segment(row))
                .collect(),
        });
        index = end + 1;
    }
    result
}

fn block_occurrences(
    rows: &[&ProgrammeGuideItem],
    behavior: XmltvBlockBehavior,
) -> Vec<ProgrammeOccurrence> {
    type GroupKey = (DateTime<Utc>, DateTime<Utc>, i32);
    let mut groups: BTreeMap<GroupKey, Vec<&ProgrammeGuideItem>> = BTreeMap::new();
    for row in rows {
        if row.guide.filler_kind != FillerKind::None {
            continue;
        }
        let guide = &row.guide;
        groups
            .entry((
                guide.guide_start.unwrap_or(guide.start),
                guide.guide_finish.unwrap_or(guide.finish),
                guide.guide_group,
            ))
            .or_default()
            .push(row);
    }
    let mut result = Vec::new();
    for ((guide_start, guide_finish, _), group) in groups {
        if guide_finish <= guide_start {
            continue;
        }
        let mut guides: Vec<XmltvGuideItem> = Vec::new();
        let mut members: Vec<Vec<ProgrammeSegment>> = Vec::new();
        let mut identities = Vec::new();
        for row in group {
            let continues = members
                .last()
                .and_then(|members| members.last())
                .is_some_and(|last| {
                    last.media_item_id.is_some()
                        && last.media_item_id == row.media_item_id
                        && last.out_point == row.in_point
                        && row.in_point > Duration::zero()
                        && last.finish <= row.guide.start
                });
            if continues {
                let guide = guides
                    .last_mut()
                    .expect("members and guides remain aligned");
                guide.finish = row.guide.finish;
                members
                    .last_mut()
                    .expect("continuation has a segment")
                    .push(segment(row));
            } else {
                guides.push(row.guide.clone());
                members.push(vec![segment(row)]);
                identities.push((row.playout_id, row.item_id));
            }
        }
        // Preserve the configured guide-window math after coalescing proven
        // source-contiguous fragments. A repeated film starting at zero is separate.
        if behavior == XmltvBlockBehavior::SplitTimeEvenly
            && usize::try_from((guide_finish - guide_start).num_milliseconds())
                .is_ok_and(|milliseconds| milliseconds < guides.len())
        {
            continue;
        }
        for ((programme, segments), (playout_id, first_item_id)) in
            project_programmes(&guides, behavior)
                .into_iter()
                .zip(members)
                .zip(identities)
        {
            result.push(ProgrammeOccurrence {
                playout_id,
                first_item_id,
                programme,
                segments,
            });
        }
    }
    result
}

/// Optional programme roles at one explicit broadcast instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgrammeSelection<'a> {
    pub current: Option<&'a ProgrammeOccurrence>,
    pub next: Option<&'a ProgrammeOccurrence>,
    pub continuing: Option<&'a ProgrammeOccurrence>,
    pub later_today: Vec<&'a ProgrammeOccurrence>,
    pub ambiguous: bool,
}

/// Select half-open guide windows; never guess through overlapping programmes.
#[must_use]
pub fn select_programmes(
    programmes: &[ProgrammeOccurrence],
    now: DateTime<Utc>,
    day_finish: DateTime<Utc>,
    limit: usize,
) -> ProgrammeSelection<'_> {
    let active: Vec<_> = programmes
        .iter()
        .filter(|item| item.programme.start <= now && now < item.programme.stop)
        .collect();
    let ambiguous = active.len() > 1;
    let current = (active.len() == 1).then(|| active[0]);
    let mut future: Vec<_> = programmes
        .iter()
        .filter(|item| {
            item.programme.start > now
                && item.programme.stop > item.programme.start
                && current.is_none_or(|current| item.programme.start >= current.programme.stop)
        })
        .collect();
    future.sort_by_key(|item| (item.programme.start, item.playout_id, item.first_item_id));
    let future_ambiguous = future
        .first()
        .zip(future.get(1))
        .is_some_and(|(first, second)| second.programme.start < first.programme.stop);
    let next = if ambiguous || future_ambiguous {
        None
    } else {
        future.first().copied()
    };
    let continuing = current.filter(|item| {
        let previous = item
            .segments
            .iter()
            .rev()
            .find(|segment| segment.finish <= now);
        let following = item.segments.iter().find(|segment| segment.start > now);
        !item
            .segments
            .iter()
            .any(|segment| segment.start <= now && now < segment.finish)
            && previous
                .zip(following)
                .is_some_and(|(previous, following)| {
                    previous.media_item_id.is_some()
                        && previous.media_item_id == following.media_item_id
                        && previous.out_point == following.in_point
                        && following.in_point > Duration::zero()
                })
    });
    ProgrammeSelection {
        current,
        next,
        continuing,
        later_today: if ambiguous || future_ambiguous {
            Vec::new()
        } else {
            future
                .into_iter()
                .filter(|item| item.programme.start < day_finish)
                .take(limit.min(32))
                .collect()
        },
        ambiguous: ambiguous || future_ambiguous,
    }
}

/// Civil calendar clock shared by the occurrence preview and native graphics.
#[derive(Debug, Clone)]
pub struct ProgrammeClock(TimeZone);

impl ProgrammeClock {
    /// Resolve the same host civil timezone used for channel scheduling.
    ///
    /// # Errors
    /// Returns an error if the OS timezone is unavailable.
    pub fn system() -> Result<Self, jiff::Error> {
        TimeZone::try_system().map(Self)
    }

    /// Resolve a named clock for reproducible render contexts.
    ///
    /// # Errors
    /// Returns an error for an unavailable IANA zone.
    pub fn named(name: &str) -> Result<Self, jiff::Error> {
        TimeZone::get(name).map(Self)
    }

    /// Genuine resolved IANA identity, including an explicitly resolved UTC clock.
    /// Unnamed `TZif`, nonzero fixed offsets, POSIX and unknown clocks return `None`;
    /// callers requiring a named request context must reject rather than guess.
    #[must_use]
    pub fn iana_name(&self) -> Option<&str> {
        self.0.iana_name()
    }

    /// Return UTC boundaries of the instant's local calendar day.
    ///
    /// # Errors
    /// Returns an error for timestamps outside Jiff's supported range.
    ///
    /// # Panics
    /// Panics if a valid Jiff timestamp cannot be represented by Chrono, whose
    /// supported year range includes Jiff's entire supported range.
    pub fn day_bounds(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), jiff::Error> {
        let zoned = Timestamp::new(now.timestamp(), 0)?.to_zoned(self.0.clone());
        let start = zoned.start_of_day()?.timestamp();
        let finish = zoned
            .date()
            .tomorrow()?
            .to_zoned(self.0.clone())?
            .timestamp();
        Ok((
            DateTime::from_timestamp(start.as_second(), 0).expect("Jiff fits Chrono"),
            DateTime::from_timestamp(finish.as_second(), 0).expect("Jiff fits Chrono"),
        ))
    }

    /// Return UTC bounds for one through 255 civil dates starting with today.
    ///
    /// The end date is computed directly from the capture instant's local date,
    /// rather than adding elapsed hours or advancing through zoned days. This
    /// preserves civil date counting across skipped dates. Jiff's compatible
    /// gap/fold resolution and `start_of_day` select each actual boundary; an
    /// entirely skipped end date resolves to the following existing date.
    ///
    /// # Errors
    /// Returns an error for unsupported timestamps (including leap seconds) or
    /// end dates, or if resolution produces an empty or reversed interval.
    ///
    /// # Panics
    /// Panics if a valid Jiff timestamp cannot be represented by Chrono, whose
    /// supported year range includes Jiff's entire supported range.
    pub fn calendar_day_bounds(
        &self,
        captured_at: DateTime<Utc>,
        days: NonZeroU8,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), jiff::Error> {
        if captured_at.timestamp_subsec_nanos() >= 1_000_000_000 {
            return Err(jiff::Error::from_args(format_args!(
                "civil calendar capture cannot represent a leap second"
            )));
        }
        let nanoseconds = i32::try_from(captured_at.timestamp_subsec_nanos()).map_err(|_| {
            jiff::Error::from_args(format_args!(
                "civil calendar capture nanoseconds are unsupported"
            ))
        })?;
        // This constructor checks Jiff's seconds range before construction.
        let zoned = Timestamp::new(captured_at.timestamp(), nanoseconds)?.to_zoned(self.0.clone());
        let start = zoned.start_of_day()?.timestamp();
        let finish = zoned
            .date()
            .checked_add(Span::new().days(days.get()))?
            .to_zoned(self.0.clone())?
            .start_of_day()?
            .timestamp();
        if finish <= start {
            return Err(jiff::Error::from_args(format_args!(
                "civil calendar window must have increasing UTC boundaries"
            )));
        }
        Ok((
            DateTime::from_timestamp(start.as_second(), 0).expect("Jiff fits Chrono"),
            DateTime::from_timestamp(finish.as_second(), 0).expect("Jiff fits Chrono"),
        ))
    }

    /// Format an existing instant in the channel's local clock, retaining its offset.
    ///
    /// # Errors
    /// Returns an error outside Jiff's supported range.
    pub fn local_time(&self, now: DateTime<Utc>) -> Result<String, jiff::Error> {
        Ok(Timestamp::new(now.timestamp(), 0)?
            .to_zoned(self.0.clone())
            .strftime("%Y-%m-%dT%H:%M:%S%:z")
            .to_string())
    }
}

#[cfg(test)]
#[path = "programme_tests.rs"]
mod tests;

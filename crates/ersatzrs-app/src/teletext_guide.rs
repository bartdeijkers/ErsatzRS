//! Supplied guide context to bounded request bytes, without acquisition (ADR 0121).
//!
//! Callers must supply complete grouped occurrences and an authoritative locale
//! allowlist. This builder cannot establish guide completeness, source revision,
//! generation ownership, invocation authority or publication readiness.

use std::{collections::BTreeSet, num::NonZeroU8};

use chrono::{DateTime, Duration, Utc};
use ersatzrs_addon_contract as wire;
use ersatzrs_core::programme::{
    select_programmes, ProgrammeClock, ProgrammeOccurrence, ProgrammeSegment,
};

/// Read-only channel metadata and complete, already grouped guide occurrences.
pub struct TeletextGuideChannel<'a> {
    pub id: i64,
    pub number: &'a str,
    pub name: &'a str,
    pub programmes: &'a [ProgrammeOccurrence],
}

/// Explicit host context; locale membership never normalizes or falls back.
pub struct SuppliedTeletextGuideContext<'a> {
    pub generation: &'a str,
    pub locale: &'a str,
    /// Canonical values supplied by the host's authoritative locale boundary.
    pub supported_locales: &'a [&'a str],
    pub timezone: &'a str,
    pub captured_at: DateTime<Utc>,
    pub channels: &'a [TeletextGuideChannel<'a>],
}

/// Static failure without channel names, programme text or correlation values.
#[derive(Debug, thiserror::Error)]
pub enum TeletextGuideRequestError {
    #[error(transparent)]
    Contract(#[from] wire::TeletextPagesRequestError),
    #[error("teletext guide locale is not an explicitly supported canonical value")]
    UnsupportedLocale,
    #[error("teletext guide timezone cannot be resolved as a named clock")]
    Timezone,
    #[error("teletext guide calendar window cannot be represented")]
    CalendarWindow,
    #[error("teletext guide channel identity is invalid or repeated")]
    ChannelIdentity,
    #[error("teletext guide occurrence identity is invalid or repeated within its channel")]
    OccurrenceIdentity,
    #[error("teletext guide programme interval is invalid")]
    ProgrammeInterval,
    #[error("teletext guide segment identity is invalid or repeated within its occurrence")]
    SegmentIdentity,
    #[error("teletext guide segment intervals are invalid or unordered")]
    SegmentInterval,
    #[error("teletext guide source offsets are invalid")]
    SourceOffsets,
    #[error("teletext guide source offsets cannot be represented in exact milliseconds")]
    SourceOffsetPrecision,
}

/// Build one complete request for three civil dates, preserving grouped content.
///
/// Whole occurrences intersecting the half-open window retain their complete
/// guide intervals and actual segments. Roles use the shared selector, with
/// Later Today bounded separately to the capture date and 32 references. Empty
/// guides remain explicit; no editorial titles or missing source data are filled.
///
/// # Errors
/// Rejects unsupported context, invalid selected records, inexact source offsets
/// and count/serialized-byte overflow. No partial request bytes are returned.
pub fn build_request(
    context: &SuppliedTeletextGuideContext<'_>,
) -> Result<Vec<u8>, TeletextGuideRequestError> {
    build_request_for_schema(context, wire::TELETEXT_PAGES_V1_REQUEST_SCHEMA_VERSION)
}

/// Build the unchanged guide request under a trusted explicit schema 1, 2 or 3.
///
/// Host selection precedes conversion. Civil dates, exact source offsets,
/// programme roles/content and all input limits are identical for all versions.
/// This does not negotiate an add-on capability or grant invocation authority.
/// # Errors
/// Returns `UnsupportedSchema` for any other version and the same context,
/// semantic, count and serialized-byte failures as `build_request`.
pub fn build_request_for_schema(
    context: &SuppliedTeletextGuideContext<'_>,
    expected_schema: u32,
) -> Result<Vec<u8>, TeletextGuideRequestError> {
    if !matches!(
        expected_schema,
        wire::TELETEXT_PAGES_V1_REQUEST_SCHEMA_VERSION
            | wire::TELETEXT_PAGES_V2_REQUEST_SCHEMA_VERSION
            | wire::TELETEXT_PAGES_V3_REQUEST_SCHEMA_VERSION
    ) {
        return Err(wire::TeletextPagesRequestError::UnsupportedSchema.into());
    }
    if context.locale.is_empty() || !context.supported_locales.contains(&context.locale) {
        return Err(TeletextGuideRequestError::UnsupportedLocale);
    }
    if context.generation.is_empty()
        || context.generation.len() > wire::MAX_TELETEXT_PAGES_GENERATION_BYTES
    {
        return Err(wire::TeletextPagesRequestError::InvalidGeneration.into());
    }
    let clock =
        ProgrammeClock::named(context.timezone).map_err(|_| TeletextGuideRequestError::Timezone)?;
    let (start, finish) = clock
        .calendar_day_bounds(
            context.captured_at,
            NonZeroU8::new(3).ok_or(TeletextGuideRequestError::CalendarWindow)?,
        )
        .map_err(|_| TeletextGuideRequestError::CalendarWindow)?;
    let (_, day_finish) = clock
        .calendar_day_bounds(context.captured_at, NonZeroU8::MIN)
        .map_err(|_| TeletextGuideRequestError::CalendarWindow)?;
    check_record_bounds(context.channels, start, finish)?;
    let mut channel_ids = BTreeSet::new();
    let channels = context
        .channels
        .iter()
        .map(|channel| {
            if channel.id <= 0 || !channel_ids.insert(channel.id) {
                return Err(TeletextGuideRequestError::ChannelIdentity);
            }
            convert_channel(channel, context.captured_at, start, finish, day_finish)
        })
        .collect::<Result<_, _>>()?;
    Ok(wire::TeletextPagesRequest {
        schema_version: expected_schema,
        generation: context.generation.to_owned(),
        locale: context.locale.to_owned(),
        timezone: context.timezone.to_owned(),
        captured_at: context.captured_at,
        window: wire::TeletextGuideWindowWire { start, finish },
        channels,
    }
    .to_json_for_schema(expected_schema)?)
}

fn intersects(
    programme: &ProgrammeOccurrence,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> bool {
    programme.programme.start < finish && programme.programme.stop > start
}

fn check_record_bounds(
    channels: &[TeletextGuideChannel<'_>],
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
) -> Result<(), TeletextGuideRequestError> {
    if channels.len() > wire::MAX_TELETEXT_PAGES_CHANNELS {
        return Err(wire::TeletextPagesRequestError::ChannelCount.into());
    }
    let mut count = 0usize;
    for channel in channels {
        for _ in channel
            .programmes
            .iter()
            .filter(|item| intersects(item, start, finish))
        {
            count = count
                .checked_add(1)
                .filter(|value| *value <= wire::MAX_TELETEXT_PAGES_OCCURRENCES)
                .ok_or(wire::TeletextPagesRequestError::OccurrenceCount)?;
        }
    }
    Ok(())
}

fn reference(item: &ProgrammeOccurrence) -> wire::TeletextProgrammeReferenceWire {
    wire::TeletextProgrammeReferenceWire {
        playout_id: item.playout_id,
        first_item_id: item.first_item_id,
    }
}

fn convert_channel(
    channel: &TeletextGuideChannel<'_>,
    captured_at: DateTime<Utc>,
    start: DateTime<Utc>,
    finish: DateTime<Utc>,
    day_finish: DateTime<Utc>,
) -> Result<wire::TeletextGuideChannelWire, TeletextGuideRequestError> {
    let selected: Vec<_> = channel
        .programmes
        .iter()
        .filter(|item| intersects(item, start, finish))
        .cloned()
        .collect();
    let mut identities = BTreeSet::new();
    for item in &selected {
        if item.playout_id <= 0
            || item.first_item_id < 0
            || item.programme.channel_number != channel.number
            || !identities.insert((item.playout_id, item.first_item_id))
        {
            return Err(TeletextGuideRequestError::OccurrenceIdentity);
        }
        if item.programme.stop <= item.programme.start {
            return Err(TeletextGuideRequestError::ProgrammeInterval);
        }
    }
    let selection = select_programmes(&selected, captured_at, day_finish, 32);
    let context = wire::TeletextProgrammeContextWire {
        current: selection.current.map(reference),
        next: selection.next.map(reference),
        continuing: selection.continuing.map(reference),
        later_today: selection.later_today.into_iter().map(reference).collect(),
        ambiguous: selection.ambiguous,
    };
    let programmes = selected
        .iter()
        .map(convert_programme)
        .collect::<Result<_, _>>()?;
    Ok(wire::TeletextGuideChannelWire {
        id: channel.id,
        number: channel.number.to_owned(),
        name: channel.name.to_owned(),
        programmes,
        context,
    })
}

fn convert_programme(
    item: &ProgrammeOccurrence,
) -> Result<wire::TeletextProgrammeWire, TeletextGuideRequestError> {
    let mut ids = BTreeSet::new();
    let mut previous_finish = None;
    let mut segments = Vec::new();
    for segment in &item.segments {
        if segment.item_id < 0
            || segment.media_item_id.is_some_and(|id| id <= 0)
            || !ids.insert(segment.item_id)
        {
            return Err(TeletextGuideRequestError::SegmentIdentity);
        }
        if segment.finish <= segment.start
            || previous_finish.is_some_and(|finish| finish > segment.start)
        {
            return Err(TeletextGuideRequestError::SegmentInterval);
        }
        previous_finish = Some(segment.finish);
        segments.push(convert_segment(segment)?);
    }
    Ok(wire::TeletextProgrammeWire {
        identity: reference(item),
        start: item.programme.start,
        stop: item.programme.stop,
        title: item.programme.title.clone(),
        custom_title: item.programme.custom_title.clone(),
        sub_title: item.programme.content.sub_title.clone(),
        description: item.programme.content.description.clone(),
        segments,
    })
}

fn convert_segment(
    segment: &ProgrammeSegment,
) -> Result<wire::TeletextProgrammeSegmentWire, TeletextGuideRequestError> {
    let unknown = segment.media_item_id.is_none()
        && segment.in_point == Duration::zero()
        && segment.out_point == Duration::zero();
    if segment.in_point < Duration::zero() || (!unknown && segment.out_point <= segment.in_point) {
        return Err(TeletextGuideRequestError::SourceOffsets);
    }
    Ok(wire::TeletextProgrammeSegmentWire {
        item_id: segment.item_id,
        media_item_id: segment.media_item_id,
        start: segment.start,
        finish: segment.finish,
        in_point_milliseconds: exact_milliseconds(segment.in_point)?,
        out_point_milliseconds: exact_milliseconds(segment.out_point)?,
    })
}

fn exact_milliseconds(value: Duration) -> Result<i64, TeletextGuideRequestError> {
    let milliseconds = value.num_milliseconds();
    // Chrono's supported duration range excludes i64::MIN milliseconds.
    if Duration::milliseconds(milliseconds) != value {
        return Err(TeletextGuideRequestError::SourceOffsetPrecision);
    }
    Ok(milliseconds)
}

#[cfg(test)]
#[path = "teletext_guide_tests.rs"]
mod tests;

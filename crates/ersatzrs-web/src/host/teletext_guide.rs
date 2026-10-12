//! Transaction-owned host guide context for explicit unpublished preparation.
//! This does not establish source freshness, trusted generation ownership,
//! mutable-file provenance or automatic service/publication authority.

use std::{fmt, num::NonZeroU8, time::Instant};

use chrono::{DateTime, Utc};
use ersatzrs_app::{
    teletext_guide::{
        build_request_for_schema, SuppliedTeletextGuideContext, TeletextGuideChannel,
        TeletextGuideRequestError,
    },
    ApplicationServices,
};
use ersatzrs_core::programme::ProgrammeClock;
use ersatzrs_domain::EntityId;
use ersatzrs_infra::{
    db::DbPool,
    xmltv::{capture_guide_snapshot, GuideReadBudget, GuideSnapshot, GuideSnapshotError},
};

use super::teletext_channel_documents::{
    prepare_channel_teletext_document_candidate, ChannelTeletextDocumentEncodingError,
    EncodedChannelTeletextDocumentCandidate, TeletextChannelDocumentEncodingPolicy,
};

/// Static/numeric failures; no source text, paths or unsupported settings escape.
#[derive(Debug, thiserror::Error)]
pub enum TeletextGuideCaptureError {
    #[error("teletext guide capture exceeded its original deadline")]
    TimedOut,
    #[error("teletext guide capture requires at most 256 selected channels")]
    ChannelLimit,
    #[error("teletext guide host clock has no resolvable IANA identity")]
    Clock,
    #[error("teletext guide host civil window cannot be represented")]
    CalendarWindow,
    #[error("teletext guide host locale is missing or unsupported")]
    Locale,
    #[error("teletext guide read transaction failed")]
    Database,
    #[error(transparent)]
    Snapshot(#[from] GuideSnapshotError),
    #[error(transparent)]
    Request(#[from] TeletextGuideRequestError),
    #[error(transparent)]
    Candidate(#[from] ChannelTeletextDocumentEncodingError),
}

/// Owned database guide and resolved host context under the caller's deadline.
/// SQL facts remain at the captured revision; subsequent preparation is inert.
pub struct CapturedTeletextGuide {
    snapshot: GuideSnapshot,
    locale: &'static str,
    timezone: String,
    captured_at: DateTime<Utc>,
    deadline: Instant,
}

impl CapturedTeletextGuide {
    #[must_use]
    pub fn snapshot(&self) -> &GuideSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub const fn locale(&self) -> &'static str {
        self.locale
    }

    #[must_use]
    pub fn timezone(&self) -> &str {
        &self.timezone
    }

    /// Serialize the captured guide with the existing complete request validator.
    /// # Errors
    /// Rejects an expired deadline or invalid/unrepresentable request context.
    pub fn request_bytes(
        &self,
        generation: &str,
        schema: u32,
    ) -> Result<Vec<u8>, TeletextGuideCaptureError> {
        check_deadline(self.deadline)?;
        let channels = self.borrow_channels();
        let locales = supported_locales();
        let bytes =
            build_request_for_schema(&self.context(generation, &channels, &locales), schema)?;
        check_deadline(self.deadline)?;
        Ok(bytes)
    }

    /// Invoke the existing installed-profile path and encode an unpublished candidate.
    /// Source acquisition does not promote pages or install/enable any package.
    /// The outer deadline bounds this await and can only shorten the candidate's
    /// lifetime. Existing producer/CPU jobs retain their own bounded ownership;
    /// abandoning this await does not synchronously stop running CPU work.
    /// # Errors
    /// Rejects the original deadline or existing selection/invocation/encoding failures.
    pub async fn prepare_document_candidate(
        &self,
        application: &ApplicationServices,
        output_channel_id: EntityId,
        entry_addon_id: Option<&str>,
        generation: &str,
        policy: TeletextChannelDocumentEncodingPolicy,
    ) -> Result<EncodedChannelTeletextDocumentCandidate, TeletextGuideCaptureError> {
        check_deadline(self.deadline)?;
        let channels = self.borrow_channels();
        let locales = supported_locales();
        let context = self.context(generation, &channels, &locales);
        let candidate = tokio::time::timeout_at(
            tokio::time::Instant::from_std(self.deadline),
            prepare_channel_teletext_document_candidate(
                application,
                output_channel_id,
                entry_addon_id,
                &context,
                policy,
            ),
        )
        .await
        .map_err(|_| TeletextGuideCaptureError::TimedOut)??;
        check_deadline(self.deadline)?;
        Ok(candidate.restrict_deadline(self.deadline))
    }

    fn borrow_channels(&self) -> Vec<TeletextGuideChannel<'_>> {
        self.snapshot
            .channels()
            .iter()
            .map(|channel| TeletextGuideChannel {
                id: channel.id(),
                number: channel.number(),
                name: channel.name(),
                programmes: channel.occurrences(),
            })
            .collect()
    }

    fn context<'a>(
        &'a self,
        generation: &'a str,
        channels: &'a [TeletextGuideChannel<'a>],
        locales: &'a [&'a str],
    ) -> SuppliedTeletextGuideContext<'a> {
        SuppliedTeletextGuideContext {
            generation,
            locale: self.locale,
            supported_locales: locales,
            timezone: &self.timezone,
            captured_at: self.captured_at,
            channels,
        }
    }
}

impl fmt::Debug for CapturedTeletextGuide {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CapturedTeletextGuide")
            .field("channels", &self.snapshot.channels().len())
            .field("locale", &self.locale)
            .field("timezone", &self.timezone)
            .finish_non_exhaustive()
    }
}

/// Capture a complete selected-channel guide using the actual named host clock.
/// Three civil dates use the same clock as scheduling, never `xmltv.time_zone`.
/// # Errors
/// Rejects unnamed clocks, unknown/missing locales, source/model limits, invalid
/// source data and the caller's absolute deadline. No partial guide is returned.
pub async fn capture_teletext_guide(
    pool: &DbPool,
    channel_ids: &[i64],
    captured_at: DateTime<Utc>,
    budget: &mut GuideReadBudget,
    deadline: Instant,
) -> Result<CapturedTeletextGuide, TeletextGuideCaptureError> {
    check_deadline(deadline)?;
    let clock = ProgrammeClock::system().map_err(|_| TeletextGuideCaptureError::Clock)?;
    capture_with_clock(pool, channel_ids, captured_at, budget, deadline, &clock).await
}

async fn capture_with_clock(
    pool: &DbPool,
    channel_ids: &[i64],
    captured_at: DateTime<Utc>,
    budget: &mut GuideReadBudget,
    deadline: Instant,
    clock: &ProgrammeClock,
) -> Result<CapturedTeletextGuide, TeletextGuideCaptureError> {
    check_deadline(deadline)?;
    if channel_ids.len() > ersatzrs_addon_contract::MAX_TELETEXT_PAGES_CHANNELS {
        return Err(TeletextGuideCaptureError::ChannelLimit);
    }
    let timezone = clock.iana_name().ok_or(TeletextGuideCaptureError::Clock)?;
    let (start, finish) = clock
        .calendar_day_bounds(
            captured_at,
            NonZeroU8::new(3).ok_or(TeletextGuideCaptureError::CalendarWindow)?,
        )
        .map_err(|_| TeletextGuideCaptureError::CalendarWindow)?;
    let captured = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), async {
        // This read-only transaction captures channels, raw settings and all sources.
        let mut transaction = pool
            .begin()
            .await
            .map_err(|_| TeletextGuideCaptureError::Database)?;
        let snapshot =
            capture_guide_snapshot(&mut transaction, channel_ids, start, finish, budget).await?;
        let locale = resolve_locale(snapshot.page_language())?;
        transaction
            .rollback()
            .await
            .map_err(|_| TeletextGuideCaptureError::Database)?;
        Ok::<_, TeletextGuideCaptureError>(CapturedTeletextGuide {
            snapshot,
            locale,
            timezone: timezone.to_owned(),
            captured_at,
            deadline,
        })
    })
    .await
    .map_err(|_| TeletextGuideCaptureError::TimedOut)??;
    // Timeout polls the inner future first; synchronous work must not return late.
    check_deadline(deadline)?;
    Ok(captured)
}

fn resolve_locale(raw: Option<&str>) -> Result<&'static str, TeletextGuideCaptureError> {
    // Catalog tokens are tiny; reject corrupt storage before allocating normalization.
    let raw = raw
        .filter(|value| value.len() <= 64)
        .ok_or(TeletextGuideCaptureError::Locale)?;
    let normalized = raw.trim().to_ascii_lowercase();
    crate::admin::languages::LANGUAGES
        .iter()
        .find(|definition| {
            definition.code == normalized || definition.aliases.contains(&normalized.as_str())
        })
        .map(|definition| definition.code)
        .ok_or(TeletextGuideCaptureError::Locale)
}

fn supported_locales() -> Vec<&'static str> {
    crate::admin::languages::LANGUAGES
        .iter()
        .map(|definition| definition.code)
        .collect()
}

fn check_deadline(deadline: Instant) -> Result<(), TeletextGuideCaptureError> {
    if Instant::now() >= deadline {
        Err(TeletextGuideCaptureError::TimedOut)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "teletext_guide_tests.rs"]
mod tests;

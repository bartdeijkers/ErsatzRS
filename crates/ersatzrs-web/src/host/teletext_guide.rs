//! Transaction-owned host guide context for explicit unpublished preparation.
//! Rechecks compare effective guide context at invocation boundaries, not raw-source
//! freshness, atomic publication, trusted generation ownership,
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
use ersatzrs_core::{
    programme::ProgrammeClock,
    repositories::{TeletextGuideSourceFence, TELETEXT_GUIDE_SOURCE_FENCE_VERSION},
};
use ersatzrs_domain::EntityId;
use ersatzrs_infra::{
    db::DbPool,
    xmltv::{
        capture_guide_snapshot, GuideReadBudget, GuideSnapshot, GuideSnapshotComparison,
        GuideSnapshotDigestError, GuideSnapshotDigestLimits, GuideSnapshotError,
        GUIDE_SNAPSHOT_DIGEST_VERSION,
    },
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
    #[error("teletext guide context changed for channel {channel_id:?}")]
    SourceChanged { channel_id: Option<i64> },
    #[error("teletext guide read transaction failed")]
    Database,
    #[error("teletext guide fingerprint worker failed")]
    Worker,
    #[error("teletext guide source association is invalid")]
    Association,
    #[error(transparent)]
    Digest(#[from] GuideSnapshotDigestError),
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
    pool: DbPool,
    snapshot: GuideSnapshot,
    locale: &'static str,
    timezone: String,
    captured_at: DateTime<Utc>,
    deadline: Instant,
    guide_source: Option<TeletextGuideSourceFence>,
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
    #[must_use]
    pub const fn guide_source(&self) -> Option<&TeletextGuideSourceFence> {
        self.guide_source.as_ref()
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
    /// Before and after invocation, recapture the same civil window and compare
    /// effective guide context using the caller's cumulative source/model budget.
    /// This does not fence intervening mutations or authorize later publication.
    /// # Errors
    /// Rejects changed context, exhausted read budgets, the original deadline or
    /// existing selection/invocation/encoding failures.
    pub async fn prepare_document_candidate(
        &self,
        application: &ApplicationServices,
        output_channel_id: EntityId,
        entry_addon_id: Option<&str>,
        generation: &str,
        policy: TeletextChannelDocumentEncodingPolicy,
        budget: &mut GuideReadBudget,
    ) -> Result<EncodedChannelTeletextDocumentCandidate, TeletextGuideCaptureError> {
        self.recheck_context(budget).await?;
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
        self.recheck_context(budget).await?;
        let candidate = candidate.restrict_deadline(self.deadline);
        let candidate = match &self.guide_source {
            Some(source) if candidate.encoded().is_some() => {
                candidate.with_guide_source(source.clone())
            }
            // Requestless Off remains inert and cannot establish a guide-input association.
            _ => candidate,
        };
        check_deadline(self.deadline)?;
        Ok(candidate)
    }

    async fn recheck_context(
        &self,
        budget: &mut GuideReadBudget,
    ) -> Result<(), TeletextGuideCaptureError> {
        check_deadline(self.deadline)?;
        // The originally admitted channel count is bounded by the capture limit.
        let channel_ids: Vec<_> = self
            .snapshot
            .channels()
            .iter()
            .map(ersatzrs_infra::xmltv::CapturedGuideChannel::id)
            .collect();
        let current = capture_teletext_guide(
            &self.pool,
            &channel_ids,
            self.captured_at,
            budget,
            self.deadline,
        )
        .await?;
        let comparison = self.snapshot.compare_context(&current.snapshot);
        if self.locale != current.locale || self.timezone != current.timezone {
            return Err(TeletextGuideCaptureError::SourceChanged { channel_id: None });
        }
        match comparison {
            GuideSnapshotComparison::Unchanged => check_deadline(self.deadline),
            GuideSnapshotComparison::GlobalChanged => {
                Err(TeletextGuideCaptureError::SourceChanged { channel_id: None })
            }
            GuideSnapshotComparison::ChannelChanged(channel_id) => {
                Err(TeletextGuideCaptureError::SourceChanged {
                    channel_id: Some(channel_id),
                })
            }
        }
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

/// Capture and fingerprint exactly the admitted historical semantic context.
/// The owned snapshot moves to a bounded CPU worker; no complete model is cloned.
/// This association permits later atomic context checks, never output promotion.
/// # Errors
/// Rejects capture, fingerprint bounds, invalid association or the original
/// deadline. Abandoned CPU work retains bounded ownership until completion.
pub async fn capture_source_bound_teletext_guide(
    pool: &DbPool,
    channel_ids: &[i64],
    captured_at: DateTime<Utc>,
    budget: &mut GuideReadBudget,
    deadline: Instant,
    digest_limits: GuideSnapshotDigestLimits,
) -> Result<CapturedTeletextGuide, TeletextGuideCaptureError> {
    let captured = capture_teletext_guide(pool, channel_ids, captured_at, budget, deadline).await?;
    let captured = tokio::time::timeout_at(
        tokio::time::Instant::from_std(deadline),
        tokio::task::spawn_blocking(move || {
            check_deadline(deadline)?;
            let context_digest = captured.snapshot.context_digest(digest_limits)?;
            let source = TeletextGuideSourceFence {
                version: TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
                context_digest_version: GUIDE_SNAPSHOT_DIGEST_VERSION,
                context_digest,
                channel_ids: captured
                    .snapshot
                    .channels()
                    .iter()
                    .map(ersatzrs_infra::xmltv::CapturedGuideChannel::id)
                    .collect(),
                captured_at,
                start: captured.snapshot.start(),
                finish: captured.snapshot.finish(),
                locale: captured.locale.to_owned(),
                timezone: captured.timezone.clone(),
            };
            source
                .validate()
                .map_err(|_| TeletextGuideCaptureError::Association)?;
            check_deadline(deadline)?;
            Ok::<_, TeletextGuideCaptureError>(CapturedTeletextGuide {
                guide_source: Some(source),
                ..captured
            })
        }),
    )
    .await
    .map_err(|_| TeletextGuideCaptureError::TimedOut)?
    .map_err(|_| TeletextGuideCaptureError::Worker)??;
    check_deadline(deadline)?;
    Ok(captured)
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
            pool: pool.clone(),
            snapshot,
            locale,
            timezone: timezone.to_owned(),
            captured_at,
            deadline,
            guide_source: None,
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

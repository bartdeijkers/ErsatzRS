//! Finite current-role validity, separate from historical source equality.

use super::{
    TeletextGuideSourceFence, TeletextGuideSourceFenceError, TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
};
use crate::programme::{ProgrammeClock, ProgrammeOccurrence};
use chrono::{DateTime, Utc};
use std::num::NonZeroU8;

/// Deterministic validity of an exact saved source context. The native caller
/// separately proves its request roles; the repository independently recaptures
/// source data and this boundary. This value alone grants no output authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextGuideGenerationQualification {
    pub source: TeletextGuideSourceFence,
    pub valid_until: DateTime<Utc>,
}

/// Static failures never include guide content, names or digests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TeletextGuideQualificationError {
    #[error(transparent)]
    Source(#[from] TeletextGuideSourceFenceError),
    #[error("current guide activation requires the exact raw-source digest version")]
    ContextVersion,
    #[error("current clock precedes the original guide capture")]
    ClockRewind,
    #[error("current guide qualification has expired")]
    Expired,
    #[error("current guide validity boundary is invalid")]
    Boundary,
}

impl TeletextGuideSourceFence {
    /// Derive the first possible role transition after the ORIGINAL capture.
    /// Callers supply the complete, already bounded source occurrences; no
    /// acquisition, producer invocation or role guessing occurs.
    /// Segment transitions preserve Continuing across intervening filler gaps.
    /// # Errors
    /// Rejects legacy semantic-only source versions, clock rewind, expiry or an
    /// unavailable named-clock calendar boundary. Source equality remains separate.
    pub fn qualify_current<'a>(
        &self,
        programmes: impl IntoIterator<Item = &'a ProgrammeOccurrence>,
        now: DateTime<Utc>,
    ) -> Result<TeletextGuideGenerationQualification, TeletextGuideQualificationError> {
        let mut valid_until = next_midnight(self)?;
        for programme in programmes {
            for boundary in [programme.programme.start, programme.programme.stop]
                .into_iter()
                .chain(
                    programme
                        .segments
                        .iter()
                        .flat_map(|segment| [segment.start, segment.finish]),
                )
            {
                if boundary > self.captured_at && boundary < valid_until {
                    valid_until = boundary;
                }
            }
        }
        let qualification = TeletextGuideGenerationQualification {
            source: self.clone(),
            valid_until,
        };
        qualification.validate_current(now)?;
        Ok(qualification)
    }
}

impl TeletextGuideGenerationQualification {
    /// Check the half-open wall-clock validity without renewing its boundary.
    /// The source-aware repository must additionally derive and compare the
    /// exact earliest source transition, including an earlier Scripted expiry.
    /// # Errors
    /// Rejects invalid source/version/boundary, clock rewind or expiry.
    pub fn validate_current(
        &self,
        now: DateTime<Utc>,
    ) -> Result<(), TeletextGuideQualificationError> {
        let midnight = next_midnight(&self.source)?;
        if self.valid_until <= self.source.captured_at || self.valid_until > midnight {
            return Err(TeletextGuideQualificationError::Boundary);
        }
        if now < self.source.captured_at {
            return Err(TeletextGuideQualificationError::ClockRewind);
        }
        if now >= self.valid_until {
            return Err(TeletextGuideQualificationError::Expired);
        }
        Ok(())
    }
}

fn next_midnight(
    source: &TeletextGuideSourceFence,
) -> Result<DateTime<Utc>, TeletextGuideQualificationError> {
    source.validate()?;
    if source.channel_ids.is_empty() {
        return Err(TeletextGuideSourceFenceError::Channels.into());
    }
    if source.context_digest_version != TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION {
        return Err(TeletextGuideQualificationError::ContextVersion);
    }
    ProgrammeClock::named(&source.timezone)
        .and_then(|clock| clock.calendar_day_bounds(source.captured_at, NonZeroU8::MIN))
        .map(|(_, finish)| finish)
        .map_err(|_| TeletextGuideQualificationError::Boundary)
}

#[cfg(test)]
#[path = "teletext_guide_source_current_tests.rs"]
mod tests;

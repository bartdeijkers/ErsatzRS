//! `GetGraphicsCanvasStreamHandler` timing, on the already shifted channel clock.

use chrono::{DateTime, TimeDelta, Utc};

use super::{CanvasRequest, InvalidCanvasRequest};

/// Programme timing remains separate from the finite producer chunk. In
/// particular, a seek or a short chunk must not move `MediaItem_Stop`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanvasTimeline {
    pub content_start: DateTime<Utc>,
    pub content_finish: DateTime<Utc>,
    pub now: DateTime<Utc>,
    pub chunk_finish: DateTime<Utc>,
    pub duration: std::time::Duration,
    pub content_remaining: std::time::Duration,
}

impl CanvasTimeline {
    /// Match upstream's finite late-offset fallback and remaining-content clock.
    /// Mirror offsets must already be applied to start and finish.
    ///
    /// # Errors
    /// Rejects invalid programme bounds or request dates outside chrono's range.
    pub fn new(
        content_start: DateTime<Utc>,
        content_finish: DateTime<Utc>,
        request: &CanvasRequest,
    ) -> Result<Self, InvalidCanvasRequest> {
        if content_finish <= content_start {
            return Err(InvalidCanvasRequest);
        }
        let offset = TimeDelta::from_std(request.offset()).map_err(|_| InvalidCanvasRequest)?;
        let requested_duration =
            TimeDelta::from_std(request.duration()).map_err(|_| InvalidCanvasRequest)?;
        let now = content_start
            .checked_add_signed(offset)
            .ok_or(InvalidCanvasRequest)?;
        let requested_finish = now
            .checked_add_signed(requested_duration)
            .ok_or(InvalidCanvasRequest)?;
        let chunk_finish = if content_finish > now {
            content_finish.min(requested_finish)
        } else {
            requested_finish
        };
        let duration = (chunk_finish - now)
            .to_std()
            .map_err(|_| InvalidCanvasRequest)?;
        let content_remaining = if content_finish > now {
            (content_finish - now)
                .to_std()
                .map_err(|_| InvalidCanvasRequest)?
        } else {
            duration
        };
        Ok(Self {
            content_start,
            content_finish,
            now,
            chunk_finish,
            duration,
            content_remaining,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn canvas_timeline_preserves_programme_finish_across_chunks_and_late_seeks() {
        let start = DateTime::from_timestamp(1_000, 0).unwrap();
        let finish = start + TimeDelta::minutes(30);
        for (offset, duration, remaining) in [
            (0, 44, 1800),
            (44, 44, 1756),
            (1760, 40, 40),
            (1800, 44, 44),
            (1844, 44, 44),
        ] {
            let request =
                CanvasRequest::parse(&(offset * 1000).to_string(), "44000", "25").unwrap();
            let timeline = CanvasTimeline::new(start, finish, &request).unwrap();
            assert_eq!(timeline.content_finish, finish);
            assert_eq!(timeline.now, start + TimeDelta::seconds(offset));
            assert_eq!(timeline.duration, Duration::from_secs(duration));
            assert_eq!(timeline.content_remaining, Duration::from_secs(remaining));
            let mirror_offset = TimeDelta::hours(1);
            let mirror =
                CanvasTimeline::new(start + mirror_offset, finish + mirror_offset, &request)
                    .unwrap();
            assert_eq!(mirror.now, timeline.now + mirror_offset);
            assert_eq!(mirror.duration, timeline.duration);
            assert_eq!(mirror.content_remaining, timeline.content_remaining);
        }
    }

    #[test]
    fn canvas_timeline_rejects_date_overflow_without_panicking() {
        let request = CanvasRequest::parse("1000", "44000", "25").unwrap();
        let finish = DateTime::<Utc>::MAX_UTC;
        let start = finish - TimeDelta::seconds(1);
        assert!(CanvasTimeline::new(start, finish, &request).is_err());
        assert!(CanvasTimeline::new(finish, start, &request).is_err());
    }
}

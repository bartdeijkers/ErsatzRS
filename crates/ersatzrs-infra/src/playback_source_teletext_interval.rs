//! Requested playback time, kept separate from observed source-clock evidence.

use std::time::Duration;

/// One finite media-relative interval from the actual native playback request.
///
/// This preserves the caller's subsecond seek and duration without timestamps,
/// timezone conversion or an inferred source origin. It proves no raw PCR/PTS
/// epoch, decoded-page availability or temporal coverage by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextPlaybackInterval {
    start: Duration,
    end: Duration,
}

impl SourceTeletextPlaybackInterval {
    /// Preserve a finite request, rejecting empty or overflowing intervals.
    /// An absent duration remains unknown at the caller; never substitute EOF,
    /// an arbitrary horizon or a rounded seconds value for it.
    #[must_use]
    pub fn from_start_and_duration(start: Duration, duration: Duration) -> Option<Self> {
        if duration.is_zero() {
            return None;
        }
        Some(Self {
            start,
            end: start.checked_add(duration)?,
        })
    }

    /// Exact media-relative seek passed by the consuming native request.
    #[must_use]
    pub const fn start(self) -> Duration {
        self.start
    }

    /// Exclusive media-relative end; this is requested time, not an observed PTS.
    #[must_use]
    pub const fn end(self) -> Duration {
        self.end
    }

    /// Original positive duration without loss of subsecond precision.
    #[must_use]
    pub fn duration(self) -> Duration {
        self.end.saturating_sub(self.start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_cue_retains_subsecond_seek_and_exclusive_end() {
        let start = Duration::new(40, 123_456_789);
        let duration = Duration::new(12, 876_543_210);
        let requested =
            SourceTeletextPlaybackInterval::from_start_and_duration(start, duration).unwrap();
        assert_eq!(requested.start(), start);
        assert_eq!(requested.duration(), duration);
        assert_eq!(requested.end(), Duration::new(52, 999_999_999));
    }

    #[test]
    fn empty_and_overflowing_requests_gain_no_finite_interval() {
        assert!(SourceTeletextPlaybackInterval::from_start_and_duration(
            Duration::from_secs(40),
            Duration::ZERO
        )
        .is_none());
        assert!(SourceTeletextPlaybackInterval::from_start_and_duration(
            Duration::MAX,
            Duration::from_nanos(1)
        )
        .is_none());
        let near_limit = SourceTeletextPlaybackInterval::from_start_and_duration(
            Duration::MAX.saturating_sub(Duration::from_nanos(1)),
            Duration::from_nanos(1),
        )
        .unwrap();
        assert_eq!(near_limit.end(), Duration::MAX);
    }
}

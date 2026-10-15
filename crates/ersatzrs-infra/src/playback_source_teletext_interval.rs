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

/// Effective microsecond request under the current native command policy.
///
/// This is distinct from exact caller intent, native format origin and raw
/// source clocks. It grants no PCR epoch, subtitle availability or cue coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextEffectiveInterval {
    start_microseconds: i64,
    end_microseconds: i64,
}

impl SourceTeletextEffectiveInterval {
    /// Effective media-relative native seek after command parsing.
    #[must_use]
    pub const fn start_microseconds(self) -> i64 {
        self.start_microseconds
    }

    /// Exclusive end from separately parsed native seek and duration.
    #[must_use]
    pub const fn end_microseconds(self) -> i64 {
        self.end_microseconds
    }

    /// Positive effective native duration, without flooring the requested end.
    #[must_use]
    pub const fn duration_microseconds(self) -> i64 {
        self.end_microseconds
            .saturating_sub(self.start_microseconds)
    }
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

    /// Apply only the current native `-ss`/`-t` command precision policy.
    ///
    /// The existing `format_time_limit` renders seven fractional digits for
    /// each argument independently. Native `av_parse_time` consumes six digits,
    /// so the seek and original duration are separately floored to microseconds.
    /// Flooring the requested end instead can add a spurious microsecond when
    /// the discarded fractions carry. Exact caller intent remains unchanged.
    ///
    /// Each generated argument must also fit native's four-digit hour syntax.
    /// Empty effective duration, an unparseable argument or end overflow remains
    /// unknown (`None`). This is no source-clock or cue admission.
    #[must_use]
    pub fn effective_native_interval(self) -> Option<SourceTeletextEffectiveInterval> {
        let start_microseconds = ersatzrs_ffmpeg::parsed_time_limit_microseconds(self.start)?;
        let duration_microseconds =
            ersatzrs_ffmpeg::parsed_time_limit_microseconds(self.duration())?;
        if duration_microseconds == 0 {
            return None;
        }
        Some(SourceTeletextEffectiveInterval {
            start_microseconds,
            end_microseconds: start_microseconds.checked_add(duration_microseconds)?,
        })
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

    #[test]
    fn native_seek_and_duration_drop_fractional_microseconds_independently() {
        let start = Duration::new(40, 900);
        let duration = Duration::new(12, 900);
        let requested =
            SourceTeletextPlaybackInterval::from_start_and_duration(start, duration).unwrap();
        assert_eq!(
            ersatzrs_ffmpeg::format_time_limit(start),
            "00:00:40.0000009"
        );
        assert_eq!(
            ersatzrs_ffmpeg::format_time_limit(duration),
            "00:00:12.0000009"
        );
        let effective = requested.effective_native_interval().unwrap();
        assert_eq!(effective.start_microseconds(), 40_000_000);
        assert_eq!(effective.duration_microseconds(), 12_000_000);
        assert_eq!(effective.end_microseconds(), 52_000_000);
        assert_eq!(requested.end().as_micros(), 52_000_001);
        assert_eq!(requested.start(), start);
        assert_eq!(requested.duration(), duration);
        assert_eq!(requested.end(), start + duration);
    }

    #[test]
    fn positive_submicrosecond_duration_preserves_intent_without_native_interval() {
        for nanos in [1, 99, 100, 999] {
            let duration = Duration::from_nanos(nanos);
            let requested = SourceTeletextPlaybackInterval::from_start_and_duration(
                Duration::from_secs(40),
                duration,
            )
            .unwrap();
            assert_eq!(requested.duration(), duration);
            assert_eq!(requested.effective_native_interval(), None);
        }
        assert_eq!(
            ersatzrs_ffmpeg::format_time_limit(Duration::from_nanos(999)),
            "00:00:00.0000009"
        );
    }

    #[test]
    fn native_effective_interval_retains_total_days_and_current_argument_precision() {
        let start = Duration::new(90_061, 123_456_789);
        let duration = Duration::new(172_862, 987_654_321);
        let requested =
            SourceTeletextPlaybackInterval::from_start_and_duration(start, duration).unwrap();
        assert_eq!(
            ersatzrs_ffmpeg::format_time_limit(start),
            "25:01:01.1234567"
        );
        assert_eq!(
            ersatzrs_ffmpeg::format_time_limit(duration),
            "48:01:02.9876543"
        );
        let effective = requested.effective_native_interval().unwrap();
        assert_eq!(effective.start_microseconds(), 90_061_123_456);
        assert_eq!(effective.duration_microseconds(), 172_862_987_654);
        assert_eq!(effective.end_microseconds(), 262_924_111_110);
        assert_eq!(requested.start(), start);
        assert_eq!(requested.duration(), duration);
    }

    #[test]
    fn native_arguments_require_four_hour_digits_but_their_sum_is_not_an_argument() {
        let last = Duration::new(35_999_999, 999_999_999);
        let effective = SourceTeletextPlaybackInterval::from_start_and_duration(last, last)
            .unwrap()
            .effective_native_interval()
            .unwrap();
        assert_eq!(effective.start_microseconds(), 35_999_999_999_999);
        assert_eq!(effective.duration_microseconds(), 35_999_999_999_999);
        assert_eq!(effective.end_microseconds(), 71_999_999_999_998);
        for (start, duration) in [
            (Duration::from_hours(10_000), Duration::from_secs(1)),
            (Duration::ZERO, Duration::from_hours(10_000)),
            (
                Duration::from_micros(u64::try_from(i64::MAX).unwrap()),
                Duration::from_micros(1),
            ),
            (
                Duration::MAX.saturating_sub(Duration::from_nanos(1)),
                Duration::from_nanos(1),
            ),
        ] {
            let requested =
                SourceTeletextPlaybackInterval::from_start_and_duration(start, duration).unwrap();
            assert_eq!(requested.effective_native_interval(), None);
            assert_eq!(requested.start(), start);
            assert_eq!(requested.duration(), duration);
        }
    }
}

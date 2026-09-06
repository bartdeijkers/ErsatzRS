//! A finite source's retained timeline, shared by publication and playback (ADR 0087).

use std::ops::Range;

use thiserror::Error;

/// Maximum number of source exclusions accepted in one timeline.
pub const MAX_MEDIA_PLAYBACK_EXCLUSIONS: usize = 1_024;

/// Invalid finite-media timing, without provider URLs or diagnostic text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum MediaPlaybackTimelineError {
    #[error("source duration must be positive and fit signed millisecond timing")]
    InvalidDuration,
    #[error("media intervals must be nonempty and within the source duration")]
    InvalidInterval,
    #[error("too many media exclusion intervals")]
    TooManyExclusions,
    #[error("no playable media remains after interval removal")]
    NoPlayableContent,
    #[error("programme seek exceeds the retained duration")]
    SeekOutsideTimeline,
}

/// Validated, ordered, non-overlapping source ranges in integer milliseconds.
///
/// Ranges are half-open. The programme clock concatenates them without the
/// excluded gaps; its duration is derived rather than independently writable.
/// This value intentionally has no unchecked deserializer. Persisted/provider
/// data must pass through [`Self::from_exclusions`] at the owning boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaPlaybackTimeline {
    retained: Vec<Range<u64>>,
}

impl MediaPlaybackTimeline {
    /// Normalize exclusions and retain their complement inside a source window.
    ///
    /// `source_duration_milliseconds` describes the whole source; `window` selects
    /// the full item or an existing chapter/fragment. Exclusions use whole-source
    /// coordinates. Overlapping, adjacent and duplicate exclusions are combined;
    /// valid exclusions outside the window are ignored, not treated as invalid.
    ///
    /// # Errors
    /// Rejects unknown/zero/oversized duration, malformed or out-of-source ranges,
    /// excessive exclusions, and removal of all content in the chosen window.
    pub fn from_exclusions(
        source_duration_milliseconds: u64,
        window: Range<u64>,
        exclusions: &[Range<u64>],
    ) -> Result<Self, MediaPlaybackTimelineError> {
        if source_duration_milliseconds == 0 || source_duration_milliseconds > i64::MAX as u64 {
            return Err(MediaPlaybackTimelineError::InvalidDuration);
        }
        if window.start >= window.end || window.end > source_duration_milliseconds {
            return Err(MediaPlaybackTimelineError::InvalidInterval);
        }
        if exclusions.len() > MAX_MEDIA_PLAYBACK_EXCLUSIONS {
            return Err(MediaPlaybackTimelineError::TooManyExclusions);
        }
        if exclusions
            .iter()
            .any(|range| range.start >= range.end || range.end > source_duration_milliseconds)
        {
            return Err(MediaPlaybackTimelineError::InvalidInterval);
        }

        let mut exclusions = exclusions.to_vec();
        exclusions.sort_unstable_by_key(|range| (range.start, range.end));
        let mut cursor = window.start;
        let mut retained = Vec::new();
        for excluded in exclusions {
            let start = excluded.start.max(window.start);
            let end = excluded.end.min(window.end);
            if start >= end || end <= cursor {
                continue;
            }
            if start > cursor {
                retained.push(cursor..start);
            }
            cursor = end;
        }
        if cursor < window.end {
            retained.push(cursor..window.end);
        }
        if retained.is_empty() {
            return Err(MediaPlaybackTimelineError::NoPlayableContent);
        }
        Ok(Self { retained })
    }

    /// Exact programme duration after exclusions, in milliseconds.
    #[must_use]
    pub fn duration_milliseconds(&self) -> u64 {
        // Disjoint source ranges sum to at most the validated source duration.
        self.retained
            .iter()
            .map(|range| range.end - range.start)
            .sum()
    }

    /// Retained source coordinates, ordered by programme playback.
    #[must_use]
    pub fn retained_ranges(&self) -> &[Range<u64>] {
        &self.retained
    }

    /// Map a programme join to the remaining source ranges without a second seek.
    ///
    /// A join exactly at a cut begins at the next retained range. A join exactly
    /// at programme EOF returns an empty vector; it must not replay the source.
    ///
    /// # Errors
    /// Rejects joins beyond programme EOF rather than clamping or wrapping them.
    pub fn ranges_from(
        &self,
        mut programme_milliseconds: u64,
    ) -> Result<Vec<Range<u64>>, MediaPlaybackTimelineError> {
        for (index, range) in self.retained.iter().enumerate() {
            let duration = range.end - range.start;
            if programme_milliseconds < duration {
                let mut remaining = self.retained[index..].to_vec();
                remaining[0].start += programme_milliseconds;
                return Ok(remaining);
            }
            programme_milliseconds -= duration;
        }
        if programme_milliseconds == 0 {
            Ok(Vec::new())
        } else {
            Err(MediaPlaybackTimelineError::SeekOutsideTimeline)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_normalizes_overlapping_adjacent_duplicate_and_unsorted_exclusions() {
        let timeline = MediaPlaybackTimeline::from_exclusions(
            10_000,
            0..10_000,
            &[
                5_000..6_000,
                1_000..3_000,
                2_000..4_000,
                4_000..5_000,
                1_000..3_000,
            ],
        )
        .unwrap();
        assert_eq!(timeline.retained_ranges(), &[0..1_000, 6_000..10_000]);
        assert_eq!(timeline.duration_milliseconds(), 5_000);
    }

    #[test]
    fn timeline_intersects_full_source_exclusions_with_a_chapter_window() {
        let timeline = MediaPlaybackTimeline::from_exclusions(
            20_000,
            5_000..15_000,
            &[
                0..1_000,
                4_000..6_000,
                10_000..11_000,
                14_000..16_000,
                18_000..20_000,
            ],
        )
        .unwrap();
        assert_eq!(timeline.retained_ranges(), &[6_000..10_000, 11_000..14_000]);
        assert_eq!(timeline.duration_milliseconds(), 7_000);
    }

    #[test]
    fn timeline_joins_use_the_edited_clock_including_exact_cut_and_eof() {
        let timeline = MediaPlaybackTimeline::from_exclusions(
            10_000,
            0..10_000,
            &[0..1_250, 3_750..6_125, 9_500..10_000],
        )
        .unwrap();
        assert_eq!(timeline.duration_milliseconds(), 5_875);
        assert_eq!(
            timeline.ranges_from(0).unwrap(),
            vec![1_250..3_750, 6_125..9_500]
        );
        assert_eq!(
            timeline.ranges_from(2_499).unwrap(),
            vec![3_749..3_750, 6_125..9_500]
        );
        assert_eq!(timeline.ranges_from(2_500).unwrap(), vec![6_125..9_500]);
        assert_eq!(timeline.ranges_from(3_000).unwrap(), vec![6_625..9_500]);
        assert!(timeline.ranges_from(5_875).unwrap().is_empty());
        assert_eq!(
            timeline.ranges_from(5_876),
            Err(MediaPlaybackTimelineError::SeekOutsideTimeline)
        );
        assert_eq!(
            timeline.ranges_from(u64::MAX),
            Err(MediaPlaybackTimelineError::SeekOutsideTimeline)
        );
    }

    #[test]
    fn timeline_without_exclusions_preserves_the_source_window() {
        let timeline = MediaPlaybackTimeline::from_exclusions(10_000, 2_000..8_000, &[]).unwrap();
        assert_eq!(timeline.retained_ranges().len(), 1);
        assert_eq!(timeline.retained_ranges()[0], 2_000..8_000);
        assert_eq!(timeline.duration_milliseconds(), 6_000);
    }

    #[test]
    fn timeline_rejects_invalid_source_ranges_even_outside_the_window() {
        use MediaPlaybackTimelineError::{InvalidDuration, InvalidInterval, NoPlayableContent};
        assert_eq!(
            MediaPlaybackTimeline::from_exclusions(0, 0..1, &[]),
            Err(InvalidDuration)
        );
        assert_eq!(
            MediaPlaybackTimeline::from_exclusions(u64::MAX, 0..1, &[]),
            Err(InvalidDuration)
        );
        for window in [10..10, Range { start: 20, end: 10 }, 0..101] {
            assert_eq!(
                MediaPlaybackTimeline::from_exclusions(100, window, &[]),
                Err(InvalidInterval)
            );
        }
        for excluded in [0..0, Range { start: 20, end: 10 }, 90..101] {
            assert_eq!(
                MediaPlaybackTimeline::from_exclusions(100, 30..50, &[excluded]),
                Err(InvalidInterval)
            );
        }
        assert_eq!(
            MediaPlaybackTimeline::from_exclusions(100, 30..50, &[0..40, 40..100]),
            Err(NoPlayableContent)
        );
    }

    #[test]
    fn timeline_bounds_exclusion_count_before_allocating_or_sorting() {
        let exclusions = vec![1..2; MAX_MEDIA_PLAYBACK_EXCLUSIONS + 1];
        assert_eq!(
            MediaPlaybackTimeline::from_exclusions(100, 0..100, &exclusions),
            Err(MediaPlaybackTimelineError::TooManyExclusions)
        );
    }
}

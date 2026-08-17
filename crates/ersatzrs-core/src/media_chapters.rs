//! Parsing and validation for operator-authored media chapter markers.

use thiserror::Error;

/// One validated chapter boundary expressed in whole seconds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedMediaChapter {
    /// Stable zero-based order from the input.
    pub index: usize,
    /// Human-readable chapter title.
    pub title: String,
    /// Inclusive chapter start in whole seconds.
    pub start_seconds: u64,
    /// Exclusive chapter end, or `None` for the final chapter when duration is unknown.
    pub end_seconds: Option<u64>,
}

/// A safe, line-oriented validation failure for chapter input.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ChapterInputError {
    #[error("chapter line {line} is invalid; use 'M:SS Title' or 'H:MM:SS Title'")]
    InvalidLine { line: usize },
    #[error("chapter line {line} has an invalid timecode")]
    InvalidTimecode { line: usize },
    #[error("chapter line {line} is missing a title")]
    MissingTitle { line: usize },
    #[error("chapter line {line} duplicates the timecode at {seconds} seconds")]
    DuplicateTimecode { line: usize, seconds: u64 },
    #[error("chapter line {line} must start after the previous chapter")]
    NonIncreasingTimecode { line: usize },
    #[error("chapter line {line} starts outside the video duration of {duration_seconds} seconds")]
    OutsideDuration { line: usize, duration_seconds: u64 },
}

/// Parse provider-neutral timestamp/title chapter input and calculate ends.
///
/// Blank lines are ignored. Two-part timecodes use `M:SS`/`MM:SS`; three-part
/// timecodes use `H:MM:SS`. The final end is the known video duration, or stays
/// open when no duration is available.
///
/// # Errors
///
/// Returns the first line-specific validation error. No partial chapter set is
/// returned, so invalid input cannot silently produce incorrect fragments.
pub fn parse_chapter_input(
    input: &str,
    duration_seconds: Option<u64>,
) -> Result<Vec<ParsedMediaChapter>, ChapterInputError> {
    let mut starts: Vec<ChapterStart> = Vec::new();
    for (offset, raw_line) in input.lines().enumerate() {
        let line = offset + 1;
        let raw_line = raw_line.trim();
        if raw_line.is_empty() {
            continue;
        }
        let Some(split_at) = raw_line.find(char::is_whitespace) else {
            return if parse_timecode(raw_line).is_some() {
                Err(ChapterInputError::MissingTitle { line })
            } else {
                Err(ChapterInputError::InvalidLine { line })
            };
        };
        let timecode = &raw_line[..split_at];
        let title = raw_line[split_at..].trim();
        if title.is_empty() {
            return Err(ChapterInputError::MissingTitle { line });
        }
        let start_seconds =
            parse_timecode(timecode).ok_or(ChapterInputError::InvalidTimecode { line })?;
        if let Some(previous) = starts.last() {
            if start_seconds == previous.start_seconds {
                return Err(ChapterInputError::DuplicateTimecode {
                    line,
                    seconds: start_seconds,
                });
            }
            if start_seconds < previous.start_seconds {
                return Err(ChapterInputError::NonIncreasingTimecode { line });
            }
        }
        if duration_seconds.is_some_and(|duration| start_seconds >= duration) {
            return Err(ChapterInputError::OutsideDuration {
                line,
                duration_seconds: duration_seconds.unwrap_or_default(),
            });
        }
        starts.push(ChapterStart {
            title: title.to_owned(),
            start_seconds,
        });
    }

    Ok(starts
        .iter()
        .enumerate()
        .map(|(index, chapter)| ParsedMediaChapter {
            index,
            title: chapter.title.clone(),
            start_seconds: chapter.start_seconds,
            end_seconds: starts
                .get(index + 1)
                .map(|next| next.start_seconds)
                .or(duration_seconds),
        })
        .collect())
}

#[derive(Clone, Debug)]
struct ChapterStart {
    title: String,
    start_seconds: u64,
}

fn parse_timecode(value: &str) -> Option<u64> {
    let parts = value.split(':').collect::<Vec<_>>();
    if parts
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    match parts.as_slice() {
        [minutes, seconds] if seconds.len() == 2 => {
            let minutes = minutes.parse::<u64>().ok()?;
            let seconds = seconds.parse::<u64>().ok()?;
            (seconds < 60).then_some(minutes.checked_mul(60)?.checked_add(seconds)?)
        }
        [hours, minutes, seconds] if minutes.len() == 2 && seconds.len() == 2 => {
            let hours = hours.parse::<u64>().ok()?;
            let minutes = minutes.parse::<u64>().ok()?;
            let seconds = seconds.parse::<u64>().ok()?;
            if minutes >= 60 || seconds >= 60 {
                return None;
            }
            hours
                .checked_mul(3_600)?
                .checked_add(minutes.checked_mul(60)?)?
                .checked_add(seconds)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_timecodes_and_known_duration() {
        let chapters = parse_chapter_input(
            "0:00 Introduction\n1:35 First topic\n10:08 Demo\n1:02:03 Conclusion",
            Some(4_000),
        )
        .expect("valid chapters");

        assert_eq!(
            chapters,
            vec![
                ParsedMediaChapter {
                    index: 0,
                    title: "Introduction".to_owned(),
                    start_seconds: 0,
                    end_seconds: Some(95)
                },
                ParsedMediaChapter {
                    index: 1,
                    title: "First topic".to_owned(),
                    start_seconds: 95,
                    end_seconds: Some(608)
                },
                ParsedMediaChapter {
                    index: 2,
                    title: "Demo".to_owned(),
                    start_seconds: 608,
                    end_seconds: Some(3_723)
                },
                ParsedMediaChapter {
                    index: 3,
                    title: "Conclusion".to_owned(),
                    start_seconds: 3_723,
                    end_seconds: Some(4_000)
                },
            ]
        );
    }

    #[test]
    fn leaves_last_end_open_without_duration() {
        let chapters = parse_chapter_input("0:00 Start\n02:30 Finish", None).expect("valid");
        assert_eq!(chapters[0].end_seconds, Some(150));
        assert_eq!(chapters[1].end_seconds, None);
    }

    #[test]
    fn rejects_invalid_missing_duplicate_and_non_increasing_markers() {
        assert!(matches!(
            parse_chapter_input("intro", None),
            Err(ChapterInputError::InvalidLine { line: 1 })
        ));
        assert!(matches!(
            parse_chapter_input("0:00   ", None),
            Err(ChapterInputError::MissingTitle { line: 1 })
        ));
        assert!(matches!(
            parse_chapter_input("0:61 Bad", None),
            Err(ChapterInputError::InvalidTimecode { line: 1 })
        ));
        assert!(matches!(
            parse_chapter_input("0:00 A\n00:00 B", None),
            Err(ChapterInputError::DuplicateTimecode {
                line: 2,
                seconds: 0
            })
        ));
        assert!(matches!(
            parse_chapter_input("2:00 A\n1:59 B", None),
            Err(ChapterInputError::NonIncreasingTimecode { line: 2 })
        ));
    }

    #[test]
    fn rejects_marker_at_or_beyond_duration() {
        assert!(matches!(
            parse_chapter_input("1:00 Too late", Some(60)),
            Err(ChapterInputError::OutsideDuration {
                line: 1,
                duration_seconds: 60
            })
        ));
    }
}

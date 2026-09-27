//! Embedded text-subtitle and font extraction for deep show scans, playout
//! builds, and channel/settings save side effects.
//!
//! Ports `ErsatzTV.Application/Subtitles/Commands/ExtractEmbeddedShowSubtitlesHandler.cs`,
//! `ExtractEmbeddedSubtitlesHandler.cs`, and `ExtractEmbeddedSubtitlesHandlerBase.cs`.
//! ErsatzTV queues `ExtractEmbeddedShowSubtitles` after a successful deep show
//! scan (`QueueShowScanByLibraryIdHandler` lines 92-95) and
//! `ExtractEmbeddedSubtitles(playoutId)` after channel/settings/playout changes
//! can require subtitles.
//!
//! Deliberate divergences from ErsatzTV, all consistent with existing
//! ErsatzRS patterns:
//!   * **Input path**: ErsatzTV's `GetMediaItemPath` builds internal
//!     streaming-proxy URLs for Plex/Jellyfin/Emby. ErsatzRS feeds ffmpeg the
//!     `MediaFile."Path"` with path-replacements applied, mirroring
//!     `crates/ersatzrs-web/src/transport/iptv/stream.rs` `load_path_replacements`.
//!   * **ffmpeg path**: resolved via `ersatzrs_ffmpeg::ffmpeg_path` (the shared
//!     locator: DB config / next-to-exe / `PATH`), mirroring ErsatzTV's
//!     `FFmpegLocator`.
//!   * **seek re-base delivery**: ErsatzTV serves re-based playback subtitles
//!     through a localhost HTTP pipe. ErsatzRS writes the same re-based stream
//!     to the subtitle cache and burns that temp file, matching the existing
//!     file-cache model used for extracted subtitles.
//!   * **Cue-aware seek rebase** (ADR 0061): ErsatzTV re-bases burned
//!     text-subtitle cues via an ffmpeg `-ss`-seeked stream copy, which
//!     silently drops any cue already displaying when the seek point lands
//!     mid-cue (copy-mode seeking decides packet inclusion by start time
//!     only). ErsatzRS normalizes local or HTTP(S) UTF-8/UTF-16 sources and
//!     rewrites their cue timestamps directly ([`rebase_subtitle_cues`]),
//!     clamping a straddling cue's start to the new chunk's zero instead of
//!     dropping it. `FFmpeg` remains the compatibility fallback for syntax or
//!     encodings the cue-aware rewriter cannot safely handle.
use crate::repositories::SqlxResultExt as _;

use std::path::{Path, PathBuf};

use chrono::{Duration, Utc};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::Result;
use ersatzrs_ffmpeg::{ffmpeg_path, InputFile, InputKind, SubtitleMethod, SubtitleTextFormat};
use md5::{Digest, Md5};
use sqlx::{Pool, Row};
use tokio::process::Command;

use crate::db::Db;
use crate::ffmpeg_config::{extract_embedded_subtitles, use_embedded_subtitles};
use crate::file_system_layout::{
    fonts_cache_folder, subtitle_cache_folder, subtitle_seek_cache_folder,
};
use crate::metadata::subtitle_kind;
use crate::repositories::common::encode_datetime;

/// Image-subtitle codecs that cannot be extracted as text. Mirrors
/// `Subtitle.IsImage` (`Subtitle.cs`) and the filters in
/// `ExtractEmbeddedSubtitlesHandlerBase`.
const IMAGE_CODECS: &[&str] = &[
    "hdmv_pgs_subtitle",
    "dvd_subtitle",
    "dvdsub",
    "vobsub",
    "pgssub",
    "pgs",
];

/// True when `codec` is an image-subtitle codec (not extractable as text).
fn is_image_codec(codec: &str) -> bool {
    IMAGE_CODECS.contains(&codec)
}

/// Port of `GetRelativeOutputPath` (`ExtractEmbeddedSubtitlesHandlerBase.cs`):
/// `MD5_hex_upper("{mediaItemId}_{streamIndex}_{codec}")` split into
/// `[..2]/[2..4]/<hash>.<ext>`. Returns `None` for codecs without a text
/// extension (the row is skipped).
fn relative_output_path(media_item_id: i64, stream_index: i32, codec: &str) -> Option<PathBuf> {
    let extension = match codec {
        "subrip" | "srt" | "mov_text" => "srt",
        "ass" => "ass",
        "webvtt" => "vtt",
        _ => return None,
    };

    let mut hasher = Md5::new();
    hasher.update(format!("{media_item_id}_{stream_index}_{codec}").as_bytes());
    let digest = hasher.finalize();
    let mut name = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02X}").expect("writing to String should not fail");
    }

    let subfolder = &name[..2];
    let subfolder2 = &name[2..4];
    Some(
        Path::new(subfolder)
            .join(subfolder2)
            .join(format!("{name}.{extension}")),
    )
}

/// `-c:s` value for a codec: `text` only for `mov_text`, else `copy`.
/// Mirrors `ExtractSubtitles`.
fn subtitle_codec_arg(codec: &str) -> &'static str {
    if codec == "mov_text" {
        "text"
    } else {
        "copy"
    }
}

fn seek_subtitle_output_format(
    input_path: &str,
    format_hint: Option<SubtitleTextFormat>,
) -> (&'static str, &'static str) {
    match format_hint {
        Some(SubtitleTextFormat::Ass) => return ("ass", "ass"),
        Some(SubtitleTextFormat::WebVtt) => return ("webvtt", "vtt"),
        Some(SubtitleTextFormat::Srt) => return ("srt", "srt"),
        None => {}
    }

    let extension_source = input_path.split(['?', '#']).next().unwrap_or(input_path);
    match Path::new(extension_source)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("ass" | "ssa") => ("ass", "ass"),
        Some("vtt") => ("webvtt", "vtt"),
        _ => ("srt", "srt"),
    }
}

fn md5_upper_hex(value: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut name = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02X}").expect("writing to String should not fail");
    }
    name
}

fn format_seek_seconds(seek_seconds: i64) -> String {
    let hours = seek_seconds / 3_600;
    let minutes = (seek_seconds % 3_600) / 60;
    let seconds = seek_seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

fn seek_subtitle_output_path(
    input_path: &str,
    format_hint: Option<SubtitleTextFormat>,
    seek_seconds: i64,
) -> PathBuf {
    let (_, extension) = seek_subtitle_output_format(input_path, format_hint);
    let name = md5_upper_hex(input_path);

    subtitle_seek_cache_folder().join(format!("{name}_{seek_seconds}.{extension}"))
}

/// Build the compatibility ffmpeg subtitle-seek argv used when a supported
/// source cannot be normalized or parsed by the cue-aware rewriter.
fn build_seek_subtitle_args(
    input_path: &str,
    format_hint: Option<SubtitleTextFormat>,
    seek_seconds: i64,
    out_path: &Path,
) -> Vec<String> {
    let (format, _) = seek_subtitle_output_format(input_path, format_hint);
    vec![
        "-nostdin".to_owned(),
        "-hide_banner".to_owned(),
        "-nostats".to_owned(),
        "-loglevel".to_owned(),
        "error".to_owned(),
        "-i".to_owned(),
        input_path.to_owned(),
        "-ss".to_owned(),
        format_seek_seconds(seek_seconds),
        "-c:s".to_owned(),
        "copy".to_owned(),
        "-f".to_owned(),
        format.to_owned(),
        out_path.to_string_lossy().into_owned(),
    ]
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum SubtitleTextSourceError {
    #[error("failed to read local subtitle: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to fetch remote subtitle: {0}")]
    Http(reqwest::Error),
    #[error("subtitle is not valid UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("subtitle is not valid UTF-16: {0}")]
    Utf16(#[from] std::string::FromUtf16Error),
    #[error("UTF-16 subtitle has an odd byte length")]
    OddUtf16ByteLength,
}

/// Read a local or HTTP(S) subtitle source and normalize the encodings ffmpeg
/// commonly accepts without an explicit character-set option. UTF-8 BOMs are
/// stripped, and BOM-marked UTF-16 is decoded in its declared byte order.
async fn read_subtitle_text_source(
    input_path: &str,
) -> std::result::Result<String, SubtitleTextSourceError> {
    let bytes = if input_path.starts_with("http://") || input_path.starts_with("https://") {
        reqwest::get(input_path)
            .await
            .map_err(|error| SubtitleTextSourceError::Http(error.without_url()))?
            .error_for_status()
            .map_err(|error| SubtitleTextSourceError::Http(error.without_url()))?
            .bytes()
            .await
            .map_err(|error| SubtitleTextSourceError::Http(error.without_url()))?
            .to_vec()
    } else {
        tokio::fs::read(input_path).await?
    };

    decode_subtitle_text(bytes)
}

pub(crate) fn decode_subtitle_text(
    mut bytes: Vec<u8>,
) -> std::result::Result<String, SubtitleTextSourceError> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
        return Ok(String::from_utf8(bytes)?);
    }

    let byte_order = if bytes.starts_with(&[0xFF, 0xFE]) {
        Some(u16::from_le_bytes as fn([u8; 2]) -> u16)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some(u16::from_be_bytes as fn([u8; 2]) -> u16)
    } else {
        None
    };

    let Some(decode_unit) = byte_order else {
        return Ok(String::from_utf8(bytes)?);
    };
    let payload = &bytes[2..];
    if !payload.len().is_multiple_of(2) {
        return Err(SubtitleTextSourceError::OddUtf16ByteLength);
    }
    let units = payload
        .chunks_exact(2)
        .map(|chunk| decode_unit([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    Ok(String::from_utf16(&units)?)
}

/// Cue timestamp grammar shared by the blank-line-block subtitle formats.
/// SRT and `WebVTT` differ only in the decimal separator and whether the
/// hour group may be omitted (`WebVTT`'s short `MM:SS.mmm` form).
#[derive(Clone, Copy)]
enum TimestampSyntax {
    Srt,
    WebVtt,
}

/// Rewritten subtitle text plus how many cues survived, so callers can tell
/// "no cues remain" apart from a normal non-empty rewrite without
/// re-scanning the output.
struct RebasedSubtitle {
    text: String,
    remaining_cues: usize,
}

/// Why a cue-aware rebase could not be applied safely. Callers fall back to
/// the previous ffmpeg seek-and-copy rebase on any of these.
#[derive(Debug)]
enum RebaseSubtitleError {
    UnparseableCue,
}

/// A cue's fate after shifting by `-seek_units` (same sub-second unit as
/// `start`/`end`: ms for SRT/`WebVTT`, centiseconds for ASS).
enum CueDecision {
    Drop,
    Keep { start: i64, end: i64 },
}

/// A format's cue-timestamp parse/format function pair, selected once per
/// [`rebase_line_block_cues`] call by [`TimestampSyntax`].
type TimestampParseFn = fn(&str) -> Option<i64>;
type TimestampFormatFn = fn(i64) -> String;

/// Shift a cue's `[start, end)` window by `-seek_units`. A cue that's fully
/// in the past after the shift is dropped; a cue that straddles the seek
/// point (`start` goes negative but `end` stays positive) is **clamped** to
/// start at `0` instead of being dropped, so it keeps displaying for its
/// remaining visible duration in the new chunk. This clamp is the fix for
/// ADR 0061: ErsatzTV's ffmpeg `-ss`-seeked stream-copy rebase silently
/// drops any cue already on screen when the seek point lands mid-cue,
/// because copy-mode seeking decides packet inclusion by start time only.
fn shift_cue(start: i64, end: i64, seek_units: i64) -> CueDecision {
    let shifted_end = end - seek_units;
    if shifted_end <= 0 {
        CueDecision::Drop
    } else {
        let shifted_start = (start - seek_units).max(0);
        CueDecision::Keep {
            start: shifted_start,
            end: shifted_end,
        }
    }
}

/// Parse a `[[H]H:]MM:SS<sep>fff` timestamp (hour group required for SRT,
/// optional for `WebVTT`'s short form) to whole milliseconds.
fn parse_hms_millis(text: &str, sep: char) -> Option<i64> {
    let text = text.trim();
    let (whole, fraction) = text.split_once(sep)?;
    let millis = parse_fraction_digits(fraction, 3)?;
    let parts: Vec<&str> = whole.split(':').collect();
    let (hours, minutes, seconds): (i64, i64, i64) = match parts.as_slice() {
        [h, m, s] => (h.parse().ok()?, m.parse().ok()?, s.parse().ok()?),
        [m, s] => (0, m.parse().ok()?, s.parse().ok()?),
        _ => return None,
    };
    if !(0..60).contains(&minutes) || !(0..60).contains(&seconds) {
        return None;
    }
    if hours < 0 {
        return None;
    }
    hours
        .checked_mul(60)?
        .checked_add(minutes)?
        .checked_mul(60)?
        .checked_add(seconds)?
        .checked_mul(1_000)?
        .checked_add(millis)
}

/// A source subtitle cannot be safely transformed into the published clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SubtitleTimelineError {
    #[error("subtitle exceeds its byte limit")]
    TooLarge,
    #[error("subtitle contains invalid SRT cues")]
    InvalidSrt,
}

/// Map normalized source-clock SRT cues onto a validated edited timeline.
///
/// Intersects cues with each retained source range, including fragment bounds,
/// and splits cues spanning cuts. Payload lines are preserved; sequence numbers
/// and timestamps are regenerated. The returned clock starts at programme zero;
/// the caller must apply any later programme join exactly once.
///
/// # Errors
/// Rejects malformed or cue-free input atomically and enforces `max_bytes` on
/// both input and output, including amplification from splitting cues. An empty
/// successful result means valid cues had no intersection with retained media.
pub fn map_srt_through_timeline(
    content: &str,
    timeline: &ersatzrs_domain::MediaPlaybackTimeline,
    max_bytes: usize,
) -> std::result::Result<String, SubtitleTimelineError> {
    if content.len() > max_bytes {
        return Err(SubtitleTimelineError::TooLarge);
    }
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let lines: Vec<&str> = content.lines().collect();
    let mut cues = Vec::new();
    for block in lines
        .split(|line| line.trim().is_empty())
        .filter(|block| !block.is_empty())
    {
        if block.len() < 3
            || block[0].is_empty()
            || !block[0].bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(SubtitleTimelineError::InvalidSrt);
        }
        let (start, end) =
            split_subtitle_timing_line(block[1]).ok_or(SubtitleTimelineError::InvalidSrt)?;
        let start = parse_srt_timestamp(start).ok_or(SubtitleTimelineError::InvalidSrt)?;
        let end = parse_srt_timestamp(end).ok_or(SubtitleTimelineError::InvalidSrt)?;
        if start >= end
            || block[2..]
                .iter()
                .any(|line| split_subtitle_timing_line(line).is_some())
        {
            return Err(SubtitleTimelineError::InvalidSrt);
        }
        cues.push((start, end, &block[2..]));
    }
    if cues.is_empty() {
        return Err(SubtitleTimelineError::InvalidSrt);
    }
    cues.sort_by_key(|(start, _, _)| *start);
    let mut output = String::new();
    let mut offset = 0_i64;
    let mut sequence = 0_usize;
    for range in timeline.retained_ranges() {
        // The domain constructor bounds source coordinates to signed milliseconds.
        let range_start =
            i64::try_from(range.start).map_err(|_| SubtitleTimelineError::InvalidSrt)?;
        let range_end = i64::try_from(range.end).map_err(|_| SubtitleTimelineError::InvalidSrt)?;
        for &(start, end, payload) in &cues {
            let start = start.max(range_start);
            let end = end.min(range_end);
            if start >= end {
                continue;
            }
            sequence += 1;
            let mut piece = format!(
                "{sequence}\n{} --> {}\n",
                format_srt_timestamp(offset + (start - range_start)),
                format_srt_timestamp(offset + (end - range_start)),
            );
            for line in payload {
                piece.push_str(line);
                piece.push('\n');
            }
            piece.push('\n');
            if piece.len() > max_bytes.saturating_sub(output.len()) {
                return Err(SubtitleTimelineError::TooLarge);
            }
            output.push_str(&piece);
        }
        offset += range_end - range_start;
    }
    Ok(output)
}

/// Parse a right-padded/truncated fractional-second digit string to a fixed
/// `width`-digit integer (e.g. `width = 3` for milliseconds).
fn parse_fraction_digits(fraction: &str, width: usize) -> Option<i64> {
    if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut digits = fraction.to_owned();
    match digits.len().cmp(&width) {
        std::cmp::Ordering::Less => digits.push_str(&"0".repeat(width - digits.len())),
        std::cmp::Ordering::Greater => digits.truncate(width),
        std::cmp::Ordering::Equal => {}
    }
    digits.parse().ok()
}

fn parse_srt_timestamp(text: &str) -> Option<i64> {
    parse_hms_millis(text, ',').or_else(|| parse_hms_millis(text, '.'))
}

fn format_srt_timestamp(ms: i64) -> String {
    format_hms_millis(ms, ',')
}

fn parse_vtt_timestamp(text: &str) -> Option<i64> {
    parse_hms_millis(text, '.')
}

fn format_vtt_timestamp(ms: i64) -> String {
    format_hms_millis(ms, '.')
}

fn format_hms_millis(ms: i64, sep: char) -> String {
    let ms = ms.max(0);
    let hours = ms / 3_600_000;
    let minutes = (ms / 60_000) % 60;
    let seconds = (ms / 1_000) % 60;
    let millis = ms % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02}{sep}{millis:03}")
}

/// ASS/SSA timestamps are `H:MM:SS.cc` (centiseconds, hour not zero-padded).
fn parse_ass_timestamp(text: &str) -> Option<i64> {
    let text = text.trim();
    let (whole, fraction) = text.split_once('.')?;
    let centis = parse_fraction_digits(fraction, 2)?;
    let parts: Vec<&str> = whole.split(':').collect();
    let [h, m, s] = parts.as_slice() else {
        return None;
    };
    let hours: i64 = h.parse().ok()?;
    let minutes: i64 = m.parse().ok()?;
    let seconds: i64 = s.parse().ok()?;
    if !(0..60).contains(&minutes) || !(0..60).contains(&seconds) {
        return None;
    }
    Some(((hours * 60 + minutes) * 60 + seconds) * 100 + centis)
}

fn format_ass_timestamp(cs: i64) -> String {
    let cs = cs.max(0);
    let hours = cs / 360_000;
    let minutes = (cs / 6_000) % 60;
    let seconds = (cs / 100) % 60;
    let centis = cs % 100;
    format!("{hours}:{minutes:02}:{seconds:02}.{centis:02}")
}

/// Case-insensitive prefix strip that never panics on a short/non-ASCII-boundary line.
fn strip_prefix_ci<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let head = line.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        Some(line[prefix.len()..].trim_start())
    } else {
        None
    }
}

/// Rebase an independently readable text artifact without filesystem work or fallback.
/// SRT/WebVTT preserve millisecond timing; ASS retains its native centisecond precision.
/// `None` means every parsed cue ended before the requested join.
pub(crate) fn rebase_text_cues_milliseconds(
    bytes: Vec<u8>,
    format: SubtitleTextFormat,
    seek_milliseconds: u64,
    limit: usize,
) -> Result<Option<String>> {
    let invalid = || {
        ersatzrs_core::repositories::RepositoryError::unavailable(
            "Next live subtitles",
            "standalone subtitle cues could not be safely rebased",
        )
    };
    if bytes.len() > limit {
        return Err(invalid());
    }
    let content = decode_subtitle_text(bytes).map_err(|_| invalid())?;
    if content.len() > limit {
        return Err(invalid());
    }
    let seek = i64::try_from(seek_milliseconds).map_err(|_| invalid())?;
    let rebased = match format {
        SubtitleTextFormat::Ass => rebase_ass_cues_at(&content, seek / 10),
        SubtitleTextFormat::Srt => rebase_line_block_cues_at(&content, seek, TimestampSyntax::Srt),
        SubtitleTextFormat::WebVtt => {
            rebase_line_block_cues_at(&content, seek, TimestampSyntax::WebVtt)
        }
    }
    .map_err(|_| invalid())?;
    if rebased.text.len() > limit {
        return Err(invalid());
    }
    Ok((rebased.remaining_cues != 0).then_some(rebased.text))
}

/// Rewrite every cue's timestamps in `content` for `seek_seconds`, dispatched
/// by the format `seek_subtitle_output_format` selected (`"srt"`, `"ass"`, or
/// `"webvtt"`).
fn rebase_subtitle_cues(
    content: &str,
    format: &str,
    seek_seconds: i64,
) -> std::result::Result<RebasedSubtitle, RebaseSubtitleError> {
    match format {
        "ass" => rebase_ass_cues(content, seek_seconds),
        "webvtt" => rebase_line_block_cues(content, seek_seconds, TimestampSyntax::WebVtt),
        _ => rebase_line_block_cues(content, seek_seconds, TimestampSyntax::Srt),
    }
}

/// SRT/`WebVTT` engine: cues are blank-line-delimited blocks containing one
/// `-->`-separated timestamp line, with ASCII whitespace (including tabs)
/// around the arrow. A block containing multiple timing lines is rejected so
/// the complete file takes the `FFmpeg` compatibility path instead of being
/// partially rewritten. `WebVTT`'s leading `WEBVTT` header and any
/// `NOTE`/`STYLE`/`REGION` blocks pass through untouched.
fn rebase_line_block_cues(
    content: &str,
    seek_seconds: i64,
    syntax: TimestampSyntax,
) -> std::result::Result<RebasedSubtitle, RebaseSubtitleError> {
    let seek_units = seek_seconds
        .checked_mul(1_000)
        .ok_or(RebaseSubtitleError::UnparseableCue)?;
    rebase_line_block_cues_at(content, seek_units, syntax)
}

fn rebase_line_block_cues_at(
    content: &str,
    seek_units: i64,
    syntax: TimestampSyntax,
) -> std::result::Result<RebasedSubtitle, RebaseSubtitleError> {
    let (parse, format): (TimestampParseFn, TimestampFormatFn) = match syntax {
        TimestampSyntax::Srt => (parse_srt_timestamp, format_srt_timestamp),
        TimestampSyntax::WebVtt => (parse_vtt_timestamp, format_vtt_timestamp),
    };

    let mut blocks: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for line in content.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        blocks.push(current);
    }

    let mut out = String::new();
    let mut remaining_cues = 0_usize;
    let mut recognized_cues = 0_usize;
    let mut sequence = 0_usize;
    let mut first_block = true;

    for block in blocks {
        let mut timing_lines = block.iter().enumerate().filter_map(|(index, line)| {
            split_subtitle_timing_line(line).map(|parts| (index, parts))
        });
        let Some((timestamp_line_index, (start_text, rest))) = timing_lines.next() else {
            if !first_block {
                out.push('\n');
            }
            first_block = false;
            for line in &block {
                out.push_str(line);
                out.push('\n');
            }
            continue;
        };
        if timing_lines.next().is_some() {
            return Err(RebaseSubtitleError::UnparseableCue);
        }

        recognized_cues += 1;
        // WebVTT allows trailing cue settings after the end timestamp
        // (`align:start position:10%`); split off only the first token as
        // the end time and keep the rest untouched.
        let (end_text, settings_suffix) = match rest.split_once(char::is_whitespace) {
            Some((end, suffix)) => (end, Some(suffix)),
            None => (rest, None),
        };
        let start = parse(start_text.trim()).ok_or(RebaseSubtitleError::UnparseableCue)?;
        let end = parse(end_text.trim()).ok_or(RebaseSubtitleError::UnparseableCue)?;

        let CueDecision::Keep { start, end } = shift_cue(start, end, seek_units) else {
            continue;
        };

        if !first_block {
            out.push('\n');
        }
        first_block = false;
        remaining_cues += 1;

        for (index, line) in block.iter().enumerate() {
            if index == timestamp_line_index {
                out.push_str(&format(start));
                out.push_str(" --> ");
                out.push_str(&format(end));
                if let Some(suffix) = settings_suffix {
                    out.push(' ');
                    out.push_str(suffix);
                }
            } else if index == 0 && matches!(syntax, TimestampSyntax::Srt) {
                // SRT sequence number: renumber sequentially for kept cues
                // (matches the previous ffmpeg-muxer's own behavior — the
                // index is regenerated metadata either way).
                sequence += 1;
                out.push_str(&sequence.to_string());
            } else {
                out.push_str(line);
            }
            out.push('\n');
        }
    }

    if recognized_cues == 0 {
        return Err(RebaseSubtitleError::UnparseableCue);
    }

    Ok(RebasedSubtitle {
        text: out,
        remaining_cues,
    })
}

/// Split an SRT/WebVTT cue timing line around `-->`, accepting the ASCII
/// whitespace that both formats permit around the arrow (including tabs).
fn split_subtitle_timing_line(line: &str) -> Option<(&str, &str)> {
    let arrow = line.find("-->")?;
    let (start, tail) = line.split_at(arrow);
    let rest = &tail[3..];
    if !start
        .chars()
        .next_back()
        .is_some_and(|ch| ch.is_ascii_whitespace())
        || !rest
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_whitespace())
    {
        return None;
    }
    Some((
        start.trim_end_matches(|ch: char| ch.is_ascii_whitespace()),
        rest.trim_start_matches(|ch: char| ch.is_ascii_whitespace()),
    ))
}

/// ASS/SSA engine: only `Dialogue:` lines inside `[Events]` are touched. ASCII
/// whitespace around the section name inside the brackets is accepted.
/// `Start`/`End` field positions are located from the section's own
/// `Format:` line (never assumed fixed), and each `Dialogue:` line is split
/// with `splitn(field_count, ',')` so the last field (`Text`, which may
/// contain literal commas inside override tags like `\move(x1,y1,x2,y2)`)
/// stays intact.
fn rebase_ass_cues(
    content: &str,
    seek_seconds: i64,
) -> std::result::Result<RebasedSubtitle, RebaseSubtitleError> {
    let seek_units = seek_seconds
        .checked_mul(100)
        .ok_or(RebaseSubtitleError::UnparseableCue)?;
    rebase_ass_cues_at(content, seek_units)
}

fn rebase_ass_cues_at(
    content: &str,
    seek_units: i64,
) -> std::result::Result<RebasedSubtitle, RebaseSubtitleError> {
    let mut out = String::new();
    let mut remaining_cues = 0_usize;
    let mut recognized_cues = 0_usize;
    let mut in_events = false;
    let mut fields: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') {
            in_events = trimmed
                .strip_prefix('[')
                .and_then(|section| section.strip_suffix(']'))
                .is_some_and(|section| section.trim().eq_ignore_ascii_case("events"));
            out.push_str(line);
            out.push('\n');
            continue;
        }

        if in_events {
            if let Some(rest) = strip_prefix_ci(trimmed, "Format:") {
                fields = rest
                    .split(',')
                    .map(|field| field.trim().to_ascii_lowercase())
                    .collect();
                out.push_str(line);
                out.push('\n');
                continue;
            }

            if let Some(rest) = strip_prefix_ci(trimmed, "Dialogue:") {
                let start_index = fields.iter().position(|field| field == "start");
                let end_index = fields.iter().position(|field| field == "end");
                let (Some(start_index), Some(end_index)) = (start_index, end_index) else {
                    return Err(RebaseSubtitleError::UnparseableCue);
                };

                let mut parts: Vec<&str> = rest.splitn(fields.len(), ',').collect();
                if parts.len() != fields.len() {
                    return Err(RebaseSubtitleError::UnparseableCue);
                }
                let start = parse_ass_timestamp(parts[start_index].trim())
                    .ok_or(RebaseSubtitleError::UnparseableCue)?;
                let end = parse_ass_timestamp(parts[end_index].trim())
                    .ok_or(RebaseSubtitleError::UnparseableCue)?;
                recognized_cues += 1;

                let CueDecision::Keep { start, end } = shift_cue(start, end, seek_units) else {
                    continue;
                };
                remaining_cues += 1;

                let formatted_start = format_ass_timestamp(start);
                let formatted_end = format_ass_timestamp(end);
                parts[start_index] = &formatted_start;
                parts[end_index] = &formatted_end;

                out.push_str("Dialogue: ");
                out.push_str(&parts.join(","));
                out.push('\n');
                continue;
            }
        }

        out.push_str(line);
        out.push('\n');
    }

    if recognized_cues == 0 {
        return Err(RebaseSubtitleError::UnparseableCue);
    }

    Ok(RebasedSubtitle {
        text: out,
        remaining_cues,
    })
}

/// Re-base a text subtitle file to `seek_seconds` for burned playback.
///
/// Rewrites normalized cue timestamps directly (see ADR 0061):
/// every cue is shifted by `-seek_seconds`, a cue fully in the past after the
/// shift is dropped, and a cue that straddles the seek point (already
/// displaying when it lands) is clamped to start at `0` instead of being
/// dropped, so it keeps showing for its remaining visible duration in the new
/// chunk.
///
/// Local and HTTP(S) sources are normalized to Unicode before parsing,
/// including UTF-8 and BOM-marked UTF-16. If normalization or cue parsing is
/// unsupported, the previous ffmpeg seek-and-copy rebase remains as a
/// compatibility fallback rather than passing absolute cue timestamps to a
/// zero-based playback chunk. Returns `None` when no cues remain after
/// rebasing: an empty file is written and cached as a skip-burn marker.
pub async fn rebase_text_subtitle(input_path: &str, seek_seconds: i64) -> Option<String> {
    rebase_text_subtitle_with_format(input_path, None, seek_seconds).await
}

/// Strict retained-SRT path: source bytes have already passed publication verification.
pub(crate) async fn rebase_verified_srt(
    content: &str,
    seek_milliseconds: u64,
) -> Result<Option<String>> {
    rebase_verified_srt_in(content, seek_milliseconds, &subtitle_seek_cache_folder()).await
}

async fn rebase_verified_srt_in(
    content: &str,
    seek_milliseconds: u64,
    cache: &Path,
) -> Result<Option<String>> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let invalid = || {
        ersatzrs_core::repositories::RepositoryError::Conflict(
            "verified subtitle seek could not be prepared".into(),
        )
    };
    let timeline = ersatzrs_domain::MediaPlaybackTimeline::from_exclusions(
        i64::MAX as u64,
        seek_milliseconds..i64::MAX as u64,
        &[],
    )
    .map_err(|_| invalid())?;
    let limit = 8 * 1024 * 1024;
    let output = map_srt_through_timeline(content, &timeline, limit).map_err(|_| invalid())?;
    if output.is_empty() {
        return Ok(None);
    }
    let mut hash = sha2::Sha256::new();
    hash.update(b"ersatzrs:verified-srt-seek:v1\0");
    hash.update(seek_milliseconds.to_le_bytes());
    hash.update(content.as_bytes());
    let name = hash
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut name, byte| {
            use std::fmt::Write as _;
            write!(name, "{byte:02x}").expect("string write");
            name
        });
    tokio::fs::create_dir_all(cache)
        .await
        .map_err(|_| invalid())?;
    let path = cache.join(format!("verified-{name}.srt"));
    let path_text = path.to_str().ok_or_else(invalid)?.to_owned();
    match tokio::fs::File::open(&path).await {
        Ok(file) => {
            let mut cached = Vec::new();
            file.take(u64::try_from(limit + 1).map_err(|_| invalid())?)
                .read_to_end(&mut cached)
                .await
                .map_err(|_| invalid())?;
            if cached == output.as_bytes() {
                return Ok(Some(path_text));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(invalid()),
    }
    // A unique temporary file and atomic replacement prevent concurrent joins
    // from observing a partial cache entry. Dropped futures remove their temp file.
    let temporary = tempfile::NamedTempFile::new_in(cache).map_err(|_| invalid())?;
    let mut file = tokio::fs::File::from_std(temporary.reopen().map_err(|_| invalid())?);
    file.write_all(output.as_bytes())
        .await
        .map_err(|_| invalid())?;
    file.flush().await.map_err(|_| invalid())?;
    drop(file);
    temporary.persist(&path).map_err(|_| invalid())?;
    Ok(Some(path_text))
}

async fn rebase_text_subtitle_with_format(
    input_path: &str,
    format_hint: Option<SubtitleTextFormat>,
    seek_seconds: i64,
) -> Option<String> {
    if seek_seconds <= 0 {
        return Some(input_path.to_owned());
    }

    let output_path = seek_subtitle_output_path(input_path, format_hint, seek_seconds);
    if output_path.exists() {
        return non_empty_subtitle_path(&output_path).await;
    }

    if let Some(parent) = output_path.parent() {
        if let Err(error) = tokio::fs::create_dir_all(parent).await {
            tracing::warn!(
                %error,
                input_hash = %md5_upper_hex(input_path),
                seek_seconds,
                "failed to create subtitle seek cache directory"
            );
            return Some(input_path.to_owned());
        }
    }

    let content = match read_subtitle_text_source(input_path).await {
        Ok(content) => content,
        Err(error) => {
            tracing::warn!(
                %error,
                input_hash = %md5_upper_hex(input_path),
                seek_seconds,
                "failed to normalize subtitle for cue-aware playback seek; falling back to ffmpeg"
            );
            return rebase_text_subtitle_with_ffmpeg(
                input_path,
                format_hint,
                seek_seconds,
                &output_path,
            )
            .await;
        }
    };

    let (format, _extension) = seek_subtitle_output_format(input_path, format_hint);
    let rebased = match rebase_subtitle_cues(&content, format, seek_seconds) {
        Ok(rebased) => rebased,
        Err(error) => {
            tracing::warn!(
                ?error,
                input_hash = %md5_upper_hex(input_path),
                seek_seconds,
                "failed to parse subtitle for cue-aware playback seek; falling back to ffmpeg"
            );
            return rebase_text_subtitle_with_ffmpeg(
                input_path,
                format_hint,
                seek_seconds,
                &output_path,
            )
            .await;
        }
    };

    let bytes: &[u8] = if rebased.remaining_cues == 0 {
        b""
    } else {
        rebased.text.as_bytes()
    };
    match tokio::fs::write(&output_path, bytes).await {
        Ok(()) => non_empty_subtitle_path(&output_path).await,
        Err(error) => {
            tracing::warn!(
                %error,
                input_hash = %md5_upper_hex(input_path),
                seek_seconds,
                "failed to write rebased subtitle for playback seek"
            );
            Some(input_path.to_owned())
        }
    }
}

async fn rebase_text_subtitle_with_ffmpeg(
    input_path: &str,
    format_hint: Option<SubtitleTextFormat>,
    seek_seconds: i64,
    output_path: &Path,
) -> Option<String> {
    let args = build_seek_subtitle_args(input_path, format_hint, seek_seconds, output_path);
    let output = match Command::new(ffmpeg_path()).args(&args).output().await {
        Ok(output) => output,
        Err(error) => {
            tracing::warn!(
                %error,
                input_hash = %md5_upper_hex(input_path),
                seek_seconds,
                "failed to spawn ffmpeg compatibility subtitle seek rebase"
            );
            return Some(input_path.to_owned());
        }
    };

    if output.status.success() {
        non_empty_subtitle_path(output_path).await
    } else {
        tracing::warn!(
            input_hash = %md5_upper_hex(input_path),
            seek_seconds,
            status = %output.status,
            stderr_bytes = output.stderr.len(),
            "failed ffmpeg compatibility subtitle seek rebase"
        );
        Some(input_path.to_owned())
    }
}

/// A successfully re-based subtitle with no remaining cues is written as an
/// empty file; report it as `None` so callers drop the burn instead of handing
/// libass a file it cannot open. The empty file stays in the cache as a marker
/// for subsequent requests at the same seek point.
async fn non_empty_subtitle_path(output_path: &Path) -> Option<String> {
    match tokio::fs::metadata(output_path).await {
        Ok(metadata) if metadata.len() == 0 => {
            tracing::debug!(
                path = %output_path.display(),
                "seeked subtitle has no remaining cues; skipping burn"
            );
            None
        }
        _ => Some(output_path.to_string_lossy().into_owned()),
    }
}

/// Re-base only burned text subtitle inputs. Image subtitles and copied
/// subtitle streams already ride the seeked media input and must not be
/// rewritten.
pub async fn rebase_burned_text_subtitle_input(
    input: Option<InputFile>,
    seek_seconds: i64,
) -> Option<InputFile> {
    let mut input = input?;
    if input.kind == InputKind::Subtitle
        && input.subtitle_method == Some(SubtitleMethod::Burn)
        && !input.is_image_based_subtitle
        && seek_seconds > 0
    {
        input.path =
            rebase_text_subtitle_with_format(&input.path, input.subtitle_text_format, seek_seconds)
                .await?;
    }
    Some(input)
}

/// One subtitle row scheduled for extraction.
struct SubtitleToExtract {
    id: i64,
    stream_index: i32,
    codec: String,
    relative_output: PathBuf,
}

#[derive(Clone, Copy)]
struct MetadataSubtitleJoin {
    metadata_table: &'static str,
    owner_column: &'static str,
    subtitle_column: &'static str,
}

const METADATA_SUBTITLE_JOINS: &[MetadataSubtitleJoin] = &[
    MetadataSubtitleJoin {
        metadata_table: "EpisodeMetadata",
        owner_column: "EpisodeId",
        subtitle_column: "EpisodeMetadataId",
    },
    MetadataSubtitleJoin {
        metadata_table: "MovieMetadata",
        owner_column: "MovieId",
        subtitle_column: "MovieMetadataId",
    },
    MetadataSubtitleJoin {
        metadata_table: "MusicVideoMetadata",
        owner_column: "MusicVideoId",
        subtitle_column: "MusicVideoMetadataId",
    },
    MetadataSubtitleJoin {
        metadata_table: "OtherVideoMetadata",
        owner_column: "OtherVideoId",
        subtitle_column: "OtherVideoMetadataId",
    },
];

/// Build the ffmpeg subtitle-extraction argv (port of the `ArgumentsBuilder`
/// block in `ExtractSubtitles`): `-nostdin -hide_banner -i <input>` then per
/// subtitle `-map 0:<idx> -c:s <copy|text> <fullOutputPath>`.
fn build_subtitle_args(input_path: &str, subtitles: &[SubtitleToExtract]) -> Vec<String> {
    let mut args = vec![
        "-nostdin".to_owned(),
        "-hide_banner".to_owned(),
        "-i".to_owned(),
        input_path.to_owned(),
    ];
    let cache = subtitle_cache_folder();
    for sub in subtitles {
        let full = cache.join(&sub.relative_output);
        args.push("-map".to_owned());
        args.push(format!("0:{}", sub.stream_index));
        args.push("-c:s".to_owned());
        args.push(subtitle_codec_arg(&sub.codec).to_owned());
        args.push(full.to_string_lossy().into_owned());
    }
    args
}

/// Build the ffmpeg font-dump argv (port of the `arguments` string in
/// `ExtractFonts`): `-nostdin -hide_banner -dump_attachment:t:<i> "" -i <input> -y`.
/// Run with the fonts cache as the working directory.
fn build_font_args(input_path: &str, attachment_index: usize) -> Vec<String> {
    vec![
        "-nostdin".to_owned(),
        "-hide_banner".to_owned(),
        format!("-dump_attachment:t:{attachment_index}"),
        String::new(),
        "-i".to_owned(),
        input_path.to_owned(),
        "-y".to_owned(),
    ]
}

/// Load all configured path-replacements across providers, mirroring
/// `stream.rs` `load_path_replacements` but querying directly (infra cannot
/// depend on `ersatzrs-web`). Each provider table stores `{Provider}Path`
/// (source) and `LocalPath` (replacement).
async fn load_path_replacements(pool: &Pool<Db>) -> Result<Vec<PathReplacement>> {
    let rows = sqlx::query(
        r#"SELECT "PlexPath" AS "Source", "LocalPath" AS "Local" FROM "PlexPathReplacement"
           UNION ALL
           SELECT "JellyfinPath" AS "Source", "LocalPath" AS "Local" FROM "JellyfinPathReplacement"
           UNION ALL
           SELECT "EmbyPath" AS "Source", "LocalPath" AS "Local" FROM "EmbyPathReplacement""#,
    )
    .fetch_all(pool)
    .await
    .repository()?;

    let mut replacements = Vec::new();
    for row in rows {
        let source: Option<String> = row.try_get("Source").repository()?;
        let local: Option<String> = row.try_get("Local").repository()?;
        if let (Some(source_path), Some(replacement_path)) = (source, local) {
            let source_platform = if source_path.contains('\\') || source_path.contains(':') {
                PathPlatform::Windows
            } else {
                PathPlatform::Unix
            };
            replacements.push(PathReplacement {
                source_platform,
                source_path,
                replacement_path,
            });
        }
    }
    Ok(replacements)
}

/// Resolve the media item's head-version `MediaFile."Path"`, with path-replacements
/// applied (target platform Unix, matching `stream.rs`).
async fn resolve_input_path(
    pool: &Pool<Db>,
    media_item_id: i64,
    replacements: &[PathReplacement],
) -> Result<Option<String>> {
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "Path"
           FROM "MediaVersion" mv
           JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE COALESCE(
                     mv."EpisodeId", mv."MovieId", mv."MusicVideoId", mv."OtherVideoId"
                 ) = ?
             AND mf."Path" IS NOT NULL
           ORDER BY mv."Id" ASC, mf."Id" ASC
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await
    .repository()?;

    let Some(row) = row else {
        return Ok(None);
    };
    let path: String = row.try_get("Path").repository()?;
    let resolved = if replacements.is_empty() {
        path
    } else {
        replace_path(replacements, &path, PathPlatform::Unix)
    };
    Ok(Some(resolved))
}

/// Episode IDs under `show_id` that have an embedded text `Subtitle` needing
/// extraction. Ports the SQL in `ExtractAll` (handler lines 75-85) AND the
/// text-codec filter from `GetMediaItemIdsWithTextSubtitles` (image codecs
/// excluded directly in SQL).
async fn episode_ids_with_text_subtitles(pool: &Pool<Db>, show_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query(
        r#"SELECT E."Id" AS "Id"
           FROM "Episode" E
           INNER JOIN "Season" S ON S."Id" = E."SeasonId"
           WHERE S."ShowId" = ?
           ORDER BY E."Id""#,
    )
    .bind(show_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let episode_ids: Vec<i64> = rows
        .into_iter()
        .map(|row| row.try_get("Id"))
        .collect::<std::result::Result<_, _>>()
        .repository()?;
    media_item_ids_with_text_subtitles(pool, &episode_ids).await
}

async fn playout_media_item_ids_with_text_subtitles(
    pool: &Pool<Db>,
    playout_id: i64,
) -> Result<Vec<i64>> {
    let now = Utc::now();
    let until = now + Duration::hours(1);
    let rows = sqlx::query(
        r#"SELECT PI."MediaItemId" AS "MediaItemId", MIN(PI."Start") AS "SortStart"
           FROM "PlayoutItem" PI
           INNER JOIN "Playout" P ON P."Id" = PI."PlayoutId"
           INNER JOIN "Channel" C ON C."Id" = P."ChannelId"
           WHERE PI."PlayoutId" = ?
             AND PI."Finish" >= ?
             AND PI."Start" <= ?
             AND (
               C."SubtitleMode" != 0
               -- Custom selectors can request embedded text with channel subtitles off.
               OR C."StreamSelectorMode" = 1
               OR EXISTS (
                 SELECT 1
                 FROM "ProgramScheduleItem" PSI
                 WHERE PSI."ProgramScheduleId" = P."ProgramScheduleId"
                   AND PSI."SubtitleMode" IS NOT NULL
                   AND PSI."SubtitleMode" != 0
               )
             )
           GROUP BY PI."MediaItemId"
           ORDER BY MIN(PI."Start")"#,
    )
    .bind(playout_id)
    .bind(encode_datetime(now))
    .bind(encode_datetime(until))
    .fetch_all(pool)
    .await
    .repository()?;
    let media_item_ids: Vec<i64> = rows
        .into_iter()
        .map(|row| row.try_get("MediaItemId"))
        .collect::<std::result::Result<_, _>>()
        .repository()?;
    media_item_ids_with_text_subtitles(pool, &media_item_ids).await
}

async fn media_item_ids_with_text_subtitles(
    pool: &Pool<Db>,
    media_item_ids: &[i64],
) -> Result<Vec<i64>> {
    if media_item_ids.is_empty() {
        return Ok(Vec::new());
    }

    let media_placeholders = media_item_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let placeholders = IMAGE_CODECS
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let mut result = Vec::new();

    for join in METADATA_SUBTITLE_JOINS {
        let sql = format!(
            r#"SELECT DISTINCT M."{owner_column}" AS "Id"
               FROM "{metadata_table}" M
               INNER JOIN "Subtitle" SUB ON SUB."{subtitle_column}" = M."Id"
               WHERE M."{owner_column}" IN ({media_placeholders})
                 AND SUB."SubtitleKind" = {embedded}
                 AND SUB."Codec" IS NOT NULL
                 AND SUB."Codec" NOT IN ({placeholders})"#,
            metadata_table = join.metadata_table,
            owner_column = join.owner_column,
            subtitle_column = join.subtitle_column,
            embedded = subtitle_kind::EMBEDDED,
        );
        let mut query = sqlx::query(crate::db::assert_sql_safe(&sql));
        for id in media_item_ids {
            query = query.bind(*id);
        }
        for codec in IMAGE_CODECS {
            query = query.bind(*codec);
        }
        let rows = query.fetch_all(pool).await.repository()?;
        for row in rows {
            let id: i64 = row.try_get("Id").repository()?;
            if !result.contains(&id) {
                result.push(id);
            }
        }
    }

    result.sort_by_key(|id| {
        media_item_ids
            .iter()
            .position(|candidate| candidate == id)
            .unwrap_or(usize::MAX)
    });
    Ok(result)
}

/// Extract embedded text subtitles + font attachments for every episode under
/// `show_id`. Self-gates on the ffmpeg config keys, so a default-configured
/// deployment (`extract_embedded_subtitles = false`) is a no-op. Per-episode
/// ffmpeg failures are logged and skipped (matching ErsatzTV's swallow).
///
/// # Errors
///
/// Returns an error only when a database query fails; ffmpeg spawn/exit
/// failures are logged and do not abort the run.
pub async fn extract_embedded_show_subtitles(pool: &Pool<Db>, show_id: i64) -> Result<()> {
    extract_embedded_episode_subtitles(pool, episode_ids_with_text_subtitles(pool, show_id).await?)
        .await
}

/// Extract embedded text subtitles + font attachments for every eligible media
/// item in one playout. This is the ErsatzRS equivalent of ErsatzTV's
/// `ExtractEmbeddedSubtitles(Some(playoutId))` background request.
///
/// # Errors
///
/// Returns an error only when a database query fails; ffmpeg spawn/exit
/// failures are logged and do not abort the run.
pub async fn extract_embedded_playout_subtitles(pool: &Pool<Db>, playout_id: i64) -> Result<()> {
    extract_embedded_media_item_subtitles(
        pool,
        playout_media_item_ids_with_text_subtitles(pool, playout_id).await?,
    )
    .await
}

async fn extract_embedded_episode_subtitles(pool: &Pool<Db>, episode_ids: Vec<i64>) -> Result<()> {
    extract_embedded_media_item_subtitles(pool, episode_ids).await
}

async fn extract_embedded_media_item_subtitles(
    pool: &Pool<Db>,
    media_item_ids: Vec<i64>,
) -> Result<()> {
    // ffmpeg path resolved via the shared locator (DB config / next-to-exe /
    // PATH), matching ErsatzTV's FFmpegLocator.
    let ffmpeg_path = ffmpeg_path();

    // Gate (mirror handler lines 48-64): both keys must be enabled.
    if !use_embedded_subtitles(pool).await? || !extract_embedded_subtitles(pool).await? {
        return Ok(());
    }

    if media_item_ids.is_empty() {
        return Ok(());
    }

    let replacements = load_path_replacements(pool).await?;
    for media_item_id in media_item_ids {
        let Some(input_path) = resolve_input_path(pool, media_item_id, &replacements).await? else {
            tracing::debug!(
                media_item_id,
                "no media path for media item; skipping extraction"
            );
            continue;
        };
        extract_subtitles(pool, media_item_id, &input_path, &ffmpeg_path).await?;
        extract_fonts(pool, media_item_id, &input_path, &ffmpeg_path).await?;
    }

    Ok(())
}

/// Load extraction candidates against the cache root, independent of process cwd.
async fn subtitles_to_extract(
    pool: &Pool<Db>,
    media_item_id: i64,
    cache: &Path,
) -> Result<Vec<SubtitleToExtract>> {
    // All embedded subtitles for this media item. Image codecs and "already
    // extracted" rows are filtered in-memory below, mirroring the `.Filter(...)`
    // chain in ErsatzTV's `ExtractSubtitles`.
    let mut rows = Vec::new();
    for join in METADATA_SUBTITLE_JOINS {
        let sql = format!(
            r#"SELECT SUB."Id" AS "Id", SUB."StreamIndex" AS "StreamIndex",
                      SUB."Codec" AS "Codec", SUB."IsExtracted" AS "IsExtracted",
                      SUB."Path" AS "Path"
               FROM "Subtitle" SUB
               INNER JOIN "{metadata_table}" M ON M."Id" = SUB."{subtitle_column}"
               WHERE M."{owner_column}" = ?
                 AND SUB."SubtitleKind" = {embedded}
                 AND SUB."Codec" IS NOT NULL"#,
            metadata_table = join.metadata_table,
            subtitle_column = join.subtitle_column,
            owner_column = join.owner_column,
            embedded = subtitle_kind::EMBEDDED,
        );
        rows.extend(
            sqlx::query(crate::db::assert_sql_safe(&sql))
                .bind(media_item_id)
                .fetch_all(pool)
                .await
                .repository()?,
        );
    }

    let mut to_extract = Vec::new();
    for row in rows {
        let codec: String = row.try_get("Codec").repository()?;
        let is_extracted: i64 = row.try_get("IsExtracted").repository()?;
        let path: Option<String> = row.try_get("Path").repository()?;
        let stream_index: i32 = row.try_get("StreamIndex").repository()?;
        let id: i64 = row.try_get("Id").repository()?;

        if is_image_codec(&codec) {
            continue;
        }
        let Some(relative_output) = relative_output_path(media_item_id, stream_index, &codec)
        else {
            continue;
        };
        let full = cache.join(&relative_output);
        // Re-extract when not extracted, no path recorded, or the cache file is
        // missing (mirror the `FileDoesntExist` filter in ExtractSubtitles).
        let needs = is_extracted == 0
            || path.as_deref().is_none_or(|path| path.trim().is_empty())
            || !full.exists();
        if !needs {
            continue;
        }
        to_extract.push(SubtitleToExtract {
            id,
            stream_index,
            codec,
            relative_output,
        });
    }

    Ok(to_extract)
}

/// Extract this media item's embedded text subtitles. Ports `ExtractSubtitles`.
async fn extract_subtitles(
    pool: &Pool<Db>,
    media_item_id: i64,
    input_path: &str,
    ffmpeg_path: &str,
) -> Result<()> {
    let to_extract = subtitles_to_extract(pool, media_item_id, &subtitle_cache_folder()).await?;

    if to_extract.is_empty() {
        return Ok(());
    }

    // Ensure output directories exist and delete any stale output.
    for sub in &to_extract {
        let full = subtitle_cache_folder().join(&sub.relative_output);
        if let Some(parent) = full.parent() {
            if let Err(error) = tokio::fs::create_dir_all(parent).await {
                tracing::error!(%error, "failed to create subtitle cache directory");
                return Ok(());
            }
        }
        if full.exists() {
            let _ = tokio::fs::remove_file(&full).await;
        }
    }

    let args = build_subtitle_args(input_path, &to_extract);
    let output = match Command::new(ffmpeg_path).args(&args).output().await {
        Ok(output) => output,
        Err(error) => {
            tracing::error!(%error, media_item_id, "failed to spawn ffmpeg for subtitle extraction");
            return Ok(());
        }
    };

    if output.status.success() {
        for sub in &to_extract {
            sqlx::query(r#"UPDATE "Subtitle" SET "IsExtracted" = 1, "Path" = ? WHERE "Id" = ?"#)
                .bind(sub.relative_output.to_string_lossy().into_owned())
                .bind(sub.id)
                .execute(pool)
                .await
                .repository()?;
        }
        tracing::debug!(
            media_item_id,
            count = to_extract.len(),
            "successfully extracted subtitles"
        );
    } else {
        tracing::error!(
            media_item_id,
            stderr = %String::from_utf8_lossy(&output.stderr),
            "failed to extract subtitles"
        );
    }

    Ok(())
}

/// Extract this media item's font attachments. Ports `ExtractFonts`. ffmpeg exits
/// non-zero even on a successful dump, so success is detected by the output
/// file existing.
async fn extract_fonts(
    pool: &Pool<Db>,
    media_item_id: i64,
    input_path: &str,
    ffmpeg_path: &str,
) -> Result<()> {
    // Attachment streams for the episode head version, ordered by Index. The
    // 0-based position in this list is the `-dump_attachment:t:<i>` index.
    let rows = sqlx::query(
        r#"SELECT ms."FileName" AS "FileName", ms."MimeType" AS "MimeType"
           FROM "MediaVersion" mv
           JOIN "MediaStream" ms ON ms."MediaVersionId" = mv."Id"
           WHERE COALESCE(
                     mv."EpisodeId", mv."MovieId", mv."MusicVideoId", mv."OtherVideoId"
                 ) = ?
             AND ms."MediaStreamKind" = 4
           ORDER BY mv."Id" ASC, ms."Index" ASC"#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await
    .repository()?;

    let fonts_dir = fonts_cache_folder();
    for (attachment_index, row) in rows.iter().enumerate() {
        let mime_type: Option<String> = row.try_get("MimeType").repository()?;
        let file_name: Option<String> = row.try_get("FileName").repository()?;

        let mime = mime_type.unwrap_or_default();
        if !mime.contains("font") && !mime.contains("opentype") {
            continue;
        }
        let Some(file_name) = file_name.filter(|name| !name.trim().is_empty()) else {
            continue;
        };

        let full = fonts_dir.join(&file_name);
        if full.exists() {
            // already extracted
            continue;
        }
        if let Err(error) = tokio::fs::create_dir_all(&fonts_dir).await {
            tracing::error!(%error, "failed to create fonts cache directory");
            return Ok(());
        }

        let args = build_font_args(input_path, attachment_index);
        let result = Command::new(ffmpeg_path)
            .args(&args)
            .current_dir(&fonts_dir)
            .output()
            .await;
        let stderr = match result {
            Ok(output) => String::from_utf8_lossy(&output.stderr).into_owned(),
            Err(error) => {
                tracing::error!(%error, media_item_id, "failed to spawn ffmpeg for font extraction");
                continue;
            }
        };

        // ffmpeg returns exit code 1 even on a successful dump; check the file.
        if full.exists() {
            tracing::debug!(media_item_id, font = %file_name, "successfully extracted font");
        } else {
            tracing::error!(media_item_id, font = %file_name, %stderr, "failed to extract attached font");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use sqlx::Executor;

    async fn test_db() -> Pool<Db> {
        let temp_dir =
            std::env::temp_dir().join(format!("ersatzrs-subtitles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create subtitles test db dir");
        let database_url = format!("sqlite:{}", temp_dir.join("ErsatzRS.db").display());
        crate::db::init(&database_url)
            .await
            .expect("initialize subtitles test database")
    }

    #[test]
    fn timeline_srt_maps_before_after_and_split_cues_with_unicode_payload() {
        let timeline = ersatzrs_domain::MediaPlaybackTimeline::from_exclusions(
            10_000,
            0..10_000,
            &[2_000..4_000, 6_000..7_000],
        )
        .unwrap();
        let source = "1\n00:00:00,500 --> 00:00:01,500\nBefore\n\n2\n00:00:01,500 --> 00:00:04,500\nHéllo 世界\nSecond line\n\n3\n00:00:07,000 --> 00:00:08,000\nAfter\n";
        assert_eq!(
            map_srt_through_timeline(source, &timeline, 4096).unwrap(),
            "1\n00:00:00,500 --> 00:00:01,500\nBefore\n\n2\n00:00:01,500 --> 00:00:02,000\nHéllo 世界\nSecond line\n\n3\n00:00:02,000 --> 00:00:02,500\nHéllo 世界\nSecond line\n\n4\n00:00:04,000 --> 00:00:05,000\nAfter\n\n",
        );
    }

    #[test]
    fn timeline_srt_respects_cut_boundaries_and_source_fragment() {
        let timeline = ersatzrs_domain::MediaPlaybackTimeline::from_exclusions(
            10_000,
            2_000..8_000,
            &[4_000..6_000, 8_000..9_000],
        )
        .unwrap();
        let source = "1\n00:00:00,000 --> 00:00:02,000\nBefore fragment\n\n2\n00:00:01,000 --> 00:00:04,000\nLeft\n\n3\n00:00:04,000 --> 00:00:06,000\nExcluded\n\n4\n00:00:06,000 --> 00:00:09,000\nRight\n\n5\n00:00:08,000 --> 00:00:10,000\nAfter fragment\n";
        assert_eq!(
            map_srt_through_timeline(source, &timeline, 4096).unwrap(),
            "1\n00:00:00,000 --> 00:00:02,000\nLeft\n\n2\n00:00:02,000 --> 00:00:04,000\nRight\n\n",
        );
        assert_eq!(
            map_srt_through_timeline(
                "1\n00:00:04,000 --> 00:00:06,000\nRemoved\n",
                &timeline,
                4096
            )
            .unwrap(),
            "",
        );
    }

    #[test]
    fn timeline_srt_rejects_invalid_input_atomically_and_bounds_split_output() {
        let timeline = ersatzrs_domain::MediaPlaybackTimeline::from_exclusions(
            10_000,
            0..10_000,
            &[2_000..4_000, 6_000..7_000],
        )
        .unwrap();
        let valid = "1\n00:00:00,000 --> 00:00:10,000\nCaption\n";
        for invalid in [
            String::new(),
            "Not SRT".into(),
            "1\n00:00:02,000 --> 00:00:01,000\nReversed\n".into(),
            "1\n00:00:02,000 --> 00:00:02,000\nEmpty\n".into(),
            "1\n9223372036854775807:00:00,000 --> 00:00:02,000\nOverflow\n".into(),
            "1\n-1:00:00,000 --> 00:00:02,000\nNegative\n".into(),
            format!("{valid}\n2\ninvalid --> timing\nBad trailing cue\n"),
            format!("{valid}\nUnrecognized trailing block\n"),
        ] {
            assert_eq!(
                map_srt_through_timeline(&invalid, &timeline, 4096),
                Err(SubtitleTimelineError::InvalidSrt)
            );
        }
        assert_eq!(
            map_srt_through_timeline(valid, &timeline, valid.len() - 1),
            Err(SubtitleTimelineError::TooLarge)
        );
        assert_eq!(
            map_srt_through_timeline(valid, &timeline, valid.len()),
            Err(SubtitleTimelineError::TooLarge)
        );
        let expected = map_srt_through_timeline(valid, &timeline, 4096).unwrap();
        let crlf = format!("\u{feff}{}", valid.replace('\n', "\r\n"));
        assert_eq!(
            map_srt_through_timeline(&crlf, &timeline, 4096).unwrap(),
            expected
        );
    }

    #[test]
    fn image_codec_filter_matches_etv_list() {
        for codec in IMAGE_CODECS {
            assert!(is_image_codec(codec), "{codec} should be an image codec");
        }
        for codec in ["subrip", "ass", "webvtt", "mov_text", "srt"] {
            assert!(!is_image_codec(codec), "{codec} should be text");
        }
    }

    #[tokio::test]
    async fn verified_srt_seek_is_millisecond_exact_and_content_fenced() {
        let directory = tempfile::tempdir().unwrap();
        let content = "1\n00:00:01,000 --> 00:00:03,000\nHéllo\n";
        let path = rebase_verified_srt_in(content, 1843, directory.path())
            .await
            .unwrap()
            .unwrap();
        let expected = "1\n00:00:00,000 --> 00:00:01,157\nHéllo\n\n";
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), expected);
        assert_eq!(
            rebase_verified_srt_in(content, 1843, directory.path())
                .await
                .unwrap(),
            Some(path.clone())
        );
        let revised = content.replace("Héllo", "Revised");
        let revised_path = rebase_verified_srt_in(&revised, 1843, directory.path())
            .await
            .unwrap()
            .unwrap();
        assert_ne!(path, revised_path);
        assert!(tokio::fs::read_to_string(&revised_path)
            .await
            .unwrap()
            .contains("Revised"));
        let other_join = rebase_verified_srt_in(content, 1844, directory.path())
            .await
            .unwrap()
            .unwrap();
        assert_ne!(path, other_join);
        // A digest-derived filename is not permission to trust corrupted cache bytes.
        tokio::fs::write(&path, b"stale cached captions")
            .await
            .unwrap();
        assert_eq!(
            rebase_verified_srt_in(content, 1843, directory.path())
                .await
                .unwrap(),
            Some(path.clone())
        );
        assert_eq!(tokio::fs::read_to_string(path).await.unwrap(), expected);
    }

    #[tokio::test]
    async fn verified_srt_seek_exhaustion_is_distinct_from_invalid_or_failed_cache() {
        let directory = tempfile::tempdir().unwrap();
        let content = "1\n00:00:01,000 --> 00:00:03,000\nCaption\n";
        assert!(rebase_verified_srt_in(content, 3000, directory.path())
            .await
            .unwrap()
            .is_none());
        assert!(rebase_verified_srt_in("invalid", 0, directory.path())
            .await
            .is_err());
        let blocked = directory.path().join("not-a-directory");
        tokio::fs::write(&blocked, b"fixture").await.unwrap();
        assert!(rebase_verified_srt_in(content, 1843, &blocked)
            .await
            .is_err());
        assert!(rebase_verified_srt_in(content, u64::MAX, directory.path())
            .await
            .is_err());
    }

    #[test]
    fn relative_output_path_uses_md5_upper_hex_and_extension_map() {
        // MD5("1_2_subrip") upper-hex.
        let path = relative_output_path(1, 2, "subrip").unwrap();
        let expected_hash = {
            let mut hasher = Md5::new();
            hasher.update(b"1_2_subrip");
            let digest = hasher.finalize();
            let mut s = String::new();
            for byte in digest {
                use std::fmt::Write as _;
                write!(&mut s, "{byte:02X}").unwrap();
            }
            s
        };
        let expected = Path::new(&expected_hash[..2])
            .join(&expected_hash[2..4])
            .join(format!("{expected_hash}.srt"));
        assert_eq!(path, expected);

        // Extension mapping.
        assert!(relative_output_path(1, 0, "srt")
            .unwrap()
            .to_string_lossy()
            .ends_with(".srt"));
        assert!(relative_output_path(1, 0, "ass")
            .unwrap()
            .to_string_lossy()
            .ends_with(".ass"));
        assert!(relative_output_path(1, 0, "webvtt")
            .unwrap()
            .to_string_lossy()
            .ends_with(".vtt"));
        assert!(relative_output_path(1, 0, "mov_text")
            .unwrap()
            .to_string_lossy()
            .ends_with(".srt"));
        // Unknown codec yields no output path.
        assert!(relative_output_path(1, 0, "pgssub").is_none());
    }

    #[test]
    fn subtitle_codec_arg_uses_text_only_for_mov_text() {
        assert_eq!(subtitle_codec_arg("mov_text"), "text");
        assert_eq!(subtitle_codec_arg("subrip"), "copy");
        assert_eq!(subtitle_codec_arg("ass"), "copy");
    }

    #[test]
    fn build_subtitle_args_snapshot() {
        let cache = subtitle_cache_folder();
        let subs = vec![
            SubtitleToExtract {
                id: 1,
                stream_index: 2,
                codec: "subrip".to_owned(),
                relative_output: Path::new("ab").join("cd").join("AABBCC.srt"),
            },
            SubtitleToExtract {
                id: 2,
                stream_index: 3,
                codec: "mov_text".to_owned(),
                relative_output: Path::new("ef").join("01").join("DDEEFF.srt"),
            },
        ];
        let args = build_subtitle_args("/srv/media/show/s01e01.mkv", &subs);

        let out1 = cache.join("ab").join("cd").join("AABBCC.srt");
        let out2 = cache.join("ef").join("01").join("DDEEFF.srt");
        assert_eq!(
            args,
            vec![
                "-nostdin".to_owned(),
                "-hide_banner".to_owned(),
                "-i".to_owned(),
                "/srv/media/show/s01e01.mkv".to_owned(),
                "-map".to_owned(),
                "0:2".to_owned(),
                "-c:s".to_owned(),
                "copy".to_owned(),
                out1.to_string_lossy().into_owned(),
                "-map".to_owned(),
                "0:3".to_owned(),
                "-c:s".to_owned(),
                "text".to_owned(),
                out2.to_string_lossy().into_owned(),
            ]
        );
    }

    #[test]
    fn seek_subtitle_output_format_selects_by_extension() {
        assert_eq!(
            seek_subtitle_output_format("/tmp/subtitle.ass", None).0,
            "ass"
        );
        assert_eq!(
            seek_subtitle_output_format("/tmp/subtitle.ssa", None).0,
            "ass"
        );
        assert_eq!(
            seek_subtitle_output_format("/tmp/subtitle.vtt", None).0,
            "webvtt"
        );
        assert_eq!(
            seek_subtitle_output_format(
                "https://example.test/subtitle.ass?X-Plex-Token=redacted",
                None,
            )
            .0,
            "ass"
        );
        assert_eq!(
            seek_subtitle_output_format("/tmp/subtitle.subrip", None).0,
            "srt"
        );
    }

    #[test]
    fn seek_subtitle_output_format_uses_codec_for_extensionless_plex_ass() {
        let plex_stream = "https://example.test/library/streams/123?X-Plex-Token=redacted";
        let (format, extension) =
            seek_subtitle_output_format(plex_stream, Some(SubtitleTextFormat::Ass));
        assert_eq!((format, extension), ("ass", "ass"));

        let ass =
            "[Events]\nFormat: Start, End, Text\nDialogue: 0:00:42.00,0:00:46.00,Visible cue\n";
        let rebased = rebase_subtitle_cues(ass, format, 44).expect("ASS rebase should succeed");
        assert_eq!(rebased.remaining_cues, 1);
        assert!(rebased
            .text
            .contains("Dialogue: 0:00:00.00,0:00:02.00,Visible cue"));

        let args = build_seek_subtitle_args(
            plex_stream,
            Some(SubtitleTextFormat::Ass),
            44,
            Path::new("out.ass"),
        );
        assert!(args.windows(2).any(|pair| pair == ["-f", "ass"]));
        assert!(args.windows(2).any(|pair| pair == ["-i", plex_stream]));
        assert!(args.windows(2).any(|pair| pair == ["-ss", "00:00:44"]));
    }

    #[test]
    fn build_seek_subtitle_args_preserves_ffmpeg_compatibility_fallback() {
        let output = Path::new("cache/subtitle.srt");
        assert_eq!(
            build_seek_subtitle_args("https://example.test/subtitle.srt", None, 42, output),
            vec![
                "-nostdin",
                "-hide_banner",
                "-nostats",
                "-loglevel",
                "error",
                "-i",
                "https://example.test/subtitle.srt",
                "-ss",
                "00:00:42",
                "-c:s",
                "copy",
                "-f",
                "srt",
                "cache/subtitle.srt",
            ]
        );
    }

    #[tokio::test]
    async fn read_subtitle_text_source_fetches_http_subtitles_before_parsing() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET)
                    .path("/subtitle.srt")
                    .query_param("exp", "123456")
                    .query_param("sig", "synthetic-fixture");
                then.status(200)
                    .body("1\n00:00:42,000 --> 00:00:46,000\nRemote cue\n");
            })
            .await;

        let content = read_subtitle_text_source(&format!(
            "{}/subtitle.srt?exp=123456&sig=synthetic-fixture",
            server.base_url()
        ))
        .await
        .expect("remote subtitle should be fetched with its original query");
        mock.assert_async().await;
        let rebased = rebase_subtitle_cues(&content, "srt", 44).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 1);
        assert!(rebased.text.contains("00:00:00,000 --> 00:00:02,000"));
    }

    #[tokio::test]
    async fn read_subtitle_text_source_redacts_remote_url_from_http_errors() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/library/streams/123");
                then.status(401);
            })
            .await;
        let url = format!(
            "{}/library/streams/123?X-Plex-Token=redacted-sensitive-value",
            server.base_url()
        );

        let error = read_subtitle_text_source(&url)
            .await
            .expect_err("non-success status should fail");
        mock.assert_async().await;
        let rendered = error.to_string();
        assert!(rendered.contains("401"));
        assert!(!rendered.contains("X-Plex-Token"));
        assert!(!rendered.contains("redacted-sensitive-value"));
        assert!(!rendered.contains(&server.base_url()));
    }

    #[tokio::test]
    async fn read_subtitle_text_source_decodes_bom_marked_utf16_srt() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("subtitle.srt");
        let source = "1\n00:00:42,000 --> 00:00:46,000\nZażółć gęślą\n";
        let mut encoded = vec![0xFF, 0xFE];
        for unit in source.encode_utf16() {
            encoded.extend_from_slice(&unit.to_le_bytes());
        }
        tokio::fs::write(&path, encoded)
            .await
            .expect("write UTF-16 subtitle");

        let content = read_subtitle_text_source(&path.to_string_lossy())
            .await
            .expect("UTF-16 subtitle should decode");
        assert_eq!(content, source);
        let rebased = rebase_subtitle_cues(&content, "srt", 44).expect("rebase should succeed");
        assert!(rebased.text.contains("Zażółć gęślą"));
    }

    #[test]
    fn shift_cue_drops_fully_past_cues() {
        assert!(matches!(shift_cue(1_000, 2_000, 5_000), CueDecision::Drop));
    }

    #[test]
    fn shift_cue_drops_when_shifted_end_is_exactly_zero() {
        assert!(matches!(shift_cue(3_000, 5_000, 5_000), CueDecision::Drop));
    }

    #[test]
    fn shift_cue_shifts_fully_future_cues() {
        match shift_cue(10_000, 12_000, 5_000) {
            CueDecision::Keep { start, end } => {
                assert_eq!(start, 5_000);
                assert_eq!(end, 7_000);
            }
            CueDecision::Drop => panic!("expected Keep"),
        }
    }

    #[test]
    fn shift_cue_clamps_straddling_cue_start_to_zero() {
        match shift_cue(3_000, 7_000, 5_000) {
            CueDecision::Keep { start, end } => {
                assert_eq!(start, 0);
                assert_eq!(end, 2_000);
            }
            CueDecision::Drop => panic!("straddling cue must not be dropped"),
        }
    }

    #[test]
    fn shift_cue_keeps_cue_whose_shifted_start_lands_exactly_at_zero() {
        match shift_cue(5_000, 8_000, 5_000) {
            CueDecision::Keep { start, end } => {
                assert_eq!(start, 0);
                assert_eq!(end, 3_000);
            }
            CueDecision::Drop => panic!("expected Keep"),
        }
    }

    #[test]
    fn rebase_subtitle_cues_srt_clamps_straddling_cue_and_renumbers() {
        let srt = "\
1
00:00:40,000 --> 00:00:43,000
Before the boundary

2
00:00:42,000 --> 00:00:46,000
Straddles the boundary

3
00:00:50,000 --> 00:00:52,000
After the boundary
";
        let rebased = rebase_subtitle_cues(srt, "srt", 44).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 2);
        assert_eq!(
            rebased.text,
            "\
1
00:00:00,000 --> 00:00:02,000
Straddles the boundary

2
00:00:06,000 --> 00:00:08,000
After the boundary
"
        );
    }

    #[test]
    fn rebase_subtitle_cues_srt_accepts_dot_timestamp_separator() {
        let srt = "1\n00:00:42.000 --> 00:00:46.000\nDot-separated cue\n";
        let rebased = rebase_subtitle_cues(srt, "srt", 44).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 1);
        assert_eq!(
            rebased.text,
            "1\n00:00:00,000 --> 00:00:02,000\nDot-separated cue\n"
        );
    }

    #[test]
    fn rebase_subtitle_cues_srt_keeps_synthetic_cue_at_44s_chunk_boundary() {
        // Synthetic fixture for a sidecar SRT cue straddling the HLS
        // work-ahead chunk boundary.
        let srt = "\
12
00:00:41,200 --> 00:00:45,800
Boundary cue remains visible.

13
00:00:50,000 --> 00:00:53,000
Later cue remains visible.
";
        let rebased = rebase_subtitle_cues(srt, "srt", 44).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 2);
        assert!(rebased.text.contains("Boundary cue remains visible."));
        assert!(rebased.text.starts_with("1\n00:00:00,000"));
    }

    #[test]
    fn rebase_subtitle_cues_srt_without_blank_separator_uses_fallback() {
        let srt = "\
1
00:00:42,000 --> 00:00:46,000
Boundary cue
2
00:00:50,000 --> 00:00:52,000
Later cue
";
        assert!(matches!(
            rebase_subtitle_cues(srt, "srt", 44),
            Err(RebaseSubtitleError::UnparseableCue)
        ));
    }

    #[test]
    fn rebase_subtitle_cues_webvtt_preserves_identifier_and_cue_settings() {
        let vtt = "\
WEBVTT

intro
00:00:40.000 --> 00:00:43.000 align:start position:10%
Before the boundary

cue-2
00:00:42.000 --> 00:00:46.000 align:start position:10%
Straddles the boundary
";
        let rebased = rebase_subtitle_cues(vtt, "webvtt", 44).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 1);
        assert_eq!(
            rebased.text,
            "\
WEBVTT

cue-2
00:00:00.000 --> 00:00:02.000 align:start position:10%
Straddles the boundary
"
        );
    }

    #[test]
    fn rebase_subtitle_cues_webvtt_accepts_tabs_around_timing_arrow() {
        let vtt = "WEBVTT\n\ncue-1\n00:00:42.000\t-->\t00:00:46.000 align:start\nVisible cue\n";
        let rebased = rebase_subtitle_cues(vtt, "webvtt", 44).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 1);
        assert_eq!(
            rebased.text,
            "WEBVTT\n\ncue-1\n00:00:00.000 --> 00:00:02.000 align:start\nVisible cue\n"
        );
    }

    #[test]
    fn rebase_subtitle_cues_rejects_content_without_recognized_cues() {
        let vtt = "WEBVTT\n\ncue-1\nnot-a-timing-line\nVisible cue\n";
        assert!(matches!(
            rebase_subtitle_cues(vtt, "webvtt", 44),
            Err(RebaseSubtitleError::UnparseableCue)
        ));
    }

    #[test]
    fn rebase_subtitle_cues_ass_preserves_commas_and_uses_format_header_order() {
        // Format declares End before Start (non-default order) and the Text
        // field contains an override tag with embedded commas.
        let ass = "\
[Script Info]
ScriptType: v4.00+

[Events]
Format: Layer, End, Start, Style, Name, MarginL, MarginR, MarginV, Effect, Text
Dialogue: 0,0:00:46.00,0:00:42.00,Default,,0,0,0,,{\\move(10,20,30,40)}Straddles the boundary
";
        let rebased = rebase_subtitle_cues(ass, "ass", 44).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 1);
        assert!(rebased.text.contains(
            "Dialogue: 0,0:00:02.00,0:00:00.00,Default,,0,0,0,,{\\move(10,20,30,40)}Straddles the boundary"
        ));
    }

    #[test]
    fn rebase_subtitle_cues_ass_accepts_padded_events_header() {
        let ass = concat!(
            "[Script Info]\n",
            "ScriptType: v4.00+\n\n",
            "[Events ]\n",
            "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n",
            "Dialogue: 0,0:00:42.00,0:00:46.00,Default,,0,0,0,,Padded header cue\n",
        );
        let rebased = rebase_subtitle_cues(ass, "ass", 44).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 1);
        assert!(rebased
            .text
            .contains("Dialogue: 0,0:00:00.00,0:00:02.00,Default,,0,0,0,,Padded header cue"));
    }

    #[test]
    fn rebase_subtitle_cues_ass_without_recognized_dialogue_uses_fallback() {
        let ass = "[Events ]\nFormat: Start, End, Text\nComment: 0:00:42.00,0:00:46.00,Cue\n";
        assert!(matches!(
            rebase_subtitle_cues(ass, "ass", 44),
            Err(RebaseSubtitleError::UnparseableCue)
        ));
    }

    #[test]
    fn rebase_subtitle_cues_returns_zero_remaining_when_seek_lands_past_last_cue() {
        let srt = "1\n00:00:00,000 --> 00:00:02,000\nHallo\n";
        let rebased = rebase_subtitle_cues(srt, "srt", 100).expect("rebase should succeed");
        assert_eq!(rebased.remaining_cues, 0);
    }

    #[test]
    fn rebase_subtitle_cues_rejects_unparseable_timestamp() {
        let srt = "1\nnot-a-timestamp --> also-not\nHallo\n";
        assert!(matches!(
            rebase_subtitle_cues(srt, "srt", 10),
            Err(RebaseSubtitleError::UnparseableCue)
        ));
    }

    #[tokio::test]
    async fn rebase_text_subtitle_falls_back_to_original_on_unparseable_cue() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("broken.srt");
        tokio::fs::write(&path, b"1\nnot-a-timestamp --> also-not\nHallo\n")
            .await
            .expect("write broken srt");
        let path_str = path.to_string_lossy().into_owned();
        assert_eq!(rebase_text_subtitle(&path_str, 10).await, Some(path_str));
    }

    #[tokio::test]
    async fn rebase_text_subtitle_skips_non_positive_seek() {
        assert_eq!(
            rebase_text_subtitle("/srv/media/show/subtitle.srt", 0).await,
            Some("/srv/media/show/subtitle.srt".to_owned())
        );
        assert_eq!(
            rebase_text_subtitle("/srv/media/show/subtitle.srt", -1).await,
            Some("/srv/media/show/subtitle.srt".to_owned())
        );
    }

    #[tokio::test]
    async fn non_empty_subtitle_path_skips_empty_seek_output() {
        let dir = tempfile::tempdir().expect("temp dir");
        let empty = dir.path().join("empty.srt");
        tokio::fs::write(&empty, b"").await.expect("write empty");
        assert_eq!(non_empty_subtitle_path(&empty).await, None);

        let cues = dir.path().join("cues.srt");
        tokio::fs::write(&cues, b"1\n00:00:00,000 --> 00:00:02,000\nHallo\n")
            .await
            .expect("write cues");
        assert_eq!(
            non_empty_subtitle_path(&cues).await,
            Some(cues.to_string_lossy().into_owned())
        );
    }

    #[tokio::test]
    async fn rebase_burned_text_subtitle_input_skips_non_text_burn_inputs() {
        let image = InputFile::image_subtitle("/srv/media/show/video.mkv", SubtitleMethod::Burn);
        assert_eq!(
            rebase_burned_text_subtitle_input(Some(image.clone()), 900).await,
            Some(image)
        );

        let copied = InputFile::text_subtitle("/srv/media/show/video.mkv", SubtitleMethod::Copy);
        assert_eq!(
            rebase_burned_text_subtitle_input(Some(copied.clone()), 900).await,
            Some(copied)
        );

        let burned = InputFile::text_subtitle("/srv/media/show/subtitle.srt", SubtitleMethod::Burn);
        assert_eq!(
            rebase_burned_text_subtitle_input(Some(burned.clone()), 0).await,
            Some(burned)
        );
    }

    #[test]
    fn build_font_args_matches_etv_dump_attachment() {
        let args = build_font_args("/srv/media/show/s01e01.mkv", 0);
        assert_eq!(
            args,
            vec![
                "-nostdin".to_owned(),
                "-hide_banner".to_owned(),
                "-dump_attachment:t:0".to_owned(),
                String::new(),
                "-i".to_owned(),
                "/srv/media/show/s01e01.mkv".to_owned(),
                "-y".to_owned(),
            ]
        );
    }

    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn playout_selection_matches_etv_window_and_media_type_filter() {
        let pool = test_db().await;
        pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
               VALUES (1, 'local', 1, 1)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, '/media')"#,
        )
        .await
        .unwrap();
        for id in [10_i64, 20, 30] {
            sqlx::query(
                r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#,
            )
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool.execute(r#"INSERT INTO "Movie" ("Id") VALUES (10), (30)"#)
            .await
            .unwrap();
        pool.execute(r#"INSERT INTO "OtherVideo" ("Id") VALUES (20)"#)
            .await
            .unwrap();
        pool.execute(
            r#"INSERT INTO "MovieMetadata" ("Id", "MovieId", "DateAdded", "DateUpdated", "MetadataKind")
               VALUES (100, 10, '2026-05-31 00:00:00', '2026-05-31 00:00:00', 1),
                      (300, 30, '2026-05-31 00:00:00', '2026-05-31 00:00:00', 1)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "OtherVideoMetadata" ("Id", "OtherVideoId", "DateAdded", "DateUpdated", "MetadataKind")
               VALUES (200, 20, '2026-05-31 00:00:00', '2026-05-31 00:00:00', 1)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Subtitle" (
                   "MovieMetadataId", "SubtitleKind", "StreamIndex", "Codec",
                   "Default", "Forced", "SDH", "IsExtracted", "DateAdded", "DateUpdated"
               ) VALUES
                   (100, 0, 2, 'subrip', 0, 0, 0, 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00'),
                   (300, 0, 2, 'subrip', 0, 0, 0, 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Subtitle" (
                   "OtherVideoMetadataId", "SubtitleKind", "StreamIndex", "Codec",
                   "Default", "Forced", "SDH", "IsExtracted", "DateAdded", "DateUpdated"
               ) VALUES (200, 0, 2, 'pgssub', 0, 0, 0, 0, '2026-05-31 00:00:00', '2026-05-31 00:00:00')"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "FFmpegProfile" (
                   "Id", "AudioBitrate", "AudioBufferSize", "AudioChannels",
                   "AudioFormat", "AudioSampleRate", "BitDepth", "HardwareAcceleration",
                   "Name", "NormalizeLoudnessMode", "ResolutionId", "ScalingBehavior",
                   "ThreadCount", "VaapiDriver", "VideoBitrate", "VideoBufferSize",
                   "VideoFormat"
               ) VALUES (1, 192, 0, 2, 0, 48000, 8, 0, 'default', 0, 1, 0, 0, 0, 4000, 0, 0)"#,
        )
        .await
        .unwrap();
        pool.execute(
            r#"INSERT INTO "Channel" (
                   "Id", "UniqueId", "Number", "SortNumber", "Name", "FFmpegProfileId",
                   "StreamingMode", "StreamSelectorMode", "SubtitleMode",
                   "MusicVideoCreditsMode", "SongVideoMode", "PlayoutSource",
                   "PlayoutMode", "TranscodeMode", "IdleBehavior"
               ) VALUES (1, '00000000-0000-0000-0000-000000000001', '1', 1.0, 'Channel 1', 1,
                         0, 0, 2, 0, 0, 0, 0, 0, 0)"#,
        )
        .await
        .unwrap();
        pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, 0, 0)"#)
            .await
            .unwrap();

        let now = Utc::now();
        let inside_start = encode_datetime(now);
        let inside_finish = encode_datetime(now + Duration::minutes(30));
        let outside_start = encode_datetime(now + Duration::hours(2));
        let outside_finish = encode_datetime(now + Duration::hours(3));
        for (id, media_item_id, start, finish) in [
            (1_i64, 10_i64, inside_start.as_str(), inside_finish.as_str()),
            (2, 20, inside_start.as_str(), inside_finish.as_str()),
            (3, 30, outside_start.as_str(), outside_finish.as_str()),
        ] {
            sqlx::query(
                r#"INSERT INTO "PlayoutItem" (
                       "Id", "PlayoutId", "MediaItemId", "Start", "Finish",
                       "InPoint", "OutPoint", "DisableWatermarks", "FillerKind", "GuideGroup"
                   ) VALUES (?, 1, ?, ?, ?, '00:00:00', '00:30:00', 0, 0, 0)"#,
            )
            .bind(id)
            .bind(media_item_id)
            .bind(start)
            .bind(finish)
            .execute(&pool)
            .await
            .unwrap();
        }

        assert_eq!(
            playout_media_item_ids_with_text_subtitles(&pool, 1)
                .await
                .unwrap(),
            vec![10]
        );

        pool.execute(r#"UPDATE "Channel" SET "SubtitleMode" = 0 WHERE "Id" = 1"#)
            .await
            .unwrap();
        assert!(playout_media_item_ids_with_text_subtitles(&pool, 1)
            .await
            .unwrap()
            .is_empty());

        pool.execute(r#"UPDATE "Channel" SET "StreamSelectorMode" = 1 WHERE "Id" = 1"#)
            .await
            .unwrap();
        assert_eq!(
            playout_media_item_ids_with_text_subtitles(&pool, 1)
                .await
                .unwrap(),
            vec![10],
            "custom selectors can choose embedded text despite channel subtitles being off"
        );

        pool.execute(r#"UPDATE "Channel" SET "StreamSelectorMode" = 2 WHERE "Id" = 1"#)
            .await
            .unwrap();
        assert!(playout_media_item_ids_with_text_subtitles(&pool, 1)
            .await
            .unwrap()
            .is_empty());
    }
}

#[cfg(test)]
#[path = "subtitles_extraction_tests.rs"]
mod extraction_tests;

#[cfg(test)]
#[path = "subtitles_rebase_millisecond_tests.rs"]
mod rebase_millisecond_tests;

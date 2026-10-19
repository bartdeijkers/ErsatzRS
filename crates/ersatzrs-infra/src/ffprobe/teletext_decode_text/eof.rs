//! A missing final clear is closed by actual selected-video presentation, not format duration.

use super::{
    cues, expected_with_end, invalid, normalized, output, Deserialize, FfprobeCommand, Frame,
    Instant, Path, PreparedLocalTeletextSource, SourceTeletextPageDecodeError,
};
use std::fmt::Write as _;

#[derive(Deserialize)]
struct VideoOutput {
    streams: Vec<VideoStream>,
    programs: Vec<VideoProgram>,
    frames: Vec<VideoFrame>,
}
#[derive(Deserialize, PartialEq, Eq)]
struct VideoStream {
    index: u32,
    codec_type: String,
    id: String,
    time_base: String,
}
#[derive(Deserialize)]
struct VideoProgram {
    program_num: u16,
    streams: Vec<VideoStream>,
}
#[derive(Deserialize)]
struct VideoFrame {
    media_type: String,
    stream_index: u32,
    pts: Option<i64>,
    duration: Option<i64>,
    #[serde(default)]
    decode_error_flags: u32,
}

pub(crate) fn video_command(
    ffprobe: &str,
    index: u32,
    path: &Path,
) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
    Ok(FfprobeCommand {
        program: ffprobe.into(),
        args: [
            "-v", "error", "-select_streams", &index.to_string(), "-show_frames",
            "-show_streams", "-show_programs", "-show_entries",
            "stream=index,codec_type,id,time_base:program=program_num:program_stream=index,codec_type,id,time_base:frame=media_type,stream_index,pts,duration,decode_error_flags",
            "-of", "json", "-f", "mpegts", path.to_str().ok_or_else(invalid)?,
        ].into_iter().map(str::to_owned).collect(),
    })
}

pub(crate) fn video_end(
    source: &PreparedLocalTeletextSource,
    index: u32,
    bytes: &[u8],
    deadline: Instant,
) -> Result<i64, SourceTeletextPageDecodeError> {
    use ersatzrs_ffmpeg::teletext::source_av_timestamps::SourceAvStreamKind;
    let native = source
        .native_origin()
        .streams()
        .iter()
        .find(|stream| {
            stream.identity().stream_index == i32::try_from(index).unwrap_or(-1)
                && stream.identity().kind == SourceAvStreamKind::Video
        })
        .ok_or_else(invalid)?;
    let origin = source
        .native_origin()
        .format_start()
        .and_then(
            crate::ffprobe::teletext_origin::NativeSourceTime::ffprobe_n9_format_microsecond_bounds,
        )
        .ok_or_else(invalid)?;
    let output: VideoOutput = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    end(
        &output,
        index,
        native.identity().pid,
        source.request.program_number,
        origin,
        deadline,
    )
}

fn end(
    output: &VideoOutput,
    index: u32,
    pid: u16,
    program_number: u16,
    origin: (i64, i64),
    deadline: Instant,
) -> Result<i64, SourceTeletextPageDecodeError> {
    if output.streams.len() != 1 || output.programs.len() > 128 || output.frames.is_empty() {
        return Err(invalid());
    }
    let stream = &output.streams[0];
    let mut programs = output
        .programs
        .iter()
        .filter(|p| p.program_num == program_number);
    let program = programs.next().ok_or_else(invalid)?;
    if programs.next().is_some()
        || program.streams.len() != 1
        || program.streams[0] != *stream
        || stream.index != index
        || stream.codec_type != "video"
        || stream
            .id
            .strip_prefix("0x")
            .and_then(|s| u16::from_str_radix(s, 16).ok())
            != Some(pid)
    {
        return Err(SourceTeletextPageDecodeError::StreamChanged);
    }
    let base = crate::ffprobe::teletext_origin::NativeSourceTimeBase::parse(&stream.time_base)
        .map_err(|_| invalid())?;
    let mut previous = None;
    let mut last_end = None;
    for frame in &output.frames {
        super::super::super::check_deadline(deadline)?;
        let pts = frame.pts.ok_or_else(invalid)?;
        let duration = frame
            .duration
            .filter(|duration| *duration > 0)
            .ok_or_else(invalid)?;
        if frame.media_type != "video"
            || frame.stream_index != index
            || frame.decode_error_flags != 0
            || previous.is_some_and(|last| pts <= last)
        {
            return Err(invalid());
        }
        previous = Some(pts);
        let end = pts.checked_add(duration).ok_or_else(invalid)?;
        last_end = Some(last_end.map_or(end, |last: i64| last.max(end)));
    }
    let ticks = i128::from(last_end.ok_or_else(invalid)?);
    let numerator = ticks * i128::from(base.numerator()) * 1_000_000;
    let denominator = i128::from(base.denominator());
    let round = |origin| {
        let value = numerator - i128::from(origin) * denominator;
        if value < 0 {
            return Err(invalid());
        }
        i64::try_from((value + 500 * denominator) / (1000 * denominator)).map_err(|_| invalid())
    };
    let first = round(origin.0)?;
    (first == round(origin.1)? && first > 0)
        .then_some(first)
        .ok_or_else(invalid)
}

struct Block<'a> {
    timing: (i64, i64),
    body: &'a str,
}

fn blocks(content: &str) -> Result<Vec<Block<'_>>, SourceTeletextPageDecodeError> {
    let times = cues(content)?;
    content
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .zip(times)
        .map(|(block, timing)| {
            let (_, rest) = block.split_once('\n').ok_or_else(invalid)?;
            let (_, body) = rest.split_once('\n').ok_or_else(invalid)?;
            // A normalized final block ends in one newline, while an interior
            // block's separator owns both. Neither newline is subtitle text.
            Ok(Block {
                timing,
                body: body.trim_end_matches('\n'),
            })
        })
        .collect()
}

/// Keep all fixed-duration bytes; recover only the missing final native text.
pub(crate) fn recover(
    source: &PreparedLocalTeletextSource,
    events: &[u8],
    fixed: &str,
    raw: &str,
    video_end: i64,
    deadline: Instant,
) -> Result<String, SourceTeletextPageDecodeError> {
    let (output, origin) = output(source, events)?;
    recover_events(&output.frames, origin, fixed, raw, video_end, deadline)
}

fn recover_events(
    frames: &[Frame],
    origin: (i64, i64),
    fixed: &str,
    raw: &str,
    video_end: i64,
    deadline: Instant,
) -> Result<String, SourceTeletextPageDecodeError> {
    if !frames
        .last()
        .is_some_and(|f| f.num_rects != 0 && f.end_display_time == u32::MAX)
    {
        return Err(invalid());
    }
    let expected = expected_with_end(frames, origin, deadline, Some(video_end))?;
    let native = blocks(raw)?;
    let fixed_blocks = blocks(fixed)?;
    if native.len() != expected.len() || fixed_blocks.len() + 1 != expected.len() {
        return Err(invalid());
    }
    for (block, frame) in native
        .iter()
        .zip(frames.iter().filter(|f| f.num_rects != 0))
    {
        let base = normalized(frame.pts.ok_or_else(invalid)?, origin)?;
        if block.timing
            != (
                base.checked_add(i64::from(frame.start_display_time))
                    .ok_or_else(invalid)?,
                base.checked_add(i64::from(frame.end_display_time))
                    .ok_or_else(invalid)?,
            )
        {
            return Err(invalid());
        }
    }
    for ((fixed, native), expected) in fixed_blocks.iter().zip(&native).zip(&expected) {
        if fixed.timing != *expected || fixed.body != native.body {
            return Err(invalid());
        }
    }
    let last = native.last().ok_or_else(invalid)?;
    let (start, end) = *expected.last().ok_or_else(invalid)?;
    let stamp = |ms: i64| {
        format!(
            "{:02}:{:02}:{:02},{:03}",
            ms / 3_600_000,
            ms / 60_000 % 60,
            ms / 1000 % 60,
            ms % 1000
        )
    };
    let mut content = fixed.trim_end_matches('\n').to_owned();
    if !content.is_empty() {
        content.push_str("\n\n");
    }
    write!(
        content,
        "{}\n{} --> {}\n{}\n\n",
        expected.len(),
        stamp(start),
        stamp(end),
        last.body.trim_end_matches('\n')
    )
    .map_err(|_| invalid())?;
    if content.len() > super::super::MAX_TEXT_BYTES || cues(&content)? != expected {
        return Err(invalid());
    }
    Ok(content)
}

#[cfg(test)]
#[path = "eof_tests.rs"]
mod tests;

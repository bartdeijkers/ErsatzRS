//! Exact finite-video cutoff observations, without changing playback policy.
//!
//! Only a complete-frame prefix followed by an excluded frame and a later
//! outpoint bookend qualifies. A short packet read is not a clean-EOF proof:
//! n9.0 `read_interval_packets` does not propagate every `av_read_frame` error.

use std::path::Path;

use serde::Deserialize;
use tokio::time::Instant;

use super::{FfprobeCommand, PreparedLocalTeletextSource, SourceTeletextPageDecodeError};
use crate::ffprobe::teletext_origin::{NativeSourceOrigin, NativeSourceTime, NativeSourceTimeBase};

/// Constructed only from actual selected decoded frames in the held source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FiniteVideoCut {
    out_ms: u64,
    cut_pts_90k: i64,
}

impl FiniteVideoCut {
    pub(super) const fn out_ms(self) -> u64 {
        self.out_ms
    }

    /// First excluded frame start, in the source-normalized 1/90000 clock.
    pub(super) const fn cut_pts_90k(self) -> i64 {
        self.cut_pts_90k
    }
}

#[derive(Deserialize)]
struct Output {
    streams: Vec<Stream>,
    programs: Vec<Program>,
    frames: Vec<Frame>,
}

#[derive(Deserialize, PartialEq, Eq)]
struct Stream {
    index: u32,
    codec_type: String,
    id: String,
    time_base: String,
    nb_read_packets: String,
}

#[derive(Deserialize)]
struct Program {
    program_num: u16,
    streams: Vec<Stream>,
}

#[derive(Deserialize)]
struct Frame {
    media_type: String,
    stream_index: u32,
    pts: Option<i64>,
    duration: Option<i64>,
    #[serde(default)]
    decode_error_flags: u32,
}

pub(super) fn command(
    ffprobe: &str,
    index: u32,
    path: &Path,
) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
    // Start from the actual source beginning: MPEG-TS timestamp seeks can land
    // before reference pictures and silently conceal frames. A short read or
    // successful decoder exit is not permission to qualify that uncertainty.
    let interval = format!("%+#{}", super::super::MAX_FRAMES);
    Ok(FfprobeCommand {
        program: ffprobe.into(),
        args: [
            "-v", "error", "-select_streams", &index.to_string(),
            "-read_intervals", &interval, "-count_packets", "-show_frames",
            "-show_streams", "-show_programs", "-show_entries",
            "stream=index,codec_type,id,time_base,nb_read_packets:program=program_num:program_stream=index,codec_type,id,time_base,nb_read_packets:frame=media_type,stream_index,pts,duration,decode_error_flags",
            "-of", "json", "-f", "mpegts", path.to_str().ok_or_else(invalid)?,
        ].into_iter().map(str::to_owned).collect(),
    })
}

/// Qualify only observations from the caller's retained, checked full snapshot.
/// `None` means insufficient bookends, never an inferred clean EOF or permission
/// to change output duration. The caller still owns process/source revalidation.
pub(super) fn qualify(
    source: &PreparedLocalTeletextSource,
    index: u32,
    out_ms: u64,
    bytes: &[u8],
    deadline: Instant,
) -> Result<Option<FiniteVideoCut>, SourceTeletextPageDecodeError> {
    super::check_deadline(deadline)?;
    if source.native_origin().format_name() != Some("mpegts") {
        return Err(invalid());
    }
    let pid = native_video_pid(source.native_origin(), index)?;
    let origin = source
        .native_origin()
        .format_start()
        .and_then(NativeSourceTime::ffprobe_n9_format_microsecond_bounds)
        .ok_or_else(invalid)?;
    if bytes.len() > crate::ffprobe::MAX_REMOTE_FFPROBE_OUTPUT_BYTES {
        return Err(invalid());
    }
    let output = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    qualify_output(
        &output,
        (index, pid, source.request.program_number),
        out_ms,
        origin,
        deadline,
    )
}

fn native_video_pid(
    origin: &NativeSourceOrigin,
    index: u32,
) -> Result<u16, SourceTeletextPageDecodeError> {
    use ersatzrs_ffmpeg::teletext::source_av_timestamps::SourceAvStreamKind;
    let mut selected = origin.streams().iter().filter(|stream| {
        stream.identity().stream_index == i32::try_from(index).unwrap_or(-1)
            && stream.identity().kind == SourceAvStreamKind::Video
    });
    let native = selected.next().ok_or_else(invalid)?;
    if selected.next().is_some()
        || !native
            .time_base()
            .is_some_and(|base| base.numerator() == 1 && base.denominator() == 90_000)
    {
        return Err(invalid());
    }
    Ok(native.identity().pid)
}

fn validate_stream(
    output: &Output,
    index: u32,
    pid: u16,
    program_number: u16,
) -> Result<(), SourceTeletextPageDecodeError> {
    if output.streams.len() != 1
        || output.programs.len() > 128
        || output.frames.len() > super::super::MAX_FRAMES
    {
        return Err(invalid());
    }
    let stream = &output.streams[0];
    let mut programs = output
        .programs
        .iter()
        .filter(|p| p.program_num == program_number);
    let program = programs
        .next()
        .ok_or(SourceTeletextPageDecodeError::StreamChanged)?;
    if programs.next().is_some()
        || program.streams.len() != 1
        || program.streams[0] != *stream
        || stream.index != index
        || stream.codec_type != "video"
        || stream
            .id
            .strip_prefix("0x")
            .and_then(|id| u16::from_str_radix(id, 16).ok())
            != Some(pid)
    {
        return Err(SourceTeletextPageDecodeError::StreamChanged);
    }
    let base = NativeSourceTimeBase::parse(&stream.time_base).map_err(|_| invalid())?;
    let count = stream
        .nb_read_packets
        .parse::<usize>()
        .map_err(|_| invalid())?;
    if base.numerator() != 1
        || base.denominator() != 90_000
        || count == 0
        || count > super::super::MAX_FRAMES
    {
        return Err(invalid());
    }
    Ok(())
}

fn qualify_output(
    output: &Output,
    identity: (u32, u16, u16),
    out_ms: u64,
    origin: (i64, i64),
    deadline: Instant,
) -> Result<Option<FiniteVideoCut>, SourceTeletextPageDecodeError> {
    super::check_deadline(deadline)?;
    let (index, pid, program_number) = identity;
    validate_stream(output, index, pid, program_number)?;
    let limit_microseconds = out_microseconds(out_ms)?;
    let limit = |origin| {
        i128::from(origin)
            .checked_add(limit_microseconds)
            .and_then(|value| value.checked_mul(90_000))
            .ok_or_else(invalid)
    };
    let limits = (limit(origin.0)?, limit(origin.1)?);
    if origin.0 > origin.1 {
        return Err(invalid());
    }
    let mut previous = None;
    let mut complete = false;
    let mut excluded = false;
    let mut cut = None;
    let mut bookend = false;
    for frame in &output.frames {
        super::check_deadline(deadline)?;
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
        let end = i128::from(end).checked_mul(1_000_000).ok_or_else(invalid)?;
        if end <= limits.0 {
            if excluded {
                return Err(invalid());
            }
            complete = true;
        } else if end > limits.1 {
            excluded = true;
            if complete {
                cut.get_or_insert(pts);
            }
        } else {
            return Err(invalid());
        }
        // A frame flushed at a truncated packet boundary cannot alone certify
        // the first exclusion: require an actual later decoded start past out.
        if cut.is_some_and(|cut| pts > cut) && i128::from(pts) * 1_000_000 >= limits.1 {
            bookend = true;
        }
    }
    let Some(cut) = cut.filter(|_| bookend) else {
        return Ok(None);
    };
    let first = normalize_cut(cut, origin.0)?;
    if first != normalize_cut(cut, origin.1)? || first < 0 {
        return Err(invalid());
    }
    Ok(Some(FiniteVideoCut {
        out_ms,
        cut_pts_90k: first,
    }))
}

fn out_microseconds(out_ms: u64) -> Result<i128, SourceTeletextPageDecodeError> {
    if out_ms == 0 {
        return Err(invalid());
    }
    i64::try_from(out_ms)
        .ok()
        .and_then(|value| value.checked_mul(1000))
        .map(i128::from)
        .ok_or_else(invalid)
}

fn normalize_cut(pts: i64, origin: i64) -> Result<i64, SourceTeletextPageDecodeError> {
    let value = i128::from(origin) * 9;
    let ticks = if value < 0 {
        -((-value + 50) / 100)
    } else {
        (value + 50) / 100
    };
    i64::try_from(i128::from(pts) - ticks).map_err(|_| invalid())
}

fn invalid() -> SourceTeletextPageDecodeError {
    SourceTeletextPageDecodeError::InvalidOutput
}

#[cfg(test)]
#[path = "finite_video_cut_tests.rs"]
mod tests;

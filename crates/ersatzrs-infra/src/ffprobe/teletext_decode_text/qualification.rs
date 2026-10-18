//! Ordered decoder events qualify every complete generated SRT cue, including EOF.

use serde::Deserialize;
use std::path::Path;
use tokio::time::Instant;

use super::{FfprobeCommand, PreparedLocalTeletextSource, SourceTeletextPageDecodeError};

#[derive(Deserialize)]
struct Output {
    streams: Vec<super::super::Stream>,
    programs: Vec<super::super::Program>,
    frames: Vec<Frame>,
}
#[derive(Deserialize)]
struct Frame {
    media_type: String,
    pts: Option<i64>,
    format: u8,
    num_rects: u32,
    start_display_time: u32,
    end_display_time: u32,
}

fn invalid() -> SourceTeletextPageDecodeError {
    SourceTeletextPageDecodeError::InvalidOutput
}

pub(super) fn check(
    source: &PreparedLocalTeletextSource,
    bytes: &[u8],
    content: &str,
    deadline: Instant,
) -> Result<(), SourceTeletextPageDecodeError> {
    let output: Output = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if output.frames.is_empty()
        || output.frames.len() > super::super::MAX_FRAMES
        || output.streams.len() != 1
        || output.programs.len() > 128
    {
        return Err(invalid());
    }
    let matches = |stream: &super::super::Stream| {
        stream.index == source.request.stream_index
            && stream.codec_name == "dvb_teletext"
            && stream.codec_type == "subtitle"
            && stream
                .id
                .strip_prefix("0x")
                .and_then(|value| u16::from_str_radix(value, 16).ok())
                == Some(source.request.pid)
    };
    let mut selected = output
        .programs
        .iter()
        .filter(|program| program.program_num == source.request.program_number);
    let program = selected.next().ok_or_else(invalid)?;
    if selected.next().is_some()
        || program.streams.len() != 1
        || !matches(&program.streams[0])
        || !matches(&output.streams[0])
    {
        return Err(SourceTeletextPageDecodeError::StreamChanged);
    }
    let origin = source
        .native_origin()
        .format_start()
        .and_then(
            crate::ffprobe::teletext_origin::NativeSourceTime::ffprobe_n9_format_microsecond_bounds,
        )
        .ok_or_else(invalid)?;
    if cues(content)? != expected(&output.frames, origin, deadline)? {
        return Err(invalid());
    }
    Ok(())
}

fn expected(
    frames: &[Frame],
    origin: (i64, i64),
    deadline: Instant,
) -> Result<Vec<(i64, i64)>, SourceTeletextPageDecodeError> {
    if frames.is_empty() || frames.len() > super::super::MAX_FRAMES {
        return Err(invalid());
    }
    let mut expected = Vec::new();
    let mut previous = None;
    for (index, frame) in frames.iter().enumerate() {
        super::super::check_deadline(deadline)?;
        let pts = frame.pts.ok_or_else(invalid)?;
        if frame.media_type != "subtitle"
            || frame.format != 1
            || frame.num_rects > 4096
            || previous.is_some_and(|previous| pts < previous)
        {
            return Err(invalid());
        }
        previous = Some(pts);
        if frame.num_rects == 0 {
            continue;
        }
        let next = frames
            .get(index + 1)
            .map(|next| next.pts.ok_or_else(invalid))
            .transpose()?
            .map(|next| next.checked_sub(pts).ok_or_else(invalid))
            .transpose()?;
        let duration = match next {
            Some(delta) => u32::try_from(rounded_ms(delta)?)
                .map_err(|_| invalid())?
                .min(frame.end_display_time),
            None if frame.end_display_time != u32::MAX => frame.end_display_time,
            None => return Err(invalid()), // FFmpeg fix_sub_duration cannot certify a queued infinite last cue.
        };
        if duration <= frame.start_display_time {
            return Err(invalid());
        }
        let base = normalized(pts, origin)?;
        expected.push((
            base.checked_add(i64::from(frame.start_display_time))
                .ok_or_else(invalid)?,
            base.checked_add(i64::from(duration)).ok_or_else(invalid)?,
        ));
    }
    if expected.is_empty() {
        return Err(invalid());
    }
    Ok(expected)
}

fn rounded_ms(value: i64) -> Result<i64, SourceTeletextPageDecodeError> {
    if value < 0 {
        return Err(invalid());
    }
    value
        .checked_add(500)
        .map(|value| value / 1000)
        .ok_or_else(invalid)
}

fn normalized(pts: i64, origin: (i64, i64)) -> Result<i64, SourceTeletextPageDecodeError> {
    let signed = |value: i64| {
        if value >= 0 {
            rounded_ms(value)
        } else {
            rounded_ms(value.checked_neg().ok_or_else(invalid)?)?
                .checked_neg()
                .ok_or_else(invalid)
        }
    };
    let first = signed(pts.checked_sub(origin.0).ok_or_else(invalid)?)?;
    let last = signed(pts.checked_sub(origin.1).ok_or_else(invalid)?)?;
    (first == last && first >= 0)
        .then_some(first)
        .ok_or_else(invalid)
}

fn timestamp(text: &str) -> Result<i64, SourceTeletextPageDecodeError> {
    let fields = text.split([':', ',']).collect::<Vec<_>>();
    if fields.len() != 4
        || fields[0].len() < 2
        || fields[1].len() != 2
        || fields[2].len() != 2
        || fields[3].len() != 3
    {
        return Err(invalid());
    }
    let values = fields
        .iter()
        .map(|value| value.parse::<i64>().map_err(|_| invalid()))
        .collect::<Result<Vec<_>, _>>()?;
    if values.iter().any(|value| *value < 0)
        || values[1] >= 60
        || values[2] >= 60
        || values[3] >= 1000
    {
        return Err(invalid());
    }
    values[0]
        .checked_mul(3_600_000)
        .and_then(|value| value.checked_add(values[1] * 60_000 + values[2] * 1000 + values[3]))
        .ok_or_else(invalid)
}

fn cues(content: &str) -> Result<Vec<(i64, i64)>, SourceTeletextPageDecodeError> {
    content
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .map(|block| {
            let mut lines = block.lines();
            let index = lines.next().ok_or_else(invalid)?;
            if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid());
            }
            let (start, end) = lines
                .next()
                .and_then(|line| line.split_once(" --> "))
                .ok_or_else(invalid)?;
            if lines.next().is_none() {
                return Err(invalid());
            }
            Ok((timestamp(start)?, timestamp(end)?))
        })
        .collect()
}

pub(super) fn command(
    ffprobe: &str,
    request: &super::super::SourceTeletextPageDecodeRequest,
    path: &Path,
) -> Result<FfprobeCommand, SourceTeletextPageDecodeError> {
    let index = request.stream_index;
    Ok(FfprobeCommand {program:ffprobe.to_owned(),args:vec!["-v".into(),"error".into(),
        format!("-txt_page:{index}"),request.page.get().to_string(),format!("-txt_format:{index}"),"text".into(),
        "-select_streams".into(),index.to_string(),"-show_frames".into(),"-show_streams".into(),"-show_programs".into(),
        "-show_entries".into(),"stream=index,codec_name,codec_type,id:program=program_num:program_stream=index,codec_name,codec_type,id:frame=media_type,pts,format,num_rects,start_display_time,end_display_time".into(),
        "-of".into(),"json".into(),"-f".into(),"mpegts".into(),path.to_str().ok_or_else(invalid)?.to_owned()]})
}

#[cfg(test)]
#[path = "qualification_tests.rs"]
mod tests;

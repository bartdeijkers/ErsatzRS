//! Exact finite packet/frame coordinates, distinct from playback admission.
use serde::Deserialize;

use super::super::{SourceTeletextBitmapEvents, SourceTeletextPageDecodeError};

const MAX_GROUPS: usize = super::super::MAX_FRAMES;
const MAX_RECORDS: usize = MAX_GROUPS * 4;

/// All subtitle events sharing one indivisible artifact PTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextBitmapArtifactGroup {
    pts_microseconds: i64,
    nonempty_frames: usize,
    clear_frames: usize,
    all_packets_key: bool,
}
impl SourceTeletextBitmapArtifactGroup {
    #[must_use]
    pub const fn pts_microseconds(self) -> i64 {
        self.pts_microseconds
    }
    #[must_use]
    pub const fn nonempty_frames(self) -> usize {
        self.nonempty_frames
    }
    #[must_use]
    pub const fn clear_frames(self) -> usize {
        self.clear_frames
    }
    /// Observed packet flags only; this does not certify complete decoder context.
    #[must_use]
    pub const fn all_packets_key(self) -> bool {
        self.all_packets_key
    }
}
/// Bounded observed Matroska coordinates; these do not initialize a seek.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTeletextBitmapArtifactObservation {
    groups: Vec<SourceTeletextBitmapArtifactGroup>,
    nonempty_frames: usize,
    clear_frames: usize,
    last_packet_end_microseconds: i64,
}
impl SourceTeletextBitmapArtifactObservation {
    #[must_use]
    pub fn groups(&self) -> &[SourceTeletextBitmapArtifactGroup] {
        &self.groups
    }
    #[must_use]
    pub const fn nonempty_frames(&self) -> usize {
        self.nonempty_frames
    }
    #[must_use]
    pub const fn clear_frames(&self) -> usize {
        self.clear_frames
    }
    /// Finite packet-duration endpoint, not the last visible cue's clear time.
    #[must_use]
    pub const fn last_packet_end_microseconds(&self) -> i64 {
        self.last_packet_end_microseconds
    }
}
#[derive(Deserialize)]
struct Output {
    streams: Vec<Stream>,
    format: Format,
    packets_and_frames: Vec<Record>,
}
#[derive(Deserialize)]
struct Stream {
    index: usize,
    codec_name: String,
    codec_type: String,
    time_base: String,
}
#[derive(Deserialize)]
struct Format {
    format_name: String,
    nb_streams: usize,
}
#[derive(Deserialize)]
#[serde(tag = "type")]
enum Record {
    #[serde(rename = "packet")]
    Packet {
        codec_type: String,
        stream_index: usize,
        pts: i64,
        dts: i64,
        duration: i64,
        flags: String,
    },
    #[serde(rename = "subtitle")]
    Frame {
        media_type: String,
        pts: i64,
        format: u8,
        start_display_time: u32,
        end_display_time: u32,
        num_rects: u32,
    },
}
fn invalid() -> SourceTeletextPageDecodeError {
    SourceTeletextPageDecodeError::InvalidOutput
}
fn ticks(value: i64) -> Result<i64, SourceTeletextPageDecodeError> {
    value
        .checked_mul(1000)
        .filter(|value| *value >= 0)
        .ok_or_else(invalid)
}
fn expected_endpoint(
    value: Option<i64>,
    origin: i64,
) -> Result<i64, SourceTeletextPageDecodeError> {
    let value = value
        .and_then(|value| value.checked_sub(origin))
        .ok_or_else(invalid)?;
    // No guessed rounding window: nonintegral millisecond mapping remains open.
    if value < 0 || value % 1000 != 0 {
        return Err(invalid());
    }
    Ok(value)
}
pub(super) fn parse(
    bytes: &[u8],
    source: SourceTeletextBitmapEvents,
    origin: i64,
) -> Result<SourceTeletextBitmapArtifactObservation, SourceTeletextPageDecodeError> {
    if bytes.len() > crate::ffprobe::MAX_REMOTE_FFPROBE_OUTPUT_BYTES {
        return Err(invalid());
    }
    let output: Output = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if output.streams.len() != 1
        || output.format.nb_streams != 1
        || output.format.format_name != "matroska,webm"
        || output.streams[0].index != 0
        || output.streams[0].codec_name != "dvb_subtitle"
        || output.streams[0].codec_type != "subtitle"
        || output.streams[0].time_base != "1/1000"
        || output.packets_and_frames.is_empty()
        || output.packets_and_frames.len() > MAX_RECORDS
        || !output.packets_and_frames.len().is_multiple_of(2)
    {
        return Err(invalid());
    }
    let mut observation = SourceTeletextBitmapArtifactObservation {
        groups: Vec::new(),
        nonempty_frames: 0,
        clear_frames: 0,
        last_packet_end_microseconds: 0,
    };
    let mut first_nonempty = None;
    let mut last_nonempty = None;
    let mut records = output.packets_and_frames.into_iter();
    while let Some(packet) = records.next() {
        let frame = records.next().ok_or_else(invalid)?;
        let (pts, end, nonempty) = pair(packet, frame)?;
        if observation
            .groups
            .last()
            .is_some_and(|last| pts < last.pts_microseconds)
        {
            return Err(invalid());
        }
        if observation
            .groups
            .last()
            .is_none_or(|last| pts != last.pts_microseconds)
        {
            if observation.groups.len() >= MAX_GROUPS {
                return Err(invalid());
            }
            observation.groups.push(SourceTeletextBitmapArtifactGroup {
                pts_microseconds: pts,
                nonempty_frames: 0,
                clear_frames: 0,
                all_packets_key: true,
            });
        }
        let group = observation.groups.last_mut().ok_or_else(invalid)?;
        if nonempty {
            group.nonempty_frames += 1;
            observation.nonempty_frames += 1;
            first_nonempty.get_or_insert(pts);
            last_nonempty = Some(pts);
        } else {
            group.clear_frames += 1;
            observation.clear_frames += 1;
        }
        observation.last_packet_end_microseconds =
            observation.last_packet_end_microseconds.max(end);
    }
    if source.count == 0
        || observation.nonempty_frames != source.count
        || observation.clear_frames == 0
        || observation
            .groups
            .last()
            .is_none_or(|group| group.clear_frames == 0 || group.nonempty_frames != 0)
        || first_nonempty != Some(expected_endpoint(source.first_pts_microseconds, origin)?)
        || last_nonempty != Some(expected_endpoint(source.last_pts_microseconds, origin)?)
    {
        return Err(invalid());
    }
    Ok(observation)
}
fn pair(packet: Record, frame: Record) -> Result<(i64, i64, bool), SourceTeletextPageDecodeError> {
    let Record::Packet {
        codec_type,
        stream_index,
        pts,
        dts,
        duration,
        flags,
    } = packet
    else {
        return Err(invalid());
    };
    let Record::Frame {
        media_type,
        pts: frame_pts,
        format,
        start_display_time,
        end_display_time,
        num_rects,
    } = frame
    else {
        return Err(invalid());
    };
    if flags != "K__"
        || codec_type != "subtitle"
        || stream_index != 0
        || pts != dts
        || duration <= 0
        || media_type != "subtitle"
        || format != 0
        || num_rects > 4096
        || start_display_time != 0
        || end_display_time == 0
    {
        return Err(invalid());
    }
    let pts = ticks(pts)?;
    if frame_pts != pts {
        return Err(invalid());
    }
    let end = pts.checked_add(ticks(duration)?).ok_or_else(invalid)?;
    Ok((pts, end, num_rects != 0))
}

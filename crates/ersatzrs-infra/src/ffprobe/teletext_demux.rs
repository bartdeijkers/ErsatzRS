//! Bounded original-source demux observations, without raw/PCR epoch authority.

use std::{collections::BTreeSet, fmt, marker::PhantomData};

use serde::{de, Deserialize, Deserializer};
use serde_json::Value;

use super::teletext_origin::NativeSourceTimeBase;

pub(super) const MAX_PACKETS: usize = 4096;
const MAX_STREAMS: usize = 8192;
const MAX_PROGRAMS: usize = 1024;
const MAX_MEMBERSHIPS: usize = 16_384;

/// Global native stream/PID identity, including non-AV program members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSourceDemuxStream {
    index: i32,
    pid: u16,
    codec_type: String,
    time_base: Option<NativeSourceTimeBase>,
}

impl NativeSourceDemuxStream {
    #[must_use]
    pub const fn index(&self) -> i32 {
        self.index
    }
    #[must_use]
    pub const fn pid(&self) -> u16 {
        self.pid
    }
    #[must_use]
    pub fn codec_type(&self) -> &str {
        &self.codec_type
    }
    #[must_use]
    pub const fn time_base(&self) -> Option<NativeSourceTimeBase> {
        self.time_base
    }
}

/// Native program order and memberships; actual stable PSI remains separate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSourceDemuxProgram {
    number: u16,
    pmt_pid: u16,
    pcr_pid: u16,
    stream_indices: Vec<i32>,
}

impl NativeSourceDemuxProgram {
    #[must_use]
    pub const fn number(&self) -> u16 {
        self.number
    }
    #[must_use]
    pub const fn pmt_pid(&self) -> u16 {
        self.pmt_pid
    }
    #[must_use]
    pub const fn pcr_pid(&self) -> u16 {
        self.pcr_pid
    }
    #[must_use]
    pub fn stream_indices(&self) -> &[i32] {
        &self.stream_indices
    }
}

/// One native demux packet, in returned demux order rather than physical order.
/// Native DTS may be copied from PTS even when the raw PES contains no DTS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSourceDemuxPacket {
    stream_index: i32,
    pts: Option<i64>,
    dts: Option<i64>,
    position: Option<u64>,
    flags: Option<u8>,
}

impl NativeSourceDemuxPacket {
    #[must_use]
    pub const fn stream_index(self) -> i32 {
        self.stream_index
    }
    #[must_use]
    pub const fn pts(self) -> Option<i64> {
        self.pts
    }
    #[must_use]
    pub const fn dts(self) -> Option<i64> {
        self.dts
    }
    #[must_use]
    pub const fn position(self) -> Option<u64> {
        self.position
    }
    /// Explicit native corrupt disposition; absence never means clean.
    #[must_use]
    pub fn is_corrupt(self) -> Option<bool> {
        self.flags.map(|flags| flags & 4 != 0)
    }
    /// Native discard disposition, distinct from pre-wrap corrupt rejection.
    #[must_use]
    pub fn is_discard(self) -> Option<bool> {
        self.flags.map(|flags| flags & 2 != 0)
    }
    /// A physical TS188 candidate only; raw PUSI/header association is required.
    #[must_use]
    pub fn source_packet(self) -> Option<u64> {
        self.position
            .filter(|position| position.is_multiple_of(188))
            .map(|position| position / 188)
    }
}

/// Original-source native observations without inferred initialization or epoch.
///
/// No initial seek or stream selection is used. The native probe disables PES
/// parsing/fill-in, but its demuxer may still correct timestamps. Missing values
/// stay unknown. Positions, first returned packet and membership order require
/// raw source and native initialization qualification before applying a policy
/// to PCR. This trace alone grants neither cue coverage nor page readiness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSourceDemuxTrace {
    streams: Vec<NativeSourceDemuxStream>,
    programs: Vec<NativeSourceDemuxProgram>,
    packets: Vec<NativeSourceDemuxPacket>,
}

impl NativeSourceDemuxTrace {
    #[must_use]
    pub fn streams(&self) -> &[NativeSourceDemuxStream] {
        &self.streams
    }
    #[must_use]
    pub fn programs(&self) -> &[NativeSourceDemuxProgram] {
        &self.programs
    }
    #[must_use]
    pub fn packets(&self) -> &[NativeSourceDemuxPacket] {
        &self.packets
    }
    /// Hitting the configured cap does not distinguish more input from exact EOF.
    #[must_use]
    pub fn hit_packet_limit(&self) -> bool {
        self.packets.len() == MAX_PACKETS
    }
}

/// Static supplied-observation failures, without source paths or diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NativeSourceDemuxError {
    #[error("native source demux metadata is invalid or exceeds its bound")]
    Metadata,
    #[error("native source demux identity is invalid or ambiguous")]
    Identity,
    #[error("native source demux timestamp or position is invalid")]
    Timestamp,
}

#[derive(Deserialize)]
struct Output {
    #[serde(deserialize_with = "bounded::<_, _, MAX_STREAMS>")]
    streams: Vec<Stream>,
    #[serde(deserialize_with = "bounded::<_, _, MAX_PROGRAMS>")]
    programs: Vec<Program>,
    #[serde(deserialize_with = "bounded::<_, _, MAX_PACKETS>")]
    packets: Vec<Packet>,
}
#[derive(Deserialize)]
struct Stream {
    index: i32,
    id: String,
    codec_type: String,
    time_base: Option<String>,
}
#[derive(Deserialize)]
struct Program {
    #[serde(rename = "program_num")]
    number: u16,
    pmt_pid: u16,
    pcr_pid: u16,
    #[serde(deserialize_with = "bounded::<_, _, MAX_STREAMS>")]
    streams: Vec<Member>,
}
#[derive(Deserialize)]
struct Member {
    index: i32,
}
#[derive(Deserialize)]
struct Packet {
    stream_index: i32,
    pts: Option<Value>,
    dts: Option<Value>,
    pos: Option<Value>,
    flags: Option<String>,
}

fn bounded<'de, D: Deserializer<'de>, T: Deserialize<'de>, const LIMIT: usize>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    struct Items<T, const LIMIT: usize>(PhantomData<T>);
    impl<'de, T: Deserialize<'de>, const LIMIT: usize> de::Visitor<'de> for Items<T, LIMIT> {
        type Value = Vec<T>;
        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded native demux array")
        }
        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = sequence.next_element()? {
                if values.len() == LIMIT {
                    return Err(de::Error::custom("native demux array exceeds its bound"));
                }
                values.push(value);
            }
            Ok(values)
        }
    }
    deserializer.deserialize_seq(Items::<T, LIMIT>(PhantomData))
}

fn timestamp(value: Option<Value>) -> Result<Option<i64>, NativeSourceDemuxError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value == "N/A" => Ok(None),
        Some(value) => value
            .as_i64()
            .filter(|value| *value != i64::MIN)
            .map(Some)
            .ok_or(NativeSourceDemuxError::Timestamp),
    }
}

fn position(value: Option<Value>) -> Result<Option<u64>, NativeSourceDemuxError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value == "N/A" || value == "-1" => Ok(None),
        Some(Value::String(value))
            if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            value
                .parse()
                .map(Some)
                .map_err(|_| NativeSourceDemuxError::Timestamp)
        }
        _ => Err(NativeSourceDemuxError::Timestamp),
    }
}

fn packet_flags(value: Option<&str>) -> Option<u8> {
    let bytes = value?.as_bytes();
    if bytes.len() != 3
        || !matches!(bytes[0], b'K' | b'_')
        || !matches!(bytes[1], b'D' | b'_')
        || !matches!(bytes[2], b'C' | b'_')
    {
        return None;
    }
    Some(
        u8::from(bytes[0] == b'K')
            | (u8::from(bytes[1] == b'D') << 1)
            | (u8::from(bytes[2] == b'C') << 2),
    )
}

pub(super) fn parse_source_demux(
    bytes: &[u8],
) -> Result<NativeSourceDemuxTrace, NativeSourceDemuxError> {
    use NativeSourceDemuxError::{Identity, Metadata, Timestamp};
    if bytes.len() > super::MAX_REMOTE_FFPROBE_OUTPUT_BYTES {
        return Err(Metadata);
    }
    let output: Output = serde_json::from_slice(bytes).map_err(|_| Metadata)?;
    let mut indices = BTreeSet::new();
    let mut pids = BTreeSet::new();
    let mut streams = Vec::new();
    for stream in output.streams {
        let pid = stream
            .id
            .strip_prefix("0x")
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 4
                    && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .and_then(|value| u16::from_str_radix(value, 16).ok())
            .filter(|pid| (0x20..=0x1ffe).contains(pid))
            .ok_or(Identity)?;
        if stream.index < 0
            || !indices.insert(stream.index)
            || !pids.insert(pid)
            || stream.codec_type.is_empty()
            || stream.codec_type.len() > 32
        {
            return Err(Identity);
        }
        let time_base = stream
            .time_base
            .as_deref()
            .filter(|value| *value != "N/A")
            .map(NativeSourceTimeBase::parse)
            .transpose()
            .map_err(|_| Timestamp)?;
        streams.push(NativeSourceDemuxStream {
            index: stream.index,
            pid,
            codec_type: stream.codec_type,
            time_base,
        });
    }
    let mut numbers = BTreeSet::new();
    let mut programs = Vec::new();
    let mut total_memberships = 0_usize;
    for program in output.programs {
        total_memberships = total_memberships
            .checked_add(program.streams.len())
            .filter(|count| *count <= MAX_MEMBERSHIPS)
            .ok_or(Metadata)?;
        if program.number == 0
            || !numbers.insert(program.number)
            || !(0x20..=0x1ffe).contains(&program.pmt_pid)
            || !(0x20..=0x1fff).contains(&program.pcr_pid)
        {
            return Err(Identity);
        }
        let mut seen = BTreeSet::new();
        let mut members = Vec::new();
        for stream in program.streams {
            if !indices.contains(&stream.index) || !seen.insert(stream.index) {
                return Err(Identity);
            }
            members.push(stream.index);
        }
        programs.push(NativeSourceDemuxProgram {
            number: program.number,
            pmt_pid: program.pmt_pid,
            pcr_pid: program.pcr_pid,
            stream_indices: members,
        });
    }
    let mut packets = Vec::new();
    for packet in output.packets {
        if !indices.contains(&packet.stream_index) {
            return Err(Identity);
        }
        packets.push(NativeSourceDemuxPacket {
            stream_index: packet.stream_index,
            pts: timestamp(packet.pts)?,
            dts: timestamp(packet.dts)?,
            position: position(packet.pos)?,
            flags: packet_flags(packet.flags.as_deref()),
        });
    }
    Ok(NativeSourceDemuxTrace {
        streams,
        programs,
        packets,
    })
}

#[cfg(test)]
#[path = "teletext_demux_tests.rs"]
mod tests;

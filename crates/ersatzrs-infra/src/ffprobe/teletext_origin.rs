//! Exact native timestamp metadata, without source or media-origin qualification.
//!
//! This separate projection never changes ordinary media metadata. Its caller
//! owns the source association, probe version and requested decoder policy.

use std::{collections::BTreeSet, fmt};

use ersatzrs_ffmpeg::teletext::source_av_timestamps::{SourceAvStreamIdentity, SourceAvStreamKind};
use serde::{de, Deserialize, Deserializer};
use serde_json::Value;

const MAX_STREAMS: usize = 8192;
const MAX_DECIMAL_BYTES: usize = 64;
const MAX_FRACTIONAL_DIGITS: usize = 18;

/// A reported signed decimal value with its original decimal precision.
///
/// The exact reported value is units/scale seconds. It is an observation, not
/// proof of a native timestamp origin or its relationship to a source PCR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSourceTime {
    units: i128,
    scale: u64,
}

impl NativeSourceTime {
    /// Signed integer in the original reported decimal units.
    #[must_use]
    pub const fn units(self) -> i128 {
        self.units
    }

    /// Original decimal units per second, including trailing-zero precision.
    #[must_use]
    pub const fn scale(self) -> u64 {
        self.scale
    }

    /// One quantum at the reported precision, as exact rational seconds.
    #[must_use]
    pub const fn quantum(self) -> (u64, u64) {
        (1, self.scale)
    }

    /// Whether an exact rational lies in the closed reported ±one-quantum range.
    ///
    /// This conservative observational enclosure assumes no tighter native
    /// writer/rescale rounding contract. It does not qualify media origin.
    /// A zero denominator or unrepresentable checked arithmetic returns `None`.
    #[must_use]
    pub fn contains_rational_within_quantum(
        self,
        numerator: i128,
        denominator: u64,
    ) -> Option<bool> {
        if denominator == 0 {
            return None;
        }
        let denominator = i128::from(denominator);
        let value = numerator.checked_mul(i128::from(self.scale))?;
        let lower = self.units.checked_sub(1)?.checked_mul(denominator)?;
        let upper = self.units.checked_add(1)?.checked_mul(denominator)?;
        Some(lower <= value && value <= upper)
    }

    fn parse(value: &str) -> Result<Self, NativeSourceOriginError> {
        if value.is_empty() || value.len() > MAX_DECIMAL_BYTES {
            return Err(NativeSourceOriginError::Timing);
        }
        let (negative, digits) = value
            .strip_prefix('-')
            .map_or((false, value), |digits| (true, digits));
        let (whole, fraction) = digits
            .split_once('.')
            .map_or((digits, None), |(whole, fraction)| (whole, Some(fraction)));
        if whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.is_some_and(|fraction| {
                fraction.is_empty()
                    || fraction.len() > MAX_FRACTIONAL_DIGITS
                    || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            return Err(NativeSourceOriginError::Timing);
        }
        let digits = u32::try_from(fraction.map_or(0, str::len))
            .map_err(|_| NativeSourceOriginError::Timing)?;
        let scale = 10_u64.pow(digits);
        // Accumulate the sign directly so i128::MIN is representable too.
        let units = whole
            .bytes()
            .chain(fraction.unwrap_or_default().bytes())
            .try_fold(0_i128, |units, byte| {
                let units = units.checked_mul(10)?;
                let digit = i128::from(byte - b'0');
                if negative {
                    units.checked_sub(digit)
                } else {
                    units.checked_add(digit)
                }
            })
            .ok_or(NativeSourceOriginError::Timing)?;
        Ok(Self { units, scale })
    }
}

/// Positive native stream time-base numerator/denominator, without rounding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSourceTimeBase {
    numerator: i32,
    denominator: i32,
}

impl NativeSourceTimeBase {
    /// Explicit positive native numerator, without reduction or rescaling.
    #[must_use]
    pub const fn numerator(self) -> i32 {
        self.numerator
    }

    /// Explicit positive native denominator, without reduction or rescaling.
    #[must_use]
    pub const fn denominator(self) -> i32 {
        self.denominator
    }

    pub(super) fn parse(value: &str) -> Result<Self, NativeSourceOriginError> {
        if value.len() > 21 {
            return Err(NativeSourceOriginError::Timing);
        }
        let (numerator, denominator) = value
            .split_once('/')
            .ok_or(NativeSourceOriginError::Timing)?;
        let positive = |value: &str| {
            (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| value.parse::<i32>().ok())
                .flatten()
                .filter(|value| *value > 0)
                .ok_or(NativeSourceOriginError::Timing)
        };
        Ok(Self {
            numerator: positive(numerator)?,
            denominator: positive(denominator)?,
        })
    }
}

/// One native AV record; absent start/time-base fields remain independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeAvOrigin {
    identity: SourceAvStreamIdentity,
    start_pts: Option<i64>,
    time_base: Option<NativeSourceTimeBase>,
}

impl NativeAvOrigin {
    /// Actual global stream index, elementary PID and native AV role.
    #[must_use]
    pub const fn identity(self) -> SourceAvStreamIdentity {
        self.identity
    }

    /// Signed reported start tick; absence remains distinct from zero.
    #[must_use]
    pub const fn start_pts(self) -> Option<i64> {
        self.start_pts
    }

    /// Reported time base, independent of whether a start tick was reported.
    #[must_use]
    pub const fn time_base(self) -> Option<NativeSourceTimeBase> {
        self.time_base
    }
}

/// Supplied format start and every AV track, never a first-track origin choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSourceOrigin {
    format_name: Option<String>,
    format_start: Option<NativeSourceTime>,
    streams: Vec<NativeAvOrigin>,
}

impl NativeSourceOrigin {
    /// Actual native demux format identity, not inferred from a filename.
    #[must_use]
    pub fn format_name(&self) -> Option<&str> {
        self.format_name.as_deref()
    }
    /// Native container start as reported; absence remains unknown.
    #[must_use]
    pub const fn format_start(&self) -> Option<NativeSourceTime> {
        self.format_start
    }

    /// Every native AV row in its supplied order, without first-track selection.
    #[must_use]
    pub fn streams(&self) -> &[NativeAvOrigin] {
        &self.streams
    }
}

/// Opaque optional-projection failures; no supplied input or locator is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NativeSourceOriginError {
    /// Invalid JSON/shape or supplied input/stream-count capacity exceeded.
    #[error("native source timestamp metadata is invalid or exceeds its bound")]
    Metadata,
    /// Missing, invalid or duplicated native AV global index or PID.
    #[error("native source AV identity is invalid or duplicated")]
    Identity,
    /// Invalid type, syntax, precision, sign or arithmetic range for timing.
    #[error("native source timestamp value is invalid or exceeds its bound")]
    Timing,
}

#[derive(Deserialize)]
struct Output {
    format: Option<Format>,
    #[serde(default, deserialize_with = "bounded_streams")]
    streams: Vec<Stream>,
}

#[derive(Deserialize)]
struct Format {
    format_name: Option<String>,
    start_time: Option<Value>,
}

#[derive(Deserialize)]
struct Stream {
    codec_type: Option<String>,
    index: Option<Value>,
    id: Option<Value>,
    start_pts: Option<Value>,
    time_base: Option<Value>,
}

fn bounded_streams<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Stream>, D::Error> {
    struct Streams;
    impl<'de> de::Visitor<'de> for Streams {
        type Value = Vec<Stream>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded native stream array")
        }

        fn visit_seq<A: de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut streams = Vec::new();
            while let Some(stream) = sequence.next_element()? {
                if streams.len() == MAX_STREAMS {
                    return Err(de::Error::custom("native stream array exceeds its bound"));
                }
                streams.push(stream);
            }
            Ok(streams)
        }
    }
    deserializer.deserialize_seq(Streams)
}

fn optional_text(value: Option<&Value>) -> Result<Option<&str>, NativeSourceOriginError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value == "N/A" => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(NativeSourceOriginError::Timing),
    }
}

fn stream_identity(
    stream: &Stream,
    kind: SourceAvStreamKind,
) -> Result<SourceAvStreamIdentity, NativeSourceOriginError> {
    let index = stream
        .index
        .as_ref()
        .and_then(Value::as_i64)
        .and_then(|index| i32::try_from(index).ok())
        .filter(|index| *index >= 0)
        .ok_or(NativeSourceOriginError::Identity)?;
    let pid = stream
        .id
        .as_ref()
        .and_then(Value::as_str)
        .and_then(|value| value.strip_prefix("0x"))
        .filter(|digits| {
            !digits.is_empty()
                && digits.len() <= 4
                && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .and_then(|digits| u16::from_str_radix(digits, 16).ok())
        .filter(|pid| (0x20..=0x1ffe).contains(pid))
        .ok_or(NativeSourceOriginError::Identity)?;
    Ok(SourceAvStreamIdentity {
        stream_index: index,
        pid,
        kind,
    })
}

/// Project supplied native metadata without changing ordinary media rows.
/// # Errors
/// Rejects malformed/bounded input, ambiguous AV identities and invalid timing.
pub(super) fn parse_source_origin(
    bytes: &[u8],
) -> Result<NativeSourceOrigin, NativeSourceOriginError> {
    if bytes.len() > super::MAX_REMOTE_FFPROBE_OUTPUT_BYTES {
        return Err(NativeSourceOriginError::Metadata);
    }
    let output: Output =
        serde_json::from_slice(bytes).map_err(|_| NativeSourceOriginError::Metadata)?;
    let format_start = optional_text(
        output
            .format
            .as_ref()
            .and_then(|format| format.start_time.as_ref()),
    )?
    .map(NativeSourceTime::parse)
    .transpose()?;
    let format_name = output
        .format
        .as_ref()
        .and_then(|format| format.format_name.as_deref())
        .filter(|name| *name != "N/A");
    if format_name.is_some_and(|name| {
        name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b','))
    }) {
        return Err(NativeSourceOriginError::Metadata);
    }
    let mut indexes = BTreeSet::new();
    let mut pids = BTreeSet::new();
    let mut streams = Vec::new();
    for stream in output.streams {
        let kind = match stream.codec_type.as_deref() {
            Some("audio") => SourceAvStreamKind::Audio,
            Some("video") => SourceAvStreamKind::Video,
            _ => continue,
        };
        let identity = stream_identity(&stream, kind)?;
        if !indexes.insert(identity.stream_index) || !pids.insert(identity.pid) {
            return Err(NativeSourceOriginError::Identity);
        }
        let start_pts = match stream.start_pts.as_ref() {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value == "N/A" => None,
            Some(value) => Some(value.as_i64().ok_or(NativeSourceOriginError::Timing)?),
        };
        let time_base = optional_text(stream.time_base.as_ref())?
            .map(NativeSourceTimeBase::parse)
            .transpose()?;
        streams.push(NativeAvOrigin {
            identity,
            start_pts,
            time_base,
        });
    }
    Ok(NativeSourceOrigin {
        format_name: format_name.map(str::to_owned),
        format_start,
        streams,
    })
}

#[cfg(test)]
#[path = "teletext_origin_tests.rs"]
mod tests;

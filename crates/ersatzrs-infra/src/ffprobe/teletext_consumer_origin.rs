//! Explicit enabled-stream timing projection for the audited native input policy.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value;

use super::teletext_origin::{parse_source_origin, NativeSourceTime, NativeSourceTimeBase};

const MAX_STREAMS: usize = 8192;

/// Exact supplied timing for one explicitly enabled global input stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSourceConsumerStream {
    index: i32,
    pid: u16,
    codec_type: String,
    start_pts: Option<i64>,
    time_base: Option<NativeSourceTimeBase>,
}

impl NativeSourceConsumerStream {
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
    pub const fn start_pts(&self) -> Option<i64> {
        self.start_pts
    }

    #[must_use]
    pub const fn time_base(&self) -> Option<NativeSourceTimeBase> {
        self.time_base
    }
}

/// Supplied format origin and selected streams in the caller's requested order.
///
/// Selection is explicit, including an enabled subtitle stream. No stream is
/// enabled from its position, language, default flag or AV role. The caller
/// owns same-source, same-flags, native-version and actual enabled-set evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSourceConsumerOrigin {
    format_name: Option<String>,
    format_start: Option<NativeSourceTime>,
    streams: Vec<NativeSourceConsumerStream>,
}

impl NativeSourceConsumerOrigin {
    #[must_use]
    pub fn format_name(&self) -> Option<&str> {
        self.format_name.as_deref()
    }

    #[must_use]
    pub const fn format_start(&self) -> Option<NativeSourceTime> {
        self.format_start
    }

    #[must_use]
    pub fn streams(&self) -> &[NativeSourceConsumerStream] {
        &self.streams
    }

    /// Derive checked bounds under the audited plain n9.0 MPEG-TS input policy.
    ///
    /// This assumes normal stream-info with the captured input flags, no
    /// `copyts`, `seek_timestamp`, `itsoffset`, synchronization, scaling or loop.
    /// Input opening first seeks from the container origin F plus effective
    /// seek S. Later `correct_input_start_times` uses E, the minimum rescaled
    /// known start of ALL enabled streams, and presentation zero is max(F+S,E).
    /// Missing starts are ignored as native `AV_NOPTS` values; a known start with
    /// missing time base, no known enabled start, or overflow remains unknown.
    ///
    /// These are origin bounds, not the demuxer's video-delay seek preroll,
    /// actual seek landing, decoded output normalization or cue eligibility.
    #[must_use]
    pub fn timing(&self, effective_seek_us: i64) -> Option<NativeSourceConsumerTiming> {
        if self.format_name() != Some("mpegts") || effective_seek_us < 0 {
            return None;
        }
        let format = self.format_start?.ffprobe_n9_format_microsecond_bounds()?;
        let mut enabled_start = None;
        for stream in &self.streams {
            let Some(start) = stream.start_pts else {
                continue;
            };
            let start = rescale_microseconds(start, stream.time_base?)?;
            enabled_start = Some(enabled_start.map_or(start, |before: i64| before.min(start)));
        }
        let enabled_start = enabled_start?;
        let initial_seek = (
            format.0.checked_add(effective_seek_us)?,
            format.1.checked_add(effective_seek_us)?,
        );
        Some(NativeSourceConsumerTiming {
            format,
            effective_seek_us,
            enabled_start,
            initial_seek,
            presentation_zero: (
                initial_seek.0.max(enabled_start),
                initial_seek.1.max(enabled_start),
            ),
        })
    }
}

/// Conservative numeric consequences of one explicit, separately audited policy.
/// This type itself grants no source, consumer, epoch, page or stream admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeSourceConsumerTiming {
    format: (i64, i64),
    effective_seek_us: i64,
    enabled_start: i64,
    initial_seek: (i64, i64),
    presentation_zero: (i64, i64),
}

impl NativeSourceConsumerTiming {
    #[must_use]
    pub const fn format_start_bounds_microseconds(self) -> (i64, i64) {
        self.format
    }

    #[must_use]
    pub const fn effective_seek_microseconds(self) -> i64 {
        self.effective_seek_us
    }

    #[must_use]
    pub const fn enabled_start_microseconds(self) -> i64 {
        self.enabled_start
    }

    /// Container-origin seek target, before any video-delay preroll heuristic.
    #[must_use]
    pub const fn initial_seek_bounds_microseconds(self) -> (i64, i64) {
        self.initial_seek
    }

    /// Later enabled-stream correction; never a replacement seek origin.
    #[must_use]
    pub const fn presentation_zero_bounds_microseconds(self) -> (i64, i64) {
        self.presentation_zero
    }
}

/// Opaque failures of the entire supplied optional timing projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NativeSourceConsumerOriginError {
    #[error("native consumer timing metadata is invalid or exceeds its bound")]
    Metadata,
    #[error("native consumer enabled stream inventory is invalid or ambiguous")]
    Inventory,
    #[error("native consumer stream identity is invalid or duplicated")]
    Identity,
    #[error("native consumer stream timing is invalid")]
    Timing,
}

#[derive(Deserialize)]
struct Output {
    #[serde(default)]
    streams: Vec<Stream>,
}

#[derive(Deserialize)]
struct Stream {
    index: Option<Value>,
    id: Option<Value>,
    codec_type: Option<String>,
    start_pts: Option<Value>,
    time_base: Option<Value>,
}

/// Project explicit enabled indices without changing ordinary AV metadata.
///
/// Existing source parsing first enforces the shared byte/stream bounds and
/// all ordinary AV validation. All global indices must be valid and unique;
/// nonselected non-AV identity/timing fields remain uninterpreted. Selected
/// rows must be unique by PID and retain explicit native codec roles/timing.
pub(super) fn parse_consumer_origin(
    bytes: &[u8],
    enabled: &[i32],
) -> Result<NativeSourceConsumerOrigin, NativeSourceConsumerOriginError> {
    let requested = validate_enabled(enabled)?;
    let native =
        parse_source_origin(bytes).map_err(|_| NativeSourceConsumerOriginError::Metadata)?;
    // The existing bounded projection above checks this same immutable input
    // before this selected-stream DTO can allocate its rows.
    let output: Output =
        serde_json::from_slice(bytes).map_err(|_| NativeSourceConsumerOriginError::Metadata)?;
    let mut indices = BTreeSet::new();
    let mut pids = BTreeSet::new();
    let mut selected = Vec::with_capacity(enabled.len());
    for row in output.streams {
        let index = row
            .index
            .as_ref()
            .and_then(Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .filter(|value| *value >= 0)
            .ok_or(NativeSourceConsumerOriginError::Identity)?;
        if !indices.insert(index) {
            return Err(NativeSourceConsumerOriginError::Inventory);
        }
        if !requested.contains_key(&index) {
            continue;
        }
        let stream = selected_stream(index, row)?;
        if !pids.insert(stream.pid) {
            return Err(NativeSourceConsumerOriginError::Identity);
        }
        selected.push(stream);
    }
    if selected.len() != enabled.len() {
        return Err(NativeSourceConsumerOriginError::Inventory);
    }
    selected.sort_by_key(|stream| requested.get(&stream.index).copied());
    Ok(NativeSourceConsumerOrigin {
        format_name: native.format_name().map(str::to_owned),
        format_start: native.format_start(),
        streams: selected,
    })
}

pub(super) fn validate_enabled(
    enabled: &[i32],
) -> Result<BTreeMap<i32, usize>, NativeSourceConsumerOriginError> {
    if enabled.is_empty() || enabled.len() > MAX_STREAMS {
        return Err(NativeSourceConsumerOriginError::Inventory);
    }
    let requested: BTreeMap<_, _> = enabled
        .iter()
        .copied()
        .enumerate()
        .map(|(rank, index)| (index, rank))
        .collect();
    if requested.len() != enabled.len() || requested.keys().any(|index| *index < 0) {
        return Err(NativeSourceConsumerOriginError::Inventory);
    }
    Ok(requested)
}

fn selected_stream(
    index: i32,
    row: Stream,
) -> Result<NativeSourceConsumerStream, NativeSourceConsumerOriginError> {
    let pid = row
        .id
        .as_ref()
        .and_then(Value::as_str)
        .and_then(|value| value.strip_prefix("0x"))
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 4
                && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .and_then(|value| u16::from_str_radix(value, 16).ok())
        .filter(|pid| (0x20..=0x1ffe).contains(pid))
        .ok_or(NativeSourceConsumerOriginError::Identity)?;
    let codec_type = row
        .codec_type
        .ok_or(NativeSourceConsumerOriginError::Identity)?;
    if !matches!(
        codec_type.as_str(),
        "audio" | "video" | "subtitle" | "data" | "attachment" | "unknown"
    ) {
        return Err(NativeSourceConsumerOriginError::Identity);
    }
    let start_pts = match row.start_pts.as_ref() {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value == "N/A" => None,
        Some(value) => Some(
            value
                .as_i64()
                .filter(|value| *value != i64::MIN)
                .ok_or(NativeSourceConsumerOriginError::Timing)?,
        ),
    };
    let time_base = match row.time_base.as_ref() {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) if value == "N/A" => None,
        Some(Value::String(value)) => Some(
            NativeSourceTimeBase::parse(value)
                .map_err(|_| NativeSourceConsumerOriginError::Timing)?,
        ),
        Some(_) => return Err(NativeSourceConsumerOriginError::Timing),
    };
    Ok(NativeSourceConsumerStream {
        index,
        pid,
        codec_type,
        start_pts,
        time_base,
    })
}

fn rescale_microseconds(start: i64, base: NativeSourceTimeBase) -> Option<i64> {
    let numerator = i128::from(start)
        .checked_mul(i128::from(base.numerator()))?
        .checked_mul(1_000_000)?;
    let denominator = i128::from(base.denominator());
    let magnitude = numerator.checked_abs()?;
    let rounded = magnitude.checked_add(denominator / 2)? / denominator;
    let signed = if numerator < 0 {
        rounded.checked_neg()?
    } else {
        rounded
    };
    i64::try_from(signed)
        .ok()
        .filter(|value| *value != i64::MIN)
}

#[cfg(test)]
mod tests;

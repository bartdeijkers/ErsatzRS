//! Native-format cue bookends from the same owned source walk, not playback admission.

use ersatzrs_ffmpeg::teletext::{
    source_clock::{SourceTeletextClockReport, SourceTeletextClockSample},
    source_native_epoch::SourceNativeEpoch,
};

use super::{SourceTeletextNativeClockEvidence, SourceTeletextPacketSpan};
use crate::ffprobe::teletext_origin::NativeSourceOrigin;

/// Explicit effective native microseconds and bounded source-PCR observation policy.
///
/// Callers must derive seek and duration separately from the actual command.
/// These values neither identify an enabled-stream consumer origin nor grant
/// page, decoder, stream-route or playback readiness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextCueRequest {
    start: i64,
    end: i64,
    max_gap_packets: u64,
    max_gap_ticks: u64,
}

impl SourceTeletextCueRequest {
    /// Validate a positive finite request and explicit PCR gap bounds.
    /// Tick limits must be strictly below half the 33-bit PCR period. Observed
    /// bounds cannot independently exclude an omitted whole timestamp period.
    #[must_use]
    pub fn new(start: i64, end: i64, max_gap_packets: u64, max_gap_ticks: u64) -> Option<Self> {
        (start >= 0
            && end > start
            && max_gap_packets > 0
            && max_gap_ticks > 0
            && max_gap_ticks < (1_u64 << 33) * 150)
            .then_some(Self {
                start,
                end,
                max_gap_packets,
                max_gap_ticks,
            })
    }

    #[must_use]
    pub const fn start_microseconds(self) -> i64 {
        self.start
    }

    #[must_use]
    pub const fn end_microseconds(self) -> i64 {
        self.end
    }
}

/// Actual native-format/PCR bookends and a bounded span on the held source.
///
/// Only the existing owned source walker constructs this receipt, after its
/// epoch, PSI, AV witnesses, full-source hash and source bookends succeed.
/// The span must still contain its own current PSI, complete selected PES and
/// requested-page header before decoding. This is native FORMAT time, not
/// consumer time: enabled-stream start correction, actual input flags, seek
/// landing, initial page state and bitmap presentation remain separate gates.
#[derive(Debug, Clone)]
pub struct SourceTeletextCueEvidence {
    clock: SourceTeletextNativeClockEvidence,
    request: SourceTeletextCueRequest,
    target_ticks: (i64, i64),
    before: SourceTeletextClockSample,
    after: SourceTeletextClockSample,
    span: SourceTeletextPacketSpan,
}

impl SourceTeletextCueEvidence {
    #[must_use]
    pub fn native_clock(&self) -> &SourceTeletextNativeClockEvidence {
        &self.clock
    }

    #[must_use]
    pub const fn request(&self) -> SourceTeletextCueRequest {
        self.request
    }

    /// Conservative start/end in native program 27 MHz coordinates, including
    /// the audited format writer's numeric enclosure; never a rounded duration.
    #[must_use]
    pub const fn target_ticks(&self) -> (i64, i64) {
        self.target_ticks
    }

    /// Last actually observed PCR at or before the conservative requested start.
    #[must_use]
    pub const fn before(&self) -> SourceTeletextClockSample {
        self.before
    }

    /// First actually observed PCR at or after the conservative requested end.
    #[must_use]
    pub const fn after(&self) -> SourceTeletextClockSample {
        self.after
    }

    /// Contains the actual bounding PCR packets; no packet-rate guess or seek hint.
    #[must_use]
    pub const fn span(&self) -> SourceTeletextPacketSpan {
        self.span
    }
}

pub(super) struct CueCollector {
    request: SourceTeletextCueRequest,
    target_ticks: (i64, i64),
    epoch: SourceNativeEpoch,
    first_native_ticks: Option<i64>,
    previous: Option<SourceTeletextClockSample>,
    before: Option<SourceTeletextClockSample>,
    after: Option<SourceTeletextClockSample>,
    invalid: bool,
}

impl CueCollector {
    pub(super) fn new(
        request: SourceTeletextCueRequest,
        origin: &NativeSourceOrigin,
        epoch: SourceNativeEpoch,
    ) -> Option<Self> {
        let (lower, upper) = origin
            .format_start()?
            .ffprobe_n9_format_microsecond_bounds()?;
        Some(Self {
            request,
            target_ticks: (
                lower.checked_add(request.start)?.checked_mul(27)?,
                upper.checked_add(request.end)?.checked_mul(27)?,
            ),
            epoch,
            first_native_ticks: None,
            previous: None,
            before: None,
            after: None,
            invalid: false,
        })
    }

    pub(super) fn push(&mut self, report: SourceTeletextClockReport) {
        if self.invalid || self.after.is_some() {
            return;
        }
        let Some(last) = report.last else {
            self.invalid = report.packets_seen > self.request.max_gap_packets;
            return;
        };
        if report
            .packets_since_last_pcr()
            .is_none_or(|gap| gap > self.request.max_gap_packets)
        {
            self.invalid = true;
            return;
        }
        if self.previous == Some(last) {
            return;
        }
        if let Some(previous) = self.previous {
            if last.source_packet - previous.source_packet > self.request.max_gap_packets
                || last.elapsed_ticks - previous.elapsed_ticks > self.request.max_gap_ticks
            {
                self.invalid = true;
                return;
            }
        } else {
            self.invalid = last.source_packet > self.request.max_gap_packets;
            self.first_native_ticks = self.epoch.apply_pcr_ticks(last.raw_ticks).ok().flatten();
        }
        self.previous = Some(last);
        let ticks = self
            .first_native_ticks
            .and_then(|first| first.checked_add(i64::try_from(last.elapsed_ticks).ok()?));
        let Some(ticks) = ticks else {
            self.invalid = true;
            return;
        };
        if ticks <= self.target_ticks.0 {
            self.before = Some(last);
        }
        if ticks >= self.target_ticks.1 {
            self.after = Some(last);
        }
    }

    pub(super) fn finish(
        self,
        clock: SourceTeletextNativeClockEvidence,
    ) -> Option<SourceTeletextCueEvidence> {
        if self.invalid {
            return None;
        }
        let (before, after) = (self.before?, self.after?);
        let count = after
            .source_packet
            .checked_sub(before.source_packet)?
            .checked_add(1)?;
        let span =
            SourceTeletextPacketSpan::new(before.source_packet, u32::try_from(count).ok()?).ok()?;
        Some(SourceTeletextCueEvidence {
            clock,
            request: self.request,
            target_ticks: self.target_ticks,
            before,
            after,
            span,
        })
    }
}

#[cfg(test)]
mod tests;

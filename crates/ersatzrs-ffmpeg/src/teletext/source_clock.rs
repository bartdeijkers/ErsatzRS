//! Continuing raw source PCR observations, without a muxrate or media origin.

use mpeg2ts_reader::packet::AdaptationFieldError;

use super::{clock::validated_pcr_ticks, signalling::valid_packet};

const PCR_MODULUS: u64 = (1_u64 << 33) * 300;

/// One actual selected-PCR sample, not a presentation or wall-clock timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextClockSample {
    /// Absolute zero-based physical source packet ordinal.
    pub source_packet: u64,
    /// Supplied PCR base * 300 + extension, modulo 2^33 * 300.
    pub raw_ticks: u64,
    /// Unwrapped 27 MHz ticks since the first actual observed PCR, not media zero.
    pub elapsed_ticks: u64,
}

/// Historical raw observations only; no gap, rate, PSI or media-time admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextClockReport {
    /// Absolute ordinal supplied at construction, including a nonzero prefix.
    pub first_source_packet: u64,
    /// All successfully observed physical packets, including other/null PIDs.
    pub packets_seen: u64,
    /// First actual selected-PCR sample; absent when none was observed.
    pub first: Option<SourceTeletextClockSample>,
    /// Latest actual selected-PCR sample; trailing packets remain counted above.
    pub last: Option<SourceTeletextClockSample>,
    /// Successful PCR samples; no minimum count confers readiness.
    pub pcr_samples: u64,
    /// Observed forward modulo wraps, under the less-than-half-wrap assumption.
    pub wraps: u64,
}

impl SourceTeletextClockReport {
    /// Physical packets observed after the latest PCR, without extrapolating time.
    #[must_use]
    pub fn packets_since_last_pcr(&self) -> Option<u64> {
        let last = self.last?;
        self.first_source_packet
            .checked_add(self.packets_seen)?
            .checked_sub(last.source_packet.checked_add(1)?)
    }
}

/// Sticky source-clock failures; no input content is retained in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceTeletextClockError {
    /// Reserved, null or out-of-range selected PCR PID.
    #[error("source teletext clock requires a nonreserved PCR PID")]
    Pid,
    /// Invalid TS188 framing, scrambling, transport error or adaptation extent.
    #[error("source teletext clock has invalid TS188 framing")]
    Framing,
    /// Malformed/truncated PCR, reserved bits or extension at least 300.
    #[error("source teletext clock has invalid PCR syntax")]
    Pcr,
    /// Selected-PID discontinuity outside its explicitly allowed first packet.
    #[error("source teletext clock signals a discontinuity")]
    Discontinuity,
    /// Repeated, backward or ambiguous half-wrap PCR interval.
    #[error("source teletext clock does not advance unambiguously")]
    ClockOrder,
    /// Source position, sample count or unwrapped elapsed ticks overflow.
    #[error("source teletext clock arithmetic exceeds its bound")]
    Arithmetic,
}

/// Constant-memory observer for one caller-owned, unchanged source/PCR epoch.
///
/// Supply every physical packet exactly once in source order, including other
/// programs and null packets. The caller owns positional continuity and the
/// source/program association; packet bytes alone cannot prove either. One
/// packet per call bounds work, while counters persist across any number of
/// calls. No packet payload or timeline history is retained.
///
/// Forward PCR differences must be less than half the modulo period. As with
/// any modulo clock, absent independent gap bounds this cannot exclude an
/// unobserved full wrap. This observer supplies raw evidence, never a media
/// origin, temporal coverage, muxrate, delivery pacing or playback readiness.
/// Every observation failure fences subsequent observations and reports.
#[derive(Debug)]
pub struct SourceTeletextClockObserver {
    pcr_pid: u16,
    report: SourceTeletextClockReport,
    initial_discontinuity_allowed: bool,
    failure: Option<SourceTeletextClockError>,
}

impl SourceTeletextClockObserver {
    /// Observe an explicit source range without allowing clock discontinuities.
    /// # Errors
    /// Rejects a reserved/null/out-of-range PCR PID. Position arithmetic is
    /// checked before each packet; an overflowing observation fences the source.
    pub fn new(pcr_pid: u16, first_source_packet: u64) -> Result<Self, SourceTeletextClockError> {
        if !(0x20..=0x1ffe).contains(&pcr_pid) {
            return Err(SourceTeletextClockError::Pid);
        }
        Ok(Self {
            pcr_pid,
            report: SourceTeletextClockReport {
                first_source_packet,
                packets_seen: 0,
                first: None,
                last: None,
                pcr_samples: 0,
                wraps: 0,
            },
            initial_discontinuity_allowed: false,
            failure: None,
        })
    }

    /// Allow discontinuity only on the first packet on the selected PID.
    /// Other PIDs do not consume the allowance; a selected packet without PCR
    /// does consume it. This does not invent a clock sample or admit later resets.
    /// # Errors
    /// Rejects the same invalid PCR PIDs as [`Self::new`].
    pub fn new_source_epoch(
        pcr_pid: u16,
        first_source_packet: u64,
    ) -> Result<Self, SourceTeletextClockError> {
        let mut observer = Self::new(pcr_pid, first_source_packet)?;
        observer.initial_discontinuity_allowed = true;
        Ok(observer)
    }

    /// Inspect the next complete actual TS188 packet without modifying it.
    /// # Errors
    /// Framing, PCR syntax/order, discontinuity and arithmetic failures are sticky.
    pub fn push(
        &mut self,
        raw: &[u8; 188],
    ) -> Result<SourceTeletextClockReport, SourceTeletextClockError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let result = self.inspect(raw);
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }

    /// Read successful evidence, including packets after the last PCR sample.
    /// # Errors
    /// Returns the first sticky failure; no partial report escapes a failed walk.
    pub fn report(&self) -> Result<SourceTeletextClockReport, SourceTeletextClockError> {
        self.failure.map_or(Ok(self.report), Err)
    }

    fn inspect(
        &mut self,
        raw: &[u8; 188],
    ) -> Result<SourceTeletextClockReport, SourceTeletextClockError> {
        let packet = valid_packet(raw).map_err(|_| SourceTeletextClockError::Framing)?;
        let position = self
            .report
            .first_source_packet
            .checked_add(self.report.packets_seen)
            .ok_or(SourceTeletextClockError::Arithmetic)?;
        position
            .checked_add(1)
            .ok_or(SourceTeletextClockError::Arithmetic)?;
        let count = self
            .report
            .packets_seen
            .checked_add(1)
            .ok_or(SourceTeletextClockError::Arithmetic)?;
        if u16::from(packet.pid()) == self.pcr_pid {
            let initial = std::mem::replace(&mut self.initial_discontinuity_allowed, false);
            if let Some(field) = packet.adaptation_field() {
                if field.discontinuity_indicator() && !initial {
                    return Err(SourceTeletextClockError::Discontinuity);
                }
                match field.pcr() {
                    Ok(clock) => {
                        let ticks = validated_pcr_ticks(&packet, clock)
                            .ok_or(SourceTeletextClockError::Pcr)?;
                        self.sample(position, ticks)?;
                    }
                    Err(AdaptationFieldError::FieldNotPresent) => (),
                    Err(_) => return Err(SourceTeletextClockError::Pcr),
                }
            }
        }
        self.report.packets_seen = count;
        Ok(self.report)
    }

    fn sample(
        &mut self,
        source_packet: u64,
        raw_ticks: u64,
    ) -> Result<(), SourceTeletextClockError> {
        let mut elapsed_ticks = 0;
        if let Some(previous) = self.report.last {
            let delta = (raw_ticks + PCR_MODULUS - previous.raw_ticks) % PCR_MODULUS;
            if delta == 0 || delta >= PCR_MODULUS / 2 {
                return Err(SourceTeletextClockError::ClockOrder);
            }
            elapsed_ticks = previous
                .elapsed_ticks
                .checked_add(delta)
                .ok_or(SourceTeletextClockError::Arithmetic)?;
            if raw_ticks < previous.raw_ticks {
                self.report.wraps = self
                    .report
                    .wraps
                    .checked_add(1)
                    .ok_or(SourceTeletextClockError::Arithmetic)?;
            }
        }
        let sample = SourceTeletextClockSample {
            source_packet,
            raw_ticks,
            elapsed_ticks,
        };
        self.report.pcr_samples = self
            .report
            .pcr_samples
            .checked_add(1)
            .ok_or(SourceTeletextClockError::Arithmetic)?;
        self.report.first.get_or_insert(sample);
        self.report.last = Some(sample);
        Ok(())
    }
}

#[cfg(test)]
#[path = "source_clock_tests.rs"]
mod tests;

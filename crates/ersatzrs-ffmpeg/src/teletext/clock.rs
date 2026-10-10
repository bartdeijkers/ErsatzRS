//! Bounded validation of declared CBR timing against the selected program PCR.
//!
//! H.222.0 section 2.4.2.2 derives transport rate from the complete stream's
//! physical byte distance between PCR fields, not just the selected PID. The
//! PCR field position is fixed within TS188 packets, so packet distance gives
//! that byte distance exactly. PCR ticks are base * 300 + extension; wrap is
//! at 2^33 * 300, rather than at 2^42.

use mpeg2ts_reader::packet::{AdaptationFieldError, ClockRef, Packet};

use super::signalling::{valid_packet, MAX_WINDOW_PACKETS};

const PCR_HZ: u64 = 27_000_000;
const PCR_MODULUS: u64 = (1_u64 << 33) * 300;
const PARTS_PER_MILLION: u128 = 1_000_000;

/// Explicit policy for one immutable, selected-PCR, constant-rate source.
///
/// There are no standard-derived tolerance defaults here. The owner must pick
/// limits appropriate to its verified output path. These limits measure media
/// timing and physical packet distance; none measure wall-clock delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextClockConfig {
    /// Selected program's non-reserved, non-null DVB PCR PID.
    pub pcr_pid: u16,
    /// Declared full TS188 multiplex rate, including null packets, in bit/s.
    pub muxrate_bps: u64,
    /// Relative error allowed against the declared rate, below 1,000,000 ppm.
    pub rate_tolerance_ppm: u32,
    /// Additional absolute interval error allowed in 27 MHz PCR ticks.
    pub pcr_jitter_ticks: u64,
    /// Maximum packet distance between PCRs, also bounding startup search.
    pub max_pcr_gap_packets: u64,
    /// Maximum forward PCR delta, strictly below half the PCR wrap period.
    pub max_pcr_gap_ticks: u64,
    /// Minimum validated successive-PCR intervals before readiness, at least 1.
    pub min_pcr_intervals: u32,
    /// Maximum packets per observation, between 1 and 32,768 inclusive.
    pub max_window_packets: usize,
}

/// PCR/CBR admission failure, without retaining transport payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextClockError {
    /// A policy parameter cannot describe a bounded, unambiguous clock check.
    #[error("teletext clock configuration has invalid {field}")]
    Configuration {
        /// Invalid policy parameter's name.
        field: &'static str,
    },
    /// The caller supplied more packets than the configured per-call bound.
    #[error("teletext clock observation exceeds its window bound")]
    Capacity,
    /// A packet has invalid sync/header/adaptation syntax or is corrupt/scrambled.
    #[error("teletext clock source has invalid TS188 framing")]
    Framing,
    /// The selected PCR field is truncated, reserved bits differ or extension >= 300.
    #[error("teletext source PCR syntax is invalid")]
    Pcr,
    /// The selected PID signals a clock discontinuity outside an explicitly
    /// admitted first packet, even without a PCR field.
    #[error("teletext source PCR PID signals a discontinuity")]
    Discontinuity,
    /// PCRs repeat, run backwards or have an ambiguous half-wrap delta.
    #[error("teletext source PCR does not advance unambiguously")]
    ClockOrder,
    /// PCR packet spacing or a forward PCR interval exceeds its configured limit.
    #[error("teletext source PCR exceeds its configured gap")]
    PcrGap,
    /// Physical stream distance contradicts the declared constant muxrate.
    #[error("teletext source does not match its declared constant muxrate")]
    Muxrate,
    /// A lifetime counter or an intermediate calculation exceeds its integer bound.
    #[error("teletext clock arithmetic exceeds its bound")]
    Arithmetic,
}

/// Current valid evidence for one source generation, including startup state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TeletextClockReport {
    /// Number of complete raw packets observed across all PIDs.
    pub packets_seen: u64,
    /// Number of syntactically valid selected-PCR samples observed.
    pub pcr_samples: u64,
    /// Number of successive PCR intervals matching the configured rate/gaps.
    pub validated_intervals: u64,
    /// Enough intervals were validated; future packets still require observation.
    pub ready: bool,
}

/// Constant-memory continuing PCR/rate fence for one raw TS188 generation.
///
/// Feed every new packet exactly once, in physical output order, including
/// null packets and packets from other programs. Feed the source before any
/// replacement; equal-size null replacement preserves these clock distances.
/// The source, selected PID and declared rate must remain immutable. No history
/// or input bytes are retained. Only two scalar values for the last PCR and
/// bounded counters persist across bounded windows. Repeated identical PCR
/// packets are rejected: each physical packet consumes distance/time at CBR.
///
/// Every observation error permanently fences this instance, including caller
/// capacity errors. A new instance requires explicit source readmission by the
/// owner. Readiness proves the observed clock/rate evidence only; it neither
/// paces playback nor admits buffers, deadlines, PSI/PID ownership or a live
/// channel. Finite output with insufficient samples remains unready.
pub struct TeletextClockValidator {
    config: TeletextClockConfig,
    report: TeletextClockReport,
    previous: Option<(u64, u64)>,
    failure: Option<TeletextClockError>,
    initial_discontinuity_allowed: bool,
}

impl TeletextClockValidator {
    /// Start a source-bound clock check with an explicit policy.
    /// # Errors
    /// Rejects invalid PID/rate, zero bounds and ambiguous clock/tolerance bounds.
    pub fn new(config: TeletextClockConfig) -> Result<Self, TeletextClockError> {
        let invalid = [
            (!(0x20..=0x1ffe).contains(&config.pcr_pid), "pcr_pid"),
            (config.muxrate_bps == 0, "muxrate_bps"),
            (
                u128::from(config.rate_tolerance_ppm) >= PARTS_PER_MILLION,
                "rate_tolerance_ppm",
            ),
            (
                config.max_pcr_gap_ticks == 0 || config.max_pcr_gap_ticks >= PCR_MODULUS / 2,
                "max_pcr_gap_ticks",
            ),
            (
                config.pcr_jitter_ticks >= config.max_pcr_gap_ticks,
                "pcr_jitter_ticks",
            ),
            (config.max_pcr_gap_packets == 0, "max_pcr_gap_packets"),
            (config.min_pcr_intervals == 0, "min_pcr_intervals"),
            (
                !(1..=MAX_WINDOW_PACKETS).contains(&config.max_window_packets),
                "max_window_packets",
            ),
        ];
        for (is_invalid, field) in invalid {
            if is_invalid {
                return Err(TeletextClockError::Configuration { field });
            }
        }
        Ok(Self {
            config,
            report: TeletextClockReport::default(),
            previous: None,
            failure: None,
            initial_discontinuity_allowed: false,
        })
    }

    /// Admit a fresh source epoch whose first selected-PCR-PID packet may
    /// signal its initial discontinuity.
    ///
    /// Any first packet on that PID consumes the allowance, including payload
    /// or adaptation-only packets without PCR. Subsequent discontinuities are
    /// terminal. This neither resets the clock nor relaxes PCR syntax, gaps,
    /// ordering, declared rate or readiness; verified intervals are still required.
    /// # Errors
    /// Rejects the same invalid policies as [`Self::new`].
    pub fn new_source_epoch(config: TeletextClockConfig) -> Result<Self, TeletextClockError> {
        let mut validator = Self::new(config)?;
        validator.initial_discontinuity_allowed = true;
        Ok(validator)
    }

    /// Inspect new raw packets without modifying them or retaining their bytes.
    ///
    /// Window boundaries do not reset interval validation or PCR gap tracking.
    /// An empty window is a read-only readiness query. On error the owner must
    /// discard unreleased output; no partial window is admitted by this API.
    /// # Errors
    /// Any clock, rate, framing, capacity or arithmetic failure remains sticky.
    pub fn observe(
        &mut self,
        window: &[[u8; 188]],
    ) -> Result<TeletextClockReport, TeletextClockError> {
        self.report()?;
        let result = self.inspect(window);
        if let Err(error) = result {
            self.failure = Some(error);
        }
        result
    }

    /// Read current evidence without clearing failure or advancing the stream.
    /// # Errors
    /// Returns the first retained observation failure if this source is fenced.
    pub fn report(&self) -> Result<TeletextClockReport, TeletextClockError> {
        self.failure.map_or(Ok(self.report), Err)
    }

    fn inspect(&mut self, window: &[[u8; 188]]) -> Result<TeletextClockReport, TeletextClockError> {
        if window.len() > self.config.max_window_packets {
            return Err(TeletextClockError::Capacity);
        }
        for raw in window {
            let packet = valid_packet(raw).map_err(|_| TeletextClockError::Framing)?;
            let position = self
                .report
                .packets_seen
                .checked_add(1)
                .ok_or(TeletextClockError::Arithmetic)?;
            let previous_position = self.previous.map_or(0, |(position, _)| position);
            if position - previous_position > self.config.max_pcr_gap_packets {
                return Err(TeletextClockError::PcrGap);
            }
            if u16::from(packet.pid()) == self.config.pcr_pid {
                let initial = std::mem::replace(&mut self.initial_discontinuity_allowed, false);
                if let Some(field) = packet.adaptation_field() {
                    if field.discontinuity_indicator() && !initial {
                        return Err(TeletextClockError::Discontinuity);
                    }
                    match field.pcr() {
                        Ok(clock) => self.observe_clock(&packet, clock, position)?,
                        Err(AdaptationFieldError::FieldNotPresent) => (),
                        Err(_) => return Err(TeletextClockError::Pcr),
                    }
                }
            }
            self.report.packets_seen = position;
        }
        Ok(self.report)
    }

    fn observe_clock(
        &mut self,
        packet: &Packet<'_>,
        clock: ClockRef,
        position: u64,
    ) -> Result<(), TeletextClockError> {
        // The dependency decodes, but does not validate, these syntax fields.
        // PCR begins at byte 6, making byte 10 its base/reserved/extension byte.
        if clock.extension() >= 300 || packet.buffer()[10] & 0x7e != 0x7e {
            return Err(TeletextClockError::Pcr);
        }
        let ticks = u64::from(clock);
        if let Some((previous_position, previous_ticks)) = self.previous {
            let delta = (ticks + PCR_MODULUS - previous_ticks) % PCR_MODULUS;
            if delta == 0 || delta >= PCR_MODULUS / 2 {
                return Err(TeletextClockError::ClockOrder);
            }
            if delta > self.config.max_pcr_gap_ticks {
                return Err(TeletextClockError::PcrGap);
            }
            self.validate_rate(position - previous_position, delta)?;
            self.report.validated_intervals = self
                .report
                .validated_intervals
                .checked_add(1)
                .ok_or(TeletextClockError::Arithmetic)?;
        }
        self.report.pcr_samples = self
            .report
            .pcr_samples
            .checked_add(1)
            .ok_or(TeletextClockError::Arithmetic)?;
        self.previous = Some((position, ticks));
        self.report.ready =
            self.report.validated_intervals >= u64::from(self.config.min_pcr_intervals);
        Ok(())
    }

    fn validate_rate(&self, packets: u64, delta: u64) -> Result<(), TeletextClockError> {
        // Cross-multiply to retain sub-tick precision without floats/dividing
        // the measured rate. u128 covers realistic rates on all native targets.
        let expected = u128::from(packets)
            .checked_mul(188 * 8 * u128::from(PCR_HZ))
            .ok_or(TeletextClockError::Arithmetic)?;
        let actual = u128::from(delta)
            .checked_mul(u128::from(self.config.muxrate_bps))
            .ok_or(TeletextClockError::Arithmetic)?;
        // Measured bitrate is packet bits * PCR_HZ / delta. Its relative
        // deviation from the declared bitrate therefore uses delta * muxrate
        // as the denominator after cross-multiplication, not packet distance.
        let relative = actual
            .checked_mul(u128::from(self.config.rate_tolerance_ppm))
            .ok_or(TeletextClockError::Arithmetic)?
            / PARTS_PER_MILLION;
        let absolute = u128::from(self.config.pcr_jitter_ticks)
            .checked_mul(u128::from(self.config.muxrate_bps))
            .ok_or(TeletextClockError::Arithmetic)?;
        let allowed = relative
            .checked_add(absolute)
            .ok_or(TeletextClockError::Arithmetic)?;
        if expected.abs_diff(actual) > allowed {
            return Err(TeletextClockError::Muxrate);
        }
        Ok(())
    }
}

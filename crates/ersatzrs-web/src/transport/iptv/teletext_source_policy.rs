//! Explicit host limits qualified by final-command hints and actual source PSI.
//!
//! No programme/PCR identity, authored content or production defaults are held
//! here. Qualification is configuration validation, not source clock evidence,
//! accepted-generation authority or permission to publish installed packages.

use std::time::Duration;

use ersatzrs_ffmpeg::{
    teletext::{TeletextClockConfig, TeletextOutputConfig, TeletextPmtPlan},
    MpegTsOutputPolicy,
};
use tokio::time::Instant;

const PACKET_BYTES: usize = 188;
const PACKET_BITS: u128 = 1_504;
const PCR_HZ: u128 = 27_000_000;
const HALF_PCR_WRAP: u64 = ((1_u64 << 33) * 300) / 2;
const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_WINDOW_PACKETS: usize = 32_768;

/// Host-selected limits; no field is inferred from a source or fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSourcePolicyLimits {
    /// Proposed authored-data PID; discovery must prove it is unoccupied.
    pub elementary_pid: u16,
    /// Permitted relative declared-rate error, strictly below one million ppm.
    pub rate_tolerance_ppm: u32,
    /// Permitted absolute interval error in 27 MHz PCR ticks.
    pub pcr_jitter_ticks: u64,
    /// Additional permitted PCR time beyond the command's requested period.
    pub pcr_gap_slack_ticks: u64,
    /// Additional physical packet spacing beyond rounded declared-rate time.
    pub pcr_gap_slack_packets: u64,
    /// Required successive validated PCR intervals, at least one.
    pub min_pcr_intervals: u32,
    /// Per-observation packet bound, between one and 32,768 inclusive.
    pub max_window_packets: usize,
    /// Held raw transport capacity; at most 32 MiB of TS188 packets.
    pub max_buffer_packets: usize,
    /// Maximum original first-byte holding age.
    pub max_hold: Duration,
    /// Delay before physical packet delivery starts.
    pub playout_delay: Duration,
    /// Maximum sink lateness relative to each packet's due time.
    pub max_lateness: Duration,
    /// Existing source owner's deadline for its first output byte.
    pub first_byte_timeout: Duration,
    /// Bounded raw source prefix used for PSI discovery.
    pub probe_maximum_bytes: usize,
    /// Absolute elapsed-time allowance for source discovery.
    pub probe_maximum_time: Duration,
    /// Total journal bound through the first nonempty sink-write attempt.
    pub startup_maximum_bytes: usize,
}

/// Checked limits, without a source identity or any allocation/content copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSourcePolicy {
    limits: TeletextSourcePolicyLimits,
}

/// Exact command-derived rate and checked bounds awaiting the actual PSI plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextQualifiedSourcePolicy {
    policy: TeletextSourcePolicy,
    muxrate_bps: u64,
    max_pcr_gap_ticks: u64,
    max_pcr_gap_packets: u64,
}

/// Configuration failures contain neither authored nor source payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextSourcePolicyError {
    /// A named host limit cannot describe the bounded forwarding path.
    #[error("teletext source policy has invalid {0}")]
    Configuration(&'static str),
    /// A derived quantity or monotonic deadline cannot be represented.
    #[error("teletext source policy arithmetic exceeds its bound")]
    Arithmetic,
    /// Source discovery selected a different authored-data PID.
    #[error("teletext source plan does not reserve the selected authored PID")]
    ElementaryPidMismatch,
}

impl TeletextSourcePolicy {
    /// Validate source-independent limits before reading or allocating input.
    /// # Errors
    /// Rejects invalid PID, capacity, timing, observation or journal limits.
    pub fn new(limits: TeletextSourcePolicyLimits) -> Result<Self, TeletextSourcePolicyError> {
        let capacity = limits
            .max_buffer_packets
            .checked_mul(PACKET_BYTES)
            .ok_or(TeletextSourcePolicyError::Arithmetic)?;
        let invalid = [
            (
                !(0x20..=0x1ffe).contains(&limits.elementary_pid),
                "elementary_pid",
            ),
            (limits.rate_tolerance_ppm >= 1_000_000, "rate_tolerance_ppm"),
            (limits.min_pcr_intervals == 0, "min_pcr_intervals"),
            (
                !(1..=MAX_WINDOW_PACKETS).contains(&limits.max_window_packets),
                "max_window_packets",
            ),
            (capacity == 0 || capacity > MAX_BYTES, "max_buffer_packets"),
            (
                limits.max_hold <= limits.playout_delay || limits.playout_delay.is_zero(),
                "holding_delay",
            ),
            (limits.max_lateness >= limits.max_hold, "max_lateness"),
            (limits.first_byte_timeout.is_zero(), "first_byte_timeout"),
            (limits.probe_maximum_time.is_zero(), "probe_maximum_time"),
            (
                limits.probe_maximum_bytes == 0
                    || limits.probe_maximum_bytes > MAX_WINDOW_PACKETS * PACKET_BYTES,
                "probe_maximum_bytes",
            ),
            (
                limits.startup_maximum_bytes == 0
                    || limits.startup_maximum_bytes > capacity
                    || limits.probe_maximum_bytes > limits.startup_maximum_bytes,
                "startup_maximum_bytes",
            ),
        ];
        if let Some((_, field)) = invalid.into_iter().find(|&(invalid, _)| invalid) {
            return Err(TeletextSourcePolicyError::Configuration(field));
        }
        let now = Instant::now();
        for duration in [
            limits.max_hold,
            limits.playout_delay,
            limits.max_lateness,
            limits.first_byte_timeout,
            limits.probe_maximum_time,
        ] {
            if now.checked_add(duration).is_none() {
                return Err(TeletextSourcePolicyError::Arithmetic);
            }
        }
        Ok(Self { limits })
    }

    /// Return the already checked scalar limits without copying any content.
    #[must_use]
    pub const fn limits(self) -> TeletextSourcePolicyLimits {
        self.limits
    }

    /// Check the real final command before a source probe begins.
    ///
    /// The requested PCR period is not a guarantee of actual source spacing.
    /// Explicit owner slack defines independent maximum time and packet gaps;
    /// the forwarding gate must subsequently validate every actual PCR.
    /// # Errors
    /// Rejects arithmetic overflow, ambiguous PCR gaps, invalid jitter or a
    /// command rate whose carousel interval cannot fit the holding policy.
    pub fn qualify(
        self,
        command: &MpegTsOutputPolicy,
    ) -> Result<TeletextQualifiedSourcePolicy, TeletextSourcePolicyError> {
        let period_ticks = u64::from(command.pcr_period_ms())
            .checked_mul(27_000)
            .ok_or(TeletextSourcePolicyError::Arithmetic)?;
        let max_pcr_gap_ticks = period_ticks
            .checked_add(self.limits.pcr_gap_slack_ticks)
            .ok_or(TeletextSourcePolicyError::Arithmetic)?;
        if max_pcr_gap_ticks == 0 || max_pcr_gap_ticks >= HALF_PCR_WRAP {
            return Err(TeletextSourcePolicyError::Configuration(
                "max_pcr_gap_ticks",
            ));
        }
        if self.limits.pcr_jitter_ticks >= max_pcr_gap_ticks {
            return Err(TeletextSourcePolicyError::Configuration("pcr_jitter_ticks"));
        }
        let muxrate_bps = command.muxrate_bps();
        let nominal_packets = u128::from(max_pcr_gap_ticks)
            .checked_mul(u128::from(muxrate_bps))
            .ok_or(TeletextSourcePolicyError::Arithmetic)?
            .div_ceil(PCR_HZ * PACKET_BITS);
        let max_pcr_gap_packets = u64::try_from(nominal_packets)
            .map_err(|_| TeletextSourcePolicyError::Arithmetic)?
            .checked_add(self.limits.pcr_gap_slack_packets)
            .ok_or(TeletextSourcePolicyError::Arithmetic)?;
        let interval_packets = u128::from(muxrate_bps) / (50 * PACKET_BITS);
        if interval_packets == 0 || interval_packets + 1 > self.limits.max_buffer_packets as u128 {
            return Err(TeletextSourcePolicyError::Configuration(
                "carousel_interval",
            ));
        }
        let minimum_span = Duration::from_nanos(
            u64::try_from(interval_packets * PACKET_BITS * 1_000_000_000 / u128::from(muxrate_bps))
                .map_err(|_| TeletextSourcePolicyError::Arithmetic)?,
        );
        let minimum_hold = self
            .limits
            .playout_delay
            .checked_add(minimum_span)
            .ok_or(TeletextSourcePolicyError::Arithmetic)?;
        if self.limits.max_hold <= minimum_hold {
            return Err(TeletextSourcePolicyError::Configuration("max_hold"));
        }
        Ok(TeletextQualifiedSourcePolicy {
            policy: self,
            muxrate_bps,
            max_pcr_gap_ticks,
            max_pcr_gap_packets,
        })
    }
}

impl TeletextQualifiedSourcePolicy {
    /// Return scalar owner limits for bounded probe and startup forwarding.
    #[must_use]
    pub const fn limits(self) -> TeletextSourcePolicyLimits {
        self.policy.limits
    }

    /// Combine actual PSI identities with already checked command/host bounds.
    /// The plan alone does not prove whole-source PID absence or current clock.
    /// # Errors
    /// Rejects a plan that reserves a different authored-data PID.
    pub fn output_config(
        self,
        plan: &TeletextPmtPlan,
    ) -> Result<TeletextOutputConfig, TeletextSourcePolicyError> {
        let limits = self.policy.limits;
        if plan.elementary_pid() != limits.elementary_pid {
            return Err(TeletextSourcePolicyError::ElementaryPidMismatch);
        }
        Ok(TeletextOutputConfig {
            program: plan.program_number(),
            elementary_pid: limits.elementary_pid,
            clock: TeletextClockConfig {
                pcr_pid: plan.pcr_pid(),
                muxrate_bps: self.muxrate_bps,
                rate_tolerance_ppm: limits.rate_tolerance_ppm,
                pcr_jitter_ticks: limits.pcr_jitter_ticks,
                max_pcr_gap_packets: self.max_pcr_gap_packets,
                max_pcr_gap_ticks: self.max_pcr_gap_ticks,
                min_pcr_intervals: limits.min_pcr_intervals,
                max_window_packets: limits.max_window_packets,
            },
            max_buffer_packets: limits.max_buffer_packets,
            max_hold: limits.max_hold,
            playout_delay: limits.playout_delay,
            max_lateness: limits.max_lateness,
        })
    }
}

#[cfg(test)]
#[path = "teletext_source_policy_tests.rs"]
mod tests;

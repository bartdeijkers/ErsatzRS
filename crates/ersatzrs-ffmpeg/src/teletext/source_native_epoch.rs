//! Pure `FFmpeg` 9 program timestamp-wrap policy, without source qualification.
//!
//! Mirrors n9.0 libavformat/demux.c:48–60 and 441–517. The caller must prove
//! actual first demux-packet identity/order, complete program memberships and
//! inherited state. Neither this transform nor a nearest AV/PCR match provides
//! that evidence. Native format origin and requested cue coverage are separate.

const TIMESTAMP_PERIOD: i64 = 1_i64 << 33;
const PCR_PERIOD: u64 = (1_u64 << 33) * 300;
const SIXTY_SECONDS: i64 = 60 * 90_000;

/// Native timestamp transformation selected by `FFmpeg`'s first-reference rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceNativeWrapBehavior {
    /// Add one 33-bit period to timestamps strictly below the reference.
    AddOffset,
    /// Subtract one period from timestamps at or above the reference.
    SubtractOffset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WrapRule {
    reference: i64,
    behavior: SourceNativeWrapBehavior,
}

/// A pure candidate native program policy; unknown remains explicit.
///
/// Constructing this value grants no source association or native-state
/// authority. Program initialization happens in demux delivery order, which is
/// not necessarily physical PES-start order. Shared-program inheritance and
/// propagation must be driven by the caller's actual native/PSI observations.
/// This fixed 33-bit, 90 kHz policy models MPEG-TS, not arbitrary stream time bases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceNativeEpoch {
    rule: Option<WrapRule>,
}

/// Static numerical failures; supplied timestamps and source bytes are omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceNativeEpochError {
    /// Supplied raw PCR is outside base * 300 + extension modulo 2^33 * 300.
    #[error("native source epoch raw PCR exceeds its numeric range")]
    PcrRange,
    /// A checked timestamp transformation cannot be represented.
    #[error("native source epoch arithmetic exceeds its bound")]
    Arithmetic,
}

impl SourceNativeEpoch {
    /// No timestamped native initializer or inherited program state is known.
    #[must_use]
    pub const fn unknown() -> Self {
        Self { rule: None }
    }

    /// Compute the proposed rule from the actual first demux timestamps.
    ///
    /// DTS takes precedence, falling back to PTS. `None` and `FFmpeg`'s
    /// `AV_NOPTS_VALUE` (`i64::MIN`) represent missing timestamps. Only the
    /// initializer reference is masked to 33 bits, exactly as n9.0 does.
    /// For a known inherited program rule, resolve that state before use.
    #[must_use]
    pub fn from_first_demux_timestamps(pts: Option<i64>, dts: Option<i64>) -> Self {
        let Some(first) = present(dts).or_else(|| present(pts)) else {
            return Self::unknown();
        };
        let first = first & (TIMESTAMP_PERIOD - 1);
        let behavior = if first < TIMESTAMP_PERIOD - (TIMESTAMP_PERIOD >> 3)
            || first < TIMESTAMP_PERIOD - SIXTY_SECONDS
        {
            SourceNativeWrapBehavior::AddOffset
        } else {
            SourceNativeWrapBehavior::SubtractOffset
        };
        Self {
            rule: Some(WrapRule {
                reference: first - SIXTY_SECONDS,
                behavior,
            }),
        }
    }

    /// Resolve an initializing stream against all its program memberships in
    /// native order: the first known program rule wins, otherwise retain the
    /// proposal. An unknown initializer stays unknown, matching the early return
    /// before inheritance in n9.0. The caller must propagate the returned rule
    /// to every membership and its streams, including shared-program members.
    /// This method neither discovers memberships nor owns a program registry.
    #[must_use]
    pub fn resolve_program_memberships(self, memberships: &[Self]) -> Self {
        if self.rule.is_none() {
            return self;
        }
        memberships
            .iter()
            .copied()
            .find(|state| state.rule.is_some())
            .unwrap_or(self)
    }

    /// Apply n9.0's separate no-program branch: a known default-stream rule
    /// overrides the proposal. With an unknown default, the caller propagates
    /// the proposal to streams outside programs only. An unknown initializer
    /// does not invent or inherit state before a timestamp is observed.
    #[must_use]
    pub fn resolve_default_stream(self, default_stream: Self) -> Self {
        if self.rule.is_some() && default_stream.rule.is_some() {
            default_stream
        } else {
            self
        }
    }

    /// Exact signed 90 kHz reference; absence means the policy is unknown.
    #[must_use]
    pub fn reference_90khz(self) -> Option<i64> {
        self.rule.map(|rule| rule.reference)
    }

    /// Explicit native branch; an unknown rule has no guessed disposition.
    #[must_use]
    pub fn behavior(self) -> Option<SourceNativeWrapBehavior> {
        self.rule.map(|rule| rule.behavior)
    }

    /// Apply the native rule once to a supplied signed 90 kHz timestamp.
    ///
    /// Input is not masked: demux-produced or already signed timestamps may
    /// extend beyond the raw 33-bit range. Missing/unknown returns `None`.
    /// The result is a pure transformation, not a certified unwrapped epoch.
    /// # Errors
    /// Rejects arithmetic overflow instead of wrapping a signed result.
    pub fn apply_timestamp(self, timestamp: i64) -> Result<Option<i64>, SourceNativeEpochError> {
        let Some(rule) = self.rule else {
            return Ok(None);
        };
        if timestamp == i64::MIN {
            return Ok(None);
        }
        let transformed = match rule.behavior {
            SourceNativeWrapBehavior::AddOffset if timestamp < rule.reference => {
                timestamp.checked_add(TIMESTAMP_PERIOD)
            }
            SourceNativeWrapBehavior::SubtractOffset if timestamp >= rule.reference => {
                timestamp.checked_sub(TIMESTAMP_PERIOD)
            }
            _ => Some(timestamp),
        };
        transformed
            .map(Some)
            .ok_or(SourceNativeEpochError::Arithmetic)
    }

    /// Apply the same candidate rule to a raw PCR base, retaining its extension.
    ///
    /// This defines a candidate native-program PCR coordinate. The caller must
    /// still establish that the rule is the actual selected program's state,
    /// verify continuous PCR gaps/wraps, and check any constant epoch offset
    /// against the held-source walk. No media origin or cue admission follows.
    /// # Errors
    /// Rejects a non-PCR-range value or unrepresentable transformed ticks.
    pub fn apply_pcr_ticks(self, raw_ticks: u64) -> Result<Option<i64>, SourceNativeEpochError> {
        if raw_ticks >= PCR_PERIOD {
            return Err(SourceNativeEpochError::PcrRange);
        }
        let base =
            i64::try_from(raw_ticks / 300).map_err(|_| SourceNativeEpochError::Arithmetic)?;
        let extension =
            i64::try_from(raw_ticks % 300).map_err(|_| SourceNativeEpochError::Arithmetic)?;
        self.apply_timestamp(base)?
            .map(|base| {
                base.checked_mul(300)
                    .and_then(|ticks| ticks.checked_add(extension))
                    .ok_or(SourceNativeEpochError::Arithmetic)
            })
            .transpose()
    }
}

fn present(timestamp: Option<i64>) -> Option<i64> {
    timestamp.filter(|timestamp| *timestamp != i64::MIN)
}

#[cfg(test)]
#[path = "source_native_epoch_tests.rs"]
mod tests;

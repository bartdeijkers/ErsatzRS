//! Join native start observations to actual raw AV headers, without cue admission.

use ersatzrs_ffmpeg::teletext::{
    source_av_timestamps::{
        SourceAvTimestampObserver, SourceAvTimestampReport, SourceAvTimestampSample,
    },
    source_psi::ObservedSourcePsi,
};

use super::SourceTeletextContinuousClockEvidence;
use crate::ffprobe::{
    teletext_origin::{NativeAvOrigin, NativeSourceOrigin},
    FfprobeError, Result,
};

/// Native AV observations and the actual header samples from the held byte walk.
#[derive(Debug, Clone)]
pub struct SourceTeletextAvOriginWitness {
    native: NativeAvOrigin,
    raw: SourceAvTimestampReport,
    matched_native_start: Option<SourceAvTimestampSample>,
    native_start_near_format_origin: Option<bool>,
}

impl SourceTeletextAvOriginWitness {
    /// Exact native global index/PID/role and optional start timing.
    #[must_use]
    pub fn native(&self) -> &NativeAvOrigin {
        &self.native
    }

    /// Raw wire-order PTS/DTS; reordered video PTS is preserved.
    #[must_use]
    pub const fn raw(&self) -> SourceAvTimestampReport {
        self.raw
    }

    /// First actual PTS equal to the native start after exact 90 kHz rescaling
    /// and modulo conversion. This does not certify matching unwrapped epochs.
    #[must_use]
    pub const fn matched_native_start(&self) -> Option<SourceAvTimestampSample> {
        self.matched_native_start
    }

    /// Whether native start is within one printed format-time quantum. This is
    /// an explicit conservative enclosure, not proof that this track is origin.
    #[must_use]
    pub const fn native_start_near_format_origin(&self) -> Option<bool> {
        self.native_start_near_format_origin
    }
}

/// Source-associated native/raw observations, retaining all AV tracks.
///
/// Missing starts remain unknown. No native stream is picked as media origin;
/// bounded gaps, raw clock epoch agreement and actual cue coverage remain open.
#[derive(Debug, Clone)]
pub struct SourceTeletextAvOriginEvidence {
    clock: SourceTeletextContinuousClockEvidence,
    native: NativeSourceOrigin,
    streams: Vec<SourceTeletextAvOriginWitness>,
}

impl SourceTeletextAvOriginEvidence {
    /// Continuous held-source byte/PSI/PCR observations and decoder-span match.
    #[must_use]
    pub const fn clock(&self) -> SourceTeletextContinuousClockEvidence {
        self.clock
    }

    /// Native format and every observed AV start, without selecting an origin.
    #[must_use]
    pub fn native(&self) -> &NativeSourceOrigin {
        &self.native
    }

    /// Native stream/PID matched to its actual raw header history.
    #[must_use]
    pub fn streams(&self) -> &[SourceTeletextAvOriginWitness] {
        &self.streams
    }
}

pub(super) struct AvOriginCollector {
    native: NativeSourceOrigin,
    tracks: Vec<(
        SourceAvTimestampObserver,
        Option<u64>,
        Option<SourceAvTimestampSample>,
    )>,
}

impl AvOriginCollector {
    pub(super) fn new(native: NativeSourceOrigin) -> Result<Self> {
        let tracks = native
            .streams()
            .iter()
            .map(|stream| {
                let observer = SourceAvTimestampObserver::new_source_epoch(stream.identity(), 0)
                    .map_err(|_| invalid())?;
                Ok((observer, expected_raw_start(stream), None))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { native, tracks })
    }

    pub(super) fn push(&mut self, raw: &[u8; 188]) -> Result<()> {
        for (observer, expected, matched) in &mut self.tracks {
            if let Some(header) = observer.push(raw).map_err(|_| invalid())? {
                if matched.is_none() && header.pts.is_some() && header.pts == *expected {
                    *matched = header.pts.map(|ticks| SourceAvTimestampSample {
                        source_pusi_packet: header.source_pusi_packet,
                        header_completion_packet: header.header_completion_packet,
                        raw_ticks_90khz: ticks,
                    });
                }
            }
        }
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        clock: SourceTeletextContinuousClockEvidence,
        psi: &ObservedSourcePsi,
    ) -> Result<SourceTeletextAvOriginEvidence> {
        let mut streams = Vec::with_capacity(self.tracks.len());
        for (native, (observer, _, matched_native_start)) in
            self.native.streams().iter().zip(self.tracks.iter_mut())
        {
            if !psi.programs.iter().any(|program| {
                program
                    .streams
                    .iter()
                    .any(|stream| stream.pid == native.identity().pid)
            }) {
                return Err(invalid());
            }
            let raw = observer.finish().map_err(|_| invalid())?;
            streams.push(SourceTeletextAvOriginWitness {
                native: *native,
                raw,
                matched_native_start: *matched_native_start,
                native_start_near_format_origin: native
                    .start_pts()
                    .zip(native.time_base())
                    .zip(self.native.format_start())
                    .and_then(|((pts, base), format)| {
                        let numerator =
                            i128::from(pts).checked_mul(i128::from(base.numerator()))?;
                        format.contains_rational_within_quantum(
                            numerator,
                            u64::try_from(base.denominator()).ok()?,
                        )
                    }),
            });
        }
        Ok(SourceTeletextAvOriginEvidence {
            clock,
            native: self.native,
            streams,
        })
    }
}

fn expected_raw_start(stream: &NativeAvOrigin) -> Option<u64> {
    let pts = i128::from(stream.start_pts()?);
    let base = stream.time_base()?;
    let numerator = pts
        .checked_mul(i128::from(base.numerator()))?
        .checked_mul(90_000)?;
    let denominator = i128::from(base.denominator());
    if numerator % denominator != 0 {
        return None;
    }
    u64::try_from((numerator / denominator).rem_euclid(1_i128 << 33)).ok()
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput {
        path: "source teletext decode window".into(),
    }
}

//! Continuous held-source clock evidence; never a media-origin or cue grant.

use std::{path::PathBuf, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_ffmpeg::teletext::{
    source_clock::{SourceTeletextClockObserver, SourceTeletextClockReport},
    source_observation::MAX_SOURCE_OBSERVATION_PACKETS,
    source_psi::{ObservedSourcePsi, SourceTeletextPsiObserver},
};
use sha2::{Digest as _, Sha256};
use tokio::time::Instant;

use super::{
    headers,
    source_av_origin::{AvOriginCollector, SourceTeletextAvOriginEvidence},
    source_cue::{CueCollector, SourceTeletextCueEvidence, SourceTeletextCueRequest},
    source_native_clock::{NativeClockCollector, SourceTeletextNativeClockEvidence},
    source_packet, source_window, LocalTeletextSourceGuard, SourceTeletextDecodeWindowEvidence,
    SourceTeletextPacketFormat,
};
use crate::ffprobe::{
    teletext_decode::SourceTeletextPageDecodeRequest, teletext_demux::NativeSourceDemuxTrace,
    teletext_origin::NativeSourceOrigin, FfprobeError, Result,
};

#[derive(Clone, Copy)]
pub(super) struct SourceSelection {
    pub(super) pid: u16,
    pub(super) program: u16,
    pub(super) pmt: u16,
    pub(super) pcr: u16,
}

struct SourceWalkRequest {
    selection: SourceSelection,
    expected: Option<SourceTeletextDecodeWindowEvidence>,
    origin: Option<NativeSourceOrigin>,
    native_clock: Option<NativeClockCollector>,
    cue: Option<CueCollector>,
}

impl SourceWalkRequest {
    fn decoded(
        pid: u16,
        expected: SourceTeletextDecodeWindowEvidence,
        origin: Option<NativeSourceOrigin>,
    ) -> Self {
        Self {
            selection: SourceSelection {
                pid,
                program: expected.program_number,
                pmt: expected.pmt_pid,
                pcr: expected.pcr_pid,
            },
            expected: Some(expected),
            origin,
            native_clock: None,
            cue: None,
        }
    }
}

/// Raw clock/PSI evidence from every physical packet of one held source.
///
/// This retains no source bytes and grants no playback interval, subtitle
/// inventory, AV origin, presentation EOF or default decoder timestamp mapping.
/// Only the owned reader can construct it; missing PCR stays explicitly absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextContinuousClockEvidence {
    clock: SourceTeletextClockReport,
    source_sha256: [u8; 32],
    first_current_psi_packet: u64,
    format: SourceTeletextPacketFormat,
    source_byte_length: u64,
}

impl SourceTeletextContinuousClockEvidence {
    #[must_use]
    pub const fn format(&self) -> SourceTeletextPacketFormat {
        self.format
    }

    /// Actual held source byte length, including opaque transport prefixes.
    #[must_use]
    pub const fn source_byte_length(&self) -> u64 {
        self.source_byte_length
    }

    /// Physical source ordinals and unwrapped raw PCR, not media-relative time.
    #[must_use]
    pub const fn clock(&self) -> SourceTeletextClockReport {
        self.clock
    }

    /// Hash of all held source bytes, including packets after the final PCR.
    #[must_use]
    pub const fn source_sha256(&self) -> [u8; 32] {
        self.source_sha256
    }

    /// First complete current PAT/PMT observation, without inventing earlier PSI.
    #[must_use]
    pub const fn first_current_psi_packet(&self) -> u64 {
        self.first_current_psi_packet
    }
}

/// Reuse actual worker ownership and deadline for a constant-memory raw walk.
pub(crate) async fn observe_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    pid: u16,
    expected: SourceTeletextDecodeWindowEvidence,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<SourceTeletextContinuousClockEvidence> {
    walk_owned(
        guard,
        paths,
        SourceWalkRequest::decoded(pid, expected, None),
        deadline,
        hold,
    )
    .await
    .map(|observed| observed.clock)
}

pub(crate) async fn observe_av_origin_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    pid: u16,
    expected: SourceTeletextDecodeWindowEvidence,
    origin: NativeSourceOrigin,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<SourceTeletextAvOriginEvidence> {
    walk_owned(
        guard,
        paths,
        SourceWalkRequest::decoded(pid, expected, Some(origin)),
        deadline,
        hold,
    )
    .await?
    .av
    .ok_or_else(invalid)
}

pub(crate) async fn observe_native_clock_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    observed: (
        SourceTeletextPageDecodeRequest,
        NativeSourceOrigin,
        NativeSourceDemuxTrace,
    ),
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<Option<SourceTeletextNativeClockEvidence>> {
    let Some(request) = native_request(observed, None) else {
        return Ok(None);
    };
    walk_owned(guard, paths, request, deadline, hold)
        .await
        .map(|observed| observed.native_clock)
}

pub(crate) async fn observe_native_cue_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    observed: (
        SourceTeletextPageDecodeRequest,
        NativeSourceOrigin,
        NativeSourceDemuxTrace,
    ),
    cue: SourceTeletextCueRequest,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<Option<SourceTeletextCueEvidence>> {
    let Some(request) = native_request(observed, Some(cue)) else {
        return Ok(None);
    };
    walk_owned(guard, paths, request, deadline, hold)
        .await
        .map(|observed| observed.cue)
}

fn native_request(
    observed: (
        SourceTeletextPageDecodeRequest,
        NativeSourceOrigin,
        NativeSourceDemuxTrace,
    ),
    cue: Option<SourceTeletextCueRequest>,
) -> Option<SourceWalkRequest> {
    let (request, origin, trace) = observed;
    let native_clock = NativeClockCollector::new(&request, &origin, trace, None)?;
    let cue = if let Some(cue) = cue {
        Some(
            CueCollector::new(cue, &origin, native_clock.epoch())?
                .with_page(&request, native_clock.selection().pmt)?,
        )
    } else {
        None
    };
    Some(SourceWalkRequest {
        selection: native_clock.selection(),
        expected: None,
        origin: Some(origin),
        native_clock: Some(native_clock),
        cue,
    })
}

struct SourceWalkEvidence {
    clock: SourceTeletextContinuousClockEvidence,
    av: Option<SourceTeletextAvOriginEvidence>,
    native_clock: Option<SourceTeletextNativeClockEvidence>,
    cue: Option<SourceTeletextCueEvidence>,
}

async fn walk_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    request: SourceWalkRequest,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<SourceWalkEvidence> {
    headers::run_source_guard_owned(deadline, hold, move |stop| {
        stop.check_source_guard()?;
        super::check_source_guard_paths(&guard, &paths.0, &paths.1)?;
        let evidence = observe(&guard, request, stop)?;
        stop.check_source_guard()?;
        super::check_source_guard_paths(&guard, &paths.0, &paths.1)?;
        stop.check_source_guard()?;
        Ok(evidence)
    })
    .await
    .map_err(source_window::window_error)
}

fn observe<T>(
    guard: &LocalTeletextSourceGuard,
    request: SourceWalkRequest,
    stop: &headers::ReadStop<'_, T>,
) -> Result<SourceWalkEvidence> {
    let SourceWalkRequest {
        selection,
        expected,
        origin,
        mut native_clock,
        mut cue,
    } = request;
    let len = guard.guard.len;
    let format = source_packet::detect(guard.guard.original.as_file(), len, stop)?;
    let stride = format.packet_bytes() as u64;
    if len == 0 || !len.is_multiple_of(stride) || expected.is_some_and(|e| e.format != format) {
        return Err(invalid());
    }
    if let Some(collector) = &mut native_clock {
        collector.bind_format(format)?;
    }
    let mut psi = SourceTeletextPsiObserver::new(selection.pid).map_err(|_| invalid())?;
    let mut clock =
        SourceTeletextClockObserver::new_source_epoch(selection.pcr, 0).map_err(|_| invalid())?;
    let mut digest = Sha256::new();
    let span = expected
        .map(|expected| decoded_span(expected, len))
        .transpose()?;
    let mut decoder_digest = Sha256::new();
    let mut skipped_prefix = 0_u32;
    let mut retained_packets = 0_u32;
    let mut first_current_psi_packet = None;
    let mut av = origin.map(AvOriginCollector::new).transpose()?;
    let mut reader =
        source_packet::SourcePacketReader::new(guard.guard.original.as_file(), len, format);
    for ordinal in 0..len / stride {
        stop.check_source_guard()?;
        if ordinal > 0 && ordinal.is_multiple_of(MAX_SOURCE_OBSERVATION_PACKETS as u64) {
            psi.begin_continuation_window().map_err(|_| invalid())?;
        }
        let frame = reader.read_next(stop)?;
        let raw = &frame.packet;
        psi.push(raw).map_err(|_| invalid())?;
        let report = clock.push(raw).map_err(|_| invalid())?;
        if let Some(cue) = &mut cue {
            cue.observe_packet(raw, ordinal, report)?;
        }
        if let Some(av) = &mut av {
            av.push(raw)?;
        }
        if let Some(native_clock) = &mut native_clock {
            native_clock.push(raw)?;
        }
        for part in frame.parts() {
            digest.update(part);
        }
        if let Some((expected, first_packet, retained_end)) = span {
            if ordinal >= first_packet && ordinal < retained_end {
                let raw_pid = (u16::from(raw[1] & 31) << 8) | u16::from(raw[2]);
                if raw_pid == selection.pid
                    && skipped_prefix < expected.skipped_selected_prefix_packets
                {
                    skipped_prefix += 1;
                } else {
                    for part in frame.parts() {
                        decoder_digest.update(part);
                    }
                    retained_packets += 1;
                }
            }
        }
        if first_current_psi_packet.is_none() && psi.has_complete_current_tables() {
            validate_program(&psi, selection)?;
            first_current_psi_packet = Some(ordinal);
        }
    }
    stop.check_source_guard()?;
    validate_program(&psi, selection)?;
    let observed = psi.observation().map_err(|_| invalid())?;
    if observed.current_tables_changed {
        return Err(invalid());
    }
    if let Some((expected, _, _)) = span {
        let decoder_sha256: [u8; 32] = decoder_digest.finalize().into();
        if headers::fingerprint(&observed) != expected.psi_fingerprint
            || retained_packets != expected.decoder_packets
            || skipped_prefix != expected.skipped_selected_prefix_packets
            || decoder_sha256 != expected.decoder_sha256
        {
            return Err(invalid());
        }
    }
    let clock = SourceTeletextContinuousClockEvidence {
        clock: clock.report().map_err(|_| invalid())?,
        source_sha256: digest.finalize().into(),
        first_current_psi_packet: first_current_psi_packet.ok_or_else(invalid)?,
        format,
        source_byte_length: len,
    };
    finish_observations(clock, av, native_clock, cue, &observed)
}

fn finish_observations(
    clock: SourceTeletextContinuousClockEvidence,
    av: Option<AvOriginCollector>,
    native_clock: Option<NativeClockCollector>,
    cue: Option<CueCollector>,
    observed: &ObservedSourcePsi,
) -> Result<SourceWalkEvidence> {
    let mut av = av
        .map(|collector| collector.finish(clock, observed))
        .transpose()?;
    let mut native_clock = native_clock
        .map(|collector| collector.finish(av.take().ok_or_else(invalid)?, observed))
        .transpose()?
        .flatten();
    let cue = if let Some(collector) = cue {
        native_clock
            .take()
            .map(|clock| collector.finish(clock))
            .transpose()?
            .flatten()
    } else {
        None
    };
    Ok(SourceWalkEvidence {
        clock,
        av,
        native_clock,
        cue,
    })
}

fn decoded_span(
    expected: SourceTeletextDecodeWindowEvidence,
    source_len: u64,
) -> Result<(SourceTeletextDecodeWindowEvidence, u64, u64)> {
    let end = expected
        .first_source_packet
        .checked_add(u64::from(expected.source_packets))
        .ok_or_else(invalid)?;
    let byte_end = end
        .checked_mul(expected.format.packet_bytes() as u64)
        .ok_or_else(invalid)?;
    // An aggregate larger than the discovery limit is valid only for a complete
    // unomitted source receipt; partial windows retain their existing bound.
    if expected.source_packets == 0
        || byte_end > source_len
        || expected.ended_at_eof != (byte_end == source_len)
        || (expected.source_packets as usize > MAX_SOURCE_OBSERVATION_PACKETS
            && !expected.covers_entire_source_bytes())
        || u64::from(expected.decoder_packets)
            + u64::from(expected.skipped_selected_prefix_packets)
            + u64::from(expected.trimmed_source_tail_packets)
            != u64::from(expected.source_packets)
    {
        return Err(invalid());
    }
    let retained_end = end
        .checked_sub(u64::from(expected.trimmed_source_tail_packets))
        .ok_or_else(invalid)?;
    Ok((expected, expected.first_source_packet, retained_end))
}

fn validate_program(psi: &SourceTeletextPsiObserver, selection: SourceSelection) -> Result<()> {
    let observed = psi.observation().map_err(|_| invalid())?;
    let mut selected = observed
        .memberships()
        .filter(|(program, _)| program.program_number == selection.program);
    let (program, stream) = selected.next().ok_or_else(invalid)?;
    if selected.next().is_some()
        || stream.stream_type != 6
        || program.pmt_pid != selection.pmt
        || program.pcr_pid != selection.pcr
    {
        return Err(invalid());
    }
    Ok(())
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput {
        path: "source teletext decode window".into(),
    }
}

#[cfg(test)]
mod tests;

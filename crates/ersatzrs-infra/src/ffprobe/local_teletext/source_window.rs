//! Same-byte bounded source preparation, without page or temporal readiness.

use std::{io::Write as _, path::PathBuf, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_ffmpeg::teletext::{
    source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyError, TeletextAssemblyEvent},
    source_observation::{
        SuppliedTeletextHeaderObserver, SuppliedTeletextHeaderWindow,
        MAX_SOURCE_OBSERVATION_PACKETS,
    },
    source_psi::{ObservedSourcePsi, SourceTeletextPsiObserver},
};
use sha2::{Digest as _, Sha256};
use tokio::time::Instant;

use super::{headers, LocalTeletextSourceGuard};
use crate::ffprobe::{FfprobeError, Result};

const LABEL: &str = "source teletext decode window";
const PACKET_BYTES: usize = 188;

/// A bounded physical TS188 range, without source ownership or time inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextPacketSpan {
    first_packet: u64,
    packet_count: u32,
}

/// Static structural failures; no source bytes, offsets or paths are retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SourceTeletextPacketSpanError {
    #[error("source teletext packet span must contain 1..=32768 packets")]
    Count,
    #[error("source teletext packet span overflows its byte positions")]
    Offset,
}

impl SourceTeletextPacketSpan {
    /// Check the per-window count and all exclusive-end byte arithmetic.
    /// Source length, transport and temporal coverage are checked separately.
    /// # Errors
    /// Rejects zero/excessive counts or unrepresentable byte positions.
    pub fn new(
        first_packet: u64,
        packet_count: u32,
    ) -> std::result::Result<Self, SourceTeletextPacketSpanError> {
        if packet_count == 0 || u64::from(packet_count) > MAX_SOURCE_OBSERVATION_PACKETS as u64 {
            return Err(SourceTeletextPacketSpanError::Count);
        }
        first_packet
            .checked_add(u64::from(packet_count))
            .and_then(|end| end.checked_mul(PACKET_BYTES as u64))
            .ok_or(SourceTeletextPacketSpanError::Offset)?;
        Ok(Self {
            first_packet,
            packet_count,
        })
    }

    /// Absolute zero-based ordinal of the first physically inspected packet.
    #[must_use]
    pub const fn first_packet(self) -> u64 {
        self.first_packet
    }

    /// Number of physically inspected packets, before decoder filtering.
    #[must_use]
    pub const fn packet_count(self) -> u32 {
        self.packet_count
    }

    /// Exclusive source packet end, checked during construction.
    #[must_use]
    pub fn end_packet_exclusive(self) -> u64 {
        self.first_packet + u64::from(self.packet_count)
    }

    /// Absolute byte offset of the first inspected packet on the held file.
    #[must_use]
    pub fn byte_offset(self) -> u64 {
        self.first_packet * PACKET_BYTES as u64
    }

    /// Exclusive byte end of the inspected span, before decoder filtering.
    #[must_use]
    pub fn byte_end_exclusive(self) -> u64 {
        self.end_packet_exclusive() * PACKET_BYTES as u64
    }
}

/// Evidence for the exact completed temporary bytes supplied to one decoder.
///
/// Selected payload is retained only after complete current PSI; current table
/// changes anywhere in the captured source are rejected. Selected-PID packets
/// preceding its first post-PSI PES start are omitted. A capped source may also
/// omit an incomplete selected-PES tail after the last completed selected PES.
/// Complete nonselected tails are retained. Neither omission proves
/// absent pages or a complete inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextDecodeWindowEvidence {
    /// Absolute packet ordinal on the original held file, never decoder offset.
    pub first_source_packet: u64,
    /// Physically inspected range is `[first_source_packet, first+source_packets)`.
    /// Its byte range is exactly those checked ordinals multiplied by 188.
    pub source_packets: u32,
    /// Complete packets retained in the private decoder snapshot.
    pub decoder_packets: u32,
    /// Selected-PID packets omitted before the first post-current-PSI PES start.
    pub skipped_selected_prefix_packets: u32,
    /// All packets removed after the final complete selected PES when an
    /// incomplete selected PES reaches a non-EOF window end. Source count
    /// equals decoder+skipped prefix+trimmed tail.
    pub trimmed_source_tail_packets: u32,
    /// The INSPECTED span's byte end equals the held source length. Nonzero
    /// `first_source_packet` still excludes earlier bytes; this is not whole-file
    /// coverage, and decoder filtering may omit additional inspected packets.
    pub ended_at_eof: bool,
    pub decoder_sha256: [u8; 32],
    pub psi_fingerprint: [u8; 32],
    pub program_number: u16,
    pub pmt_pid: u16,
    pub pcr_pid: u16,
}

impl SourceTeletextDecodeWindowEvidence {
    /// Whether this captured decoder snapshot retained every held source byte.
    /// This is byte coverage only: it establishes neither a media-time interval
    /// nor page completeness, later source freshness or playback readiness.
    #[must_use]
    pub const fn covers_entire_source_bytes(&self) -> bool {
        self.first_source_packet == 0
            && self.ended_at_eof
            && self.skipped_selected_prefix_packets == 0
            && self.trimmed_source_tail_packets == 0
            && self.source_packets == self.decoder_packets
    }
}

/// Own the completed snapshot until the actual decoder and its readers exit.
pub(crate) struct LocalTeletextSourceWindow {
    pub(crate) snapshot: tempfile::NamedTempFile,
    pub(crate) evidence: SourceTeletextDecodeWindowEvidence,
    pub(crate) headers: Option<SuppliedTeletextHeaderWindow>,
}

impl std::fmt::Debug for LocalTeletextSourceWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalTeletextSourceWindow")
            .field("evidence", &self.evidence)
            .finish_non_exhaustive()
    }
}

/// Read the original held source and publish a completed decoder snapshot under
/// the existing blocking owner. No cleanup scope means no read is launched.
pub(crate) async fn capture_source_window_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    original: PathBuf,
    resolved: PathBuf,
    pid: u16,
    program: u16,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<LocalTeletextSourceWindow> {
    capture_owned(
        guard,
        (original, resolved),
        pid,
        program,
        (None, false),
        deadline,
        hold,
    )
    .await
}

/// Retain page headers from the same complete bounded snapshot supplied to text decoding.
pub(crate) async fn capture_observed_source_window_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    original: PathBuf,
    resolved: PathBuf,
    pid: u16,
    program: u16,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<LocalTeletextSourceWindow> {
    capture_owned(
        guard,
        (original, resolved),
        pid,
        program,
        (None, true),
        deadline,
        hold,
    )
    .await
}

/// Inspect only the supplied physical range on the original held file. Earlier
/// bytes supply no PSI, clock or page authority. A span must contain its own
/// actual current PAT/PMT and complete selected PES; no tables are manufactured.
pub(crate) async fn capture_source_packet_span_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    pid: u16,
    program: u16,
    span: SourceTeletextPacketSpan,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<LocalTeletextSourceWindow> {
    capture_owned(
        guard,
        paths,
        pid,
        program,
        (Some(span), false),
        deadline,
        hold,
    )
    .await
}

/// Retain actual headers from the exact decoder bytes, before any bitmap work.
pub(crate) async fn capture_observed_source_packet_span_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    pid: u16,
    program: u16,
    span: SourceTeletextPacketSpan,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<LocalTeletextSourceWindow> {
    capture_owned(
        guard,
        paths,
        pid,
        program,
        (Some(span), true),
        deadline,
        hold,
    )
    .await
}

async fn capture_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    pid: u16,
    program: u16,
    window: (Option<SourceTeletextPacketSpan>, bool),
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<LocalTeletextSourceWindow> {
    headers::run_source_guard_owned(deadline, hold, move |stop| {
        stop.check_source_guard()?;
        let (original, resolved) = paths;
        super::check_source_guard_paths(&guard, &original, &resolved)?;
        let span = checked_span(guard.guard.len, window.0)?;
        let captured = capture(&guard, pid, program, span, window.1, stop)?;
        stop.check_source_guard()?;
        super::check_source_guard_paths(&guard, &original, &resolved)?;
        stop.check_source_guard()?;
        Ok(captured)
    })
    .await
    .map_err(window_error)
}

fn checked_span(
    len: u64,
    requested: Option<SourceTeletextPacketSpan>,
) -> Result<SourceTeletextPacketSpan> {
    if let Some(span) = requested {
        return (span.byte_end_exclusive() <= len)
            .then_some(span)
            .ok_or_else(invalid);
    }
    let maximum_bytes = (MAX_SOURCE_OBSERVATION_PACKETS * PACKET_BYTES) as u64;
    if len <= maximum_bytes && !len.is_multiple_of(PACKET_BYTES as u64) {
        return Err(invalid());
    }
    let packets =
        u32::try_from((len / PACKET_BYTES as u64).min(MAX_SOURCE_OBSERVATION_PACKETS as u64))
            .map_err(|_| invalid())?;
    SourceTeletextPacketSpan::new(0, packets).map_err(|_| invalid())
}

fn capture<T>(
    guard: &LocalTeletextSourceGuard,
    pid: u16,
    program_number: u16,
    span: SourceTeletextPacketSpan,
    observe_headers: bool,
    stop: &headers::ReadStop<'_, T>,
) -> Result<LocalTeletextSourceWindow> {
    let ended_at_eof = span.byte_end_exclusive() == guard.guard.len;
    let packets = usize::try_from(span.packet_count()).map_err(|_| invalid())?;
    let mut psi = SourceTeletextPsiObserver::new(pid).map_err(|_| invalid())?;
    let mut assembler = SuppliedTeletextAssembler::new(pid).map_err(|_| invalid())?;
    let mut bytes = Vec::with_capacity(packets * PACKET_BYTES);
    let mut selected_started = false;
    let mut skipped = 0;
    let mut last_complete = None;
    for ordinal in 0..packets {
        stop.check_source_guard()?;
        let mut raw = [0; PACKET_BYTES];
        read_packet(
            guard.guard.original.as_file(),
            &mut raw,
            span.byte_offset() + (ordinal * PACKET_BYTES) as u64,
            stop,
        )?;
        psi.push(&raw).map_err(|_| invalid())?;
        let selected = packet_pid(&raw) == pid;
        if selected && !selected_started {
            if !psi.has_complete_current_tables() || raw[1] & 0x40 == 0 {
                skipped += 1;
                continue;
            }
            selected_started = true;
        }
        bytes.extend_from_slice(&raw);
        if matches!(
            assembler.push(&raw).map_err(|_| invalid())?,
            TeletextAssemblyEvent::Complete(_)
        ) {
            last_complete = Some(bytes.len());
        }
    }
    stop.check_source_guard()?;
    let observed = psi.observation().map_err(|_| invalid())?;
    validate_program(&observed, program_number)?;
    if observed.current_tables_changed {
        return Err(invalid());
    }
    match assembler.finish() {
        Ok(()) => (),
        Err(TeletextAssemblyError::Incomplete) if !ended_at_eof => {
            bytes.truncate(last_complete.ok_or_else(invalid)?);
        }
        Err(_) => return Err(invalid()),
    }
    let fingerprint = headers::fingerprint(&observed);
    validate_decoder_window(&bytes, pid, program_number, fingerprint, stop)?;
    let selected = observed
        .programs
        .iter()
        .find(|program| program.program_number == program_number)
        .ok_or_else(invalid)?;
    let evidence = SourceTeletextDecodeWindowEvidence {
        first_source_packet: span.first_packet(),
        source_packets: u32::try_from(packets).map_err(|_| invalid())?,
        decoder_packets: u32::try_from(bytes.len() / PACKET_BYTES).map_err(|_| invalid())?,
        skipped_selected_prefix_packets: skipped,
        trimmed_source_tail_packets: u32::try_from(packets)
            .map_err(|_| invalid())?
            .checked_sub(skipped)
            .and_then(|count| count.checked_sub(u32::try_from(bytes.len() / PACKET_BYTES).ok()?))
            .ok_or_else(invalid)?,
        ended_at_eof,
        decoder_sha256: Sha256::digest(&bytes).into(),
        psi_fingerprint: fingerprint,
        program_number,
        pmt_pid: selected.pmt_pid,
        pcr_pid: selected.pcr_pid,
    };
    let headers = if observe_headers {
        let mut collector = SuppliedTeletextHeaderObserver::new(pid).map_err(|_| invalid())?;
        for packet in bytes.chunks_exact(PACKET_BYTES) {
            stop.check_source_guard()?;
            collector
                .push(packet.try_into().map_err(|_| invalid())?)
                .map_err(|_| invalid())?;
        }
        Some(collector.finish().map_err(|_| invalid())?)
    } else {
        None
    };
    let mut captured = write_snapshot(&bytes, evidence, stop)?;
    captured.headers = headers;
    Ok(captured)
}

fn write_snapshot<T>(
    bytes: &[u8],
    evidence: SourceTeletextDecodeWindowEvidence,
    stop: &headers::ReadStop<'_, T>,
) -> Result<LocalTeletextSourceWindow> {
    stop.check_source_guard()?;
    let mut file = tempfile::Builder::new()
        .prefix("ersatzrs-teletext-window-")
        .suffix(".ts")
        .tempfile()
        .map_err(|_| invalid())?;
    file.write_all(bytes).map_err(|_| invalid())?;
    file.flush().map_err(|_| invalid())?;
    stop.check_source_guard()?;
    Ok(LocalTeletextSourceWindow {
        snapshot: file,
        evidence,
        headers: None,
    })
}

/// Revalidate actual decoder bytes after filtering/trimming: a discarded tail
/// must not supply the only complete PSI or complete a retained partial PES.
fn validate_decoder_window<T>(
    bytes: &[u8],
    pid: u16,
    program_number: u16,
    fingerprint: [u8; 32],
    stop: &headers::ReadStop<'_, T>,
) -> Result<()> {
    let mut decoder_psi = SourceTeletextPsiObserver::new(pid).map_err(|_| invalid())?;
    let mut decoder_pes = SuppliedTeletextAssembler::new(pid).map_err(|_| invalid())?;
    let mut complete = false;
    for chunk in bytes.chunks_exact(PACKET_BYTES) {
        stop.check_source_guard()?;
        let raw: &[u8; PACKET_BYTES] = chunk.try_into().map_err(|_| invalid())?;
        decoder_psi.push(raw).map_err(|_| invalid())?;
        complete |= matches!(
            decoder_pes.push(raw).map_err(|_| invalid())?,
            TeletextAssemblyEvent::Complete(_)
        );
    }
    decoder_pes.finish().map_err(|_| invalid())?;
    if !complete {
        return Err(invalid());
    }
    let decoder_observed = decoder_psi.observation().map_err(|_| invalid())?;
    validate_program(&decoder_observed, program_number)?;
    if decoder_observed.current_tables_changed
        || headers::fingerprint(&decoder_observed) != fingerprint
    {
        return Err(invalid());
    }
    Ok(())
}

fn validate_program(observed: &ObservedSourcePsi, program_number: u16) -> Result<()> {
    if program_number == 0 {
        return Err(invalid());
    }
    let mut selected = observed
        .memberships()
        .filter(|(program, _)| program.program_number == program_number);
    let (_, stream) = selected.next().ok_or_else(invalid)?;
    if stream.stream_type != 6 || selected.next().is_some() {
        return Err(invalid());
    }
    Ok(())
}

pub(super) fn read_packet<T>(
    file: &std::fs::File,
    raw: &mut [u8; PACKET_BYTES],
    offset: u64,
    stop: &headers::ReadStop<'_, T>,
) -> Result<()> {
    let mut done = 0;
    while done < raw.len() {
        stop.check_source_guard()?;
        #[cfg(unix)]
        let result = {
            use std::os::unix::fs::FileExt as _;
            file.read_at(&mut raw[done..], offset + done as u64)
        };
        #[cfg(windows)]
        let result = {
            use std::os::windows::fs::FileExt as _;
            file.seek_read(&mut raw[done..], offset + done as u64)
        };
        match result {
            Ok(0) => return Err(invalid()),
            Ok(count) => done += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(invalid()),
        }
    }
    Ok(())
}

fn packet_pid(raw: &[u8; PACKET_BYTES]) -> u16 {
    (u16::from(raw[1] & 31) << 8) | u16::from(raw[2])
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput { path: LABEL.into() }
}

pub(super) fn window_error(error: FfprobeError) -> FfprobeError {
    match error {
        FfprobeError::InvalidOutput { ref path } if path == "local teletext source association" => {
            error
        }
        FfprobeError::Cancelled { .. } => FfprobeError::Cancelled { path: LABEL.into() },
        FfprobeError::Timeout { .. } => FfprobeError::Timeout { path: LABEL.into() },
        FfprobeError::CleanupUnconfirmed { .. } => {
            FfprobeError::CleanupUnconfirmed { path: LABEL.into() }
        }
        _ => invalid(),
    }
}

#[cfg(test)]
mod tests;

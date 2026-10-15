//! Same-byte, bounded local source preparation, without page readiness or seeks.

use std::{io::Write as _, path::PathBuf, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_ffmpeg::teletext::{
    source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent},
    source_observation::MAX_SOURCE_OBSERVATION_PACKETS,
    source_psi::{ObservedSourcePsi, SourceTeletextPsiObserver},
};
use sha2::{Digest as _, Sha256};
use tokio::time::Instant;

use super::{headers, LocalTeletextSourceGuard};
use crate::ffprobe::{FfprobeError, Result};

const LABEL: &str = "source teletext decode window";
const PACKET_BYTES: usize = 188;

/// Evidence for the exact completed temporary bytes supplied to one decoder.
///
/// Selected payload is retained only after complete current PSI; current table
/// changes anywhere in the captured source are rejected. Selected-PID packets
/// preceding its first post-PSI PES start are omitted. A capped source may also
/// omit the tail after the last completed selected PES. Neither omission proves
/// absent pages or a complete inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextDecodeWindowEvidence {
    pub source_packets: u32,
    pub decoder_packets: u32,
    pub skipped_selected_prefix_packets: u32,
    pub ended_at_eof: bool,
    pub decoder_sha256: [u8; 32],
    pub psi_fingerprint: [u8; 32],
    pub program_number: u16,
    pub pmt_pid: u16,
    pub pcr_pid: u16,
}

/// Own the completed snapshot until the actual decoder and its readers exit.
pub(crate) struct LocalTeletextSourceWindow {
    pub(crate) snapshot: tempfile::NamedTempFile,
    pub(crate) evidence: SourceTeletextDecodeWindowEvidence,
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
    headers::run_source_guard_owned(deadline, hold, move |stop| {
        stop.check_source_guard()?;
        super::check_source_guard_paths(&guard, &original, &resolved)?;
        let captured = capture(&guard, pid, program, stop)?;
        stop.check_source_guard()?;
        super::check_source_guard_paths(&guard, &original, &resolved)?;
        stop.check_source_guard()?;
        Ok(captured)
    })
    .await
    .map_err(window_error)
}

fn capture<T>(
    guard: &LocalTeletextSourceGuard,
    pid: u16,
    program_number: u16,
    stop: &headers::ReadStop<'_, T>,
) -> Result<LocalTeletextSourceWindow> {
    let maximum_bytes = (MAX_SOURCE_OBSERVATION_PACKETS * PACKET_BYTES) as u64;
    let ended_at_eof = guard.guard.len <= maximum_bytes;
    if ended_at_eof && !guard.guard.len.is_multiple_of(PACKET_BYTES as u64) {
        return Err(invalid());
    }
    let packets = usize::try_from(
        (guard.guard.len / PACKET_BYTES as u64).min(MAX_SOURCE_OBSERVATION_PACKETS as u64),
    )
    .map_err(|_| invalid())?;
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
            (ordinal * PACKET_BYTES) as u64,
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
    if ended_at_eof {
        assembler.finish().map_err(|_| invalid())?;
    } else {
        bytes.truncate(last_complete.ok_or_else(invalid)?);
    }
    let fingerprint = headers::fingerprint(&observed);
    validate_decoder_window(&bytes, pid, program_number, fingerprint, stop)?;
    let selected = observed
        .programs
        .iter()
        .find(|program| program.program_number == program_number)
        .ok_or_else(invalid)?;
    let evidence = SourceTeletextDecodeWindowEvidence {
        source_packets: u32::try_from(packets).map_err(|_| invalid())?,
        decoder_packets: u32::try_from(bytes.len() / PACKET_BYTES).map_err(|_| invalid())?,
        skipped_selected_prefix_packets: skipped,
        ended_at_eof,
        decoder_sha256: Sha256::digest(&bytes).into(),
        psi_fingerprint: fingerprint,
        program_number,
        pmt_pid: selected.pmt_pid,
        pcr_pid: selected.pcr_pid,
    };
    stop.check_source_guard()?;
    let mut file = tempfile::Builder::new()
        .prefix("ersatzrs-teletext-window-")
        .suffix(".ts")
        .tempfile()
        .map_err(|_| invalid())?;
    file.write_all(&bytes).map_err(|_| invalid())?;
    file.flush().map_err(|_| invalid())?;
    stop.check_source_guard()?;
    Ok(LocalTeletextSourceWindow {
        snapshot: file,
        evidence,
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

fn read_packet<T>(
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

fn window_error(error: FfprobeError) -> FfprobeError {
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

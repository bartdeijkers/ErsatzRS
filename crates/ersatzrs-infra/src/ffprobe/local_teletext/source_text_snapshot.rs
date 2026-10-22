//! Complete finite text input, separate from bounded bitmap observation windows.

use std::{io::Write as _, path::PathBuf, sync::Arc};

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_ffmpeg::teletext::{
    source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent},
    source_header::{observe_dvb_teletext_header, TeletextHeaderObservation},
    source_observation::MAX_SOURCE_OBSERVATION_PACKETS,
    source_psi::SourceTeletextPsiObserver,
};
use tokio::time::Instant;

use super::{headers, source_window, LocalTeletextSourceGuard};
use crate::ffprobe::{teletext_decode::SourceTeletextPageDecodeRequest, FfprobeError, Result};

/// Constructed only after actual EOF and complete PSI/PES validation.
/// Ordinary inputs supply a flushed exact copy; retained private inputs share
/// their already held file only after the same complete-source validation.
/// No window span, PCR-to-media mapping or complete page inventory is implied.
pub(crate) struct CompleteTextSnapshot {
    pub(crate) snapshot: Arc<tempfile::NamedTempFile>,
    pub(crate) psi_fingerprint: [u8; 32],
    pub(crate) selected_page_observed: bool,
}

pub(crate) async fn capture_owned(
    guard: Arc<LocalTeletextSourceGuard>,
    paths: (PathBuf, PathBuf),
    request: SourceTeletextPageDecodeRequest,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<CompleteTextSnapshot> {
    headers::run_source_guard_owned(deadline, hold, move |stop| {
        stop.check_source_guard()?;
        super::check_source_guard_paths(&guard, &paths.0, &paths.1)?;
        let captured = capture(&guard, &request, stop)?;
        stop.check_source_guard()?;
        super::check_source_guard_paths(&guard, &paths.0, &paths.1)?;
        stop.check_source_guard()?;
        Ok(captured)
    })
    .await
    .map_err(source_window::window_error)
}

fn capture<T>(
    guard: &LocalTeletextSourceGuard,
    request: &SourceTeletextPageDecodeRequest,
    stop: &headers::ReadStop<'_, T>,
) -> Result<CompleteTextSnapshot> {
    let len = guard.guard.len;
    let format = super::source_packet::detect(guard.guard.original.as_file(), len, stop)?;
    let stride = format.packet_bytes() as u64;
    if len == 0 || !len.is_multiple_of(stride) {
        return Err(invalid());
    }
    let mut psi = SourceTeletextPsiObserver::new(request.pid).map_err(|_| invalid())?;
    let mut assembler = SuppliedTeletextAssembler::new(request.pid).map_err(|_| invalid())?;
    let mut file = copy_destination(guard)?;
    let mut selected_started = false;
    let mut selected_page_observed = false;
    // Memory is independent of source length. The existing worker deadline and
    // ReadStop bound total work; its hold remains owned until actual completion.
    let mut writer = file
        .as_mut()
        .map(|file| std::io::BufWriter::with_capacity(64 * 1024, file.as_file_mut()));
    for ordinal in 0..len / stride {
        stop.check_source_guard()?;
        if ordinal > 0 && ordinal.is_multiple_of(MAX_SOURCE_OBSERVATION_PACKETS as u64) {
            psi.begin_continuation_window().map_err(|_| invalid())?;
        }
        let frame =
            super::source_packet::read(guard.guard.original.as_file(), ordinal, format, stop)?;
        let raw = &frame.packet;
        psi.push(raw).map_err(|_| invalid())?;
        let pid = (u16::from(raw[1] & 31) << 8) | u16::from(raw[2]);
        if pid == request.pid && !selected_started {
            // The complete-source policy permits no skipped prefix, repaired
            // PES or manufactured PSI, even when a decoder could tolerate it.
            if !psi.has_complete_current_tables() || raw[1] & 0x40 == 0 {
                return Err(invalid());
            }
            selected_started = true;
        }
        if let TeletextAssemblyEvent::Complete(pes) = assembler.push(raw).map_err(|_| invalid())? {
            for unit in pes.units().filter_map(|unit| unit.teletext_unit()) {
                if let TeletextHeaderObservation::Header(header) =
                    observe_dvb_teletext_header(unit).map_err(|_| invalid())?
                {
                    selected_page_observed |= header.display_page() == Some(request.page);
                }
            }
        }
        if let Some(writer) = &mut writer {
            for part in frame.parts() {
                writer.write_all(part).map_err(|_| invalid())?;
            }
        }
    }
    stop.check_source_guard()?;
    assembler.finish().map_err(|_| invalid())?;
    let observed = psi.observation().map_err(|_| invalid())?;
    let mut selected = observed
        .memberships()
        .filter(|(program, _)| program.program_number == request.program_number);
    let (_, stream) = selected.next().ok_or_else(invalid)?;
    if !selected_started
        || stream.stream_type != 6
        || selected.next().is_some()
        || observed.current_tables_changed
    {
        return Err(invalid());
    }
    if let Some(writer) = &mut writer {
        writer.flush().map_err(|_| invalid())?;
    }
    drop(writer);
    stop.check_source_guard()?;
    Ok(CompleteTextSnapshot {
        snapshot: match &guard.retained_source {
            Some(source) => source.file.clone(),
            None => Arc::new(file.ok_or_else(invalid)?),
        },
        psi_fingerprint: headers::fingerprint(&observed),
        selected_page_observed,
    })
}

fn copy_destination(guard: &LocalTeletextSourceGuard) -> Result<Option<tempfile::NamedTempFile>> {
    if guard.retained_source.is_some() {
        return Ok(None);
    }
    tempfile::Builder::new()
        .prefix("ersatzrs-teletext-text-")
        .suffix(".ts")
        .tempfile()
        .map(Some)
        .map_err(|_| invalid())
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput {
        path: "source teletext decode window".into(),
    }
}

#[cfg(test)]
mod tests;

//! Owned bounded reads from the original held source, outside `SQLite`.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read as _;
use std::path::Path;
use std::time::Duration;

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_domain::{
    SourceTeletextHeaderProgram, SourceTeletextHeaderWindow, SourceTeletextObservedHeader,
};
use ersatzrs_ffmpeg::teletext::{
    source_header::TeletextDataUnitKind,
    source_observation::{
        SourceTeletextHeaderOccurrence, SuppliedTeletextHeaderObserver,
        SuppliedTeletextHeaderWindow, MAX_SOURCE_OBSERVATION_PACKETS,
    },
    source_psi::{ObservedSourcePsi, SourceTeletextPsiObserver},
};
use sha2::{Digest as _, Sha256};
use tokio::sync::oneshot;
use tokio::time::Instant;

use super::{identities, LocalTeletextAcquisition};
use crate::ffprobe::local_cleanup_scope::{self, AdmissionError, OwnerGuard};
use crate::ffprobe::{FfprobeError, LocalProbeCleanupScope};

const READ_TIMEOUT: Duration = Duration::from_secs(10);
const SETTLEMENT_TIMEOUT: Duration = Duration::from_secs(1);

/// Reuse the existing source-read owner while retaining an admitted operation
/// inside the actual blocking worker, independently of its awaiting caller.
pub(in crate::ffprobe) async fn run_source_guard_owned<T, F>(
    deadline: Instant,
    hold: TeletextWorkHold,
    work: F,
) -> crate::ffprobe::Result<T>
where
    T: Send + 'static,
    F: FnOnce(&ReadStop<'_, T>) -> crate::ffprobe::Result<T> + Send + 'static,
{
    let now = Instant::now();
    if deadline <= now {
        return Err(FfprobeError::Timeout {
            path: "local teletext source association".into(),
        });
    }
    run_owned_before(
        Path::new("local teletext source association"),
        deadline.min(now + READ_TIMEOUT),
        Some(hold),
        work,
    )
    .await
}

/// Why actual headers were not acquired; descriptor metadata stays independent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalTeletextHeaderProblem {
    NoManagedScope,
    SourceChanged,
    Read,
    Deadline,
    UnsupportedFraming,
    InvalidTransport,
    IncompletePsi,
    ChangedPsi,
    Capacity,
}

/// Complete bounded acquisition result, never page-body or selection readiness.
#[derive(Debug)]
pub enum LocalTeletextHeaders {
    NotAcquired(LocalTeletextHeaderProblem),
    Observed(BTreeMap<i32, SourceTeletextHeaderWindow>),
}

impl LocalTeletextHeaders {
    #[must_use]
    pub fn for_stream(&self, stream_index: i32) -> Option<&SourceTeletextHeaderWindow> {
        match self {
            Self::Observed(windows) => windows.get(&stream_index),
            Self::NotAcquired(_) => None,
        }
    }
}

pub(super) async fn acquire(
    mut acquired: LocalTeletextAcquisition,
    path: &Path,
) -> crate::ffprobe::Result<LocalTeletextAcquisition> {
    // Ordinary unmanaged probing stays available; never start an unowned
    // blocking reader when the managed operation has supplied no cleanup scope.
    if LocalProbeCleanupScope::current().is_none() {
        return Ok(acquired);
    }
    let source_path = path.to_owned();
    run_owned(path, READ_TIMEOUT, move |stop| {
        acquired.headers = match read_headers(&acquired, &source_path, stop) {
            Ok(headers) => LocalTeletextHeaders::Observed(headers),
            Err(problem) => LocalTeletextHeaders::NotAcquired(problem),
        };
        if stop.cancelled() {
            return Err(FfprobeError::Cancelled {
                path: source_path.display().to_string(),
            });
        }
        Ok(acquired)
    })
    .await
}

/// The blocking owner and its registered capacity survive the awaiting caller.
/// No database mutation or publication can happen in this closure.
async fn run_owned<T, F>(path: &Path, timeout: Duration, work: F) -> crate::ffprobe::Result<T>
where
    T: Send + 'static,
    F: FnOnce(&ReadStop<'_, T>) -> crate::ffprobe::Result<T> + Send + 'static,
{
    run_owned_before(path, Instant::now() + timeout, None, work).await
}

async fn run_owned_before<T, F>(
    path: &Path,
    deadline: Instant,
    hold: Option<TeletextWorkHold>,
    work: F,
) -> crate::ffprobe::Result<T>
where
    T: Send + 'static,
    F: FnOnce(&ReadStop<'_, T>) -> crate::ffprobe::Result<T> + Send + 'static,
{
    let label = path.display().to_string();
    let owner = local_cleanup_scope::register_current()
        .map_err(|error| match error {
            AdmissionError::Closed => FfprobeError::Cancelled {
                path: label.clone(),
            },
            AdmissionError::Unconfirmed => FfprobeError::CleanupUnconfirmed {
                path: label.clone(),
            },
            AdmissionError::Capacity => FfprobeError::Spawn {
                path: label.clone(),
                source: std::io::Error::other("local source cleanup scope is busy"),
            },
        })?
        .ok_or_else(|| FfprobeError::Cancelled {
            path: label.clone(),
        })?;
    let (response, receiver) = oneshot::channel();
    let worker = tokio::task::spawn_blocking(move || {
        let _hold = hold;
        let stop = ReadStop {
            owner: &owner,
            response: &response,
            deadline,
        };
        let result = if stop.cancelled() {
            Err(FfprobeError::Cancelled {
                path: "local source observation".into(),
            })
        } else {
            work(&stop)
        };
        // All blocking reads have ended. A caller may have already received an
        // uncertainty reply; only actual completion releases this owner.
        let _ = response.send(result);
        owner.complete(true);
    });
    let completion = async {
        worker.await.map_err(|_| FfprobeError::CleanupUnconfirmed {
            path: label.clone(),
        })?;
        receiver
            .await
            .map_err(|_| FfprobeError::CleanupUnconfirmed {
                path: label.clone(),
            })?
    };
    tokio::time::timeout_at(deadline + SETTLEMENT_TIMEOUT, completion)
        .await
        .map_err(|_| FfprobeError::CleanupUnconfirmed { path: label })?
}

pub(in crate::ffprobe) struct ReadStop<'a, T> {
    owner: &'a OwnerGuard,
    response: &'a oneshot::Sender<crate::ffprobe::Result<T>>,
    deadline: Instant,
}

impl<T> ReadStop<'_, T> {
    fn cancelled(&self) -> bool {
        self.response.is_closed() || self.owner.cancellation_requested()
    }

    fn check(&self) -> Result<(), LocalTeletextHeaderProblem> {
        if self.cancelled() || Instant::now() >= self.deadline {
            Err(LocalTeletextHeaderProblem::Deadline)
        } else {
            Ok(())
        }
    }

    pub(in crate::ffprobe) fn check_source_guard(&self) -> crate::ffprobe::Result<()> {
        if self.cancelled() {
            Err(FfprobeError::Cancelled {
                path: "local teletext source association".into(),
            })
        } else if Instant::now() >= self.deadline {
            Err(FfprobeError::Timeout {
                path: "local teletext source association".into(),
            })
        } else {
            Ok(())
        }
    }
}

struct ActiveHeaders {
    started_at: u32,
    observers: BTreeMap<i32, SuppliedTeletextHeaderObserver>,
}

fn read_headers<T>(
    acquired: &LocalTeletextAcquisition,
    path: &Path,
    stop: &ReadStop<'_, T>,
) -> Result<BTreeMap<i32, SourceTeletextHeaderWindow>, LocalTeletextHeaderProblem> {
    use LocalTeletextHeaderProblem as Problem;
    stop.check()?;
    if !acquired.is_current(path) {
        return Err(Problem::SourceChanged);
    }
    let ids = identities(acquired.snapshot()).ok_or(Problem::IncompletePsi)?;
    let Some(&(_, first_pid)) = ids.first() else {
        return Err(Problem::IncompletePsi);
    };
    let mut psi =
        SourceTeletextPsiObserver::new(first_pid).map_err(|_| Problem::InvalidTransport)?;
    let mut file = acquired.guard.original.as_file();
    let maximum = (acquired.guard.len / 188).min(MAX_SOURCE_OBSERVATION_PACKETS as u64);
    let ended_at_eof = acquired.guard.len <= (MAX_SOURCE_OBSERVATION_PACKETS * 188) as u64;
    if ended_at_eof && !acquired.guard.len.is_multiple_of(188) {
        return Err(Problem::UnsupportedFraming);
    }
    let mut active: Option<ActiveHeaders> = None;
    let mut pre_psi_payload = BTreeSet::new();
    for ordinal in 0..maximum {
        stop.check()?;
        let mut raw = [0; 188];
        file.read_exact(&mut raw).map_err(|_| Problem::Read)?;
        if raw[0] != 0x47 {
            return Err(Problem::UnsupportedFraming);
        }
        if active.is_none() && raw[3] & 0x10 != 0 {
            let pid = (u16::from(raw[1] & 31) << 8) | u16::from(raw[2]);
            for &(index, selected) in &ids {
                if selected == pid {
                    pre_psi_payload.insert(index);
                }
            }
        }
        psi.push(&raw).map_err(|_| Problem::InvalidTransport)?;
        if active.is_none() && psi.has_complete_current_tables() {
            active = Some(ActiveHeaders {
                started_at: u32::try_from(ordinal).map_err(|_| Problem::Capacity)?,
                observers: ids
                    .iter()
                    .map(|&(index, pid)| {
                        SuppliedTeletextHeaderObserver::new(pid)
                            .map(|observer| (index, observer))
                            .map_err(|_| Problem::InvalidTransport)
                    })
                    .collect::<Result<_, _>>()?,
            });
        }
        if let Some(active) = &mut active {
            for observer in active.observers.values_mut() {
                observer.push(&raw).map_err(|_| Problem::InvalidTransport)?;
            }
            if active
                .observers
                .values()
                .map(SuppliedTeletextHeaderObserver::retained_header_count)
                .sum::<usize>()
                > 1_024
            {
                return Err(Problem::Capacity);
            }
        }
    }
    stop.check()?;
    if !acquired.is_current(path) {
        return Err(Problem::SourceChanged);
    }
    let psi = psi.observation().map_err(|_| Problem::IncompletePsi)?;
    if psi.current_tables_changed {
        return Err(Problem::ChangedPsi);
    }
    let active = active.ok_or(Problem::IncompletePsi)?;
    let fingerprint = fingerprint(&psi);
    let mut result = BTreeMap::new();
    let mut total_headers = 0;
    for (index, observer) in active.observers {
        stop.check()?;
        let window = observer.finish().map_err(|_| Problem::InvalidTransport)?;
        total_headers += window.headers().len();
        if total_headers > 1_024 {
            return Err(Problem::Capacity);
        }
        if let Some(window) = project_window(
            &psi,
            &window,
            fingerprint,
            active.started_at,
            u32::try_from(maximum).map_err(|_| Problem::Capacity)?,
            ended_at_eof,
            pre_psi_payload.contains(&index),
        )? {
            result.insert(index, window);
        }
    }
    Ok(result)
}

fn project_window(
    psi: &ObservedSourcePsi,
    observed: &SuppliedTeletextHeaderWindow,
    fingerprint: [u8; 32],
    started_at: u32,
    packet_count: u32,
    ended_at_eof: bool,
    pre_psi_payload: bool,
) -> Result<Option<SourceTeletextHeaderWindow>, LocalTeletextHeaderProblem> {
    use LocalTeletextHeaderProblem as Problem;
    let pid = observed.elementary_pid();
    let programs: Vec<_> = psi
        .programs
        .iter()
        .flat_map(|program| {
            program
                .streams
                .iter()
                .filter(move |stream| stream.pid == pid)
                .map(move |stream| SourceTeletextHeaderProgram {
                    program_number: program.program_number,
                    pmt_pid: program.pmt_pid,
                    pcr_pid: program.pcr_pid,
                    pmt_version: program.pmt.version,
                    stream_type: stream.stream_type,
                    descriptors: stream.descriptors.clone(),
                })
        })
        .collect();
    // An auxiliary codec projection cannot invent a wire membership.
    if programs.is_empty() {
        return Ok(None);
    }
    let headers = observed
        .headers()
        .iter()
        .map(|record| project_header(record, started_at))
        .collect::<Result<_, _>>()?;
    let window = SourceTeletextHeaderWindow {
        transport_stream_id: psi.pat.table_id_extension,
        pat_version: psi.pat.version,
        psi_fingerprint: fingerprint,
        packet_count,
        complete_pes: u32::try_from(observed.complete_pes()).map_err(|_| Problem::Capacity)?,
        unobserved_prefix: observed.has_unobserved_prefix() || pre_psi_payload,
        ended_at_eof,
        programs,
        headers,
    };
    window.validate().map_err(|_| Problem::InvalidTransport)?;
    Ok(Some(window))
}

fn project_header(
    record: &SourceTeletextHeaderOccurrence,
    started_at: u32,
) -> Result<SourceTeletextObservedHeader, LocalTeletextHeaderProblem> {
    use LocalTeletextHeaderProblem as Problem;
    let header = record.header;
    let address = header.packet_address;
    Ok(SourceTeletextObservedHeader {
        wire_magazine: address.wire_magazine,
        wire_page: header.wire_page,
        subcode: header.subcode,
        controls: header.controls.raw(),
        subtitle_data_unit: address.data_unit_kind == TeletextDataUnitKind::Subtitle,
        field_parity: address.field_parity,
        line_offset: address.line_offset,
        corrected_address_bytes: address.corrected_address_bytes,
        corrected_header_bytes: header.corrected_header_bytes,
        completion_packet: started_at
            + u32::try_from(record.completion_packet).map_err(|_| Problem::Capacity)?,
        pes_ordinal: u32::try_from(record.pes_ordinal).map_err(|_| Problem::Capacity)?,
        unit_ordinal: u32::try_from(record.unit_ordinal).map_err(|_| Problem::Capacity)?,
        pts: record
            .pts
            .map(ersatzrs_ffmpeg::teletext::TeletextTimestamp::ticks),
    })
}

pub(super) fn fingerprint(psi: &ObservedSourcePsi) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"ersatzrs:source-psi-window:v1\0");
    for (pid, sections) in std::iter::once((0, &psi.pat.sections))
        .chain(psi.cat.as_ref().map(|cat| (1, &cat.sections)))
        .chain(
            psi.programs
                .iter()
                .map(|program| (program.pmt_pid, &program.pmt.sections)),
        )
    {
        digest.update(pid.to_le_bytes());
        digest.update((sections.len() as u64).to_le_bytes());
        for section in sections {
            digest.update((section.len() as u64).to_le_bytes());
            digest.update(section);
        }
    }
    digest.finalize().into()
}

#[cfg(test)]
mod tests;

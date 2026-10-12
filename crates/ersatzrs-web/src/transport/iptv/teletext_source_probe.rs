//! Bounded actual-output PSI discovery over the same borrowed final stdout.

use std::{fmt, io::ErrorKind, time::Duration};

use ersatzrs_ffmpeg::teletext::{
    TeletextPmtPlan, TeletextProgramSelection, TeletextPsiDiscovery, TeletextPsiError,
    TeletextServiceDescriptor,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _},
    time::{timeout_at, Instant},
};

use super::teletext_startup::TeletextStartupPrefix;

const PACKET_BYTES: usize = 188;
const MAXIMUM_BYTES: usize = 32_768 * PACKET_BYTES;
const READ_BYTES: usize = 256 * PACKET_BYTES;

/// Trusted discovery bounds, independent of compiler and publication authority.
#[derive(Debug, Clone, Copy)]
pub(super) struct TeletextSourceProbePolicy {
    pub maximum_bytes: usize,
    pub deadline: Instant,
    pub first_byte_timeout: Duration,
}

/// Actual source ownership only; clock validation and output installation remain.
pub(super) struct TeletextSourceProbeAdmission {
    pub prefix: TeletextStartupPrefix,
    pub plan: TeletextPmtPlan,
}
impl fmt::Debug for TeletextSourceProbeAdmission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextSourceProbeAdmission")
            .field("prefix_bytes", &self.prefix.bytes().len())
            .field("program", &self.plan.program_number())
            .field("pmt_pid", &self.plan.pmt_pid())
            .field("pcr_pid", &self.plan.pcr_pid())
            .finish_non_exhaustive()
    }
}

/// Static/numeric outcomes; source failures never become authored absence.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(super) enum TeletextSourceProbeError {
    #[error("teletext source discovery policy is invalid")]
    Configuration,
    #[error("teletext source discovery allocation failed")]
    Allocation,
    #[error("teletext source discovery read failed ({0:?})")]
    Read(ErrorKind),
    #[error("teletext source ended without an encoded byte")]
    Empty,
    #[error("teletext source did not supply its first byte before timeout")]
    FirstByteTimeout,
    #[error(transparent)]
    Psi(#[from] TeletextPsiError),
    #[error("teletext source discovery exhausted its byte bound")]
    Capacity,
    #[error("teletext source discovery exceeded its deadline")]
    Deadline,
}

/// Exact consumed bytes, including partial tail, are never diagnostic output.
/// Empty before any nonzero read; these bytes are not an admitted startup seed.
/// The caller still owns stdout and must not retry a failed authored epoch.
pub(super) struct TeletextSourceProbeFailure {
    pub raw_prefix: Vec<u8>,
    pub reason: TeletextSourceProbeError,
}
impl fmt::Debug for TeletextSourceProbeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextSourceProbeFailure")
            .field("prefix_bytes", &self.raw_prefix.len())
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

struct Probe {
    bytes: Vec<u8>,
    epoch: Instant,
    first_observed: Option<Instant>,
}
impl Probe {
    fn fail(self, reason: TeletextSourceProbeError) -> TeletextSourceProbeFailure {
        TeletextSourceProbeFailure {
            raw_prefix: self.bytes,
            reason,
        }
    }

    fn admit(
        self,
        plan: TeletextPmtPlan,
    ) -> Result<TeletextSourceProbeAdmission, TeletextSourceProbeFailure> {
        let Some(first_observed) = self.first_observed else {
            return Err(self.fail(TeletextSourceProbeError::Configuration));
        };
        match TeletextStartupPrefix::new_preserving(self.bytes, self.epoch, first_observed) {
            Ok(prefix) => Ok(TeletextSourceProbeAdmission { prefix, plan }),
            Err((_, raw_prefix)) => Err(TeletextSourceProbeFailure {
                raw_prefix,
                reason: TeletextSourceProbeError::Configuration,
            }),
        }
    }
}

/// Discover one sole advertised program without reopening or writing stdout.
/// Every complete packet of each consumed chunk is inspected before planning;
/// partial tails remain in the returned exact prefix. Neither a plan nor this
/// prefix authenticates a generation, actual PCR rate or output clock policy.
pub(super) async fn probe_teletext_source(
    source: &mut (impl AsyncRead + Unpin),
    service: &TeletextServiceDescriptor,
    candidate_pid: u16,
    policy: TeletextSourceProbePolicy,
) -> Result<TeletextSourceProbeAdmission, TeletextSourceProbeFailure> {
    let epoch = Instant::now();
    let mut probe = Probe {
        bytes: Vec::new(),
        epoch,
        first_observed: None,
    };
    if policy.maximum_bytes == 0
        || policy.maximum_bytes > MAXIMUM_BYTES
        || policy.first_byte_timeout.is_zero()
    {
        return Err(probe.fail(TeletextSourceProbeError::Configuration));
    }
    let Some(first_deadline) = epoch.checked_add(policy.first_byte_timeout) else {
        return Err(probe.fail(TeletextSourceProbeError::Configuration));
    };
    let mut discovery = match TeletextPsiDiscovery::new_source_epoch_selection(
        TeletextProgramSelection::SoleProgram,
        candidate_pid,
    ) {
        Ok(discovery) => discovery,
        Err(error) => return Err(probe.fail(TeletextSourceProbeError::Psi(error))),
    };
    if probe.bytes.try_reserve_exact(policy.maximum_bytes).is_err() {
        return Err(probe.fail(TeletextSourceProbeError::Allocation));
    }
    let mut scratch = Vec::new();
    let size = policy.maximum_bytes.min(READ_BYTES);
    if scratch.try_reserve_exact(size).is_err() {
        return Err(probe.fail(TeletextSourceProbeError::Allocation));
    }
    scratch.resize(size, 0);
    let mut observed = 0;
    loop {
        let deadline = if probe.first_observed.is_none() {
            policy.deadline.min(first_deadline)
        } else {
            policy.deadline
        };
        let timeout_reason = if probe.first_observed.is_none() && first_deadline <= policy.deadline
        {
            TeletextSourceProbeError::FirstByteTimeout
        } else {
            TeletextSourceProbeError::Deadline
        };
        if Instant::now() >= deadline {
            return Err(probe.fail(timeout_reason));
        }
        let available = policy.maximum_bytes - probe.bytes.len();
        if available == 0 {
            return Err(probe.fail(TeletextSourceProbeError::Capacity));
        }
        let length = match timeout_at(deadline, source.read(&mut scratch[..available.min(size)]))
            .await
        {
            Ok(Ok(length)) => length,
            Ok(Err(error)) => return Err(probe.fail(TeletextSourceProbeError::Read(error.kind()))),
            Err(_) => return Err(probe.fail(timeout_reason)),
        };
        if length == 0 {
            let reason = if probe.first_observed.is_none() {
                TeletextSourceProbeError::Empty
            } else {
                TeletextSourceProbeError::Psi(TeletextPsiError::Incomplete)
            };
            return Err(probe.fail(reason));
        }
        let now = Instant::now();
        probe.first_observed.get_or_insert(now);
        probe.bytes.extend_from_slice(&scratch[..length]);
        if now >= deadline {
            return Err(probe.fail(timeout_reason));
        }
        let (packets, _) = probe.bytes.as_chunks::<PACKET_BYTES>();
        for packet in &packets[observed..] {
            if let Err(error) = discovery.push(packet) {
                return Err(probe.fail(TeletextSourceProbeError::Psi(error)));
            }
        }
        observed = packets.len();
        let plan = match discovery.plan(service) {
            Ok(plan) => plan,
            Err(TeletextPsiError::Incomplete) => continue,
            Err(error) => return Err(probe.fail(TeletextSourceProbeError::Psi(error))),
        };
        if Instant::now() >= policy.deadline {
            return Err(probe.fail(TeletextSourceProbeError::Deadline));
        }
        return probe.admit(plan);
    }
}

#[cfg(test)]
#[path = "teletext_source_probe_tests.rs"]
mod tests;

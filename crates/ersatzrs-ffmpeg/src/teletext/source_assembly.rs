//! Bounded assembly from supplied TS188 packets on one explicitly selected PID.
//!
//! This follows the EN 300 472 selected transport subset: payload-only or
//! adaptation-only packets, never combined adaptation/payload. It performs no
//! source reads, PSI discovery, PID-ownership assurance, timing or page-readiness
//! inference. Every input needs a sync byte; other PID packet fields are ignored.
//! Adaptation-only packet extent and discontinuity are checked, but optional
//! PCR/OPCR/private/extension fields are not interpreted or fully validated.

use super::{
    signalling::valid_packet,
    source_pes::{SuppliedTeletextPes, TeletextPesError, MAX_PES_BYTES},
};

/// Assembly failures without supplied transport bytes retained in errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextAssemblyError {
    #[error("selected teletext PID must be in 0x0020..=0x1FFE")]
    Pid,
    #[error("supplied TS188 packet has an invalid sync byte")]
    Sync,
    #[error("selected teletext packet framing is invalid or scrambled")]
    Framing,
    #[error("selected teletext transport adaptation/payload combination is unsupported")]
    UnsupportedTransport,
    #[error("selected teletext transport signals a discontinuity")]
    Discontinuity,
    #[error("selected teletext continuity counter or duplicate bytes differ")]
    Continuity,
    #[error("selected teletext payload start interrupts a packet or has no payload")]
    UnexpectedStart,
    #[error("selected teletext payload exceeds or follows a completed PES")]
    UnexpectedPayload,
    #[error("selected teletext PES start header is invalid")]
    PesHeader,
    #[error("selected teletext PES declared length is unbounded or unsupported")]
    PacketLength,
    #[error("supplied transport ended inside a selected teletext PES")]
    Incomplete,
    #[error("supplied teletext assembly is closed")]
    Closed,
    #[error(transparent)]
    Pes(#[from] TeletextPesError),
}

/// The exact effect of one supplied packet, without an inventory conclusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAssemblyEvent<'a> {
    /// Sync was checked, but this unrelated PID acquired no assembly state.
    IgnoredPid,
    /// Selected payload preceded the first observed PES start and was not retained.
    UnobservedPrefix,
    /// A selected complete transport packet extended an incomplete PES.
    Pending,
    /// A selected adaptation-only packet carried no PES bytes.
    NoPayload,
    /// A byte-identical repeated payload packet was ignored without another emission.
    Duplicate,
    /// Exact complete bytes passed the separate supplied-PES framing parser.
    Complete(SuppliedTeletextPes<'a>),
}

/// One immutable PID selection and one bounded source observation epoch.
///
/// Retains at most 65,504 PES bytes and one previous 188-byte payload packet,
/// plus small state. Allocator overhead and the caller's input are outside this
/// byte bound. Complete views borrow this object, preventing its mutation while
/// a view is used. A later start replaces the completed buffer; no outputs queue.
/// Every failure is sticky and clears retained bytes. Starting again requires a
/// new instance; a PUSI never silently repairs an interrupted packet or epoch.
#[derive(Debug)]
pub struct SuppliedTeletextAssembler {
    pid: u16,
    pending: Vec<u8>,
    expected: Option<usize>,
    completed: bool,
    previous: Option<[u8; 188]>,
    counter: Option<u8>,
    failure: Option<TeletextAssemblyError>,
    closed: bool,
    initial_discontinuity_allowed: bool,
}

impl SuppliedTeletextAssembler {
    /// Select an elementary-PID profile without asserting PSI ownership.
    /// # Errors
    /// Rejects reserved, null and out-of-range PID values for this strict profile.
    pub fn new(pid: u16) -> Result<Self, TeletextAssemblyError> {
        if !(0x20..=0x1ffe).contains(&pid) {
            return Err(TeletextAssemblyError::Pid);
        }
        Ok(Self {
            pid,
            pending: Vec::new(),
            expected: None,
            completed: false,
            previous: None,
            counter: None,
            failure: None,
            closed: false,
            initial_discontinuity_allowed: false,
        })
    }

    /// Begin at a known source origin for this PID. Only its first physical
    /// packet may carry an initial discontinuity, including adaptation-only
    /// packets. Unflagged packets also consume this allowance. Framing, counter,
    /// PES and all later discontinuity checks remain strict; no clock is reset.
    /// # Errors
    /// Rejects the same invalid elementary PIDs as [`Self::new`].
    pub fn new_source_epoch(pid: u16) -> Result<Self, TeletextAssemblyError> {
        let mut assembler = Self::new(pid)?;
        assembler.initial_discontinuity_allowed = true;
        Ok(assembler)
    }

    /// Observe exactly one supplied complete TS188 packet, never searching bytes.
    ///
    /// No partial header can span packets in this payload-only strict profile.
    /// Before the first PUSI an unseen previous PES is reported but not acquired.
    /// There is no read budget, wall-clock deadline or whole-multiplex validation.
    /// # Errors
    /// Rejects malformed selected framing, unsupported transport, interrupted
    /// continuity/start/length or invalid completed PES; failures fence this epoch.
    pub fn push(
        &mut self,
        raw: &[u8; 188],
    ) -> Result<TeletextAssemblyEvent<'_>, TeletextAssemblyError> {
        self.ensure_open()?;
        let event = match self.observe(raw) {
            Ok(event) => event,
            Err(error) => return self.fail(error),
        };
        match event {
            PacketEvent::IgnoredPid => Ok(TeletextAssemblyEvent::IgnoredPid),
            PacketEvent::UnobservedPrefix => Ok(TeletextAssemblyEvent::UnobservedPrefix),
            PacketEvent::Pending => Ok(TeletextAssemblyEvent::Pending),
            PacketEvent::NoPayload => Ok(TeletextAssemblyEvent::NoPayload),
            PacketEvent::Duplicate => Ok(TeletextAssemblyEvent::Duplicate),
            PacketEvent::Complete => {
                // Validation precedes this borrow so failures can clear the owned
                // buffer before any completed view escapes to the caller.
                SuppliedTeletextPes::parse(&self.pending)
                    .map(TeletextAssemblyEvent::Complete)
                    .map_err(TeletextAssemblyError::Pes)
            }
        }
    }

    /// Close a finite supplied packet sequence without inferring an empty inventory.
    ///
    /// Successful closure is idempotent, including no observed PES start. Input
    /// already consists of complete TS188 packets; a caller's partial byte tail
    /// must be handled by its separate framing boundary, not discarded here.
    /// # Errors
    /// Returns the first sticky failure or an incomplete selected PES.
    pub fn finish(&mut self) -> Result<(), TeletextAssemblyError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.closed {
            return Ok(());
        }
        if self.expected.is_some() {
            return self.fail(TeletextAssemblyError::Incomplete);
        }
        self.closed = true;
        self.pending.clear();
        self.previous = None;
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), TeletextAssemblyError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.closed {
            return Err(TeletextAssemblyError::Closed);
        }
        Ok(())
    }

    fn observe(&mut self, raw: &[u8; 188]) -> Result<PacketEvent, TeletextAssemblyError> {
        if raw[0] != 0x47 {
            return Err(TeletextAssemblyError::Sync);
        }
        let pid = (u16::from(raw[1] & 31) << 8) | u16::from(raw[2]);
        if pid != self.pid {
            return Ok(PacketEvent::IgnoredPid);
        }
        let packet = valid_packet(raw).map_err(|_| TeletextAssemblyError::Framing)?;
        if raw[3] & 0x30 == 0x30 {
            return Err(TeletextAssemblyError::UnsupportedTransport);
        }
        let initial = std::mem::replace(&mut self.initial_discontinuity_allowed, false);
        if packet
            .adaptation_field()
            .is_some_and(|field| field.discontinuity_indicator())
            && !initial
        {
            return Err(TeletextAssemblyError::Discontinuity);
        }
        let counter = packet.continuity_counter().count();
        let Some(payload) = packet.payload() else {
            if packet.payload_unit_start_indicator() {
                return Err(TeletextAssemblyError::UnexpectedStart);
            }
            if self.counter.is_some_and(|before| counter != before) {
                return Err(TeletextAssemblyError::Continuity);
            }
            self.counter = Some(counter);
            return Ok(PacketEvent::NoPayload);
        };
        if let Some(before) = self.counter {
            if counter == before && self.previous.as_ref().is_some_and(|last| last == raw) {
                return Ok(PacketEvent::Duplicate);
            }
            if counter != (before + 1) & 15 {
                return Err(TeletextAssemblyError::Continuity);
            }
        }
        self.counter = Some(counter);
        self.previous = Some(*raw);
        if packet.payload_unit_start_indicator() {
            if self.expected.is_some() {
                return Err(TeletextAssemblyError::UnexpectedStart);
            }
            let expected = checked_size(payload)?;
            self.pending.clear();
            self.pending.reserve_exact(expected);
            self.expected = Some(expected);
            self.completed = false;
        } else if self.expected.is_none() {
            return if self.completed {
                Err(TeletextAssemblyError::UnexpectedPayload)
            } else {
                Ok(PacketEvent::UnobservedPrefix)
            };
        }
        self.append(payload)
    }

    fn append(&mut self, payload: &[u8]) -> Result<PacketEvent, TeletextAssemblyError> {
        let expected = self
            .expected
            .ok_or(TeletextAssemblyError::UnexpectedPayload)?;
        if payload.len() > expected - self.pending.len() {
            return Err(TeletextAssemblyError::UnexpectedPayload);
        }
        self.pending.extend_from_slice(payload);
        if self.pending.len() < expected {
            return Ok(PacketEvent::Pending);
        }
        SuppliedTeletextPes::parse(&self.pending)?;
        self.expected = None;
        self.completed = true;
        Ok(PacketEvent::Complete)
    }

    fn fail<T>(&mut self, error: TeletextAssemblyError) -> Result<T, TeletextAssemblyError> {
        self.failure = Some(error);
        self.pending.clear();
        self.expected = None;
        self.previous = None;
        Err(error)
    }
}

fn checked_size(payload: &[u8]) -> Result<usize, TeletextAssemblyError> {
    let header = payload.get(..6).ok_or(TeletextAssemblyError::PesHeader)?;
    if header[..4] != [0, 0, 1, 0xbd] {
        return Err(TeletextAssemblyError::PesHeader);
    }
    let declared = usize::from(u16::from_be_bytes([header[4], header[5]]));
    let size = declared + 6;
    if declared == 0 || size > MAX_PES_BYTES || !size.is_multiple_of(184) {
        return Err(TeletextAssemblyError::PacketLength);
    }
    Ok(size)
}

#[derive(Debug)]
enum PacketEvent {
    IgnoredPid,
    UnobservedPrefix,
    Pending,
    NoPayload,
    Duplicate,
    Complete,
}

#[cfg(test)]
#[path = "source_assembly_tests.rs"]
mod tests;

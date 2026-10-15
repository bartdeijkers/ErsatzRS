//! Bounded header observations from one supplied selected-PID TS188 window.
//!
//! This joins the existing strict PES assembler and protected header parser. It
//! performs no source reads, PSI association, body/charset decoding or selection.
//! A successful finite window never proves complete page inventory or absence.

use super::{
    source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyError, TeletextAssemblyEvent},
    source_header::{
        observe_dvb_teletext_header, SuppliedTeletextHeader, TeletextHeaderError,
        TeletextHeaderObservation,
    },
    TeletextTimestamp,
};

/// Resource policy for a supplied observation window, not a transport limit.
pub const MAX_SOURCE_OBSERVATION_PACKETS: usize = 32_768;
/// Maximum retained header occurrences, including repeated page identities.
pub const MAX_SOURCE_OBSERVATION_HEADERS: usize = 1_024;

/// Failure of this entire observation window; partial headers never escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextSourceObservationError {
    #[error(transparent)]
    Assembly(#[from] TeletextAssemblyError),
    #[error(transparent)]
    Header(#[from] TeletextHeaderError),
    #[error("source teletext observation exceeds its packet bound")]
    Packets,
    #[error("source teletext observation exceeds its header bound")]
    Headers,
}

/// One actual header occurrence in a completed, validated PES.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextHeaderOccurrence {
    pub header: SuppliedTeletextHeader,
    /// Zero-based physical TS packet where this complete PES became available.
    pub completion_packet: usize,
    /// Zero-based PES ordinal among complete selected-PID PES packets only.
    pub pes_ordinal: usize,
    /// Zero-based data-unit ordinal, including non-teletext units and stuffing.
    pub unit_ordinal: usize,
    /// Supplied 33-bit PES PTS; no unwrapping or source timeline is inferred.
    pub pts: Option<TeletextTimestamp>,
}

/// Checked finite supplied window, still without source/PSI/page readiness.
#[derive(Debug)]
pub struct SuppliedTeletextHeaderWindow {
    elementary_pid: u16,
    packets: usize,
    complete_pes: usize,
    unobserved_prefix: bool,
    headers: Vec<SourceTeletextHeaderOccurrence>,
}

impl SuppliedTeletextHeaderWindow {
    #[must_use]
    pub const fn elementary_pid(&self) -> u16 {
        self.elementary_pid
    }

    /// All supplied TS packets, including unrelated PIDs.
    #[must_use]
    pub const fn packets(&self) -> usize {
        self.packets
    }

    #[must_use]
    pub const fn complete_pes(&self) -> usize {
        self.complete_pes
    }

    /// Selected payload preceded the first observed start; it was not decoded.
    #[must_use]
    pub const fn has_unobserved_prefix(&self) -> bool {
        self.unobserved_prefix
    }

    /// Actual occurrences in wire order; repetitions are not deduplicated.
    #[must_use]
    pub fn headers(&self) -> &[SourceTeletextHeaderOccurrence] {
        &self.headers
    }
}

/// Owns one bounded assembler and at most 1,024 header occurrences.
/// No partial successful result is exposed while input remains open or failed.
#[derive(Debug)]
pub struct SuppliedTeletextHeaderObserver {
    assembler: SuppliedTeletextAssembler,
    window: SuppliedTeletextHeaderWindow,
    failure: Option<TeletextSourceObservationError>,
}

impl SuppliedTeletextHeaderObserver {
    /// Begin an observation without asserting source/PID ownership.
    /// # Errors
    /// Rejects reserved/null/out-of-range elementary PIDs.
    pub fn new(pid: u16) -> Result<Self, TeletextSourceObservationError> {
        Ok(Self {
            assembler: SuppliedTeletextAssembler::new(pid)?,
            window: SuppliedTeletextHeaderWindow {
                elementary_pid: pid,
                packets: 0,
                complete_pes: 0,
                unobserved_prefix: false,
                headers: Vec::new(),
            },
            failure: None,
        })
    }

    /// Supply the next complete TS188 packet exactly once.
    /// # Errors
    /// Bounds, transport/PES and header-protection failures permanently fence
    /// this window and release all retained header occurrences.
    pub fn push(&mut self, raw: &[u8; 188]) -> Result<(), TeletextSourceObservationError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let result = self.observe(raw);
        if let Err(error) = result {
            self.failure = Some(error);
            self.window.headers = Vec::new();
        }
        result
    }

    /// Close the supplied window without declaring EOF or a complete inventory.
    /// # Errors
    /// Returns the first sticky failure or an incomplete selected PES. Byte-tail
    /// framing and read/time budgets remain the acquiring caller's obligations.
    pub fn finish(
        mut self,
    ) -> Result<SuppliedTeletextHeaderWindow, TeletextSourceObservationError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        self.assembler.finish()?;
        Ok(self.window)
    }

    fn observe(&mut self, raw: &[u8; 188]) -> Result<(), TeletextSourceObservationError> {
        if self.window.packets == MAX_SOURCE_OBSERVATION_PACKETS {
            return Err(TeletextSourceObservationError::Packets);
        }
        let completion_packet = self.window.packets;
        self.window.packets += 1;
        match self.assembler.push(raw)? {
            TeletextAssemblyEvent::UnobservedPrefix => self.window.unobserved_prefix = true,
            TeletextAssemblyEvent::Complete(pes) => {
                for (unit_ordinal, unit) in pes.units().enumerate() {
                    let Some(unit) = unit.teletext_unit() else {
                        continue;
                    };
                    if let TeletextHeaderObservation::Header(header) =
                        observe_dvb_teletext_header(unit)?
                    {
                        if self.window.headers.len() == MAX_SOURCE_OBSERVATION_HEADERS {
                            return Err(TeletextSourceObservationError::Headers);
                        }
                        self.window.headers.push(SourceTeletextHeaderOccurrence {
                            header,
                            completion_packet,
                            pes_ordinal: self.window.complete_pes,
                            unit_ordinal,
                            pts: pes.pts(),
                        });
                    }
                }
                self.window.complete_pes += 1;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "source_observation_tests.rs"]
mod tests;

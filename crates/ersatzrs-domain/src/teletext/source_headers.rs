//! Bounded actual packet-0 observations, separate from descriptor projections.
//!
//! The adapter must acquire one stable current PSI generation from the same held
//! source association as the parent report. Structure cannot prove that contract.
//! A header window provides neither page bodies, charset resolution, a complete
//! inventory nor runtime selection/readiness.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::SourceTeletextError;

/// Maximum bounded source read, measured in complete 188-byte TS packets.
pub const MAX_SOURCE_TELETEXT_HEADER_PACKETS: u32 = 32_768;
/// Resource bound on matching-PID program memberships, not a DVB service limit.
pub const MAX_SOURCE_TELETEXT_HEADER_PROGRAMS: usize = 128;
/// Resource bound on retained header occurrences; repeated addresses survive.
pub const MAX_SOURCE_TELETEXT_OBSERVED_HEADERS: usize = 1_024;
/// Resource bound on one matching elementary stream's raw PMT descriptors.
pub const MAX_SOURCE_TELETEXT_HEADER_DESCRIPTOR_BYTES: usize = 1_024;

/// Actual current PMT membership of the parent report's PID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTeletextHeaderProgram {
    pub program_number: u16,
    pub pmt_pid: u16,
    pub pcr_pid: u16,
    pub pmt_version: u8,
    pub stream_type: u8,
    /// Raw matching elementary-stream descriptor bytes, without inferred semantics.
    pub descriptors: Vec<u8>,
}

/// One decoded packet-0 occurrence in transport order, including repeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTeletextObservedHeader {
    pub wire_magazine: u8,
    pub wire_page: u8,
    pub subcode: u16,
    pub controls: u16,
    pub subtitle_data_unit: bool,
    pub field_parity: bool,
    pub line_offset: u8,
    pub corrected_address_bytes: u8,
    pub corrected_header_bytes: u8,
    /// Zero-based TS packet completing the containing PES.
    pub completion_packet: u32,
    /// Zero-based complete PES ordinal within this read.
    pub pes_ordinal: u32,
    /// Zero-based data-unit ordinal inside the containing PES.
    pub unit_ordinal: u32,
    pub pts: Option<u64>,
}

/// One bounded source read under a stable current PSI generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "HeaderWindowWire")]
pub struct SourceTeletextHeaderWindow {
    pub transport_stream_id: u16,
    pub pat_version: u8,
    pub psi_fingerprint: [u8; 32],
    pub packet_count: u32,
    pub complete_pes: u32,
    pub unobserved_prefix: bool,
    pub ended_at_eof: bool,
    pub programs: Vec<SourceTeletextHeaderProgram>,
    pub headers: Vec<SourceTeletextObservedHeader>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderWindowWire {
    transport_stream_id: u16,
    pat_version: u8,
    psi_fingerprint: [u8; 32],
    packet_count: u32,
    complete_pes: u32,
    unobserved_prefix: bool,
    ended_at_eof: bool,
    programs: Vec<SourceTeletextHeaderProgram>,
    headers: Vec<SourceTeletextObservedHeader>,
}

impl TryFrom<HeaderWindowWire> for SourceTeletextHeaderWindow {
    type Error = SourceTeletextError;

    fn try_from(wire: HeaderWindowWire) -> Result<Self, Self::Error> {
        let window = Self {
            transport_stream_id: wire.transport_stream_id,
            pat_version: wire.pat_version,
            psi_fingerprint: wire.psi_fingerprint,
            packet_count: wire.packet_count,
            complete_pes: wire.complete_pes,
            unobserved_prefix: wire.unobserved_prefix,
            ended_at_eof: wire.ended_at_eof,
            programs: wire.programs,
            headers: wire.headers,
        };
        window.validate()?;
        Ok(window)
    }
}

impl SourceTeletextHeaderWindow {
    /// Validate bounded wire fields and occurrence order, without proving source identity.
    ///
    /// # Errors
    /// Returns a static structural failure for malformed bounds, memberships,
    /// packet-0 values or occurrence ordering. Callers must separately enforce
    /// serialized byte limits before deserializing untrusted data.
    pub fn validate(&self) -> Result<(), SourceTeletextError> {
        if !(1..=MAX_SOURCE_TELETEXT_HEADER_PACKETS).contains(&self.packet_count)
            || self.complete_pes > self.packet_count
            || self.pat_version > 31
            || self.programs.is_empty()
            || self.programs.len() > MAX_SOURCE_TELETEXT_HEADER_PROGRAMS
            || self.headers.len() > MAX_SOURCE_TELETEXT_OBSERVED_HEADERS
        {
            return Err(SourceTeletextError::HeaderWindow);
        }
        let mut memberships = BTreeSet::new();
        for program in &self.programs {
            if program.program_number == 0
                || !(0x20..=0x1ffe).contains(&program.pmt_pid)
                || !(0x20..=0x1fff).contains(&program.pcr_pid)
                || program.pmt_version > 31
                || program.descriptors.len() > MAX_SOURCE_TELETEXT_HEADER_DESCRIPTOR_BYTES
                || !memberships.insert((program.program_number, program.pmt_pid))
            {
                return Err(SourceTeletextError::HeaderProgram);
            }
        }
        let mut previous: Option<&SourceTeletextObservedHeader> = None;
        for header in &self.headers {
            if header.wire_magazine > 7
                || header.wire_page == 0xff
                || header.subcode & !0x3f7f != 0
                || header.controls > 0x7ff
                || !(header.line_offset == 0 || (7..=22).contains(&header.line_offset))
                || header.corrected_address_bytes > 2
                || header.corrected_header_bytes > 8
                || header.completion_packet >= self.packet_count
                || header.pes_ordinal >= self.complete_pes
                || header.unit_ordinal > 32_726
                || header.pts.is_some_and(|pts| pts >= 1_u64 << 33)
            {
                return Err(SourceTeletextError::ObservedHeader);
            }
            if let Some(previous) = previous {
                let key = |header: &SourceTeletextObservedHeader| {
                    (
                        header.completion_packet,
                        header.pes_ordinal,
                        header.unit_ordinal,
                    )
                };
                if key(header) <= key(previous)
                    || header.pes_ordinal < previous.pes_ordinal
                    || (header.pes_ordinal == previous.pes_ordinal
                        && header.completion_packet != previous.completion_packet)
                {
                    return Err(SourceTeletextError::HeaderOrder);
                }
            }
            previous = Some(header);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "source_headers_tests.rs"]
mod tests;

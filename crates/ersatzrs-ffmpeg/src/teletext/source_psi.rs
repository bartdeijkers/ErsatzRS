//! Read-only current PSI from a bounded, caller-supplied MPEG-TS window.
//!
//! Unlike authored-output admission, the requested elementary PID may already
//! exist and a source PMT may have no PCR. No spare PID is reserved, program is
//! selected, transport is changed or page/readiness/publication claim is made.

use std::collections::BTreeMap;

use mpeg2ts_reader::psi::{pmt::PmtSection, TableSyntaxHeader};

use super::{
    psi::{current_section, ProgramPmt, TableSections, MAX_DISCOVERY_PACKETS},
    psi_framing::SectionAssembler,
    signalling::{
        valid_packet, validate_pid, validate_source_descriptor_bytes, validate_source_pmt,
        RawDescriptors,
    },
    TeletextPsiError,
};

/// Exact complete current table, including its identity, version and CRC bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedSourcePsiTable {
    pub table_id: u8,
    pub table_id_extension: u16,
    pub version: u8,
    /// Ascending section-number order, without dropping descriptor bytes.
    pub sections: Vec<Vec<u8>>,
}

impl ObservedSourcePsiTable {
    fn new(sections: Vec<Vec<u8>>) -> Self {
        let first = &sections[0];
        let header = TableSyntaxHeader::new(&first[3..8]);
        Self {
            table_id: first[0],
            table_id_extension: header.id(),
            version: header.version(),
            sections,
        }
    }
}

/// One exact stream record from a validated source PMT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedSourceElementaryStream {
    pub pid: u16,
    pub stream_type: u8,
    /// Raw tag/length/payload bytes, preserving order and unknown descriptors.
    pub descriptors: Vec<u8>,
}

/// One PAT-advertised program, without selecting among shared elementary PIDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedSourceProgram {
    pub program_number: u16,
    pub pmt_pid: u16,
    /// `0x1fff` explicitly represents no programme PCR; this is not a clock grant.
    pub pcr_pid: u16,
    pub pmt: ObservedSourcePsiTable,
    pub program_descriptors: Vec<u8>,
    pub streams: Vec<ObservedSourceElementaryStream>,
}

/// Complete current relationships observed only in the supplied finite window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedSourcePsi {
    pub requested_pid: u16,
    /// Packets supplied in the current bounded observation window. A continuing
    /// owner tracks its total physical source position separately.
    pub packets_observed: usize,
    /// At least one previously observed current table was replaced in this window.
    /// Earlier payload cannot be associated with the final snapshot implicitly.
    pub current_tables_changed: bool,
    pub pat: ObservedSourcePsiTable,
    /// `None` means no CAT packet was observed, not that the source has no CAT.
    pub cat: Option<ObservedSourcePsiTable>,
    /// Every advertised non-network program, sorted by PMT PID.
    pub programs: Vec<ObservedSourceProgram>,
}

impl ObservedSourcePsi {
    /// Every membership of the requested PID, preserving multi-program ambiguity.
    /// An empty iterator proves only absence from these observed current PMTs.
    pub fn memberships(
        &self,
    ) -> impl Iterator<Item = (&ObservedSourceProgram, &ObservedSourceElementaryStream)> {
        let pid = self.requested_pid;
        self.programs.iter().flat_map(move |program| {
            program
                .streams
                .iter()
                .filter(move |stream| stream.pid == pid)
                .map(move |stream| (program, stream))
        })
    }
}

/// A bounded report-only PSI reader for one exact existing elementary PID.
///
/// At most 32,768 packets (6,160,384 supplied bytes) per explicit window, 128 PAT programs and 256
/// sections of at most 1,024 bytes per PAT/CAT are admitted. PMTs retain the
/// existing single-section validation boundary. Future table framing/CRC is validated, but future tables
/// never replace current evidence. A new current PAT invalidates all dependent
/// PMTs, even when entries appear unchanged. New current PMT/CAT versions replace
/// their prior evidence; same-version contradictions and malformed input fence
/// the reader permanently. Partial current PSI makes `observation` retryably
/// incomplete. A returned report is a historical snapshot, never publication or
/// selected-page authority; callers must bind it to their actual source epoch.
///
/// Without explicit continuation, the packet limit applies to the entire reader.
/// Continuation retains this same source epoch's PSI/continuity state; it is not
/// discovery for an independent decode window, which needs a fresh observer.
/// There is no I/O, task, decoder cache or implicit source restart here.
pub struct SourceTeletextPsiObserver {
    pid: u16,
    packets: usize,
    current_tables_changed: bool,
    failure: Option<TeletextPsiError>,
    pat_assembler: SectionAssembler,
    pat: TableSections,
    cat_assembler: SectionAssembler,
    cat: TableSections,
    cat_seen: bool,
    pmts: BTreeMap<u16, ProgramPmt>,
}

impl SourceTeletextPsiObserver {
    /// Observe this exact elementary PID; never derive or reserve a spare PID.
    /// # Errors
    /// Reserved, null or out-of-range elementary PIDs are rejected.
    pub fn new(pid: u16) -> Result<Self, TeletextPsiError> {
        validate_pid(pid)?;
        Ok(Self {
            pid,
            packets: 0,
            current_tables_changed: false,
            failure: None,
            pat_assembler: SectionAssembler::default(),
            pat: TableSections::default(),
            cat_assembler: SectionAssembler::default(),
            cat: TableSections::default(),
            cat_seen: false,
            pmts: BTreeMap::new(),
        })
    }

    /// Observe one complete packet without changing its bytes.
    /// # Errors
    /// Any packet, framing, CRC, continuity, table or capacity failure is sticky.
    pub fn push(&mut self, raw: &[u8; 188]) -> Result<(), TeletextPsiError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self.observe(raw);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    /// Begin another bounded window in the same continuously observed source.
    ///
    /// Only the packet counter is cleared. Complete and in-flight sections,
    /// continuity counters, table-change evidence and failures are retained.
    /// The owner must bound total source work and its absolute deadline, and
    /// retain its own absolute packet count. Observations report this window's
    /// count and may retain tables acquired in earlier windows; they cannot
    /// establish self-contained PSI for an independent decoder snapshot.
    /// Call before exceeding the current window's limit; capacity failure is
    /// terminal and cannot be recovered through continuation.
    /// # Errors
    /// Returns the existing sticky failure without changing any reader state.
    pub fn begin_continuation_window(&mut self) -> Result<(), TeletextPsiError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        self.packets = 0;
        Ok(())
    }

    /// Cheap current-table completeness check, without copying table payloads.
    /// A false result also covers a permanently failed observer.
    #[must_use]
    pub fn has_complete_current_tables(&self) -> bool {
        self.failure.is_none()
            && self.pat.complete()
            && !self.pat_assembler.is_pending()
            && self
                .pmts
                .values()
                .all(|pmt| pmt.section.is_some() && !pmt.assembler.is_pending())
            && (!self.cat_seen || self.cat.complete() && !self.cat_assembler.is_pending())
    }

    /// Copy complete current PAT, every PMT and any observed CAT.
    /// # Errors
    /// Incomplete is retryable after further packets. Sticky failures are retained.
    pub fn observation(&self) -> Result<ObservedSourcePsi, TeletextPsiError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if !self.has_complete_current_tables() {
            return Err(TeletextPsiError::Incomplete);
        }
        let mut programs = Vec::with_capacity(self.pmts.len());
        for (&pmt_pid, pmt) in &self.pmts {
            let section = pmt.section.as_ref().ok_or(TeletextPsiError::Incomplete)?;
            // The shared validator already checked the complete record extent.
            let (_, _, pcr_pid) = validate_source_pmt(section, pmt_pid)?;
            let parsed = PmtSection::from_bytes(&section[8..section.len() - 4])
                .map_err(|_| super::TeletextMultiplexError::Pmt)?;
            let mut streams = Vec::new();
            for stream in parsed.streams() {
                streams.push(ObservedSourceElementaryStream {
                    pid: u16::from(stream.elementary_pid()),
                    stream_type: u8::from(stream.stream_type()),
                    descriptors: raw_descriptors(stream.descriptors::<RawDescriptors<'_>>())?,
                });
            }
            programs.push(ObservedSourceProgram {
                program_number: pmt.program,
                pmt_pid,
                pcr_pid,
                pmt: ObservedSourcePsiTable::new(vec![section.clone()]),
                program_descriptors: raw_descriptors(parsed.descriptors::<RawDescriptors<'_>>())?,
                streams,
            });
        }
        Ok(ObservedSourcePsi {
            requested_pid: self.pid,
            packets_observed: self.packets,
            current_tables_changed: self.current_tables_changed,
            pat: ObservedSourcePsiTable::new(self.pat.sections.values().cloned().collect()),
            cat: self.cat_seen.then(|| {
                ObservedSourcePsiTable::new(self.cat.sections.values().cloned().collect())
            }),
            programs,
        })
    }

    fn observe(&mut self, raw: &[u8; 188]) -> Result<(), TeletextPsiError> {
        self.packets += 1;
        if self.packets > MAX_DISCOVERY_PACKETS {
            return Err(TeletextPsiError::Capacity);
        }
        let packet = valid_packet(raw)?;
        let pid = u16::from(packet.pid());
        match pid {
            0 => {
                for section in self.pat_assembler.push(&packet)? {
                    if !current_section(&section, 0)? {
                        continue;
                    }
                    let previous = !self.pat.sections.is_empty();
                    if self.pat.push(section)? {
                        self.current_tables_changed |= previous;
                        self.pmts.clear();
                    }
                    if self.pat.complete() && self.pmts.is_empty() {
                        self.pmts = self.pat.source_programs()?;
                    }
                }
            }
            1 => {
                self.cat_seen = true;
                for section in self.cat_assembler.push(&packet)? {
                    if !current_section(&section, 1)? {
                        continue;
                    }
                    validate_source_descriptor_bytes(&section[8..section.len() - 4], None)?;
                    let previous = !self.cat.sections.is_empty();
                    self.current_tables_changed |= self.cat.push(section)? && previous;
                }
            }
            _ => {
                if let Some(pmt) = self.pmts.get_mut(&pid) {
                    for section in pmt.assembler.push(&packet)? {
                        if !current_section(&section, 2)? {
                            continue;
                        }
                        let (program, _, _) = validate_source_pmt(&section, pid)?;
                        if program != pmt.program {
                            return Err(TeletextPsiError::Program);
                        }
                        if let Some(previous) = &pmt.section {
                            if section[5] == previous[5] && section != *previous {
                                return Err(TeletextPsiError::Version);
                            }
                            self.current_tables_changed |= section != *previous;
                        }
                        pmt.section = Some(section);
                    }
                }
            }
        }
        Ok(())
    }
}

fn raw_descriptors<'a>(
    descriptors: impl Iterator<
        Item = Result<RawDescriptors<'a>, mpeg2ts_reader::descriptor::DescriptorError>,
    >,
) -> Result<Vec<u8>, TeletextPsiError> {
    let mut bytes = Vec::new();
    for descriptor in descriptors {
        let RawDescriptors::Other(descriptor) =
            descriptor.map_err(|_| super::TeletextMultiplexError::Descriptor)?;
        bytes.push(descriptor.tag);
        bytes.push(
            u8::try_from(descriptor.payload.len())
                .map_err(|_| super::TeletextMultiplexError::Descriptor)?,
        );
        bytes.extend_from_slice(descriptor.payload);
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "source_psi_tests.rs"]
mod tests;

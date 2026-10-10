use std::collections::{BTreeMap, BTreeSet};

use mpeg2ts_reader::{
    mpegts_crc,
    psi::{
        pat::{PatSection, ProgramDescriptor},
        TableSyntaxHeader,
    },
};

use super::{
    psi_framing::SectionAssembler,
    psi_monitor::{MonitoredTable, TeletextPsiMonitor},
    signalling::{valid_packet, validate_descriptor_bytes, validate_pid, validate_pmt},
    TeletextMultiplexError, TeletextPmtPlan, TeletextServiceDescriptor,
};

const MAX_DISCOVERY_PACKETS: usize = 32_768;
const MAX_PROGRAMS: usize = 128;

/// Source-PSI admission/monitor failures, without retaining programme content.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPsiError {
    /// Packet, PMT, descriptor or dedicated-PID validation failed.
    #[error(transparent)]
    Multiplex(#[from] TeletextMultiplexError),
    /// Pointer, section extent, table identity or reserved bits are invalid.
    #[error("teletext source PSI framing is invalid")]
    Framing,
    /// A section does not pass MPEG CRC validation.
    #[error("teletext source PSI CRC is invalid")]
    Crc,
    /// A PSI payload was lost, changed with the same counter, or discontinuous.
    #[error("teletext source PSI continuity is invalid")]
    Continuity,
    /// A table contradicts another section with the same identity/version.
    #[error("teletext source PSI version has contradictory sections")]
    Version,
    /// PAT entries repeat a program/PID or contain an invalid record.
    #[error("teletext source PAT contains invalid or duplicate entries")]
    Pat,
    /// The selected program is zero or absent from a complete current PAT.
    #[error("teletext selected program is absent from the source PAT")]
    Program,
    /// Admission must wait for a complete current PAT and every advertised PMT.
    #[error("teletext source PSI discovery is incomplete")]
    Incomplete,
    /// The discovery window or advertised program count exceeds its bound.
    #[error("teletext source PSI discovery exceeds its bound")]
    Capacity,
    /// Current ownership metadata changed after the generation was admitted.
    #[error("teletext source PSI on PID {pid} changed after admission")]
    SourceChanged { pid: u16 },
    /// An ended generation cannot accept packets from a new producer.
    #[error("teletext source generation has already ended")]
    Closed,
}

/// Assemble source PAT/CAT/PMTs before planning an authored teletext service.
///
/// This finite admission scan accepts at most 32,768 packets, 128 programs and
/// 256 sections per PAT/CAT, each at most 1,024 bytes. Every advertised PMT and
/// observed CAT must be complete; the requested PID must be absent from their
/// PMT/PCR/ES/CA/NIT ownership and from every observed packet. Current table
/// changes invalidate dependent evidence; future tables never replace current
/// ones. Packet loss or malformed input permanently fences this instance.
///
/// A plan proves only the supplied source window. Seal admission with
/// `Self::into_monitor` to keep checking later PSI/PID use. The live host still
/// must compose that fence before publication, install the PMT, verify
/// rates/buffers and own cancellation/restart. This
/// reader never consumes media sources or modifies audiovisual packets.
pub struct TeletextPsiDiscovery {
    program: u16,
    pid: u16,
    packets: usize,
    failure: Option<TeletextPsiError>,
    pat_assembler: SectionAssembler,
    pat: TableSections,
    cat_assembler: SectionAssembler,
    cat: TableSections,
    cat_seen: bool,
    pmts: BTreeMap<u16, ProgramPmt>,
}

impl TeletextPsiDiscovery {
    /// Begin a new source-bound admission scan for an explicit program and PID.
    /// # Errors
    /// Rejects program zero and reserved/null/out-of-range elementary PIDs.
    pub fn new(program: u16, pid: u16) -> Result<Self, TeletextPsiError> {
        if program == 0 {
            return Err(TeletextPsiError::Program);
        }
        validate_pid(pid)?;
        Ok(Self {
            program,
            pid,
            packets: 0,
            failure: None,
            pat_assembler: SectionAssembler::default(),
            pat: TableSections::default(),
            cat_assembler: SectionAssembler::default(),
            cat: TableSections::default(),
            cat_seen: false,
            pmts: BTreeMap::new(),
        })
    }

    /// Observe one complete host-output packet without modifying it.
    /// # Errors
    /// Errors permanently invalidate admission; start a new instance only when
    /// the host establishes a new source/output generation.
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

    /// Derive a PMT plan only from complete current ownership evidence.
    /// # Errors
    /// Incomplete discovery is retryable after more packets. Source failure,
    /// missing program or insufficient PMT capacity prevents admission.
    pub fn plan(
        &self,
        service: &TeletextServiceDescriptor,
    ) -> Result<TeletextPmtPlan, TeletextPsiError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if !self.pat.complete() || self.pat_assembler.is_pending() {
            return Err(TeletextPsiError::Incomplete);
        }
        let selected = self
            .pmts
            .iter()
            .find(|(_, pmt)| pmt.program == self.program)
            .ok_or(TeletextPsiError::Program)?;
        if self
            .pmts
            .values()
            .any(|pmt| pmt.section.is_none() || pmt.assembler.is_pending())
            || self.cat_seen && (!self.cat.complete() || self.cat_assembler.is_pending())
        {
            return Err(TeletextPsiError::Incomplete);
        }
        let section = selected
            .1
            .section
            .as_ref()
            .ok_or(TeletextPsiError::Incomplete)?;
        // Rewriting this table must not modify another program's PCR/ES/CA PID.
        for (&pmt_pid, pmt) in &self.pmts {
            if pmt_pid != *selected.0 {
                validate_pmt(
                    pmt.section.as_ref().ok_or(TeletextPsiError::Incomplete)?,
                    pmt_pid,
                    *selected.0,
                )?;
            }
        }
        for cat in self.cat.sections.values() {
            validate_descriptor_bytes(&cat[8..cat.len() - 4], *selected.0)?;
        }
        Ok(TeletextPmtPlan::new(
            section,
            *selected.0,
            self.pid,
            service,
        )?)
    }

    /// Seal complete current ownership evidence into a continuing source monitor.
    ///
    /// Consumes this discovery state, retaining PSI continuity from its last
    /// observed packets. Feed only subsequent raw output packets to the monitor;
    /// replaying the startup window would contradict source continuity.
    /// # Errors
    /// Requires a complete valid admission and enough space for the service PMT.
    pub fn into_monitor(
        self,
        service: &TeletextServiceDescriptor,
    ) -> Result<TeletextPsiMonitor, TeletextPsiError> {
        let plan = self.plan(service)?;
        let mut tables = BTreeMap::new();
        tables.insert(
            0,
            MonitoredTable::new(self.pat_assembler, self.pat.sections),
        );
        tables.insert(
            1,
            MonitoredTable::new(self.cat_assembler, self.cat.sections),
        );
        for (pid, pmt) in self.pmts {
            let section = pmt.section.ok_or(TeletextPsiError::Incomplete)?;
            tables.insert(
                pid,
                MonitoredTable::new(pmt.assembler, BTreeMap::from([(0, section)])),
            );
        }
        Ok(TeletextPsiMonitor::new(plan, tables))
    }

    fn observe(&mut self, raw: &[u8; 188]) -> Result<(), TeletextPsiError> {
        self.packets += 1;
        if self.packets > MAX_DISCOVERY_PACKETS {
            return Err(TeletextPsiError::Capacity);
        }
        let packet = valid_packet(raw)?;
        let pid = u16::from(packet.pid());
        if pid == self.pid {
            return Err(TeletextMultiplexError::PidCollision { pid }.into());
        }
        match pid {
            0 => {
                for section in self.pat_assembler.push(&packet)? {
                    if !current_section(&section, 0)? {
                        continue;
                    }
                    if self.pat.push(section)? {
                        self.pmts.clear();
                    }
                    if self.pat.complete() && self.pmts.is_empty() {
                        self.pmts = self.pat.programs(self.pid)?;
                    }
                }
            }
            1 => {
                self.cat_seen = true;
                for section in self.cat_assembler.push(&packet)? {
                    if !current_section(&section, 1)? {
                        continue;
                    }
                    validate_descriptor_bytes(&section[8..section.len() - 4], self.pid)?;
                    self.cat.push(section)?;
                }
            }
            _ => {
                if let Some(pmt) = self.pmts.get_mut(&pid) {
                    for section in pmt.assembler.push(&packet)? {
                        if !current_section(&section, 2)? {
                            continue;
                        }
                        let (program, _, _) = validate_pmt(&section, pid, self.pid)?;
                        if program != pmt.program {
                            return Err(TeletextPsiError::Program);
                        }
                        if let Some(previous) = &pmt.section {
                            if section[5] == previous[5] && section != *previous {
                                return Err(TeletextPsiError::Version);
                            }
                        }
                        pmt.section = Some(section);
                    }
                }
            }
        }
        Ok(())
    }
}

struct ProgramPmt {
    program: u16,
    assembler: SectionAssembler,
    section: Option<Vec<u8>>,
}

#[derive(Default)]
struct TableSections {
    identity: Option<(u16, u8, u8)>,
    sections: BTreeMap<u8, Vec<u8>>,
}

impl TableSections {
    // Returns true when a newly observed current generation invalidates evidence.
    fn push(&mut self, section: Vec<u8>) -> Result<bool, TeletextPsiError> {
        let header = TableSyntaxHeader::new(&section[3..8]);
        let identity = (header.id(), header.version(), header.last_section_number());
        let changed = match self.identity {
            Some(previous) if previous.0 == identity.0 && previous.1 == identity.1 => {
                if previous.2 != identity.2 {
                    return Err(TeletextPsiError::Version);
                }
                false
            }
            _ => true,
        };
        if changed {
            self.sections.clear();
            self.identity = Some(identity);
        }
        let number = header.section_number();
        if self
            .sections
            .get(&number)
            .is_some_and(|previous| previous != &section)
        {
            return Err(TeletextPsiError::Version);
        }
        self.sections.insert(number, section);
        Ok(changed)
    }

    fn complete(&self) -> bool {
        self.identity
            .is_some_and(|(_, _, last)| self.sections.len() == usize::from(last) + 1)
    }

    fn programs(&self, candidate: u16) -> Result<BTreeMap<u16, ProgramPmt>, TeletextPsiError> {
        let mut programs = BTreeSet::new();
        let mut pids = BTreeSet::new();
        let mut pmts = BTreeMap::new();
        let mut network = false;
        for section in self.sections.values() {
            let bytes = &section[8..section.len() - 4];
            if bytes.len() % 4 != 0 || bytes.chunks_exact(4).any(|entry| entry[2] & 0xe0 != 0xe0) {
                return Err(TeletextPsiError::Pat);
            }
            for entry in PatSection::new(bytes).programs() {
                let pid = u16::from(entry.pid());
                if pid == candidate {
                    return Err(TeletextMultiplexError::PidCollision { pid }.into());
                }
                if !pids.insert(pid) {
                    return Err(TeletextPsiError::Pat);
                }
                match entry {
                    ProgramDescriptor::Network { .. } => {
                        if network || pid == 0 || pid == 1 || pid == 0x1fff {
                            return Err(TeletextPsiError::Pat);
                        }
                        network = true;
                    }
                    ProgramDescriptor::Program { program_number, .. } => {
                        validate_pid(pid).map_err(|_| TeletextPsiError::Pat)?;
                        if !programs.insert(program_number) {
                            return Err(TeletextPsiError::Pat);
                        }
                        if programs.len() > MAX_PROGRAMS {
                            return Err(TeletextPsiError::Capacity);
                        }
                        pmts.insert(
                            pid,
                            ProgramPmt {
                                program: program_number,
                                assembler: SectionAssembler::default(),
                                section: None,
                            },
                        );
                    }
                }
            }
        }
        Ok(pmts)
    }
}

pub(super) fn current_section(section: &[u8], table_id: u8) -> Result<bool, TeletextPsiError> {
    if section.len() < 12
        || section[0] != table_id
        || section[5] & 0xc0 != 0xc0
        || section[6] > section[7]
    {
        return Err(TeletextPsiError::Framing);
    }
    if mpegts_crc::sum32(section) != 0 {
        return Err(TeletextPsiError::Crc);
    }
    Ok(section[5] & 1 != 0)
}

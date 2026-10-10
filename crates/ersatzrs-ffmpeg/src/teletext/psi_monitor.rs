use std::collections::BTreeMap;

use mpeg2ts_reader::packet::Packet;

use super::{
    psi::current_section,
    psi_framing::{validate_section_start, SectionAssembler},
    signalling::{valid_packet, MAX_WINDOW_PACKETS},
    TeletextMultiplexError, TeletextPmtPlan, TeletextPsiError,
};

/// Continuing PSI/PID fence for one admitted raw output generation.
///
/// Create through `TeletextPsiDiscovery::into_monitor`, which moves complete
/// current PAT/CAT/all-program PMT evidence and source continuity into this
/// object. Metadata remains immutable and bounded by the discovery limits.
/// Monitoring has no lifetime packet limit; each checked window is bounded.
///
/// Feed only new host-output packets, exactly once and before rewriting or
/// publication. A false readiness result means the host must retain its output
/// until all split sections are validated. On any error this generation stays
/// permanently fenced; the host must stop publication and explicitly readmit a
/// changed source. This monitor owns no task, buffer, media source or clock.
/// PCR/rate/deadline admission, bounded retained-output ownership, generation
/// publication and async session cancellation remain the host's obligations.
pub struct TeletextPsiMonitor {
    plan: TeletextPmtPlan,
    tables: BTreeMap<u16, MonitoredTable>,
    failure: Option<TeletextPsiError>,
    closed: bool,
}

impl TeletextPsiMonitor {
    pub(super) fn new(plan: TeletextPmtPlan, tables: BTreeMap<u16, MonitoredTable>) -> Self {
        Self {
            plan,
            tables,
            failure: None,
            closed: false,
        }
    }

    /// Immutable PMT plan belonging to this admitted source generation.
    #[must_use]
    pub const fn plan(&self) -> &TeletextPmtPlan {
        &self.plan
    }

    /// Inspect new raw packets before the owner rewrites/releases held output.
    ///
    /// True means all observed table sections have finished and matched the
    /// admitted current metadata. False means keep holding output; feed only
    /// subsequent packets, never replay the held prefix. Future PAT/CAT/other-
    /// program PMTs cannot replace current ownership. A future selected PMT is
    /// rejected because its installer cannot silently rewrite a next table.
    /// # Errors
    /// Source changes, malformed PSI/continuity and observed owned-PID use
    /// permanently fence the generation. Oversized windows are also rejected.
    pub fn inspect_window(&mut self, window: &[[u8; 188]]) -> Result<bool, TeletextPsiError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.closed {
            return Err(TeletextPsiError::Closed);
        }
        let result = self.inspect(window);
        if let Err(error) = &result {
            self.failure = Some(error.clone());
        }
        result
    }

    /// Close this generation at EOF without releasing incomplete/failed PSI.
    ///
    /// Closure is idempotent. Later input needs a newly admitted source owner;
    /// the old monitor cannot be reused for another producer or restart.
    /// # Errors
    /// Returns the retained generation failure or incomplete PSI framing.
    pub fn finish(&mut self) -> Result<(), TeletextPsiError> {
        self.closed = true;
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self
            .tables
            .values()
            .any(|table| table.assembler.is_pending())
        {
            self.failure = Some(TeletextPsiError::Incomplete);
            return Err(TeletextPsiError::Incomplete);
        }
        Ok(())
    }

    fn inspect(&mut self, window: &[[u8; 188]]) -> Result<bool, TeletextPsiError> {
        if window.len() > MAX_WINDOW_PACKETS {
            return Err(TeletextPsiError::Capacity);
        }
        for raw in window {
            let packet = valid_packet(raw)?;
            let pid = u16::from(packet.pid());
            if pid == self.plan.elementary_pid() {
                return Err(TeletextMultiplexError::PidCollision { pid }.into());
            }
            if let Some(table) = self.tables.get_mut(&pid) {
                table.observe(&packet, pid, self.plan.pmt_pid())?;
            }
        }
        Ok(self
            .tables
            .values()
            .all(|table| !table.assembler.is_pending()))
    }
}

pub(super) struct MonitoredTable {
    assembler: SectionAssembler,
    sections: BTreeMap<u8, Vec<u8>>,
}

impl MonitoredTable {
    pub(super) fn new(assembler: SectionAssembler, sections: BTreeMap<u8, Vec<u8>>) -> Self {
        Self {
            assembler,
            sections,
        }
    }

    fn observe(
        &mut self,
        packet: &Packet<'_>,
        pid: u16,
        selected_pmt: u16,
    ) -> Result<(), TeletextPsiError> {
        if let Some(payload) = packet.payload() {
            if !self.assembler.is_pending() && !self.assembler.is_duplicate(packet) {
                // Standalone PSI stuffing has no section to validate.
                if packet.payload_unit_start_indicator() || payload.iter().any(|&byte| byte != 0xff)
                {
                    validate_section_start(packet)?;
                }
            }
        }
        for section in self.assembler.push(packet)? {
            let table_id = match pid {
                0 => 0,
                1 => 1,
                _ => 2,
            };
            if !current_section(&section, table_id)? {
                if pid == selected_pmt {
                    return Err(TeletextPsiError::SourceChanged { pid });
                }
                continue;
            }
            if self.sections.get(&section[6]) != Some(&section) {
                return Err(TeletextPsiError::SourceChanged { pid });
            }
        }
        Ok(())
    }
}

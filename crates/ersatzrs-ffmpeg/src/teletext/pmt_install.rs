use mpeg2ts_reader::packet::Packet;

use super::{
    psi_framing::{validate_section_start, SectionAssembler},
    signalling::{valid_packet, MAX_WINDOW_PACKETS},
    TeletextMultiplexError, TeletextPmtPlan, TeletextPsiError, TeletextServiceDescriptor,
};

/// A PMT installation preflight failure never changes output or continuity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPmtInstallError {
    /// Invalid packets, occupied PID or insufficient bounded null capacity.
    #[error(transparent)]
    Multiplex(#[from] TeletextMultiplexError),
    /// Source PSI framing or continuity is invalid.
    #[error(transparent)]
    Psi(#[from] TeletextPsiError),
    /// Retain more input before rewriting a fragmented source PMT.
    #[error("teletext PMT installation needs the complete source section")]
    Incomplete,
    /// A source section differs from the admitted generation, including future tables.
    #[error("teletext source PMT changed after service admission")]
    SourceChanged,
    /// Rewriting would discard adaptation control data rather than stuffing.
    #[error("teletext source PMT carries unsupported adaptation control data")]
    Adaptation,
    /// A replacement changes raw ownership or does not advance the output version.
    #[error("teletext PMT reconfiguration differs from its current source ownership")]
    Reconfiguration,
}

/// Physical packet accounting for one complete atomic PMT installation window.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TeletextPmtInstallReport {
    /// A pending reconfiguration's new PMT was included in this output window.
    /// This reports construction only; the host must still acknowledge delivery.
    pub announced: bool,
    /// Complete source table repetitions represented by the rewritten output.
    pub repetitions: usize,
    /// Emitted payload PMT packets, including growth into null slots.
    pub pmt_packets: usize,
    /// Previously null packets used for the larger table.
    pub consumed_null_packets: usize,
    /// Surplus source PMT slots changed to valid null packets.
    pub retired_pmt_packets: usize,
}

/// Install an admitted PMT without shifting any audiovisual or clock packets.
///
/// Each call preflights at most 32,768 packets. The window must include complete
/// source PMT sections, and each must exactly match the admitted source bytes.
/// Only source PMT slots and null slots may change. Larger rewritten tables use
/// null capacity before the next source transmission starts; fragmented source
/// tables, multiple sections per payload and exact duplicate packets are handled
/// without dropping a table repetition. Output continuity is independent from
/// source continuity and persists across successful calls.
///
/// The host still owns PAT/CAT/other-program admission, later live PID/table
/// fencing, buffer/rate/deadline admission, publication and session cancellation.
/// This synchronous adapter neither reads a source nor paces a stream. Failed
/// or incomplete preflight preserves both the window and all installer state.
#[derive(Clone)]
pub struct TeletextPmtInstaller {
    plan: TeletextPmtPlan,
    input: SectionAssembler,
    next_output_cc: Option<u8>,
    first_source_epoch_packet: bool,
    pending_announcement: bool,
}

impl TeletextPmtInstaller {
    /// Bind installation to an exact admitted source and rewritten PMT generation.
    #[must_use]
    pub fn new(plan: TeletextPmtPlan) -> Self {
        Self {
            plan,
            input: SectionAssembler::default(),
            next_output_cc: None,
            first_source_epoch_packet: false,
            pending_announcement: false,
        }
    }

    /// Bind a newly started source epoch, permitting its first PMT startup reset.
    ///
    /// Only the first physical source PMT packet may carry adaptation flags
    /// exactly `0x80`. A successful installation consumes that exception even
    /// when the first packet is unflagged or adaptation-only. Rewritten PMT
    /// continuity starts a new output sequence, so this initial reset is stripped
    /// from owned PMT slots; audiovisual and PCR packets remain byte-for-byte
    /// unchanged. Every later reset and all other adaptation control are rejected.
    #[must_use]
    pub fn new_source_epoch(plan: TeletextPmtPlan) -> Self {
        Self {
            first_source_epoch_packet: true,
            ..Self::new(plan)
        }
    }

    /// Next emitted payload counter; unset until a table is successfully installed.
    #[must_use]
    pub const fn next_continuity_counter(&self) -> Option<u8> {
        self.next_output_cc
    }

    /// Prepare the next output table without changing any installer state.
    /// # Errors
    /// Rejects a pending source fragment or invalid descriptor/section capacity.
    pub fn prepare_reconfiguration(
        &self,
        service: Option<&TeletextServiceDescriptor>,
    ) -> Result<TeletextPmtPlan, TeletextPmtInstallError> {
        if self.input.is_pending() {
            return Err(TeletextPmtInstallError::Incomplete);
        }
        Ok(self.plan.reconfigure(service)?)
    }

    /// Change the output table while preserving source assembly and output CC.
    ///
    /// The next successful installation must contain the replacement table,
    /// using bounded null capacity when the source has not repeated its PMT.
    /// Host acknowledgement and generation admission remain separate.
    /// # Errors
    /// Rejects pending source fragments, foreign ownership or stale versions.
    pub fn reconfigure(&mut self, plan: TeletextPmtPlan) -> Result<(), TeletextPmtInstallError> {
        if self.input.is_pending() {
            return Err(TeletextPmtInstallError::Incomplete);
        }
        if !plan.is_reconfiguration_of(&self.plan) {
            return Err(TeletextPmtInstallError::Reconfiguration);
        }
        self.plan = plan;
        self.pending_announcement = true;
        Ok(())
    }

    /// Keep initially absent output byte-exact while recording its PMT cursor.
    /// The normal preflight retains fragment/continuity/startup fences; only a
    /// later acknowledged transition may change this unmodified source plan.
    pub(super) fn forward_unchanged(
        &mut self,
        window: &[[u8; 188]],
    ) -> Result<TeletextPmtInstallReport, TeletextPmtInstallError> {
        if window.len() > MAX_WINDOW_PACKETS {
            return Err(TeletextMultiplexError::Window.into());
        }
        if self.pending_announcement || self.plan.section() != self.plan.source_section() {
            return Err(TeletextPmtInstallError::Reconfiguration);
        }
        let mut input = self.input.clone();
        let mut first_source_epoch_packet = self.first_source_epoch_packet;
        self.preflight(window, &mut input, &mut first_source_epoch_packet)?;
        let mut next_cc = self.next_output_cc;
        for raw in window {
            let packet = Packet::new(raw);
            if u16::from(packet.pid()) == self.plan.pmt_pid() && packet.payload().is_some() {
                next_cc = Some((packet.continuity_counter().count() + 1) & 15);
            }
        }
        self.input = input;
        self.first_source_epoch_packet = first_source_epoch_packet;
        self.next_output_cc = next_cc;
        Ok(TeletextPmtInstallReport::default())
    }

    /// Atomically replace owned PMT slots and the required null slots.
    /// # Errors
    /// Rejects incomplete/changed sections, malformed or discontinuous input,
    /// adaptation control data and insufficient local capacity. Callers may
    /// retain more input for incomplete/capacity errors; invalid source evidence
    /// requires host failure/re-admission before publishing any output.
    pub fn install(
        &mut self,
        window: &mut [[u8; 188]],
    ) -> Result<TeletextPmtInstallReport, TeletextPmtInstallError> {
        if window.len() > MAX_WINDOW_PACKETS {
            return Err(TeletextMultiplexError::Window.into());
        }
        let mut input = self.input.clone();
        let mut first_source_epoch_packet = self.first_source_epoch_packet;
        let mut preflight = self.preflight(window, &mut input, &mut first_source_epoch_packet)?;
        let repetitions = preflight.transmissions.iter().map(|t| t.repetitions).sum();
        if self.pending_announcement && preflight.transmissions.is_empty() {
            // Synthetic announcements own only null slots. They do not consume
            // source CC or its first physical packet's startup-reset allowance.
            preflight
                .available
                .retain(|&index| u16::from(Packet::new(&window[index]).pid()) == 0x1fff);
            preflight.transmissions.push(Transmission {
                start: 0,
                source_cc: self.next_output_cc.unwrap_or(0),
                repetitions: 1,
            });
        }
        let packet_count = self.plan.packets(0)?.len();
        let (allocations, consumed_null_packets) = allocate(&preflight, packet_count, window)?;
        let mut cc = self.next_output_cc;
        // All fallible work finishes before the first mutation.
        let mut emitted = Vec::new();
        for (transmission, positions) in preflight.transmissions.iter().zip(&allocations) {
            let mut position = positions.iter();
            for _ in 0..transmission.repetitions {
                let start = cc.unwrap_or(transmission.source_cc);
                let packets = self.plan.packets(start)?;
                cc = Some(
                    (start
                        + u8::try_from(packets.len())
                            .map_err(|_| TeletextMultiplexError::Continuity)?)
                        & 15,
                );
                for (&index, raw) in position.by_ref().take(packets.len()).zip(packets) {
                    emitted.push((index, raw));
                }
            }
        }
        let retained_owned = emitted.len() - consumed_null_packets;
        let report = TeletextPmtInstallReport {
            announced: self.pending_announcement,
            repetitions,
            pmt_packets: emitted.len(),
            consumed_null_packets,
            retired_pmt_packets: preflight.owned.len() - retained_owned,
        };
        for index in preflight.owned {
            window[index] = null_packet();
        }
        for (index, raw) in emitted {
            window[index] = raw;
        }
        self.input = input;
        self.next_output_cc = cc;
        self.first_source_epoch_packet = first_source_epoch_packet;
        self.pending_announcement = false;
        Ok(report)
    }

    fn preflight(
        &self,
        window: &[[u8; 188]],
        input: &mut SectionAssembler,
        first_source_epoch_packet: &mut bool,
    ) -> Result<Preflight, TeletextPmtInstallError> {
        let mut result = Preflight::default();
        let mut transmission: Option<Transmission> = None;
        for (index, raw) in window.iter().enumerate() {
            let packet = valid_packet(raw)?;
            let pid = u16::from(packet.pid());
            if pid == self.plan.elementary_pid() {
                return Err(TeletextMultiplexError::PidCollision { pid }.into());
            }
            if pid == 0x1fff || pid == self.plan.pmt_pid() {
                result.available.push(index);
            }
            if pid != self.plan.pmt_pid() {
                continue;
            }
            result.owned.push(index);
            let first_epoch_packet = *first_source_epoch_packet;
            *first_source_epoch_packet = false;
            if packet
                .adaptation_field()
                .is_some_and(|_| raw[5] != 0 && !(first_epoch_packet && raw[5] == 0x80))
            {
                return Err(TeletextPmtInstallError::Adaptation);
            }
            if packet.payload().is_some() && !input.is_duplicate(&packet) {
                if !input.is_pending() {
                    validate_section_start(&packet)?;
                }
                transmission.get_or_insert(Transmission {
                    start: index,
                    source_cc: packet.continuity_counter().count(),
                    repetitions: 0,
                });
            }
            for section in input.push_source_epoch_packet(&packet, first_epoch_packet)? {
                if section != self.plan.source_section() {
                    return Err(TeletextPmtInstallError::SourceChanged);
                }
                let group = transmission.as_mut().ok_or(TeletextPsiError::Framing)?;
                group.repetitions += 1;
            }
            if !input.is_pending() {
                if let Some(group) = transmission.take() {
                    if group.repetitions > 0 {
                        result.transmissions.push(group);
                    }
                }
            }
        }
        if input.is_pending() || transmission.is_some() {
            return Err(TeletextPmtInstallError::Incomplete);
        }
        Ok(result)
    }
}

#[derive(Default)]
struct Preflight {
    owned: Vec<usize>,
    available: Vec<usize>,
    transmissions: Vec<Transmission>,
}

struct Transmission {
    start: usize,
    source_cc: u8,
    repetitions: usize,
}

fn allocate(
    preflight: &Preflight,
    packet_count: usize,
    window: &[[u8; 188]],
) -> Result<(Vec<Vec<usize>>, usize), TeletextMultiplexError> {
    let mut allocations = Vec::new();
    let mut consumed_null = 0;
    let mut cursor = 0;
    for (index, transmission) in preflight.transmissions.iter().enumerate() {
        let end = preflight
            .transmissions
            .get(index + 1)
            .map_or(window.len(), |next| next.start);
        while cursor < preflight.available.len() && preflight.available[cursor] < transmission.start
        {
            cursor += 1;
        }
        let start = cursor;
        while cursor < preflight.available.len() && preflight.available[cursor] < end {
            cursor += 1;
        }
        let needed = transmission.repetitions * packet_count;
        let available = cursor - start;
        if needed > available {
            return Err(TeletextMultiplexError::Capacity { needed, available });
        }
        let positions = preflight.available[start..start + needed].to_vec();
        consumed_null += positions
            .iter()
            .filter(|&&index| u16::from(Packet::new(&window[index]).pid()) == 0x1fff)
            .count();
        allocations.push(positions);
    }
    Ok((allocations, consumed_null))
}

fn null_packet() -> [u8; 188] {
    let mut raw = [0xff; 188];
    raw[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    raw
}

#[cfg(test)]
#[path = "pmt_reconfiguration_tests.rs"]
mod reconfiguration_tests;

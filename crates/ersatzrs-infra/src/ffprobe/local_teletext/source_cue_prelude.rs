//! Bounded requested-page reset locators, without initial-state or cue admission.

use ersatzrs_domain::TeletextPageNumber;
use ersatzrs_ffmpeg::teletext::{
    source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent},
    source_header::{
        observe_dvb_teletext_header, SuppliedTeletextHeader, TeletextHeaderObservation,
    },
    source_native_epoch::SourceNativeEpoch,
    source_observation::SuppliedTeletextHeaderWindow,
    source_pes::SuppliedTeletextPes,
    TeletextTimestamp,
};

use crate::ffprobe::{FfprobeError, Result};

/// One protected requested-page C4 header from a complete selected PES.
///
/// The preceding PAT/PMT packet positions are locator hints only. A fresh
/// bounded snapshot must validate its own current PSI and retain this exact
/// anchor. A reset header alone proves neither preceding resource/context state
/// nor a complete page, presentation interval or actual consumer eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceTeletextPagePrelude {
    psi_start_packet: u64,
    source_pusi_packet: u64,
    completion_source_packet: u64,
    raw_pts: TeletextTimestamp,
    native_pts_ticks: i64,
    header: SuppliedTeletextHeader,
}

impl SourceTeletextPagePrelude {
    /// Preceding PAT PUSI cached when the preceding selected PMT PUSI started.
    #[must_use]
    pub const fn psi_start_packet(self) -> u64 {
        self.psi_start_packet
    }

    #[must_use]
    pub const fn source_pusi_packet(self) -> u64 {
        self.source_pusi_packet
    }

    #[must_use]
    pub const fn completion_source_packet(self) -> u64 {
        self.completion_source_packet
    }

    #[must_use]
    pub const fn raw_pts(self) -> TeletextTimestamp {
        self.raw_pts
    }

    /// Candidate native program coordinate in 27 MHz ticks, not consumer time.
    #[must_use]
    pub const fn native_pts_ticks(self) -> i64 {
        self.native_pts_ticks
    }

    #[must_use]
    pub const fn header(self) -> SuppliedTeletextHeader {
        self.header
    }

    /// Match this exact anchor in the actual decoder snapshot's header window.
    ///
    /// The caller supplies the captured physical start and its actual selected
    /// prefix omission count. Only preceding selected packets may be omitted;
    /// this checks the resulting completion ordinal, protected header and raw
    /// PTS together. A later identical header cannot replace an omitted anchor.
    /// Fresh PSI, source association and decoder-byte identity remain separate.
    #[must_use]
    pub fn is_retained_in(
        self,
        window: &SuppliedTeletextHeaderWindow,
        first_source_packet: u64,
        skipped_selected_prefix_packets: u32,
    ) -> bool {
        let skipped = u64::from(skipped_selected_prefix_packets);
        if self
            .source_pusi_packet
            .checked_sub(first_source_packet)
            .and_then(|relative| relative.checked_sub(skipped))
            .is_none()
        {
            return false;
        }
        let Some(completion) = self
            .completion_source_packet
            .checked_sub(first_source_packet)
            .and_then(|relative| relative.checked_sub(skipped))
            .and_then(|relative| usize::try_from(relative).ok())
        else {
            return false;
        };
        window.headers().iter().any(|occurrence| {
            occurrence.completion_packet == completion
                && occurrence.header == self.header
                && occurrence.pts == Some(self.raw_pts)
        })
    }
}

pub(super) struct PagePreludeCollector {
    assembler: SuppliedTeletextAssembler,
    pid: u16,
    pmt: u16,
    next_ordinal: u64,
    state: PreludeState,
    failed: bool,
}

struct PreludeState {
    page: TeletextPageNumber,
    epoch: SourceNativeEpoch,
    start_upper_ticks: i64,
    end_upper_ticks: i64,
    latest_pat: Option<u64>,
    latest_psi_start: Option<u64>,
    current_start: Option<(u64, Option<u64>)>,
    previous_pts: Option<i64>,
    seen_page: bool,
    known_reset: bool,
    unknown: bool,
    candidate: Option<SourceTeletextPagePrelude>,
}

impl PagePreludeCollector {
    pub(super) fn new(
        pid: u16,
        pmt: u16,
        page: TeletextPageNumber,
        epoch: SourceNativeEpoch,
        start_upper_ticks: i64,
        end_upper_ticks: i64,
    ) -> Option<Self> {
        if !(0x20..=0x1ffe).contains(&pmt)
            || pmt == pid
            || end_upper_ticks <= start_upper_ticks
            || epoch.reference_90khz().is_none()
        {
            return None;
        }
        Some(Self {
            assembler: SuppliedTeletextAssembler::new(pid).ok()?,
            pid,
            pmt,
            next_ordinal: 0,
            state: PreludeState {
                page,
                epoch,
                start_upper_ticks,
                end_upper_ticks,
                latest_pat: None,
                latest_psi_start: None,
                current_start: None,
                previous_pts: None,
                seen_page: false,
                known_reset: false,
                unknown: false,
                candidate: None,
            },
            failed: false,
        })
    }

    pub(super) fn push(&mut self, raw: &[u8; 188], ordinal: u64) -> Result<()> {
        if self.failed {
            return Err(invalid());
        }
        let result = self.observe(raw, ordinal);
        self.failed = result.is_err();
        result
    }

    fn observe(&mut self, raw: &[u8; 188], ordinal: u64) -> Result<()> {
        if ordinal != self.next_ordinal {
            return Err(invalid());
        }
        self.next_ordinal = ordinal.checked_add(1).ok_or_else(invalid)?;
        let event = self.assembler.push(raw).map_err(|_| invalid())?;
        let pid = (u16::from(raw[1] & 31) << 8) | u16::from(raw[2]);
        if raw[1] & 0x40 != 0 && raw[3] & 0x10 != 0 {
            if pid == 0 {
                self.state.latest_pat = Some(ordinal);
            } else if pid == self.pmt {
                self.state.latest_psi_start = self.state.latest_pat;
            } else if pid == self.pid && !matches!(event, TeletextAssemblyEvent::Duplicate) {
                self.state.current_start = Some((ordinal, self.state.latest_psi_start));
            }
        }
        match event {
            TeletextAssemblyEvent::UnobservedPrefix => self.state.unknown = true,
            TeletextAssemblyEvent::Complete(pes) => self.state.complete(pes, ordinal)?,
            _ => {}
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<Option<SourceTeletextPagePrelude>> {
        if self.failed {
            return Err(invalid());
        }
        self.assembler.finish().map_err(|_| invalid())?;
        Ok((!self.state.unknown)
            .then_some(self.state.candidate)
            .flatten())
    }
}

impl PreludeState {
    fn complete(&mut self, pes: SuppliedTeletextPes<'_>, ordinal: u64) -> Result<()> {
        let timing = pes.pts().and_then(|raw| {
            let native = self
                .epoch
                .apply_timestamp(i64::try_from(raw.ticks()).ok()?)
                .ok()??
                .checked_mul(300)?;
            Some((raw, native))
        });
        let Some((raw_pts, native_pts_ticks)) = timing else {
            self.unknown = true;
            // Still validate every supplied protected header below.
            return self.inspect_headers(pes, ordinal, None);
        };
        if self
            .previous_pts
            .is_some_and(|previous| native_pts_ticks < previous)
        {
            self.unknown = true;
        }
        self.previous_pts = Some(native_pts_ticks);
        self.inspect_headers(pes, ordinal, Some((raw_pts, native_pts_ticks)))
    }

    fn inspect_headers(
        &mut self,
        pes: SuppliedTeletextPes<'_>,
        ordinal: u64,
        timing: Option<(TeletextTimestamp, i64)>,
    ) -> Result<()> {
        for unit in pes.units().filter_map(|unit| unit.teletext_unit()) {
            let TeletextHeaderObservation::Header(header) =
                observe_dvb_teletext_header(unit).map_err(|_| invalid())?
            else {
                continue;
            };
            if header.display_page() != Some(self.page) {
                continue;
            }
            let previously_seen = std::mem::replace(&mut self.seen_page, true);
            if !header.controls.erase_page() {
                self.unknown |= !self.known_reset;
                continue;
            }
            self.known_reset = true;
            let Some((raw_pts, native_pts_ticks)) = timing else {
                continue;
            };
            if native_pts_ticks <= self.start_upper_ticks
                || (!previously_seen && native_pts_ticks <= self.end_upper_ticks)
            {
                // An unavailable locator cannot borrow later tables.
                self.candidate = self.current_start.and_then(|(source_pusi_packet, psi)| {
                    Some(SourceTeletextPagePrelude {
                        psi_start_packet: psi?,
                        source_pusi_packet,
                        completion_source_packet: ordinal,
                        raw_pts,
                        native_pts_ticks,
                        header,
                    })
                });
            }
        }
        Ok(())
    }
}

fn invalid() -> FfprobeError {
    FfprobeError::InvalidOutput {
        path: "source teletext page prelude".into(),
    }
}

#[cfg(test)]
mod tests;

//! Opt-in bounded source page occurrences and transmitted context facts.
//!
//! The existing header observer supplies each already classified complete PES
//! unit once. Raw bytes and source order survive unchanged. Closure identifies
//! an observed acquisition boundary, not decoded/service/playback readiness.

use ersatzrs_domain::{
    SourceTeletextCapturedPage, SourceTeletextCapturedUnit, SourceTeletextCharsetOrigin,
    SourceTeletextPageContext, SourceTeletextPageWindow, MAX_SOURCE_TELETEXT_CAPTURED_PAGES,
    MAX_SOURCE_TELETEXT_CAPTURED_UNITS,
};

use super::{
    packets::hamming_24_18,
    source_header::{decode_hamming, TeletextHeaderObservation},
    TeletextTimestamp,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct SourceTeletextUnitPosition {
    pub completion_packet: usize,
    pub pes_ordinal: usize,
    pub unit_ordinal: usize,
    pub pts: Option<TeletextTimestamp>,
}

#[derive(Debug, Clone, Copy)]
struct ActivePage {
    index: usize,
    serial: bool,
}

#[derive(Debug, Clone, Copy)]
struct ContextCandidate {
    unit: u32,
    origin: Option<SourceTeletextCharsetOrigin>,
    /// Raw page function, coding, primary and secondary; no language inference.
    values: Option<[u8; 4]>,
}

impl ContextCandidate {
    fn context(self) -> SourceTeletextPageContext {
        // Other page functions/codings assign different meanings to the
        // remaining bits. Raw units retain them without inventing a charset.
        let display = self.values.filter(|values| {
            values[0] == 0
                && values[1] == 0
                && self.origin != Some(SourceTeletextCharsetOrigin::Page28_3)
        });
        SourceTeletextPageContext {
            page_function: self.values.map(|values| values[0]),
            page_coding: self.values.map(|values| values[1]),
            primary: display.map(|values| values[2]),
            secondary: display.map(|values| values[3]),
            charset_origin: self.origin,
            context_unit: Some(self.unit),
            protection_error: self.values.is_none(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ContextChoices {
    zero: Option<ContextCandidate>,
    four: Option<ContextCandidate>,
    three: Option<ContextCandidate>,
    /// An uncorrectable designation may itself be an overriding /0.
    ambiguous: Option<ContextCandidate>,
}

impl ContextChoices {
    fn observe(&mut self, raw: &[u8; 46], unit: u32, magazine: bool) {
        let Some((designation, _)) = decode_hamming(raw[6].reverse_bits()) else {
            self.ambiguous = Some(ContextCandidate {
                unit,
                origin: None,
                values: None,
            });
            return;
        };
        let origin = match (magazine, designation) {
            (false, 0) => SourceTeletextCharsetOrigin::Page28_0,
            (false, 4) => SourceTeletextCharsetOrigin::Page28_4,
            (false, 3) => SourceTeletextCharsetOrigin::Page28_3,
            (true, 0) => SourceTeletextCharsetOrigin::Magazine29_0,
            (true, 4) => SourceTeletextCharsetOrigin::Magazine29_4,
            _ => return,
        };
        let candidate = ContextCandidate {
            unit,
            origin: Some(origin),
            values: context_values(raw),
        };
        if designation == 0 {
            self.zero = Some(candidate);
        } else if designation == 4 {
            self.four = Some(candidate);
        } else {
            self.three = Some(candidate);
        }
    }

    fn selected(self) -> Option<ContextCandidate> {
        if let Some(ambiguous) = self.ambiguous {
            // Only a later /0 unambiguously supersedes either possible dirty
            // designation. A /4 cannot repair an unknown overriding /0.
            return Some(
                self.zero
                    .filter(|zero| zero.unit > ambiguous.unit)
                    .unwrap_or(ambiguous),
            );
        }
        self.zero.or(self.four).or(self.three)
    }
}

/// Private incremental collector; header-only construction never allocates it.
#[derive(Debug)]
pub(super) struct SourceTeletextPageCollector {
    window: SourceTeletextPageWindow,
    active: [Option<ActivePage>; 8],
    page_contexts: Vec<ContextChoices>,
    magazine_contexts: [ContextChoices; 8],
    transmission_mode: Option<bool>,
}

impl SourceTeletextPageCollector {
    pub(super) fn new() -> Self {
        Self {
            window: SourceTeletextPageWindow {
                units: Vec::new(),
                pages: Vec::new(),
                unassociated_body_units: 0,
                capacity_exhausted: false,
                transmission_mode_conflict: false,
            },
            active: [None; 8],
            page_contexts: Vec::new(),
            magazine_contexts: [ContextChoices::default(); 8],
            transmission_mode: None,
        }
    }

    pub(super) fn retained_unit_count(&self) -> usize {
        self.window.units.len()
    }

    pub(super) fn discard_for_capacity(&mut self) {
        self.window.units = Vec::new();
        self.window.pages = Vec::new();
        self.window.unassociated_body_units = 0;
        self.window.capacity_exhausted = true;
        self.active = [None; 8];
        self.page_contexts = Vec::new();
        self.magazine_contexts = [ContextChoices::default(); 8];
    }

    pub(super) fn observe(
        &mut self,
        raw: &[u8; 46],
        observation: TeletextHeaderObservation,
        header_ordinal: usize,
        position: SourceTeletextUnitPosition,
    ) {
        if let TeletextHeaderObservation::Header(header)
        | TeletextHeaderObservation::Terminator(header) = observation
        {
            let serial = header.controls.serial_mode();
            if self.transmission_mode.is_some_and(|mode| mode != serial) {
                self.window.transmission_mode_conflict = true;
            }
            self.transmission_mode = Some(serial);
        }
        if self.window.capacity_exhausted {
            return;
        }
        let address = match observation {
            TeletextHeaderObservation::NotTeletext { .. } => return,
            TeletextHeaderObservation::NotHeader(address) => address,
            TeletextHeaderObservation::Header(header)
            | TeletextHeaderObservation::Terminator(header) => header.packet_address,
        };
        if address.packet_number > 29 {
            return;
        }
        if self.window.units.len() == MAX_SOURCE_TELETEXT_CAPTURED_UNITS
            || (matches!(observation, TeletextHeaderObservation::Header(_))
                && self.window.pages.len() == MAX_SOURCE_TELETEXT_CAPTURED_PAGES)
        {
            self.discard_for_capacity();
            return;
        }
        // All positions are bounded by the existing observer before this call.
        let (
            Ok(unit),
            Ok(completion_packet),
            Ok(pes_ordinal),
            Ok(unit_ordinal),
            Ok(header_ordinal),
        ) = (
            u32::try_from(self.window.units.len()),
            u32::try_from(position.completion_packet),
            u32::try_from(position.pes_ordinal),
            u32::try_from(position.unit_ordinal),
            u32::try_from(header_ordinal),
        )
        else {
            self.discard_for_capacity();
            return;
        };
        self.window.units.push(SourceTeletextCapturedUnit {
            wire_magazine: address.wire_magazine,
            packet_number: address.packet_number,
            data: raw.to_vec(),
            completion_packet,
            pes_ordinal,
            unit_ordinal,
            pts: position.pts.map(TeletextTimestamp::ticks),
        });
        let magazine = usize::from(address.wire_magazine);
        if address.packet_number == 0 {
            for active in self.active.into_iter().flatten() {
                if active.serial
                    || self.window.units[self.window.pages[active.index].header_unit as usize]
                        .wire_magazine
                        == address.wire_magazine
                {
                    self.close(active.index, unit);
                }
            }
            if let TeletextHeaderObservation::Header(header) = observation {
                let serial = header.controls.serial_mode();
                let index = self.window.pages.len();
                self.window.pages.push(SourceTeletextCapturedPage {
                    header_ordinal,
                    header_unit: unit,
                    body_units: Vec::new(),
                    closing_unit: None,
                    context: SourceTeletextPageContext::default(),
                });
                self.page_contexts.push(ContextChoices::default());
                self.active[magazine] = Some(ActivePage { index, serial });
            }
        } else if address.packet_number == 29 {
            self.magazine_contexts[magazine].observe(raw, unit, true);
        } else if let Some(active) = self.active[magazine] {
            self.window.pages[active.index].body_units.push(unit);
            if address.packet_number == 28 {
                self.page_contexts[active.index].observe(raw, unit, false);
            }
        } else {
            self.window.unassociated_body_units += 1;
        }
    }

    fn close(&mut self, index: usize, unit: u32) {
        let magazine = usize::from(
            self.window.units[self.window.pages[index].header_unit as usize].wire_magazine,
        );
        self.window.pages[index].context = self.context(index, magazine);
        self.window.pages[index].closing_unit = Some(unit);
        self.active[magazine] = None;
    }

    fn context(&self, index: usize, magazine: usize) -> SourceTeletextPageContext {
        self.page_contexts[index]
            .selected()
            .or_else(|| self.magazine_contexts[magazine].selected())
            .map_or_else(
                SourceTeletextPageContext::default,
                ContextCandidate::context,
            )
    }

    pub(super) fn finish(mut self) -> SourceTeletextPageWindow {
        // Snapshot open acquisitions too, retaining closing_unit=None. EOF or a
        // bounded read end never fabricates a page completion marker.
        for active in self.active.into_iter().flatten() {
            let magazine = usize::from(
                self.window.units[self.window.pages[active.index].header_unit as usize]
                    .wire_magazine,
            );
            self.window.pages[active.index].context = self.context(active.index, magazine);
        }
        self.window
    }
}

fn context_values(raw: &[u8; 46]) -> Option<[u8; 4]> {
    let mut words = [0_u32; 13];
    for (word, coded) in words.iter_mut().zip(raw[7..].chunks_exact(3)) {
        *word = decode_hamming_24([
            coded[0].reverse_bits(),
            coded[1].reverse_bits(),
            coded[2].reverse_bits(),
        ])?;
    }
    Some([
        (words[0] & 15) as u8,
        ((words[0] >> 4) & 7) as u8,
        ((words[0] >> 7) & 127) as u8,
        u8::try_from(((words[0] >> 14) & 15) | ((words[1] & 7) << 4)).ok()?,
    ])
}

/// Receive SECDED Hamming(24,18), with odd checks and overall parity. The
/// encoder only verifies the corrected result; it does not search authored
/// codewords as a substitute for receiving/correcting source protection.
fn decode_hamming_24(bytes: [u8; 3]) -> Option<u32> {
    let mut word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
    let mut syndrome = 0;
    for parity in [1_u32, 2, 4, 8, 16] {
        let mut check = 0;
        for position in 1_u32..24 {
            if position & parity != 0 {
                check ^= (word >> (position - 1)) & 1;
            }
        }
        if check != 1 {
            syndrome |= parity;
        }
    }
    let odd = !word.count_ones().is_multiple_of(2);
    match (syndrome, odd) {
        (0, true) => {}
        (0, false) => word ^= 1 << 23,
        (1..=23, false) => word ^= 1 << (syndrome - 1),
        _ => return None,
    }
    let mut data = 0;
    let mut bit = 0;
    for position in 1_u32..24 {
        if !position.is_power_of_two() {
            data |= ((word >> (position - 1)) & 1) << bit;
            bit += 1;
        }
    }
    let encoded = hamming_24_18(data);
    (word.to_le_bytes()[..3] == encoded).then_some(data)
}

#[cfg(test)]
#[path = "source_pages_tests.rs"]
mod tests;

//! Bounded source-body observations, never authored declarations or playback permission.
//!
//! Protected address/header/context decoding belongs to the acquisition adapter.
//! Validation checks the raw DVB envelope and associations supplied by that
//! adapter; it does not independently decode Hamming words or certify source bytes.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    SourceTeletextError, SourceTeletextHeaderWindow, SourceTeletextObservedHeader,
    TeletextPageCharsetDesignation,
};

/// Resource policy for retained complete DVB units, not a transmission limit.
pub const MAX_SOURCE_TELETEXT_CAPTURED_UNITS: usize = 4_096;
/// Resource policy for retained page acquisitions, including repeated identities.
pub const MAX_SOURCE_TELETEXT_CAPTURED_PAGES: usize = 1_024;

/// One exact DVB unit with adapter-decoded address and original transport position.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTeletextCapturedUnit {
    pub wire_magazine: u8,
    pub packet_number: u8,
    /// Exact 46 bytes, including data-unit identifier, length and framing envelope.
    pub data: Vec<u8>,
    pub completion_packet: u32,
    pub pes_ordinal: u32,
    pub unit_ordinal: u32,
    pub pts: Option<u64>,
}

impl std::fmt::Debug for SourceTeletextCapturedUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SourceTeletextCapturedUnit")
            .field("wire_magazine", &self.wire_magazine)
            .field("packet_number", &self.packet_number)
            .field("data", &"<redacted>")
            .field("completion_packet", &self.completion_packet)
            .field("pes_ordinal", &self.pes_ordinal)
            .field("unit_ordinal", &self.unit_ordinal)
            .field("pts", &self.pts)
            .finish()
    }
}

/// Actual protected context source; no header-based default charset is invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceTeletextCharsetOrigin {
    Page28_0,
    /// DRCS page function/coding; it does not designate a text charset.
    Page28_3,
    Page28_4,
    Magazine29_0,
    Magazine29_4,
}

/// Adapter-decoded raw context values; reserved seven-bit designations survive.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTeletextPageContext {
    pub page_function: Option<u8>,
    pub page_coding: Option<u8>,
    pub primary: Option<u8>,
    pub secondary: Option<u8>,
    pub charset_origin: Option<SourceTeletextCharsetOrigin>,
    /// Index into the containing window's exact retained units.
    pub context_unit: Option<u32>,
    pub protection_error: bool,
}

/// One acquired header occurrence, not a deduplicated page or complete service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTeletextCapturedPage {
    pub header_ordinal: u32,
    pub header_unit: u32,
    pub body_units: Vec<u32>,
    pub closing_unit: Option<u32>,
    pub context: SourceTeletextPageContext,
}

/// Finite observation window under the accompanying header/PSI association.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceTeletextPageWindow {
    pub units: Vec<SourceTeletextCapturedUnit>,
    pub pages: Vec<SourceTeletextCapturedPage>,
    pub unassociated_body_units: u32,
    pub capacity_exhausted: bool,
    pub transmission_mode_conflict: bool,
}

/// Conservative acquisition facts, not decoded content or permission to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceTeletextPageReadiness {
    /// The bounded input did not observe the page's closing header.
    Open,
    /// Erase or unambiguous complete capture is missing; prior receiver state matters.
    PriorState,
    /// Protected fields or baseline parity cannot establish clean baseline data.
    ProtectionError,
    /// No explicitly observed assigned primary character context is available.
    ContextUnknown,
    /// Non-LOP coding or enhancement packets require separate interpretation.
    NeedsEnhancementContext,
    /// Closed erased LOP with observed charset and odd baseline/header text parity.
    /// This is neither decoded display/service completeness nor playback authority.
    ClosedBaseData,
}

impl SourceTeletextCapturedUnit {
    fn position(&self) -> (u32, u32, u32) {
        (self.completion_packet, self.pes_ordinal, self.unit_ordinal)
    }

    fn valid(&self, headers: &SourceTeletextHeaderWindow) -> bool {
        self.wire_magazine <= 7
            && self.packet_number <= 31
            && self.data.len() == 46
            && matches!(self.data[0], 2 | 3)
            && self.data[1] == 44
            && self.data[2] & 0xc0 == 0xc0
            && matches!(self.data[2] & 31, 0 | 7..=22)
            && self.data[3] == 0xe4
            && self.completion_packet < headers.packet_count
            && self.pes_ordinal < headers.complete_pes
            && self.unit_ordinal <= 32_726
            && self.pts.is_none_or(|pts| pts < 1_u64 << 33)
    }

    fn matches_header(&self, header: &SourceTeletextObservedHeader) -> bool {
        self.packet_number == 0
            && self.wire_magazine == header.wire_magazine
            && self.position()
                == (
                    header.completion_packet,
                    header.pes_ordinal,
                    header.unit_ordinal,
                )
            && self.pts == header.pts
            && (self.data[0] == 3) == header.subtitle_data_unit
            && (self.data[2] & 32 != 0) == header.field_parity
            && self.data[2] & 31 == header.line_offset
    }
}

impl SourceTeletextPageWindow {
    /// Check bounds, occurrence identity, associations and context references.
    ///
    /// # Errors
    /// Returns existing header failures or `PageWindow` for malformed raw envelopes,
    /// order, references or page associations. Serialized byte limits remain the
    /// caller's responsibility before deserialization.
    pub fn validate(
        &self,
        headers: &SourceTeletextHeaderWindow,
    ) -> Result<(), SourceTeletextError> {
        headers.validate()?;
        if self.units.len() > MAX_SOURCE_TELETEXT_CAPTURED_UNITS
            || self.pages.len() > MAX_SOURCE_TELETEXT_CAPTURED_PAGES
            || self.pages.len() > headers.headers.len()
            || (!self.capacity_exhausted && self.pages.len() != headers.headers.len())
        {
            return Err(SourceTeletextError::PageWindow);
        }
        self.validate_units(headers)?;
        let modes = headers
            .headers
            .iter()
            .map(|h| h.controls & 0x80 != 0)
            .collect::<BTreeSet<_>>();
        if modes.len() > 1 && !self.transmission_mode_conflict {
            return Err(SourceTeletextError::PageWindow);
        }
        let mut assigned = BTreeSet::new();
        for (ordinal, page) in self.pages.iter().enumerate() {
            if usize::try_from(page.header_ordinal).ok() != Some(ordinal) {
                return Err(SourceTeletextError::PageWindow);
            }
            self.validate_page(page, &headers.headers[ordinal], &mut assigned)?;
        }
        let unassociated = self
            .units
            .iter()
            .enumerate()
            .filter(|(index, unit)| {
                (1..=28).contains(&unit.packet_number) && !assigned.contains(index)
            })
            .count();
        if usize::try_from(self.unassociated_body_units).ok() != Some(unassociated) {
            return Err(SourceTeletextError::PageWindow);
        }
        Ok(())
    }

    fn validate_units(
        &self,
        headers: &SourceTeletextHeaderWindow,
    ) -> Result<(), SourceTeletextError> {
        let mut previous: Option<&SourceTeletextCapturedUnit> = None;
        for unit in &self.units {
            if !unit.valid(headers) {
                return Err(SourceTeletextError::PageWindow);
            }
            if let Some(previous) = previous {
                if unit.position() <= previous.position()
                    || unit.pes_ordinal < previous.pes_ordinal
                    || (unit.pes_ordinal == previous.pes_ordinal
                        && (unit.completion_packet != previous.completion_packet
                            || unit.pts != previous.pts))
                {
                    return Err(SourceTeletextError::PageWindow);
                }
            }
            previous = Some(unit);
        }
        Ok(())
    }

    fn validate_page(
        &self,
        page: &SourceTeletextCapturedPage,
        header: &SourceTeletextObservedHeader,
        assigned: &mut BTreeSet<usize>,
    ) -> Result<(), SourceTeletextError> {
        let opening =
            usize::try_from(page.header_unit).map_err(|_| SourceTeletextError::PageWindow)?;
        let Some(unit) = self.units.get(opening) else {
            return Err(SourceTeletextError::PageWindow);
        };
        if !unit.matches_header(header) || !assigned.insert(opening) {
            return Err(SourceTeletextError::PageWindow);
        }
        // C11 is bit seven of the stored C4..C14 controls. Each page owns its
        // mode: a serial page closes on any header; a parallel page on its magazine.
        let closing = self
            .units
            .iter()
            .enumerate()
            .skip(opening + 1)
            .find_map(|(index, unit)| {
                (unit.packet_number == 0
                    && (header.controls & 0x80 != 0 || unit.wire_magazine == header.wire_magazine))
                    .then_some(index)
            });
        if page.closing_unit.and_then(|v| usize::try_from(v).ok()) != closing {
            return Err(SourceTeletextError::PageWindow);
        }
        let limit = closing.unwrap_or(self.units.len());
        if page.body_units.len() > MAX_SOURCE_TELETEXT_CAPTURED_UNITS {
            return Err(SourceTeletextError::PageWindow);
        }
        let expected = self
            .units
            .iter()
            .enumerate()
            .take(limit)
            .skip(opening + 1)
            .filter(|(_, unit)| {
                unit.wire_magazine == header.wire_magazine && (1..=28).contains(&unit.packet_number)
            })
            .map(|(index, _)| u32::try_from(index).map_err(|_| SourceTeletextError::PageWindow))
            .collect::<Result<Vec<_>, _>>()?;
        if page.body_units != expected {
            return Err(SourceTeletextError::PageWindow);
        }
        let mut previous = opening;
        for index in &page.body_units {
            let index = usize::try_from(*index).map_err(|_| SourceTeletextError::PageWindow)?;
            let Some(unit) = self.units.get(index) else {
                return Err(SourceTeletextError::PageWindow);
            };
            if index <= previous
                || index >= limit
                || unit.wire_magazine != header.wire_magazine
                || !(1..=28).contains(&unit.packet_number)
                || !assigned.insert(index)
            {
                return Err(SourceTeletextError::PageWindow);
            }
            previous = index;
        }
        self.validate_context(page, header, limit)
    }

    fn validate_context(
        &self,
        page: &SourceTeletextCapturedPage,
        header: &SourceTeletextObservedHeader,
        limit: usize,
    ) -> Result<(), SourceTeletextError> {
        let context = &page.context;
        if context.page_function.is_some_and(|v| v > 15)
            || context.page_coding.is_some_and(|v| v > 7)
            || context.primary.is_some_and(|v| v > 127)
            || context.secondary.is_some_and(|v| v > 127)
        {
            return Err(SourceTeletextError::PageWindow);
        }
        match (context.charset_origin, context.context_unit) {
            (None, None)
                if context.page_function.is_none()
                    && context.page_coding.is_none()
                    && context.primary.is_none()
                    && context.secondary.is_none() =>
            {
                Ok(())
            }
            (origin, Some(raw_index)) if origin.is_some() || context.protection_error => {
                let index =
                    usize::try_from(raw_index).map_err(|_| SourceTeletextError::PageWindow)?;
                let Some(unit) = self.units.get(index) else {
                    return Err(SourceTeletextError::PageWindow);
                };
                let page_context = unit.packet_number == 28;
                let expected_packet = match origin {
                    Some(
                        SourceTeletextCharsetOrigin::Page28_0
                        | SourceTeletextCharsetOrigin::Page28_3
                        | SourceTeletextCharsetOrigin::Page28_4,
                    ) => Some(28),
                    Some(
                        SourceTeletextCharsetOrigin::Magazine29_0
                        | SourceTeletextCharsetOrigin::Magazine29_4,
                    ) => Some(29),
                    None => None,
                };
                if index >= limit
                    || unit.wire_magazine != header.wire_magazine
                    || !matches!(unit.packet_number, 28 | 29)
                    || expected_packet.is_some_and(|packet| packet != unit.packet_number)
                    || (page_context && !page.body_units.contains(&raw_index))
                    || (origin.is_none()
                        && (context.page_function.is_some()
                            || context.page_coding.is_some()
                            || context.primary.is_some()
                            || context.secondary.is_some()))
                {
                    return Err(SourceTeletextError::PageWindow);
                }
                Ok(())
            }
            _ => Err(SourceTeletextError::PageWindow),
        }
    }

    /// Derive conservative acquisition facts after structural validation.
    ///
    /// # Errors
    /// Returns the same structural failures as `validate`. No result authorizes
    /// rendering, source selection, artifact reuse or playback.
    pub fn readiness(
        &self,
        headers: &SourceTeletextHeaderWindow,
    ) -> Result<Vec<SourceTeletextPageReadiness>, SourceTeletextError> {
        self.validate(headers)?;
        self.pages
            .iter()
            .map(|page| self.page_readiness(headers, page))
            .collect()
    }

    fn page_readiness(
        &self,
        headers: &SourceTeletextHeaderWindow,
        page: &SourceTeletextCapturedPage,
    ) -> Result<SourceTeletextPageReadiness, SourceTeletextError> {
        let ordinal =
            usize::try_from(page.header_ordinal).map_err(|_| SourceTeletextError::PageWindow)?;
        let header = headers
            .headers
            .get(ordinal)
            .ok_or(SourceTeletextError::PageWindow)?;
        if page.closing_unit.is_none() {
            return Ok(SourceTeletextPageReadiness::Open);
        }
        if self.capacity_exhausted || self.transmission_mode_conflict || header.controls & 1 == 0 {
            return Ok(SourceTeletextPageReadiness::PriorState);
        }
        let context = &page.context;
        let get_unit = |index| {
            usize::try_from(index)
                .ok()
                .and_then(|index| self.units.get(index))
        };
        let header_unit = get_unit(page.header_unit).ok_or(SourceTeletextError::PageWindow)?;
        if context.protection_error {
            return Ok(SourceTeletextPageReadiness::ProtectionError);
        }
        if context.page_function.is_some_and(|v| v != 0)
            || context.page_coding.is_some_and(|v| v != 0)
        {
            return Ok(SourceTeletextPageReadiness::NeedsEnhancementContext);
        }
        if context
            .primary
            .is_none_or(|code| TeletextPageCharsetDesignation::new(code).is_err())
            || context.page_function.is_none()
            || context.page_coding.is_none()
        {
            return Ok(SourceTeletextPageReadiness::ContextUnknown);
        }
        if header_unit.data[14..]
            .iter()
            .any(|v| v.count_ones() % 2 == 0)
            || page.body_units.iter().any(|index| {
                get_unit(*index).is_none_or(|unit| {
                    unit.packet_number <= 25
                        && unit.data[6..].iter().any(|v| v.count_ones() % 2 == 0)
                })
            })
        {
            return Ok(SourceTeletextPageReadiness::ProtectionError);
        }
        if page
            .body_units
            .iter()
            .any(|index| get_unit(*index).is_some_and(|unit| unit.packet_number >= 26))
        {
            return Ok(SourceTeletextPageReadiness::NeedsEnhancementContext);
        }
        Ok(SourceTeletextPageReadiness::ClosedBaseData)
    }
}

#[cfg(test)]
#[path = "source_pages_tests.rs"]
mod tests;

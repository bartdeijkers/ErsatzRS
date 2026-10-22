//! Recheck persisted source facts with the same bounded wire receiver.

use ersatzrs_domain::{
    SourceTeletextError, SourceTeletextHeaderWindow, SourceTeletextObservedHeader,
    SourceTeletextPageWindow,
};

use super::{
    source_header::{observe_dvb_teletext_header, TeletextDataUnitKind, TeletextHeaderObservation},
    source_pages::{SourceTeletextPageCollector, SourceTeletextUnitPosition},
    TeletextTimestamp,
};

/// Check that retained wire units reproduce their header, body and context facts.
///
/// This verifies internal consistency, not immutable-source identity, decoded
/// display completeness, service completeness or permission to select/play.
/// # Errors
/// Returns static structural/protection errors without exposing source content.
pub fn validate_source_page_observations(
    pages: &SourceTeletextPageWindow,
    headers: &SourceTeletextHeaderWindow,
) -> Result<(), SourceTeletextError> {
    use SourceTeletextError::PageWindow;
    pages.validate(headers)?;
    if pages.capacity_exhausted {
        // The collector discards all optional body evidence at its capacity
        // fence. It cannot certify a retained prefix as a complete capture.
        return if pages.units.is_empty() && pages.pages.is_empty() {
            Ok(())
        } else {
            Err(PageWindow)
        };
    }
    let mut collector = SourceTeletextPageCollector::new();
    let mut ordinal = 0;
    for unit in &pages.units {
        let raw = unit.data.as_slice().try_into().map_err(|_| PageWindow)?;
        let observation = observe_dvb_teletext_header(raw).map_err(|_| PageWindow)?;
        let address = match observation {
            TeletextHeaderObservation::NotTeletext { .. } => return Err(PageWindow),
            TeletextHeaderObservation::NotHeader(address) => address,
            TeletextHeaderObservation::Header(header)
            | TeletextHeaderObservation::Terminator(header) => header.packet_address,
        };
        if address.wire_magazine != unit.wire_magazine
            || address.packet_number != unit.packet_number
        {
            return Err(PageWindow);
        }
        let position = SourceTeletextUnitPosition {
            completion_packet: usize::try_from(unit.completion_packet).map_err(|_| PageWindow)?,
            pes_ordinal: usize::try_from(unit.pes_ordinal).map_err(|_| PageWindow)?,
            unit_ordinal: usize::try_from(unit.unit_ordinal).map_err(|_| PageWindow)?,
            pts: unit
                .pts
                .map(TeletextTimestamp::new)
                .transpose()
                .map_err(|_| PageWindow)?,
        };
        let header_ordinal = ordinal;
        if let TeletextHeaderObservation::Header(header) = observation {
            let decoded = SourceTeletextObservedHeader {
                wire_magazine: address.wire_magazine,
                wire_page: header.wire_page,
                subcode: header.subcode,
                controls: header.controls.raw(),
                subtitle_data_unit: address.data_unit_kind == TeletextDataUnitKind::Subtitle,
                field_parity: address.field_parity,
                line_offset: address.line_offset,
                corrected_address_bytes: address.corrected_address_bytes,
                corrected_header_bytes: header.corrected_header_bytes,
                completion_packet: unit.completion_packet,
                pes_ordinal: unit.pes_ordinal,
                unit_ordinal: unit.unit_ordinal,
                pts: unit.pts,
            };
            if headers.headers.get(ordinal) != Some(&decoded) {
                return Err(PageWindow);
            }
            ordinal += 1;
        }
        collector.observe(raw, observation, header_ordinal, position);
    }
    if ordinal != headers.headers.len() || collector.finish() != *pages {
        return Err(PageWindow);
    }
    Ok(())
}

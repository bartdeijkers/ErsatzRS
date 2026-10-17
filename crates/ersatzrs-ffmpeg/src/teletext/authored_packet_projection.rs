//! Lossless logical records from immutable authored display and DRCS pages.
//!
//! This is a strict inverse of the host's packet encoder, not a forgiving
//! receiver decoder. Correctable errors are rejected rather than concealed in
//! editor exports. Explicit DRCS and object inverses support resource pages;
//! transport, edit permissions and publication remain outside these projections.

use std::fmt;

use ersatzrs_domain::{
    TeletextLanguage, TeletextPageAddress, TeletextPageKind, TeletextResourceHeaderMetadata,
    TeletextResourceKind,
};

use super::packets::{hamming_24_18, hamming_8_4};
use super::source_header::{
    observe_dvb_teletext_header, SuppliedTeletextHeader, SuppliedTeletextPacketAddress,
    TeletextDataUnitKind, TeletextHeaderObservation,
};
use super::{
    encode_teletext_resource_header, EncodedTeletextAuthoredPage, EncodedTeletextPage,
    EncodedTeletextResourcePage, TeletextResourceTransmissionMode,
};

mod object;
pub use object::{
    project_encoded_teletext_object, TeletextEncodedObjectPacket, TeletextEncodedObjectProjection,
};

/// An exact logical packet body in native transmission order.
///
/// Header address/control cells are retained separately in the projection's
/// observed header. No parity bit, protected nibble or triplet is silently
/// repaired. `Debug` deliberately omits all body data.
#[derive(Clone, PartialEq, Eq)]
pub enum TeletextEncodedDisplayRecord {
    /// The thirty-two authored header cells, physical columns eight to thirty-nine.
    HeaderText([u8; 32]),
    /// One seven-bit row, including spacing controls and trailing spaces.
    BaseRow { row: u8, cells: [u8; 40] },
    /// X/27/0 is thirty-eight Hamming nibbles followed by two unprotected CRC bytes.
    EditorialLinks { nibbles: [u8; 38], crc: [u8; 2] },
    /// X/26, X/27 designation four or higher, or X/28 eighteen-bit triplets.
    Triplets {
        row: u8,
        designation: u8,
        words: [u32; 13],
    },
}

impl fmt::Debug for TeletextEncodedDisplayRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HeaderText(_) => formatter.write_str("HeaderText([redacted])"),
            Self::BaseRow { row, .. } => formatter
                .debug_struct("BaseRow")
                .field("row", row)
                .finish_non_exhaustive(),
            Self::EditorialLinks { .. } => formatter.write_str("EditorialLinks([redacted])"),
            Self::Triplets {
                row, designation, ..
            } => formatter
                .debug_struct("Triplets")
                .field("row", row)
                .field("designation", designation)
                .finish_non_exhaustive(),
        }
    }
}

/// A completely checked display projection; it grants no editing or publication authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEncodedDisplayProjection {
    /// Original decimal page and exact hexadecimal subcode.
    pub address: TeletextPageAddress,
    /// Original authored page kind, checked against the wire header and unit type.
    pub kind: TeletextPageKind,
    /// Original ISO 639 language, not inferred from national-option bits.
    pub language: TeletextLanguage,
    /// Exact clean wire identity and all raw C4–C14 controls.
    pub header: SuppliedTeletextHeader,
    /// Complete native packet order, beginning with the authored header text.
    pub records: Vec<TeletextEncodedDisplayRecord>,
}

impl fmt::Debug for TeletextEncodedDisplayProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextEncodedDisplayProjection")
            .field("address", &self.address)
            .field("kind", &self.kind)
            .field("record_count", &self.records.len())
            .finish_non_exhaustive()
    }
}

/// Static failure classification without retaining authored text or packet bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextEncodedDisplayProjectionFailure {
    /// The positive unit budget was absent or exceeded, or the page was empty.
    Budget,
    /// A unit was not a valid DVB teletext envelope/header.
    Envelope,
    /// A protected address/header/body value was not an exact encoder codeword.
    Protection,
    /// A base or header cell did not have exact odd parity.
    Parity,
    /// Wire identity, unit type or page-kind controls disagreed with the immutable page.
    Identity,
    /// The first packet was not its page header, or another header followed it.
    HeaderOrder,
    /// A packet number or designation lacks a supported logical record representation.
    UnsupportedPacket,
}

/// Located projection failure; columns are zero-based physical body positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "authored teletext projection {failure:?} at unit {unit:?}, row {row:?}, column {column:?}"
)]
pub struct TeletextEncodedDisplayProjectionError {
    /// Static failure category; it never includes authored cell data.
    pub failure: TeletextEncodedDisplayProjectionFailure,
    /// Zero-based native data-unit index, when a unit was inspected.
    pub unit: Option<usize>,
    /// Packet number when its address was cleanly observed.
    pub row: Option<u8>,
    /// Zero-based physical body column, when the failing byte is known.
    pub column: Option<u8>,
}

/// Project an entire immutable encoded display page without receiver correction.
///
/// The required positive budget is checked before allocating output records.
/// Body order, raw header flags, editorial subcodes/CRC and exact triplet words
/// are preserved. This checks packet coding, not Level 2.5 semantics or TTI
/// compatibility; those remain responsibilities of the consuming host workflow.
///
/// # Errors
/// Returns a static located error for budget, envelope, protection, parity,
/// identity, packet-order or unsupported-record failures. No partial projection
/// or original payload is returned on failure.
pub fn project_encoded_teletext_display(
    page: &EncodedTeletextPage,
    max_units: usize,
) -> Result<TeletextEncodedDisplayProjection, TeletextEncodedDisplayProjectionError> {
    project_units(page, page.data_units(), max_units)
}

fn project_units(
    page: &EncodedTeletextPage,
    units: &[[u8; 46]],
    max_units: usize,
) -> Result<TeletextEncodedDisplayProjection, TeletextEncodedDisplayProjectionError> {
    use TeletextEncodedDisplayProjectionFailure as Failure;
    if max_units == 0 || units.is_empty() || units.len() > max_units {
        return Err(error(Failure::Budget, None, None, None));
    }
    let first = observe_dvb_teletext_header(&units[0])
        .map_err(|_| error(Failure::Envelope, Some(0), None, None))?;
    let TeletextHeaderObservation::Header(header) = first else {
        return Err(error(Failure::HeaderOrder, Some(0), None, None));
    };
    check_packet_identity(page, header.packet_address, 0)?;
    if header.corrected_header_bytes != 0 {
        return Err(error(Failure::Protection, Some(0), Some(0), None));
    }
    let expected_flags = match page.kind() {
        TeletextPageKind::FullPage => 0,
        TeletextPageKind::Newsflash => 2,
        TeletextPageKind::Subtitle => 4,
    };
    if header.display_page() != Some(page.address().page)
        || header.subcode != page.address().subcode.get()
        || header.controls.raw() & 6 != expected_flags
    {
        return Err(error(Failure::Identity, Some(0), Some(0), None));
    }
    let mut records = Vec::new();
    records
        .try_reserve_exact(units.len())
        .map_err(|_| error(Failure::Budget, None, None, None))?;
    let mut header_text = [0; 32];
    for (cell, column) in header_text.iter_mut().zip(8_u8..40) {
        *cell = base_cell(
            units[0][6 + usize::from(column)].reverse_bits(),
            0,
            0,
            column,
        )?;
    }
    records.push(TeletextEncodedDisplayRecord::HeaderText(header_text));
    for (unit_index, unit) in units.iter().enumerate().skip(1) {
        let observation = observe_dvb_teletext_header(unit)
            .map_err(|_| error(Failure::Envelope, Some(unit_index), None, None))?;
        let packet = match observation {
            TeletextHeaderObservation::NotHeader(packet) => packet,
            TeletextHeaderObservation::Header(_) | TeletextHeaderObservation::Terminator(_) => {
                return Err(error(Failure::HeaderOrder, Some(unit_index), Some(0), None));
            }
            TeletextHeaderObservation::NotTeletext { .. } => {
                return Err(error(Failure::Envelope, Some(unit_index), None, None));
            }
        };
        check_packet_identity(page, packet, unit_index)?;
        let body = std::array::from_fn(|column| unit[column + 6].reverse_bits());
        records.push(project_body(packet.packet_number, &body, unit_index)?);
    }
    Ok(TeletextEncodedDisplayProjection {
        address: page.address(),
        kind: page.kind(),
        language: page.language().clone(),
        header,
        records,
    })
}

fn check_packet_identity(
    page: &EncodedTeletextPage,
    packet: SuppliedTeletextPacketAddress,
    unit: usize,
) -> Result<(), TeletextEncodedDisplayProjectionError> {
    use TeletextEncodedDisplayProjectionFailure as Failure;
    if packet.corrected_address_bytes != 0 {
        return Err(error(
            Failure::Protection,
            Some(unit),
            Some(packet.packet_number),
            None,
        ));
    }
    let kind = if page.kind() == TeletextPageKind::Subtitle {
        TeletextDataUnitKind::Subtitle
    } else {
        TeletextDataUnitKind::NonSubtitle
    };
    if packet.wire_magazine != page.address().page.wire_magazine() || packet.data_unit_kind != kind
    {
        return Err(error(
            Failure::Identity,
            Some(unit),
            Some(packet.packet_number),
            None,
        ));
    }
    Ok(())
}

fn project_body(
    row: u8,
    body: &[u8; 40],
    unit: usize,
) -> Result<TeletextEncodedDisplayRecord, TeletextEncodedDisplayProjectionError> {
    use TeletextEncodedDisplayProjectionFailure as Failure;
    if (1..=25).contains(&row) {
        let mut cells = [0; 40];
        for (cell, column) in cells.iter_mut().zip(0_u8..40) {
            *cell = base_cell(body[usize::from(column)], unit, row, column)?;
        }
        return Ok(TeletextEncodedDisplayRecord::BaseRow { row, cells });
    }
    if !matches!(row, 26..=28) {
        return Err(error(
            Failure::UnsupportedPacket,
            Some(unit),
            Some(row),
            None,
        ));
    }
    let designation = exact_nibble(body[0])
        .ok_or_else(|| error(Failure::Protection, Some(unit), Some(row), Some(0)))?;
    if row == 27 && designation == 0 {
        let mut nibbles = [0; 38];
        for (nibble, column) in nibbles.iter_mut().zip(0_u8..38) {
            *nibble = exact_nibble(body[usize::from(column)])
                .ok_or_else(|| error(Failure::Protection, Some(unit), Some(row), Some(column)))?;
        }
        return Ok(TeletextEncodedDisplayRecord::EditorialLinks {
            nibbles,
            crc: [body[38], body[39]],
        });
    }
    if row == 27 && designation < 4 {
        return Err(error(
            Failure::UnsupportedPacket,
            Some(unit),
            Some(row),
            Some(0),
        ));
    }
    let mut words = [0; 13];
    for (word, column) in words.iter_mut().zip((1_u8..40).step_by(3)) {
        let index = usize::from(column);
        let bytes = [body[index], body[index + 1], body[index + 2]];
        *word = exact_triplet(bytes)
            .ok_or_else(|| error(Failure::Protection, Some(unit), Some(row), Some(column)))?;
    }
    Ok(TeletextEncodedDisplayRecord::Triplets {
        row,
        designation,
        words,
    })
}

fn base_cell(
    byte: u8,
    unit: usize,
    row: u8,
    column: u8,
) -> Result<u8, TeletextEncodedDisplayProjectionError> {
    if byte.count_ones() % 2 != 1 {
        return Err(error(
            TeletextEncodedDisplayProjectionFailure::Parity,
            Some(unit),
            Some(row),
            Some(column),
        ));
    }
    Ok(byte & 0x7f)
}

fn exact_nibble(byte: u8) -> Option<u8> {
    (0..16).find(|&nibble| hamming_8_4(nibble) == byte)
}

fn exact_triplet(bytes: [u8; 3]) -> Option<u32> {
    // EN 300 706 annex A: data positions already independently audited by the
    // object-page reader. Re-encoding rejects parity damage without correction.
    let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
    let data = [
        3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
    ]
    .into_iter()
    .enumerate()
    .fold(0, |value, (bit, position)| {
        value | (((raw >> (position - 1)) & 1) << bit)
    });
    (hamming_24_18(data) == bytes).then_some(data)
}

const fn error(
    failure: TeletextEncodedDisplayProjectionFailure,
    unit: Option<usize>,
    row: Option<u8>,
    column: Option<u8>,
) -> TeletextEncodedDisplayProjectionError {
    TeletextEncodedDisplayProjectionError {
        failure,
        unit,
        row,
        column,
    }
}

/// Exact immutable DRCS resource context and logical packet bodies.
///
/// This retains native physical/lifecycle identity separately from any editor
/// list index. It grants no resource editing or publication authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEncodedDrcsProjection {
    /// Native logical resource identity, physical subcode, counter and lifecycle.
    pub resource_header: TeletextResourceHeaderMetadata,
    /// Native serial/parallel delivery mode, without deriving it from editor PS.
    pub transmission_mode: TeletextResourceTransmissionMode,
    /// Exact clean observed wire header, including raw resource control bits.
    pub header: SuppliedTeletextHeader,
    /// Native order: blank header text, X28/3, then ascending present PTU rows.
    pub records: Vec<TeletextEncodedDisplayRecord>,
}
impl fmt::Debug for TeletextEncodedDrcsProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextEncodedDrcsProjection")
            .field("resource_header", &self.resource_header)
            .field("transmission_mode", &self.transmission_mode)
            .field("record_count", &self.records.len())
            .finish_non_exhaustive()
    }
}

/// Project only a complete native DRCS/GDRCS resource without correction.
///
/// The positive unit budget precedes allocation. Full header metadata, exact
/// odd-parity PTU/SPACE data and every X28/3 mode word remain unchanged. This
/// checks native packet coding and order, not an editor's resource rendering,
/// permission to edit resources or complete Level 2.5 acceptance.
///
/// # Errors
/// Returns a located existing projection diagnostic for invalid bounds,
/// non-DRCS pages, mismatched native identity/header, dirty protection/parity,
/// unsupported packets or noncanonical native packet order.
pub fn project_encoded_teletext_drcs(
    page: &EncodedTeletextAuthoredPage,
    max_units: usize,
) -> Result<TeletextEncodedDrcsProjection, TeletextEncodedDisplayProjectionError> {
    let EncodedTeletextAuthoredPage::Resource(resource) = page else {
        return Err(error(
            TeletextEncodedDisplayProjectionFailure::Identity,
            None,
            None,
            None,
        ));
    };
    project_drcs_units(resource, resource.data_units(), max_units)
}

fn project_drcs_units(
    page: &EncodedTeletextResourcePage,
    units: &[[u8; 46]],
    max_units: usize,
) -> Result<TeletextEncodedDrcsProjection, TeletextEncodedDisplayProjectionError> {
    use TeletextEncodedDisplayProjectionFailure as Failure;
    if max_units == 0 || units.len() < 3 || units.len() > max_units {
        return Err(error(Failure::Budget, None, None, None));
    }
    if !matches!(
        page.header().identity().kind(),
        TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs
    ) {
        return Err(error(Failure::Identity, None, None, None));
    }
    let header = resource_header(page.header(), page.transmission_mode(), &units[0])?;
    let mut records = Vec::new();
    records
        .try_reserve_exact(units.len())
        .map_err(|_| error(Failure::Budget, None, None, None))?;
    records.push(TeletextEncodedDisplayRecord::HeaderText([b' '; 32]));
    let mut last_row = 0;
    for (unit_index, unit) in units.iter().enumerate().skip(1) {
        let observation = observe_dvb_teletext_header(unit)
            .map_err(|_| error(Failure::Envelope, Some(unit_index), None, None))?;
        let packet = match observation {
            TeletextHeaderObservation::NotHeader(packet) => packet,
            TeletextHeaderObservation::Header(_) | TeletextHeaderObservation::Terminator(_) => {
                return Err(error(Failure::HeaderOrder, Some(unit_index), Some(0), None));
            }
            TeletextHeaderObservation::NotTeletext { .. } => {
                return Err(error(Failure::Envelope, Some(unit_index), None, None));
            }
        };
        check_resource_packet(page.header(), unit, packet, unit_index)?;
        let row = packet.packet_number;
        if (unit_index == 1 && row != 28)
            || (unit_index > 1 && (!(1..=24).contains(&row) || row <= last_row))
        {
            return Err(error(
                Failure::UnsupportedPacket,
                Some(unit_index),
                Some(row),
                None,
            ));
        }
        let body = std::array::from_fn(|column| unit[column + 6].reverse_bits());
        let record = project_body(row, &body, unit_index)?;
        if unit_index == 1 {
            check_drcs_modes(page, &record, unit_index)?;
        } else {
            last_row = row;
        }
        records.push(record);
    }
    if last_row != page.header().last_packet().get() {
        return Err(error(Failure::Identity, None, Some(last_row), None));
    }
    Ok(TeletextEncodedDrcsProjection {
        resource_header: page.header(),
        transmission_mode: page.transmission_mode(),
        header,
        records,
    })
}

fn check_drcs_modes(
    page: &EncodedTeletextResourcePage,
    record: &TeletextEncodedDisplayRecord,
    unit: usize,
) -> Result<(), TeletextEncodedDisplayProjectionError> {
    use TeletextEncodedDisplayProjectionFailure as Failure;
    let TeletextEncodedDisplayRecord::Triplets {
        designation, words, ..
    } = record
    else {
        return Err(error(
            Failure::UnsupportedPacket,
            Some(unit),
            Some(28),
            None,
        ));
    };
    if *designation != 3 {
        return Err(error(
            Failure::UnsupportedPacket,
            Some(unit),
            Some(28),
            Some(0),
        ));
    }
    if words[0] != u32::from(page.header().identity().kind().function_code()) {
        return Err(error(Failure::Identity, Some(unit), Some(28), Some(1)));
    }
    Ok(())
}

fn resource_header(
    metadata: TeletextResourceHeaderMetadata,
    mode: TeletextResourceTransmissionMode,
    unit: &[u8; 46],
) -> Result<SuppliedTeletextHeader, TeletextEncodedDisplayProjectionError> {
    use TeletextEncodedDisplayProjectionFailure as Failure;
    let observation = observe_dvb_teletext_header(unit)
        .map_err(|_| error(Failure::Envelope, Some(0), None, None))?;
    let TeletextHeaderObservation::Header(header) = observation else {
        return Err(error(Failure::HeaderOrder, Some(0), None, None));
    };
    check_resource_packet(metadata, unit, header.packet_address, 0)?;
    if header.corrected_header_bytes != 0 {
        return Err(error(Failure::Protection, Some(0), Some(0), None));
    }
    for column in 8_u8..40 {
        base_cell(unit[6 + usize::from(column)].reverse_bits(), 0, 0, column)?;
    }
    if *unit != encode_teletext_resource_header(metadata, mode) {
        return Err(error(Failure::Identity, Some(0), Some(0), None));
    }
    Ok(header)
}

fn check_resource_packet(
    metadata: TeletextResourceHeaderMetadata,
    bytes: &[u8; 46],
    packet: SuppliedTeletextPacketAddress,
    unit: usize,
) -> Result<(), TeletextEncodedDisplayProjectionError> {
    use TeletextEncodedDisplayProjectionFailure as Failure;
    let row = Some(packet.packet_number);
    if packet.corrected_address_bytes != 0 {
        return Err(error(Failure::Protection, Some(unit), row, None));
    }
    if packet.data_unit_kind != TeletextDataUnitKind::NonSubtitle
        || packet.wire_magazine != metadata.identity().page().wire_magazine()
    {
        return Err(error(Failure::Identity, Some(unit), row, None));
    }
    if bytes[..4] != [2, 44, 0xe0, 0xe4] {
        return Err(error(Failure::Envelope, Some(unit), row, None));
    }
    Ok(())
}

#[cfg(test)]
#[path = "authored_packet_projection/resource_tests.rs"]
mod resource_tests;

#[cfg(test)]
mod tests;

/// One exact service-owned M/29/0 logical body, without editor or broadcast authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEncodedMagazineRecord {
    /// Human magazine one through eight; wire magazine zero is represented as eight.
    pub magazine: u8,
    /// Native authored magazine presentation currently has designation zero only.
    pub designation: u8,
    /// The thirteen exact eighteen-bit words, with no correction or semantic rewriting.
    pub words: [u32; 13],
}

impl fmt::Debug for TeletextEncodedMagazineRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextEncodedMagazineRecord")
            .field("magazine", &self.magazine)
            .field("designation", &self.designation)
            .finish_non_exhaustive()
    }
}

/// Read the immutable service's actual M/29/0 units without inventing page X/28.
///
/// The required positive budget bounds only this magazine-unit collection.
/// An empty collection is valid when the semantic service declares no magazine
/// presentation. Every actual unit must have one unique declared magazine,
/// exact protection and the non-subtitle type used by the authored encoder.
/// Editor projections derived from these words must retain their provenance;
/// this function changes neither the service nor its page-level ownership.
///
/// # Errors
/// Returns a static located failure for budget, envelope, protection, packet
/// kind/designation or mismatch with the semantic magazine keys. No partially
/// projected collection or raw data is included in an error.
pub fn project_encoded_teletext_magazines(
    service: &super::EncodedTeletextAuthoredService,
    max_units: usize,
) -> Result<Vec<TeletextEncodedMagazineRecord>, TeletextEncodedDisplayProjectionError> {
    let mut expected = [false; 8];
    for &magazine in service
        .declared()
        .semantic()
        .magazine_presentations()
        .keys()
    {
        if !(1..=8).contains(&magazine) {
            return Err(error(
                TeletextEncodedDisplayProjectionFailure::Identity,
                None,
                Some(29),
                None,
            ));
        }
        expected[usize::from(magazine - 1)] = true;
    }
    project_magazine_units(service.magazine_units(), expected, max_units)
}

fn project_magazine_units(
    units: &[[u8; 46]],
    expected: [bool; 8],
    max_units: usize,
) -> Result<Vec<TeletextEncodedMagazineRecord>, TeletextEncodedDisplayProjectionError> {
    use TeletextEncodedDisplayProjectionFailure as Failure;
    if max_units == 0 || units.len() > max_units {
        return Err(error(Failure::Budget, None, None, None));
    }
    let mut records = Vec::new();
    records
        .try_reserve_exact(units.len())
        .map_err(|_| error(Failure::Budget, None, None, None))?;
    let mut seen = [false; 8];
    for (unit_index, unit) in units.iter().enumerate() {
        let observation = observe_dvb_teletext_header(unit)
            .map_err(|_| error(Failure::Envelope, Some(unit_index), None, None))?;
        let packet = match observation {
            TeletextHeaderObservation::NotHeader(packet) => packet,
            TeletextHeaderObservation::NotTeletext { .. } => {
                return Err(error(Failure::Envelope, Some(unit_index), None, None));
            }
            TeletextHeaderObservation::Header(_) | TeletextHeaderObservation::Terminator(_) => {
                return Err(error(
                    Failure::UnsupportedPacket,
                    Some(unit_index),
                    Some(0),
                    None,
                ));
            }
        };
        let row = packet.packet_number;
        if packet.corrected_address_bytes != 0 {
            return Err(error(
                Failure::Protection,
                Some(unit_index),
                Some(row),
                None,
            ));
        }
        if row != 29 {
            return Err(error(
                Failure::UnsupportedPacket,
                Some(unit_index),
                Some(row),
                None,
            ));
        }
        let magazine = if packet.wire_magazine == 0 {
            8
        } else {
            packet.wire_magazine
        };
        let index = usize::from(magazine - 1);
        if packet.data_unit_kind != TeletextDataUnitKind::NonSubtitle
            || !expected[index]
            || seen[index]
        {
            return Err(error(Failure::Identity, Some(unit_index), Some(row), None));
        }
        let body: [u8; 40] = std::array::from_fn(|column| unit[column + 6].reverse_bits());
        let designation = exact_nibble(body[0])
            .ok_or_else(|| error(Failure::Protection, Some(unit_index), Some(row), Some(0)))?;
        if designation != 0 {
            return Err(error(
                Failure::UnsupportedPacket,
                Some(unit_index),
                Some(row),
                Some(0),
            ));
        }
        let mut words = [0; 13];
        for (word, column) in words.iter_mut().zip((1_u8..40).step_by(3)) {
            let index = usize::from(column);
            *word = exact_triplet([body[index], body[index + 1], body[index + 2]]).ok_or_else(
                || {
                    error(
                        Failure::Protection,
                        Some(unit_index),
                        Some(row),
                        Some(column),
                    )
                },
            )?;
        }
        seen[index] = true;
        records.push(TeletextEncodedMagazineRecord {
            magazine,
            designation,
            words,
        });
    }
    if seen != expected {
        return Err(error(Failure::Identity, None, Some(29), None));
    }
    Ok(records)
}

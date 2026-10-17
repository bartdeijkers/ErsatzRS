//! Exact native POP/GPOP packet coding, without editor or resource authority.
use super::{
    check_resource_packet, error, exact_nibble, exact_triplet, observe_dvb_teletext_header,
    resource_header, EncodedTeletextAuthoredPage, SuppliedTeletextHeader,
    TeletextEncodedDisplayProjectionError, TeletextEncodedDisplayProjectionFailure as Failure,
    TeletextHeaderObservation, TeletextResourceHeaderMetadata, TeletextResourceKind,
    TeletextResourceTransmissionMode,
};
use crate::teletext::EncodedTeletextObjectPage;
use std::fmt;

/// One exact pointer or content packet in native logical row order.
///
/// Rows1–25 retain their actual designation nibble (pointer1 or content0).
/// Logical26–41 correspond to physicalX26/designation0–15. FIRSTX3 offsets
/// remain unchanged: a content word's logical row is3+offset/13 even when
/// pointer extensions occupy logicalX3/X4. No semantic pointer is inferred.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEncodedObjectPacket {
    pub logical_row: u8,
    pub designation: u8,
    pub words: [u32; 13],
}

impl fmt::Debug for TeletextEncodedObjectPacket {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextEncodedObjectPacket")
            .field("logical_row", &self.logical_row)
            .field("designation", &self.designation)
            .finish_non_exhaustive()
    }
}

/// Fully checked coding/layout from one immutable authored POP/GPOP page.
/// This retains native identity, lifecycle and physical order; it grants no
/// editing, dependency resolution, publication or receiver readiness authority.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextEncodedObjectProjection {
    pub resource_header: TeletextResourceHeaderMetadata,
    pub transmission_mode: TeletextResourceTransmissionMode,
    pub header: SuppliedTeletextHeader,
    /// Exact nativeX28/0 words, kept separate from logical object row28.
    pub function_words: [u32; 13],
    /// Actual native pointer extent, derived from immutable assigned positions.
    pub pointer_rows: u8,
    /// Contiguous logical rows1 through the native last-packet value.
    pub packets: Vec<TeletextEncodedObjectPacket>,
}

impl fmt::Debug for TeletextEncodedObjectProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextEncodedObjectProjection")
            .field("resource_header", &self.resource_header)
            .field("transmission_mode", &self.transmission_mode)
            .field("pointer_rows", &self.pointer_rows)
            .field("packet_count", &self.packets.len())
            .finish_non_exhaustive()
    }
}

/// Project actual native POP/GPOP pointer/content triplets without correction.
/// The positive complete-unit budget precedes allocation. Resource header,
/// function/coding, pointer extent and contiguous native row/designation order
/// must match the immutable encoder context. Words are observed exactly; this
/// does not independently validate object semantics or authorize their editing.
///
/// # Errors
/// Returns an existing located projection error for bounds, nonobject identity,
/// dirty envelope/protection/header or noncanonical native coding/layout. No
/// partial projection or authored bytes are retained in errors.
pub fn project_encoded_teletext_object(
    page: &EncodedTeletextAuthoredPage,
    max_units: usize,
) -> Result<TeletextEncodedObjectProjection, TeletextEncodedDisplayProjectionError> {
    let EncodedTeletextAuthoredPage::Object(page) = page else {
        return Err(error(Failure::Identity, None, None, None));
    };
    project_units(page, page.data_units(), max_units)
}

fn project_units(
    page: &EncodedTeletextObjectPage,
    units: &[[u8; 46]],
    max_units: usize,
) -> Result<TeletextEncodedObjectProjection, TeletextEncodedDisplayProjectionError> {
    if max_units == 0 || units.len() < 5 || units.len() > max_units {
        return Err(error(Failure::Budget, None, None, None));
    }
    let metadata = page.header();
    let last = metadata.last_packet().get();
    let pointer_rows = pointer_extent(page)?;
    if !matches!(
        metadata.identity().kind(),
        TeletextResourceKind::Pop | TeletextResourceKind::Gpop
    ) || !(3..=41).contains(&last)
        || units.len() != usize::from(last) + 2
        || last <= pointer_rows
    {
        return Err(error(Failure::Identity, None, None, None));
    }
    let header = resource_header(metadata, page.transmission_mode(), &units[0])?;
    let (row, designation, function_words) = read_packet(metadata, &units[1], 1)?;
    if row != 28 || designation != 0 {
        return Err(error(
            Failure::UnsupportedPacket,
            Some(1),
            Some(row),
            Some(0),
        ));
    }
    let expected_function =
        0xff80 | u32::from(metadata.identity().kind().function_code()) | (2 << 4);
    if function_words[0] != expected_function || function_words[1..].iter().any(|word| *word != 0) {
        return Err(error(Failure::Identity, Some(1), Some(28), Some(1)));
    }
    let mut packets = Vec::new();
    packets
        .try_reserve_exact(usize::from(last))
        .map_err(|_| error(Failure::Budget, None, None, None))?;
    for (logical_row, unit) in (1..=last).zip(&units[2..]) {
        let unit_index = usize::from(logical_row) + 1;
        let (row, designation, words) = read_packet(metadata, unit, unit_index)?;
        let expected = if logical_row < 26 {
            (logical_row, u8::from(logical_row <= pointer_rows))
        } else {
            (26, logical_row - 26)
        };
        if (row, designation) != expected {
            return Err(error(
                Failure::UnsupportedPacket,
                Some(unit_index),
                Some(row),
                Some(0),
            ));
        }
        packets.push(TeletextEncodedObjectPacket {
            logical_row,
            designation,
            words,
        });
    }
    Ok(TeletextEncodedObjectProjection {
        resource_header: metadata,
        transmission_mode: page.transmission_mode(),
        header,
        function_words,
        pointer_rows,
        packets,
    })
}

fn pointer_extent(
    page: &EncodedTeletextObjectPage,
) -> Result<u8, TeletextEncodedDisplayProjectionError> {
    let mut rows = 2;
    for position in page.definition_positions() {
        if position.target().resource() != Some(page.header().identity())
            || position.object_number() > 511
        {
            return Err(error(Failure::Identity, None, None, None));
        }
        rows = rows.max(
            u8::try_from((position.object_number() >> 7) + 1)
                .map_err(|_| error(Failure::Identity, None, None, None))?,
        );
    }
    Ok(rows)
}

fn read_packet(
    metadata: TeletextResourceHeaderMetadata,
    unit: &[u8; 46],
    index: usize,
) -> Result<(u8, u8, [u32; 13]), TeletextEncodedDisplayProjectionError> {
    let observation = observe_dvb_teletext_header(unit)
        .map_err(|_| error(Failure::Envelope, Some(index), None, None))?;
    let packet = match observation {
        TeletextHeaderObservation::NotHeader(packet) => packet,
        TeletextHeaderObservation::NotTeletext { .. } => {
            return Err(error(Failure::Envelope, Some(index), None, None))
        }
        _ => return Err(error(Failure::HeaderOrder, Some(index), Some(0), None)),
    };
    check_resource_packet(metadata, unit, packet, index)?;
    let row = packet.packet_number;
    let body: [u8; 40] = std::array::from_fn(|column| unit[column + 6].reverse_bits());
    let designation = exact_nibble(body[0])
        .ok_or_else(|| error(Failure::Protection, Some(index), Some(row), Some(0)))?;
    let mut words = [0; 13];
    for (word, column) in words.iter_mut().zip((1_u8..40).step_by(3)) {
        let column_index = usize::from(column);
        *word = exact_triplet([
            body[column_index],
            body[column_index + 1],
            body[column_index + 2],
        ])
        .ok_or_else(|| error(Failure::Protection, Some(index), Some(row), Some(column)))?;
    }
    Ok((row, designation, words))
}

#[cfg(test)]
mod tests;

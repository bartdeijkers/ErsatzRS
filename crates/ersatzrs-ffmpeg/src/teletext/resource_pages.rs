//! Complete authored MOT/monochrome DRCS packets (EN 300 706 annexes A/B).
//! Assembly does not establish service allocation, lifecycle consistency,
//! repetition/update ordering, publication or independent client acceptance.

use ersatzrs_domain::{
    TeletextDrcsSubtable, TeletextMotLevel25Declaration, TeletextResourceHeaderError,
    TeletextResourceHeaderMetadata, TeletextResourceIdentity, TeletextResourceLastPacket,
    TeletextResourceTransmission, TeletextResourceUpdateCounter,
};

use super::packets::{data_unit, hamming_24_18, hamming_8_4, odd_parity};
use super::{encode_teletext_drcs_bodies, encode_teletext_mot_bodies};
use super::{TeletextMotDefaultAllocations, TeletextMotEncodingError};

/// C11 must agree across every display and resource page in the service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextResourceTransmissionMode {
    Parallel,
    Serial,
}

/// Explicit full-page lifecycle; the caller owns previous content and counter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextResourceFullUpdate {
    Unchanged,
    Changed,
}

impl TeletextResourceFullUpdate {
    const fn transmission(self) -> TeletextResourceTransmission {
        match self {
            Self::Unchanged => TeletextResourceTransmission::FullUnchangedFirst,
            Self::Changed => TeletextResourceTransmission::FullChangedFirst,
        }
    }
}

/// Bounded immutable page, without a closing next-page/FF header.
/// This is neither a validated service nor an output/publication capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextResourcePage {
    header: TeletextResourceHeaderMetadata,
    transmission_mode: TeletextResourceTransmissionMode,
    units: Vec<[u8; 46]>,
}

impl EncodedTeletextResourcePage {
    #[must_use]
    pub const fn header(&self) -> TeletextResourceHeaderMetadata {
        self.header
    }

    #[must_use]
    pub const fn transmission_mode(&self) -> TeletextResourceTransmissionMode {
        self.transmission_mode
    }

    /// Header, function/modes metadata, then ascending actual body rows.
    #[must_use]
    pub fn data_units(&self) -> &[[u8; 46]] {
        &self.units
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextResourcePageEncodingError {
    #[error(transparent)]
    Header(#[from] TeletextResourceHeaderError),
    #[error(transparent)]
    Mot(#[from] TeletextMotEncodingError),
}

/// Emit a non-subtitle resource header with blank non-display text.
///
/// C5=C6=0 and C7=C9=C10=1 are canonical suppression choices, not universal
/// restrictions on imported headers. C14 is reserved zero; C12/C13 carry
/// resource lifecycle, never a national option. Body/counter consistency is
/// separate from encoding the explicitly supplied metadata.
#[must_use]
pub fn encode_teletext_resource_header(
    metadata: TeletextResourceHeaderMetadata,
    mode: TeletextResourceTransmissionMode,
) -> [u8; 46] {
    let page = metadata.identity().page();
    let [low, high] = metadata.physical_subcode().to_le_bytes();
    let lifecycle = metadata.transmission();
    let controls = 1 | (u8::from(lifecycle.update_indicator()) << 1) | 4 | 8;
    let tail = u8::from(mode == TeletextResourceTransmissionMode::Serial)
        | (u8::from(lifecycle.continuation()) << 1)
        | (u8::from(lifecycle.partial()) << 2);
    let nibbles = [
        page.page() & 15,
        page.page() >> 4,
        low & 15,
        (low >> 4) | (u8::from(lifecycle.erase_page()) << 3),
        high & 15,
        high >> 4,
        controls,
        tail,
    ];
    let mut body = [odd_parity(b' '); 40];
    for (target, nibble) in body[..8].iter_mut().zip(nibbles) {
        *target = hamming_8_4(nibble);
    }
    data_unit(page.wire_magazine(), 0, &body, false)
}

/// Encode a full MOT, deriving nn=21 from its actual final row.
/// X28/0 precedes the seventeen body rows and declares function 6/coding 3.
/// Table 6 bits 8–18 use raw 0x0ff80 (ETSI TR 100 287 10.6.3.1).
/// Reserved tail bits are writer zero.
/// # Errors
/// Rejects invalid supplied default allocations before returning a page.
pub fn encode_teletext_mot_page(
    declaration: &TeletextMotLevel25Declaration,
    allocations: &TeletextMotDefaultAllocations,
    counter: TeletextResourceUpdateCounter,
    update: TeletextResourceFullUpdate,
    mode: TeletextResourceTransmissionMode,
) -> Result<EncodedTeletextResourcePage, TeletextResourcePageEncodingError> {
    let bodies = encode_teletext_mot_bodies(declaration, allocations)?;
    let header = full_header(declaration.identity(), 21, counter, update)?;
    let mut units = Vec::with_capacity(19);
    units.push(encode_teletext_resource_header(header, mode));
    units.push(data_unit(
        declaration.identity().page().wire_magazine(),
        28,
        &function_body(declaration.identity()),
        false,
    ));
    for packet in bodies {
        units.push(data_unit(
            declaration.identity().page().wire_magazine(),
            packet.row,
            &packet.body,
            false,
        ));
    }
    Ok(EncodedTeletextResourcePage {
        header,
        transmission_mode: mode,
        units,
    })
}

/// Encode a full sparse monochrome DRCS page, preserving its logical S1.
/// X28/3 precedes ascending defined rows; nn is the last actual row, not 28.
/// An empty subtable gets one SPACE-only row and all-F modes: legal nn=1
/// without defining a blank glyph. At most 26 units are allocated.
/// Partial/fragment assembly and updates remain separate work.
/// # Errors
/// Propagates checked header construction failures before returning a page.
pub fn encode_teletext_drcs_page(
    table: &TeletextDrcsSubtable,
    counter: TeletextResourceUpdateCounter,
    update: TeletextResourceFullUpdate,
    mode: TeletextResourceTransmissionMode,
) -> Result<EncodedTeletextResourcePage, TeletextResourcePageEncodingError> {
    let mut bodies = encode_teletext_drcs_bodies(table);
    if bodies.rows.iter().all(Option::is_none) {
        bodies.rows[0] = Some([odd_parity(b' '); 40]);
    }
    let last = (1_u8..=24)
        .zip(&bodies.rows)
        .rev()
        .find_map(|(row, body)| body.as_ref().map(|_| row))
        .unwrap_or(1);
    let header = full_header(table.identity(), last, counter, update)?;
    let mut units = Vec::with_capacity(26);
    units.push(encode_teletext_resource_header(header, mode));
    units.push(data_unit(
        table.identity().page().wire_magazine(),
        28,
        &bodies.modes,
        false,
    ));
    for (row, body) in (1_u8..=24).zip(bodies.rows) {
        if let Some(body) = body {
            units.push(data_unit(
                table.identity().page().wire_magazine(),
                row,
                &body,
                false,
            ));
        }
    }
    Ok(EncodedTeletextResourcePage {
        header,
        transmission_mode: mode,
        units,
    })
}

fn full_header(
    identity: TeletextResourceIdentity,
    last: u8,
    counter: TeletextResourceUpdateCounter,
    update: TeletextResourceFullUpdate,
) -> Result<TeletextResourceHeaderMetadata, TeletextResourceHeaderError> {
    TeletextResourceHeaderMetadata::new(
        identity,
        TeletextResourceLastPacket::new(last)?,
        counter,
        update.transmission(),
    )
}

fn function_body(identity: TeletextResourceIdentity) -> [u8; 40] {
    let mut body = [0_u8; 40];
    body[0] = hamming_8_4(0);
    let first = 0x0ff80
        | u32::from(identity.kind().function_code())
        | (u32::from(identity.kind().coding_code()) << 4);
    for (target, word) in body[1..]
        .chunks_exact_mut(3)
        .zip(std::iter::once(first).chain([0; 12]))
    {
        target.copy_from_slice(&hamming_24_18(word));
    }
    body
}

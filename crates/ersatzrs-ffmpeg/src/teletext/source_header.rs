//! Checked metadata from one explicitly supplied EN 300 472 DVB data unit.
//!
//! EN 300 472 sections 4.3/4.4 define the envelope; EN 300 706 sections 7.1.2,
//! 8.2 and 9.3.1 plus annex A.1 define address/protection/header semantics.
//! This neither discovers TS/PES data nor acquires a page body. Descriptor
//! language/type, source binding and selection policy remain separate. C12–C14
//! are raw bits: X/28, M/29 or non-display page context can change their meaning.
//!
//! Hamming 8/4 accepts exact codewords or corrects a single erroneous bit and
//! rejects detected double-bit errors. Larger corruptions can alias an accepted
//! codeword; neither a correction count nor acceptance certifies source bytes.

use ersatzrs_domain::TeletextPageNumber;

use super::packets::hamming_8_4;

/// Malformed recognized teletext metadata, with no payload retained in errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextHeaderError {
    #[error("DVB teletext data unit length must be 44")]
    Length,
    #[error("DVB teletext envelope uses unsupported reserved bits")]
    ReservedEnvelope,
    #[error("DVB teletext line offset must be undefined or between seven and twenty-two")]
    LineOffset,
    #[error("DVB teletext framing code is invalid")]
    FramingCode,
    #[error("DVB teletext packet address has uncorrectable protection")]
    AddressProtection,
    #[error("DVB teletext page header has uncorrectable protection")]
    HeaderProtection,
    #[error("DVB teletext null page address FF:3F7F must not be transmitted")]
    ProhibitedNullAddress,
}

/// Supplied unit classification, independent from C6 or descriptor type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextDataUnitKind {
    NonSubtitle,
    Subtitle,
}

/// Checked envelope and packet address from the supplied unit only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuppliedTeletextPacketAddress {
    pub data_unit_kind: TeletextDataUnitKind,
    /// True denotes the first field; false denotes the second field.
    pub field_parity: bool,
    /// Zero is undefined; seven through twenty-two retain the supplied VBI line.
    pub line_offset: u8,
    /// Raw three-bit magazine; zero represents magazine eight.
    pub wire_magazine: u8,
    pub packet_number: u8,
    /// Address codewords corrected by one bit, zero through two.
    pub corrected_address_bytes: u8,
}

/// All raw C4–C14 bits, stored with C4 at bit zero and C14 at bit ten.
/// These are observed header controls, not authored flags or subtitle policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextHeaderControls(u16);

impl TeletextHeaderControls {
    /// Raw controls, without applying receiver actions or page-function semantics.
    #[must_use]
    pub const fn raw(self) -> u16 {
        self.0
    }

    /// Raw C4 erase request; no retained decoder state is changed here.
    #[must_use]
    pub const fn erase_page(self) -> bool {
        self.0 & 1 != 0
    }

    /// Raw C6 flag, independently from supplied unit type and selection policy.
    #[must_use]
    pub const fn subtitle(self) -> bool {
        self.0 & 4 != 0
    }

    /// Raw C12/C13/C14 in that order with C12 most significant.
    /// This is not an inferred charset, national subset or language.
    #[must_use]
    pub const fn c12_c14_bits(self) -> u8 {
        (self.0.to_le_bytes()[1] & 7).reverse_bits() >> 5
    }
}

/// Observed header identity; no body, charset, language or readiness is implied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuppliedTeletextHeader {
    pub packet_address: SuppliedTeletextPacketAddress,
    /// Raw hexadecimal page byte, including non-display addresses.
    pub wire_page: u8,
    /// Raw S4/S3/S2/S1 identity, with C4/C5/C6 stored separately in controls.
    pub subcode: u16,
    pub controls: TeletextHeaderControls,
    /// Header codewords corrected by one bit, zero through eight.
    pub corrected_header_bytes: u8,
}

impl SuppliedTeletextHeader {
    /// A decimal address when the raw page nibbles permit it; never a ready page.
    #[must_use]
    pub fn display_page(&self) -> Option<TeletextPageNumber> {
        let tens = self.wire_page >> 4;
        let units = self.wire_page & 15;
        let wire_magazine = self.packet_address.wire_magazine;
        if tens > 9 || units > 9 || wire_magazine > 7 {
            return None;
        }
        let magazine = if wire_magazine == 0 {
            8
        } else {
            u16::from(wire_magazine)
        };
        TeletextPageNumber::new(magazine * 100 + u16::from(tens) * 10 + u16::from(units)).ok()
    }
}

/// Typed interpretation without promoting unrelated units into page headers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextHeaderObservation {
    /// Includes stuffing (0xFF), reserved and user-defined unit identifiers.
    /// Their remaining bytes are not interpreted as a teletext envelope.
    NotTeletext {
        data_unit_id: u8,
    },
    /// Checked teletext address with packet number other than zero.
    /// Header data bytes are not examined.
    NotHeader(SuppliedTeletextPacketAddress),
    Header(SuppliedTeletextHeader),
    /// FF with a non-null subcode is time filling/termination, not an acquired page.
    Terminator(SuppliedTeletextHeader),
}

/// Interpret exactly one supplied DVB unit, not a PES packet or byte-stream search.
///
/// Header text odd parity is separate from protected metadata and is not decoded.
/// The caller owns provenance, stream/service association and acquisition bounds.
/// # Errors
/// Returns static envelope/protection errors or the prohibited null page address.
pub fn observe_dvb_teletext_header(
    unit: &[u8; 46],
) -> Result<TeletextHeaderObservation, TeletextHeaderError> {
    let data_unit_kind = match unit[0] {
        2 => TeletextDataUnitKind::NonSubtitle,
        3 => TeletextDataUnitKind::Subtitle,
        data_unit_id => return Ok(TeletextHeaderObservation::NotTeletext { data_unit_id }),
    };
    if unit[1] != 44 {
        return Err(TeletextHeaderError::Length);
    }
    if unit[2] & 0xc0 != 0xc0 {
        return Err(TeletextHeaderError::ReservedEnvelope);
    }
    let line_offset = unit[2] & 31;
    if line_offset != 0 && !(7..=22).contains(&line_offset) {
        return Err(TeletextHeaderError::LineOffset);
    }
    if unit[3] != 0xe4 {
        return Err(TeletextHeaderError::FramingCode);
    }
    let (low, first_corrected) =
        decode_hamming(unit[4].reverse_bits()).ok_or(TeletextHeaderError::AddressProtection)?;
    let (high, second_corrected) =
        decode_hamming(unit[5].reverse_bits()).ok_or(TeletextHeaderError::AddressProtection)?;
    let address = low | (high << 4);
    let packet_address = SuppliedTeletextPacketAddress {
        data_unit_kind,
        field_parity: unit[2] & 32 != 0,
        line_offset,
        wire_magazine: address & 7,
        packet_number: address >> 3,
        corrected_address_bytes: u8::from(first_corrected) + u8::from(second_corrected),
    };
    if packet_address.packet_number != 0 {
        return Ok(TeletextHeaderObservation::NotHeader(packet_address));
    }
    let header = decode_header(unit, packet_address)?;
    if header.wire_page == 0xff {
        if header.subcode == 0x3f7f {
            return Err(TeletextHeaderError::ProhibitedNullAddress);
        }
        return Ok(TeletextHeaderObservation::Terminator(header));
    }
    Ok(TeletextHeaderObservation::Header(header))
}

fn decode_header(
    unit: &[u8; 46],
    packet_address: SuppliedTeletextPacketAddress,
) -> Result<SuppliedTeletextHeader, TeletextHeaderError> {
    let mut nibbles = [0_u8; 8];
    let mut corrected_header_bytes = 0;
    for (nibble, byte) in nibbles.iter_mut().zip(&unit[6..14]) {
        let (value, corrected) =
            decode_hamming(byte.reverse_bits()).ok_or(TeletextHeaderError::HeaderProtection)?;
        *nibble = value;
        corrected_header_bytes += u8::from(corrected);
    }
    let [units, tens, s1, s2_c4, s3, s4_c5_c6, c7_c10, c11_c14] = nibbles;
    let subcode = u16::from(s1)
        | (u16::from(s2_c4 & 7) << 4)
        | (u16::from(s3) << 8)
        | (u16::from(s4_c5_c6 & 3) << 12);
    let controls = TeletextHeaderControls(
        u16::from(s2_c4 >> 3)
            | (u16::from(s4_c5_c6 >> 2) << 1)
            | (u16::from(c7_c10) << 3)
            | (u16::from(c11_c14) << 7),
    );
    Ok(SuppliedTeletextHeader {
        packet_address,
        wire_page: units | (tens << 4),
        subcode,
        controls,
        corrected_header_bytes,
    })
}

fn decode_hamming(byte: u8) -> Option<(u8, bool)> {
    (0_u8..16).find_map(|value| {
        let distance = (hamming_8_4(value) ^ byte).count_ones();
        (distance <= 1).then_some((value, distance == 1))
    })
}

#[cfg(test)]
#[path = "source_header_tests.rs"]
mod tests;

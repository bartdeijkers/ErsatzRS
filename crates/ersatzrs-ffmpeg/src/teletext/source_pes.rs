//! Borrowed framing from one explicitly supplied complete DVB teletext PES.
//!
//! EN 300 472 sections 4.2–4.4 define the fixed header and data-unit framing.
//! This supports no optional fields or PTS alone, not every valid MPEG PES
//! header: DTS, extensions and other optional fields are explicitly unsupported.
//! Unknown units are retained without interpretation. Successful framing is not
//! full DVB/PSI conformity: signalled DVB teletext streams permit only unit IDs
//! 0x02/0x03/0xFF. Unknown units cannot certify a stream or usable inventory.
//! No TS discovery, acquisition, header decoding, source association, clock
//! unwrapping or page readiness is performed here.

use mpeg2ts_reader::pes::Timestamp;

use super::TeletextTimestamp;

// Largest multiple of 184 fitting the nonzero 16-bit PES length plus six bytes.
const MAX_PES_BYTES: usize = 65_504;
const PAYLOAD_START: usize = 46;

/// Static framing failures, without retaining supplied bytes in errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPesError {
    #[error("supplied teletext PES length is invalid or incomplete")]
    PacketLength,
    #[error("supplied teletext PES start code is invalid")]
    StartCode,
    #[error("supplied teletext PES must use private_stream_1")]
    StreamId,
    #[error("supplied teletext PES header marker bits are invalid")]
    HeaderMarker,
    #[error("supplied teletext PES must be data aligned")]
    Alignment,
    #[error("scrambled teletext PES is unsupported")]
    Scrambled,
    #[error("supplied teletext PES optional header is unsupported")]
    UnsupportedHeader,
    #[error("supplied teletext PES header data length must be 36")]
    HeaderDataLength,
    #[error("supplied teletext PES header stuffing is invalid")]
    HeaderStuffing,
    #[error("supplied teletext PES data identifier must be between 0x10 and 0x1F")]
    DataIdentifier,
    #[error("supplied teletext PES data unit is incomplete")]
    UnitBoundary,
    #[error("supplied teletext data unit payload length must be 44")]
    TeletextUnitLength,
    #[error("supplied teletext PES timestamp protection or prefix is invalid")]
    Timestamp,
}

/// A checked complete packet borrowing all data from the caller.
///
/// Checking framing does not certify any teletext envelope, protected header or
/// body. PTS, when present, concerns the first following access unit; it is not
/// an independently acquired timestamp for each returned data unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuppliedTeletextPes<'a> {
    bytes: &'a [u8],
    pts: Option<TeletextTimestamp>,
}

impl<'a> SuppliedTeletextPes<'a> {
    /// Validate exactly one bounded packet before exposing any of its units.
    ///
    /// Input is intrinsically bounded by the PES length field and required
    /// 184-byte multiple, at most 65,504 bytes. Parsing and iteration allocate
    /// nothing. Stuffing and unknown unit payloads may have any encoded length;
    /// only identifiers 0x02/0x03 require 44 bytes of teletext payload.
    /// # Errors
    /// Rejects malformed framing, scrambling, invalid PTS or optional fields
    /// outside the explicitly supported no-PTS/PTS-only header subset.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, TeletextPesError> {
        if bytes.len() < 184 || bytes.len() > MAX_PES_BYTES || !bytes.len().is_multiple_of(184) {
            return Err(TeletextPesError::PacketLength);
        }
        let declared = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        if declared == 0 || declared + 6 != bytes.len() {
            return Err(TeletextPesError::PacketLength);
        }
        if bytes[..3] != [0, 0, 1] {
            return Err(TeletextPesError::StartCode);
        }
        if bytes[3] != 0xbd {
            return Err(TeletextPesError::StreamId);
        }
        if bytes[6] & 0xc0 != 0x80 {
            return Err(TeletextPesError::HeaderMarker);
        }
        if bytes[6] & 0x30 != 0 {
            return Err(TeletextPesError::Scrambled);
        }
        if bytes[6] & 4 == 0 {
            return Err(TeletextPesError::Alignment);
        }
        if bytes[8] != 0x24 {
            return Err(TeletextPesError::HeaderDataLength);
        }
        let (pts, stuffing_start) = match bytes[7] {
            0 => (None, 9),
            0x80 => {
                // The complete fixed header guarantees this exact five-byte
                // slice before calling the dependency's indexed decoder.
                let timestamp = Timestamp::from_pts_bytes(&bytes[9..14])
                    .map_err(|_| TeletextPesError::Timestamp)?;
                let pts = TeletextTimestamp::new(timestamp.value())
                    .map_err(|_| TeletextPesError::Timestamp)?;
                (Some(pts), 14)
            }
            _ => return Err(TeletextPesError::UnsupportedHeader),
        };
        if bytes[stuffing_start..45].iter().any(|byte| *byte != 0xff) {
            return Err(TeletextPesError::HeaderStuffing);
        }
        if !(0x10..=0x1f).contains(&bytes[45]) {
            return Err(TeletextPesError::DataIdentifier);
        }
        validate_units(&bytes[PAYLOAD_START..])?;
        Ok(Self { bytes, pts })
    }

    /// Original complete input, without copying or retaining an owned buffer.
    #[must_use]
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Raw supplied DVB identifier; no language, service or provenance is implied.
    #[must_use]
    pub fn data_identifier(&self) -> u8 {
        self.bytes[45]
    }

    /// Checked 33-bit PTS, preserving absence independently from value zero.
    #[must_use]
    pub const fn pts(&self) -> Option<TeletextTimestamp> {
        self.pts
    }

    /// All supplied units in order, including stuffing and unknown identifiers.
    #[must_use]
    pub fn units(&self) -> SuppliedTeletextUnits<'a> {
        SuppliedTeletextUnits {
            remaining: &self.bytes[PAYLOAD_START..],
        }
    }
}

fn validate_units(mut bytes: &[u8]) -> Result<(), TeletextPesError> {
    while !bytes.is_empty() {
        let header = bytes.get(..2).ok_or(TeletextPesError::UnitBoundary)?;
        let end = 2 + usize::from(header[1]);
        let remaining = bytes.get(end..).ok_or(TeletextPesError::UnitBoundary)?;
        if matches!(header[0], 2 | 3) && header[1] != 44 {
            return Err(TeletextPesError::TeletextUnitLength);
        }
        bytes = remaining;
    }
    Ok(())
}

/// One framed unit borrowing its identifier, length and payload from the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuppliedTeletextDataUnit<'a> {
    bytes: &'a [u8],
}

impl<'a> SuppliedTeletextDataUnit<'a> {
    /// Complete framed unit, including its two-byte prefix.
    #[must_use]
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Raw unit identifier, without filtering unknown or stuffing identifiers.
    #[must_use]
    pub fn id(&self) -> u8 {
        self.bytes[0]
    }

    /// Original encoded payload length, excluding the two-byte prefix.
    #[must_use]
    pub fn length(&self) -> u8 {
        self.bytes[1]
    }

    /// Original payload; no envelope, header or text interpretation is performed.
    #[must_use]
    pub fn payload(&self) -> &'a [u8] {
        &self.bytes[2..]
    }

    /// Borrow a recognized 46-byte teletext unit for the separate header observer.
    /// Unknown identifiers remain raw even when their encoded length is 44.
    #[must_use]
    pub fn teletext_unit(&self) -> Option<&'a [u8; 46]> {
        if matches!(self.id(), 2 | 3) {
            self.bytes.try_into().ok()
        } else {
            None
        }
    }
}

/// Allocation-free traversal of a packet's fully checked unit boundaries.
#[derive(Debug, Clone)]
pub struct SuppliedTeletextUnits<'a> {
    remaining: &'a [u8],
}

impl<'a> Iterator for SuppliedTeletextUnits<'a> {
    type Item = SuppliedTeletextDataUnit<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let header = self.remaining.get(..2)?;
        let end = 2 + usize::from(header[1]);
        let bytes = self.remaining.get(..end)?;
        self.remaining = self.remaining.get(end..)?;
        Some(SuppliedTeletextDataUnit { bytes })
    }
}

impl std::iter::FusedIterator for SuppliedTeletextUnits<'_> {}

#[cfg(test)]
#[path = "source_pes_tests.rs"]
mod tests;

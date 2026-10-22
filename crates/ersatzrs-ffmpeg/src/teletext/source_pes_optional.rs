//! Structural traversal of the fixed DVB optional-header area.
//!
//! H.222.0 (03/2017), 2.4.3.6/7, table 2-21. The locked MPEG reader's
//! extension is opaque and exposes neither consumed length nor stuffing checks.
//! Reuse its timestamp decoder; only traverse the missing bounded structure.

use mpeg2ts_reader::pes::Timestamp;

use super::{TeletextPesError, TeletextTimestamp};

pub(super) fn parse(
    flags: u8,
    bytes: &[u8],
) -> Result<Option<TeletextTimestamp>, TeletextPesError> {
    let mut fields = Fields(bytes);
    let pts = match flags >> 6 {
        0 => None,
        2 => Some(fields.timestamp(2)?),
        3 => {
            let pts = fields.timestamp(3)?;
            fields.timestamp(1)?; // DTS is checked, never substituted for PTS.
            Some(pts)
        }
        _ => return Err(TeletextPesError::Timestamp),
    };
    if flags & 0x20 != 0 {
        let escr = fields.take(6)?;
        let extension = (u16::from(escr[4] & 3) << 7) | u16::from(escr[5] >> 1);
        if escr[0] & 0xc0 != 0xc0
            || [0, 2, 4].iter().any(|i| escr[*i] & 4 == 0)
            || escr[5] & 1 == 0
            || extension >= 300
        {
            return Err(TeletextPesError::OptionalHeader);
        }
    }
    if flags & 0x10 != 0 {
        let rate = fields.take(3)?;
        if rate[0] & 0x80 == 0 || rate[2] & 1 == 0 {
            return Err(TeletextPesError::OptionalHeader);
        }
    }
    if flags & 8 != 0 {
        fields.take(1)?; // Trick-mode semantics do not change unit framing.
    }
    if flags & 4 != 0 && fields.take(1)?[0] & 0x80 == 0 {
        return Err(TeletextPesError::OptionalHeader);
    }
    if flags & 2 != 0 {
        fields.take(2)?; // No previous-packet CRC authority is asserted.
    }
    if flags & 1 != 0 {
        fields.extension()?;
    }
    if fields.0.iter().any(|byte| *byte != 0xff) {
        return Err(TeletextPesError::HeaderStuffing);
    }
    Ok(pts)
}

struct Fields<'a>(&'a [u8]);

impl<'a> Fields<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], TeletextPesError> {
        let (field, remaining) = self
            .0
            .split_at_checked(length)
            .ok_or(TeletextPesError::OptionalHeader)?;
        self.0 = remaining;
        Ok(field)
    }

    fn timestamp(&mut self, prefix: u8) -> Result<TeletextTimestamp, TeletextPesError> {
        let bytes = self.take(5)?;
        if bytes[0] >> 4 != prefix {
            return Err(TeletextPesError::Timestamp);
        }
        // The indexed dependency decoder only receives an exact checked slice.
        let stamp = Timestamp::from_bytes(bytes).map_err(|_| TeletextPesError::Timestamp)?;
        TeletextTimestamp::new(stamp.value()).map_err(|_| TeletextPesError::Timestamp)
    }

    fn extension(&mut self) -> Result<(), TeletextPesError> {
        let flags = self.take(1)?[0];
        if flags & 0x0e != 0x0e {
            return Err(TeletextPesError::OptionalHeader);
        }
        if flags & 0x80 != 0 {
            self.take(16)?;
        }
        if flags & 0x40 != 0 {
            let length = usize::from(self.take(1)?[0]);
            self.take(length)?; // Bounded opaque nested pack, not pack validation.
        }
        if flags & 0x20 != 0 && self.take(2)?.iter().any(|byte| byte & 0x80 == 0) {
            return Err(TeletextPesError::OptionalHeader);
        }
        if flags & 0x10 != 0 && self.take(2)?[0] & 0xc0 != 0x40 {
            return Err(TeletextPesError::OptionalHeader);
        }
        if flags & 1 != 0 {
            self.extension2()?;
        }
        Ok(())
    }

    fn extension2(&mut self) -> Result<(), TeletextPesError> {
        let length = self.take(1)?[0];
        if length & 0x80 == 0 {
            return Err(TeletextPesError::OptionalHeader);
        }
        let mut fields = Self(self.take(usize::from(length & 0x7f))?);
        let flags = fields.take(1)?[0];
        // stream_id_extension is only legal with stream_id FD, not our BD.
        if flags & 0xfe != 0xfe {
            return Err(TeletextPesError::OptionalHeader);
        }
        if flags & 1 == 0 {
            fields.timestamp(0x0f)?; // TREF never replaces missing PTS either.
        }
        if fields.0.iter().any(|byte| *byte != 0xff) {
            return Err(TeletextPesError::OptionalHeader);
        }
        Ok(())
    }
}

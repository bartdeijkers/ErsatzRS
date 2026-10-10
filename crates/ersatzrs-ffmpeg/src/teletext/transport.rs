use super::TeletextCarouselBatch;

const PTS_MASK: u64 = (1 << 33) - 1;

/// Recoverable transmission-planning errors, separate from page validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextTransportError {
    /// A dedicated elementary PID must avoid reserved system and null PIDs.
    #[error("teletext PID must be in 0x0020..=0x1FFE")]
    Pid,
    /// A supplied wire timestamp exceeds MPEG's 33-bit field.
    #[error("teletext PTS exceeds 33 bits")]
    Timestamp,
    /// The extended programme-relative carousel clock cannot advance.
    #[error("teletext carousel clock overflow")]
    ClockOverflow,
    /// Replacing pages mid-cycle would mix distinct coherent page sets.
    #[error("teletext replacement requires a complete carousel boundary")]
    MidCycleReplacement,
}

/// Exact 33-bit MPEG presentation time, measured in 90 kHz ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextTimestamp(u64);

impl TeletextTimestamp {
    /// Accept a wire timestamp without silently truncating excess bits.
    /// # Errors
    /// Rejects values beyond the MPEG PTS field's range.
    pub fn new(ticks: u64) -> Result<Self, TeletextTransportError> {
        if ticks > PTS_MASK {
            return Err(TeletextTransportError::Timestamp);
        }
        Ok(Self(ticks))
    }

    /// Original 33-bit timestamp.
    #[must_use]
    pub const fn ticks(self) -> u64 {
        self.0
    }

    /// Add an extended programme offset with MPEG's intentional 33-bit wrap.
    #[must_use]
    pub const fn wrapping_add(self, ticks: u64) -> Self {
        Self((self.0 + (ticks & PTS_MASK)) & PTS_MASK)
    }

    fn bytes(self) -> [u8; 5] {
        let value = self.0;
        // Each narrowing extracts at most eight bits from the checked 33-bit value.
        [
            0x21 | u8::try_from((value >> 29) & 0x0e).expect("four-bit PTS field"),
            u8::try_from((value >> 22) & 255).expect("eight-bit PTS field"),
            u8::try_from(((value >> 14) & 0xfe) | 1).expect("eight-bit PTS field"),
            u8::try_from((value >> 7) & 255).expect("eight-bit PTS field"),
            u8::try_from(((value << 1) & 0xfe) | 1).expect("eight-bit PTS field"),
        ]
    }
}

/// Packetize only host-authored teletext; this does not multiplex a programme.
///
/// EN 300 472 requires `private_stream_1`, a fixed 45-byte PES header and an
/// integral number of 184-byte payloads. The dedicated PID's counter advances
/// across batches/cycles. PMT/PID ownership, capacity reservation, programme
/// clock/PCR mapping and replacing null packets are the output adapter's duties.
/// Never append these packets to a constant-rate stream or reuse an occupied PID.
#[derive(Debug)]
pub struct TeletextPacketizer {
    pid: u16,
    continuity: u8,
}

impl TeletextPacketizer {
    /// Choose an elementary PID; the caller must additionally prove ownership.
    /// # Errors
    /// Rejects reserved system, null and out-of-range PIDs.
    pub fn new(pid: u16) -> Result<Self, TeletextTransportError> {
        if !(0x20..=0x1ffe).contains(&pid) {
            return Err(TeletextTransportError::Pid);
        }
        Ok(Self { pid, continuity: 0 })
    }

    /// Dedicated elementary PID; its collision check belongs to the multiplexer.
    #[must_use]
    pub const fn pid(&self) -> u16 {
        self.pid
    }

    /// Wrap a bounded batch in one PES packet using the supplied programme PTS.
    ///
    /// Returns at most eight 188-byte TS packets, with no adaptation field or
    /// audiovisual/PCR rewriting. Call once per scheduled batch after reserving
    /// transmission capacity; repeated calls advance continuity again.
    pub fn packetize(
        &mut self,
        batch: &TeletextCarouselBatch,
        pts: TeletextTimestamp,
    ) -> Vec<[u8; 188]> {
        let units = batch.data_units();
        let padded_units = units.len() + (3 + 4 - units.len() % 4) % 4;
        let size = 46 * (1 + padded_units);
        // Private batch construction bounds size to 1,472 bytes on every target.
        let [.., length_high, length_low] = (size - 6).to_be_bytes();
        let mut pes = vec![0xff; size];
        pes[..9].copy_from_slice(&[0, 0, 1, 0xbd, length_high, length_low, 0x84, 0x80, 0x24]);
        pes[9..14].copy_from_slice(&pts.bytes());
        pes[45] = 0x10;
        for (index, unit) in units.iter().enumerate() {
            pes[46 + index * 46..92 + index * 46].copy_from_slice(unit);
        }
        // Stuffing is a real data unit with length 44, not unstructured PES tail.
        for index in units.len()..padded_units {
            pes[46 + index * 46 + 1] = 44;
        }
        let [high, low] = self.pid.to_be_bytes();
        let mut output = Vec::with_capacity(size / 184);
        for (index, payload) in pes.chunks_exact(184).enumerate() {
            let mut packet = [0_u8; 188];
            packet[..4].copy_from_slice(&[
                0x47,
                high | if index == 0 { 0x40 } else { 0 },
                low,
                0x10 | self.continuity,
            ]);
            packet[4..].copy_from_slice(payload);
            self.continuity = (self.continuity + 1) & 15;
            output.push(packet);
        }
        output
    }
}

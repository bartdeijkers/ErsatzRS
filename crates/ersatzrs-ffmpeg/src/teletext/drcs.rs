//! Monochrome DRCS bodies, EN 300 706 clauses 9.4.6 and 14.1–14.2.1.

use ersatzrs_domain::{TeletextDrcsSubtable, TeletextResourceIdentity};

use super::packets::{hamming_24_18, hamming_8_4, odd_parity};

/// Pre-bit-reversal resource bodies, not a complete service or readiness token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextDrcsPacketBodies {
    pub identity: TeletextResourceIdentity,
    /// Rows one through 24. Omitted rows contain no defined glyphs.
    pub rows: [Option<[u8; 40]>; 24],
    /// Complete X28/3 with derived function/coding and explicit sparse modes.
    pub modes: [u8; 40],
}

/// Encode exact authored slots as twelve-by-ten monochrome pattern units.
///
/// Each scanline has two six-pixel D-bytes, in left-to-right order, with bit
/// seven set and odd parity in bit eight. An undefined half of a present row
/// uses SPACE, not defined blank pixels. X28/3 uses mode zero for defined glyphs
/// and F for undefined slots, including across triplet boundaries. Reserved bits
/// are zero by writer convention. The checked input already locates/rejects
/// invalid geometry, purposes, slots and duplicates; no additional allocation
/// is needed here. Headers, carousel, invocation, transport and actual display
/// acceptance remain separate obligations.
#[must_use]
pub fn encode_teletext_drcs_bodies(table: &TeletextDrcsSubtable) -> TeletextDrcsPacketBodies {
    let mut rows = [None; 24];
    let mut modes = [15_u8; 48];
    for glyph in table.glyphs() {
        let slot = usize::from(glyph.slot.get());
        modes[slot] = 0;
        let body = rows[slot / 2].get_or_insert([odd_parity(b' '); 40]);
        let start = 20 * (slot % 2);
        for (target, pixels) in body[start..start + 20]
            .chunks_exact_mut(2)
            .zip(glyph.bitmap.get())
        {
            let [left, _] = (pixels >> 6).to_le_bytes();
            let [right, _] = pixels.to_le_bytes();
            target[0] = odd_parity(0x40 | left);
            target[1] = odd_parity(0x40 | (right & 0x3f));
        }
    }
    let mut words = [0_u32; 13];
    words[0] = u32::from(table.identity().kind().function_code());
    // Coding zero and the eleven reserved first-triplet bits remain zero.
    for (slot, mode) in modes.into_iter().enumerate() {
        for bit in 0..4 {
            let offset = 18 + slot * 4 + bit;
            words[offset / 18] |= ((u32::from(mode) >> bit) & 1) << (offset % 18);
        }
    }
    let mut body = [0_u8; 40];
    body[0] = hamming_8_4(3);
    for (target, word) in body[1..].chunks_exact_mut(3).zip(words) {
        target.copy_from_slice(&hamming_24_18(word));
    }
    TeletextDrcsPacketBodies {
        identity: table.identity(),
        rows,
        modes: body,
    }
}

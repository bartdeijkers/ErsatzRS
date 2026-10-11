//! Checked X27/4 declarations, without MOT resolution or publication authority.

use ersatzrs_domain::{TeletextPageAddress, TeletextX27Level25Links, TeletextX27LinkRole};

use super::packets::{hamming_24_18, hamming_8_4};

/// Encode one complete X27/4 body from typed host declarations.
///
/// EN 300 706 tables 15/16 fix the first four roles. Requirement masks and level
/// applicability are preserved, including explicit null records. Relative
/// magazine addressing uses XOR of three-bit magazine addresses. Reserved fields
/// are emitted as zero, a writer convention rather than a universal input rule.
///
/// The two additional Level 3.5 links use canonical null GPOP/POP declarations,
/// with Level-3.5-only validity, relative magazine zero and mask FFFF. This does
/// not encode authored Level 3.5 resources. The output is a pre-bit-reversal
/// 40-byte packet body, not a data unit, complete service or publication token.
#[must_use]
pub fn encode_teletext_compositional_links(
    owner: TeletextPageAddress,
    links: &TeletextX27Level25Links,
) -> [u8; 40] {
    let mut words = [0_u32; 13];
    for (slot, role) in TeletextX27LinkRole::ALL.into_iter().enumerate() {
        let declaration = links.link(role);
        let target = declaration.target();
        let relative = owner.page.wire_magazine() ^ target.wire_magazine();
        words[slot * 2] = first_triplet(
            u32::from(role.wire_code()),
            declaration.validity().wire_code(),
            target.page_byte(),
            relative,
        );
        words[slot * 2 + 1] = u32::from(declaration.required_subpages().get()) << 2;
    }
    // Canonical unused slots calibrated against the pinned QTeletextMaker writer.
    words[8] = 0x3c7c8;
    words[9] = 0x3fffc;
    words[10] = 0x3c7c9;
    words[11] = 0x3fffc;
    // Triplet 13 is reserved and remains zero under this writer convention.
    let mut body = [0_u8; 40];
    body[0] = hamming_8_4(4);
    for (target, word) in body[1..].chunks_exact_mut(3).zip(words) {
        target.copy_from_slice(&hamming_24_18(word));
    }
    body
}

fn first_triplet(function: u32, validity: u8, page: u8, relative_magazine: u8) -> u32 {
    function
        | (u32::from(validity) << 2)
        | (u32::from(page & 15) << 6)
        | (1 << 10)
        | (u32::from(relative_magazine) << 11)
        | (u32::from(page >> 4) << 14)
}

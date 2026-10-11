//! Normative bitmap/mode decoding, without resource headers or client proof.

use ersatzrs_domain::*;

use super::*;

fn table(
    kind: TeletextResourceKind,
    glyphs: Vec<TeletextDrcsGlyphDefinition>,
) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        TeletextResourceIdentity::new(
            kind,
            TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(15).unwrap(),
        )
        .unwrap(),
        glyphs,
    )
    .unwrap()
}

fn glyph(slot: u8, rows: [u16; 10]) -> TeletextDrcsGlyphDefinition {
    TeletextDrcsGlyphDefinition {
        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        bitmap: TeletextDrcsBitmap::new(rows).unwrap(),
    }
}

/// Independent bit-position and odd-parity equations, not production helpers.
fn decode_modes(body: &[u8; 40]) -> [u32; 13] {
    assert_eq!(body[0], 0x5e);
    std::array::from_fn(|triplet| {
        let start = 1 + triplet * 3;
        let word = u32::from_le_bytes([body[start], body[start + 1], body[start + 2], 0]);
        assert_eq!(word.count_ones() % 2, 1);
        for parity in [1_u32, 2, 4, 8, 16] {
            let count: u32 = (1..24)
                .filter(|p| p & parity != 0)
                .map(|p| (word >> (p - 1)) & 1)
                .sum();
            assert_eq!(count % 2, 1);
        }
        [
            3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
        ]
        .into_iter()
        .enumerate()
        .fold(0, |data, (bit, position)| {
            data | (((word >> (position - 1)) & 1) << bit)
        })
    })
}

fn mode(words: &[u32; 13], slot: usize) -> u8 {
    (0..4).fold(0, |value, bit| {
        let offset = 18 + slot * 4 + bit;
        value | (u8::try_from((words[offset / 18] >> (offset % 18)) & 1).unwrap() << bit)
    })
}

/// Match pixels individually in display order rather than combining the packer shifts.
fn decode_pixels(bytes: &[u8]) -> [u16; 10] {
    assert_eq!(bytes.len(), 20);
    std::array::from_fn(|row| {
        let left = bytes[row * 2];
        let right = bytes[row * 2 + 1];
        for byte in [left, right] {
            assert_eq!(byte.count_ones() % 2, 1);
            assert!(byte & 0x40 != 0);
        }
        (0..12).fold(0, |pixels, column| {
            let byte = if column < 6 { left } else { right };
            let pixel = (byte >> (5 - column % 6)) & 1;
            pixels | (u16::from(pixel) << (11 - column))
        })
    })
}

#[test]
fn literal_asymmetric_pixels_have_normative_dbytes_and_odd_parity() {
    let bitmap = [0, 0x800, 1, 0xaaa, 0x555, 0x3f, 0xfc0, 0xfff, 0x801, 0x123];
    let model = table(TeletextResourceKind::Drcs, vec![glyph(0, bitmap)]);
    let encoded = encode_teletext_drcs_bodies(&model);
    assert_eq!(encoded.identity, model.identity());
    let body = encoded.rows[0].unwrap();
    assert_eq!(
        &body[..8],
        &[0x40, 0x40, 0xe0, 0x40, 0x40, 0xc1, 0xea, 0xea]
    );
    assert_eq!(decode_pixels(&body[..20]), bitmap);
    assert_eq!(&body[20..], &[0x20; 20]);
    assert!(encoded.rows[1..].iter().all(Option::is_none));
}

#[test]
fn all_4096_scanlines_preserve_every_pixel_without_mirroring() {
    for pixels in 0..=0xfff {
        let model = table(TeletextResourceKind::Gdrcs, vec![glyph(47, [pixels; 10])]);
        let encoded = encode_teletext_drcs_bodies(&model);
        let body = encoded.rows[23].unwrap();
        assert_eq!(&body[..20], &[0x20; 20]);
        assert_eq!(decode_pixels(&body[20..]), [pixels; 10]);
    }
}

#[test]
fn every_slot_preserves_packet_half_and_mode_boundaries_for_both_purposes() {
    for kind in [TeletextResourceKind::Drcs, TeletextResourceKind::Gdrcs] {
        for slot in 0_u8..48 {
            let model = table(kind, vec![glyph(slot, [1; 10])]);
            let encoded = encode_teletext_drcs_bodies(&model);
            let words = decode_modes(&encoded.modes);
            assert_eq!(
                words[0],
                if kind == TeletextResourceKind::Drcs {
                    5
                } else {
                    4
                }
            );
            assert_eq!(words[11] >> 12, 0);
            assert_eq!(words[12], 0);
            for candidate in 0..48 {
                assert_eq!(
                    mode(&words, candidate),
                    if candidate == usize::from(slot) {
                        0
                    } else {
                        15
                    }
                );
            }
            for (index, row) in encoded.rows.iter().enumerate() {
                if index == usize::from(slot / 2) {
                    let body = row.unwrap();
                    let half = usize::from(slot % 2) * 20;
                    assert_eq!(decode_pixels(&body[half..half + 20]), [1; 10]);
                    let unused = 20 - half;
                    assert_eq!(&body[unused..unused + 20], &[0x20; 20]);
                } else {
                    assert!(row.is_none());
                }
            }
        }
    }
}

#[test]
fn sparse_undefined_and_defined_blank_are_distinct_and_input_order_is_preserved() {
    let empty = encode_teletext_drcs_bodies(&table(TeletextResourceKind::Drcs, vec![]));
    assert!(empty.rows.iter().all(Option::is_none));
    let modes = decode_modes(&empty.modes);
    assert_eq!(&modes[1..11], &[0x3ffff; 10]);
    assert_eq!(modes[11], 0xfff);
    let sparse = table(
        TeletextResourceKind::Drcs,
        vec![glyph(47, [0; 10]), glyph(0, [1; 10]), glyph(1, [0; 10])],
    );
    let reversed = table(
        TeletextResourceKind::Drcs,
        sparse.glyphs().iter().copied().rev().collect(),
    );
    let encoded = encode_teletext_drcs_bodies(&sparse);
    assert_eq!(encoded, encode_teletext_drcs_bodies(&reversed));
    assert_eq!(&encoded.rows[0].unwrap()[20..], &[0x40; 20]);
    assert_eq!(&encoded.rows[23].unwrap()[..20], &[0x20; 20]);
    assert_eq!(&encoded.rows[23].unwrap()[20..], &[0x40; 20]);
    let modes = decode_modes(&encoded.modes);
    assert_eq!(mode(&modes, 0), 0);
    assert_eq!(mode(&modes, 1), 0);
    assert_eq!(mode(&modes, 46), 15);
    assert_eq!(mode(&modes, 47), 0);
}

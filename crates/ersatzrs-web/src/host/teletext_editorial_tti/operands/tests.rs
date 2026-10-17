//! Exact field/family admission; permissions and painted effects are separate.
use super::{operand, TeletextColumnOperation, TeletextGlyphCode, TeletextMosaic};
use ersatzrs_domain::TeletextG1AlphaCode;

#[path = "tests/extended_glyphs.rs"]
mod extended_glyphs;

#[path = "tests/remaining_glyphs.rs"]
mod remaining_glyphs;

#[path = "tests/drcs_references.rs"]
mod drcs_references;

fn g0(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn g1(bits: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G1 {
        mosaic: TeletextMosaic::new(bits).unwrap(),
    }
}

#[test]
fn glyph_operands_preserve_exact_address_mode_and_seven_bit_family() {
    let original = 7 | (9 << 6) | (65 << 11);
    assert_eq!(
        operand(7, g0(b'A'), original, 7 | (9 << 6) | (66 << 11)),
        Some(g0(b'B'))
    );
    for edited in [
        8 | (9 << 6) | (66 << 11),  // different address
        7 | (1 << 6) | (66 << 11),  // different family
        7 | (9 << 6) | (31 << 11),  // control code
        7 | (9 << 6) | (128 << 11), // extra data bit
    ] {
        assert_eq!(operand(7, g0(b'A'), original, edited), None);
    }
    assert_eq!(operand(7, g0(b'C'), original, original), None);
    assert_eq!(
        operand(40, g0(b'A'), 0x28 | (9 << 6) | (65 << 11), original),
        None
    );
}

#[test]
fn mosaic_operands_do_not_normalize_alpha_or_high_bits_into_a_mosaic() {
    let original = 3 | (1 << 6) | (0x20 << 11);
    let full = 3 | (1 << 6) | (0x7f << 11);
    assert_eq!(operand(3, g1(0), original, full), Some(g1(63)));
    assert_eq!(operand(3, g1(63), full, original), Some(g1(0)));
    for edited in [
        3 | (1 << 6) | (0x41 << 11),
        full | (1 << 18),
        full ^ 1,
        full ^ (1 << 6),
    ] {
        assert_eq!(operand(3, g1(0), original, edited), None);
    }
    let alpha = TeletextColumnOperation::G1Alpha {
        code: TeletextG1AlphaCode::new(b'A').unwrap(),
    };
    assert_eq!(operand(3, alpha, 3 | (1 << 6) | (65 << 11), full), None);
}

#[test]
fn old_attribute_reserved_bit_is_still_refused() {
    let expected = TeletextColumnOperation::Attributes {
        double_height: false,
        double_width: false,
        boxed: false,
        conceal: false,
        invert: false,
        underline_or_separated_mosaics: false,
    };
    let original = 4 | (12 << 6);
    assert_eq!(operand(4, expected, original, original | (8 << 11)), None);
}

#[path = "tests/flash_charset.rs"]
mod flash_charset;

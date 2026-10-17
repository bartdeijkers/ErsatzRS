//! Blast-through text stays distinct from mosaics; G3 keeps original compatibility.
use super::{g1, operand, TeletextColumnOperation, TeletextG1AlphaCode, TeletextGlyphCode};
use ersatzrs_domain::TeletextG3Compatibility;

fn alpha(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G1Alpha {
        code: TeletextG1AlphaCode::new(code).unwrap(),
    }
}
fn g3(code: u8, compatibility: TeletextG3Compatibility) -> TeletextColumnOperation {
    TeletextColumnOperation::G3 {
        code: TeletextGlyphCode::new(code).unwrap(),
        compatibility,
    }
}
fn word(column: u8, mode: u8, code: u8) -> u32 {
    u32::from(column) | (u32::from(mode) << 6) | (u32::from(code) << 11)
}

#[test]
fn all_blast_through_alphabetic_codes_stay_separate_from_same_mode_mosaics() {
    let before = word(7, 1, b'A');
    for code in 0..=127 {
        let expected = (0x40..=0x5f).contains(&code).then(|| alpha(code));
        assert_eq!(operand(7, alpha(b'A'), before, word(7, 1, code)), expected);
        if expected.is_some() {
            assert_eq!(operand(7, g1(1), word(7, 1, 0x21), word(7, 1, code)), None);
        }
    }
    for edited in [
        word(8, 1, b'@'),
        word(7, 9, b'@'),
        word(7, 15, b'@'),
        word(7, 1, 128),
        word(7, 1, b'@') | (1 << 18),
    ] {
        assert_eq!(operand(7, alpha(b'A'), before, edited), None);
    }
    assert_eq!(operand(7, alpha(b'B'), before, word(7, 1, b'@')), None);
}

#[test]
fn g3_replacements_keep_both_original_compatibility_modes_and_exact_address() {
    for (mode, compatibility) in [
        (2, TeletextG3Compatibility::Level15),
        (11, TeletextG3Compatibility::Level25),
    ] {
        let before = word(39, mode, 0x2f);
        for code in 0x20..=0x7f {
            assert_eq!(
                operand(39, g3(0x2f, compatibility), before, word(39, mode, code)),
                Some(g3(code, compatibility))
            );
        }
        for edited in [
            word(38, mode, 0x5f),
            word(39, if mode == 2 { 11 } else { 2 }, 0x5f),
            word(39, 9, 0x5f),
            word(39, mode, 31),
            word(39, mode, 128),
            word(39, mode, 0x5f) | (1 << 18),
        ] {
            assert_eq!(operand(39, g3(0x2f, compatibility), before, edited), None);
        }
        assert_eq!(
            operand(39, g3(0x2e, compatibility), before, word(39, mode, 0x5f)),
            None
        );
        assert_eq!(
            operand(
                40,
                g3(0x2f, compatibility),
                word(40, mode, 0x2f),
                word(40, mode, 0x5f)
            ),
            None
        );
    }
}

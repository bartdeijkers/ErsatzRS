//! Existing extended character families keep original addressing and charset context.
use super::{g0, operand, TeletextColumnOperation, TeletextGlyphCode};
use ersatzrs_domain::TeletextDiacriticIndex;

fn g2(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G2 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn diacritic(code: u8, selector: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0Diacritic {
        code: TeletextGlyphCode::new(code).unwrap(),
        diacritic: TeletextDiacriticIndex::new(selector).unwrap(),
    }
}
fn word(column: u8, mode: u8, code: u8) -> u32 {
    u32::from(column) | (u32::from(mode) << 6) | (u32::from(code) << 11)
}

#[test]
fn g2_replacements_retain_original_column_mode_and_checked_seven_bit_code() {
    let before = word(7, 15, 0x23);
    for code in 0x20..=0x7f {
        assert_eq!(
            operand(7, g2(0x23), before, word(7, 15, code)),
            Some(g2(code))
        );
    }
    for after in [
        word(8, 15, 0x42),
        word(7, 9, 0x42),
        word(7, 16, 0x42),
        word(7, 15, 31),
        word(7, 15, 128),
        word(7, 15, 0x42) | (1 << 18),
    ] {
        assert_eq!(operand(7, g2(0x23), before, after), None);
    }
    assert_eq!(operand(7, g2(0x24), before, word(7, 15, 0x42)), None);
    assert_eq!(operand(7, g0(0x23), before, word(7, 15, 0x42)), None);
}

#[test]
fn diacritic_edits_preserve_family_without_restricting_original_selector() {
    for from in 0..16 {
        let before = word(39, 16 | from, b'e');
        for to in 0..16 {
            assert_eq!(
                operand(39, diacritic(b'e', from), before, word(39, 16 | to, b'E')),
                Some(diacritic(b'E', to)),
                "checked code/selector changes retain their original family"
            );
        }
    }
    // Selector0/code2A is still the authored @ identity, not a national alias.
    assert_eq!(
        operand(0, diacritic(b'A', 2), word(0, 18, b'A'), word(0, 16, 0x2a)),
        Some(diacritic(0x2a, 0))
    );
}

#[test]
fn diacritic_mode31_is_a_column_glyph_and_cannot_become_row_termination() {
    let expected = diacritic(b'e', 2);
    let before = word(7, 18, b'e');
    assert_eq!(
        operand(7, expected, before, word(7, 31, b'E')),
        Some(diacritic(b'E', 15))
    );
    for after in [
        word(40, 31, b'E'),
        word(8, 18, b'E'),
        word(7, 15, b'E'),
        word(7, 9, b'E'),
        word(7, 18, 31),
        word(7, 18, 128),
        word(7, 18, b'E') | (1 << 18),
    ] {
        assert_eq!(operand(7, expected, before, after), None);
    }
    assert_eq!(
        operand(7, diacritic(b'e', 1), before, word(7, 18, b'E')),
        None
    );
    assert_eq!(
        operand(40, expected, word(40, 18, b'e'), word(40, 18, b'E')),
        None
    );
}

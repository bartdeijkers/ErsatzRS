//! Assigned Table 29 fields, exact original words and reserved-data refusals.
use super::{operand, TeletextColumnOperation};
use ersatzrs_domain::{
    TeletextEnhancementCharsetDesignation, TeletextFlashMode, TeletextFlashRate,
};

fn word(column: u32, mode: u32, data: u8) -> u32 {
    column | (mode << 6) | (u32::from(data) << 11)
}
fn flash(data: u8) -> TeletextColumnOperation {
    let modes = [
        TeletextFlashMode::Steady,
        TeletextFlashMode::Normal,
        TeletextFlashMode::Inverted,
        TeletextFlashMode::AdjacentClut,
    ];
    let rates = [
        TeletextFlashRate::Slow1Hz,
        TeletextFlashRate::Fast2HzPhase1,
        TeletextFlashRate::Fast2HzPhase2,
        TeletextFlashRate::Fast2HzPhase3,
        TeletextFlashRate::Fast2HzIncremental,
        TeletextFlashRate::Fast2HzDecremental,
    ];
    TeletextColumnOperation::Flash {
        mode: modes[usize::from(data & 3)],
        rate: rates[usize::from(data >> 2)],
    }
}
fn charset(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::DesignateCharset {
        designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
    }
}
fn assigned_charset(code: u8) -> bool {
    matches!(
        code,
        0x00..=0x26 | 0x36 | 0x37 | 0x40 | 0x44 | 0x47 | 0x55 | 0x57
    )
}

#[test]
fn flash_operands_cover_all_modes_rates_and_refuse_reserved_data() {
    for original in 0..24 {
        for edited in 0..128 {
            let expected = (edited < 24).then(|| flash(edited));
            assert_eq!(
                operand(
                    39,
                    flash(original),
                    word(39, 7, original),
                    word(39, 7, edited)
                ),
                expected,
                "{original}->{edited}",
            );
        }
    }
}

#[test]
fn charset_operands_preserve_all_assigned_aliases_and_refuse_reserved_codes() {
    for original in (0..128).filter(|code| assigned_charset(*code)) {
        for edited in 0..128 {
            let expected = assigned_charset(edited).then(|| charset(edited));
            assert_eq!(
                operand(
                    0,
                    charset(original),
                    word(0, 8, original),
                    word(0, 8, edited)
                ),
                expected,
                "{original}->{edited}",
            );
        }
    }
}

#[test]
fn flash_charset_edits_cannot_change_address_mode_width_or_captured_original() {
    for (original, replacement, mode, from, to) in [
        (flash(0), flash(23), 7, 0, 23),
        (charset(0), charset(0x37), 8, 0, 0x37),
    ] {
        let before = word(4, mode, from);
        let after = word(4, mode, to);
        assert_eq!(operand(4, original, before, after), Some(replacement));
        for bad in [after ^ 1, after ^ (1 << 6), after | (1 << 18)] {
            assert_eq!(operand(4, original, before, bad), None);
        }
        assert_eq!(operand(4, original, before ^ (1 << 11), after), None);
        assert_eq!(operand(40, original, word(0x28, mode, from), after), None);
        assert_eq!(operand(4, replacement, before, after), None);
    }
}

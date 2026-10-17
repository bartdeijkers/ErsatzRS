//! Table 29's exact Flash and Modified G0/G2 data; no reserved-bit normalization.
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCharsetDesignation, TeletextFlashMode,
    TeletextFlashRate,
};

pub(super) fn operand(
    column: u8,
    expected: TeletextColumnOperation,
    before: u32,
    after: u32,
) -> Option<TeletextColumnOperation> {
    let (mode, data) = match expected {
        TeletextColumnOperation::Flash { mode, rate } => (7, flash_data(mode, rate)),
        TeletextColumnOperation::DesignateCharset { designation } => (8, designation.get()),
        _ => return None,
    };
    if column > 39
        || before != u32::from(column) | (mode << 6) | (u32::from(data) << 11)
        || after > 0x3ffff
        || after & 0x7ff != before & 0x7ff
    {
        return None;
    }
    let data = u8::try_from(after >> 11).ok()?;
    match expected {
        TeletextColumnOperation::Flash { .. } if data < 24 => {
            let mode = match data & 3 {
                0 => TeletextFlashMode::Steady,
                1 => TeletextFlashMode::Normal,
                2 => TeletextFlashMode::Inverted,
                _ => TeletextFlashMode::AdjacentClut,
            };
            let rate = match data >> 2 {
                0 => TeletextFlashRate::Slow1Hz,
                1 => TeletextFlashRate::Fast2HzPhase1,
                2 => TeletextFlashRate::Fast2HzPhase2,
                3 => TeletextFlashRate::Fast2HzPhase3,
                4 => TeletextFlashRate::Fast2HzIncremental,
                5 => TeletextFlashRate::Fast2HzDecremental,
                _ => return None,
            };
            Some(TeletextColumnOperation::Flash { mode, rate })
        }
        TeletextColumnOperation::DesignateCharset { .. } => {
            Some(TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(data).ok()?,
            })
        }
        _ => None,
    }
}

fn flash_data(mode: TeletextFlashMode, rate: TeletextFlashRate) -> u8 {
    let mode = match mode {
        TeletextFlashMode::Steady => 0,
        TeletextFlashMode::Normal => 1,
        TeletextFlashMode::Inverted => 2,
        TeletextFlashMode::AdjacentClut => 3,
    };
    let rate = match rate {
        TeletextFlashRate::Slow1Hz => 0,
        TeletextFlashRate::Fast2HzPhase1 => 1,
        TeletextFlashRate::Fast2HzPhase2 => 2,
        TeletextFlashRate::Fast2HzPhase3 => 3,
        TeletextFlashRate::Fast2HzIncremental => 4,
        TeletextFlashRate::Fast2HzDecremental => 5,
    };
    mode | (rate << 2)
}

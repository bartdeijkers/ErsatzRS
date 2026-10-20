//! Physical Table26 size/black controls retain raw codes and independent origins.

use super::base_footprints_tests::{attributes, empty_rows, finish, put, size_segment, source};
use super::*;

fn checked(
    base: &TeletextBaseDisplay,
    rows: Vec<[TeletextBaseDisplayCell; 40]>,
) -> Result<TeletextBaseDisplay, TeletextBaseDisplayValueError> {
    TeletextBaseDisplay::new(
        base.owner(),
        base.primary(),
        base.national_option(),
        base.presentation(),
        rows.into_boxed_slice().try_into().unwrap(),
    )
}

#[test]
fn physical_black_and_size_controls_have_exact_codes_timing_and_checked_roles() {
    for (control, code) in [
        (
            TeletextBaseSpacingControl::AlphaColour {
                colour: TeletextColour::Black,
            },
            0x00,
        ),
        (
            TeletextBaseSpacingControl::MosaicColour {
                colour: TeletextColour::Black,
            },
            0x10,
        ),
        (TeletextBaseSpacingControl::DoubleWidth, 0x0e),
        (TeletextBaseSpacingControl::DoubleSize, 0x0f),
    ] {
        assert_eq!(control.code(), code);
        assert_eq!(control.timing(), TeletextSpacingTiming::SetAfter);
        let position = TeletextEnhancementPosition::new(1, 0).unwrap();
        let kind = TeletextBaseDisplayCellKind::SpacingControl {
            control,
            timing: TeletextSpacingTiming::SetAfter,
        };
        assert!(
            TeletextBaseDisplayCell::new(position, code, kind, None, attributes(1, 7, 0)).is_ok()
        );
        assert_eq!(
            TeletextBaseDisplayCell::new(position, code ^ 1, kind, None, attributes(1, 7, 0)),
            Err(TeletextBaseDisplayValueError::CellRole)
        );
        assert_eq!(
            TeletextBaseDisplayCell::new(
                position,
                code,
                TeletextBaseDisplayCellKind::SpacingControl {
                    control,
                    timing: TeletextSpacingTiming::SetAt
                },
                None,
                attributes(1, 7, 0)
            ),
            Err(TeletextBaseDisplayValueError::CellRole)
        );
    }
}

#[test]
fn every_physical_size_control_resets_both_axes_with_original_source_and_boundary() {
    let sizes = [
        (TeletextBaseSpacingControl::NormalHeight, false, false),
        (TeletextBaseSpacingControl::DoubleHeight, true, false),
        (TeletextBaseSpacingControl::DoubleWidth, false, true),
        (TeletextBaseSpacingControl::DoubleSize, true, true),
    ];
    for (first, old_height, old_width) in sizes {
        for (next, height, width) in sizes {
            let mut rows = empty_rows(7, 0);
            size_segment(&mut rows, 1, 1, first);
            size_segment(&mut rows, 1, 8, next);
            let base = finish(rows, None);
            let start = if next == TeletextBaseSpacingControl::NormalHeight {
                8
            } else {
                9
            };
            let before = base.cell(1, start - 1).unwrap().attributes();
            assert_eq!(
                (before.double_height(), before.double_width()),
                (old_height, old_width)
            );
            for column in start..40 {
                let attributes = base.cell(1, column).unwrap().attributes();
                assert_eq!(
                    (attributes.double_height(), attributes.double_width()),
                    (height, width)
                );
                assert_eq!(attributes.sources().height, source(1, 8, next));
                assert_eq!(attributes.sources().width, source(1, 8, next));
            }
            assert_eq!(base.cell(1, 8).unwrap().code(), next.code());
        }
    }
}

#[test]
fn independent_size_fields_cannot_forge_values_or_coded_origins() {
    for control in [
        TeletextBaseSpacingControl::NormalHeight,
        TeletextBaseSpacingControl::DoubleHeight,
        TeletextBaseSpacingControl::DoubleWidth,
        TeletextBaseSpacingControl::DoubleSize,
    ] {
        let mut rows = empty_rows(7, 0);
        size_segment(&mut rows, 1, 2, control);
        let base = finish(rows, None);
        for width in [false, true] {
            let mut rows = base.cells().to_vec();
            let original = rows[1][9];
            let mut values = original.attributes().values();
            if width {
                values.double_width = !values.double_width;
            } else {
                values.double_height = !values.double_height;
            }
            let forged =
                TeletextBaseDisplayAttributes::new(values, original.attributes().sources())
                    .unwrap();
            put(&mut rows, 1, 9, b'A', None, forged);
            assert_eq!(
                checked(&base, rows),
                Err(TeletextBaseDisplayValueError::AttributeOrigin)
            );
        }
        let mut rows = base.cells().to_vec();
        let original = rows[1][9];
        let mut sources = original.attributes().sources();
        sources.width = source(1, 1, control); // No such control in the immutable coded row.
        put(
            &mut rows,
            1,
            9,
            b'A',
            None,
            TeletextBaseDisplayAttributes::new(original.attributes().values(), sources).unwrap(),
        );
        assert_eq!(
            checked(&base, rows),
            Err(TeletextBaseDisplayValueError::AttributeOrigin)
        );
    }
}

#[test]
fn physical_width_changes_reset_held_mosaic_but_reinforcement_and_alphabetics_do_not() {
    let mut rows = empty_rows(7, 0);
    let mosaic = TeletextBaseSpacingControl::MosaicColour {
        colour: TeletextColour::White,
    };
    put(&mut rows, 1, 0, 0x17, Some(mosaic), attributes(1, 7, 0));
    for column in 1..40 {
        let old = rows[1][usize::from(column)];
        let mut values = old.attributes().values();
        let mut sources = old.attributes().sources();
        values.graphics = true;
        sources.graphics = source(1, 0, mosaic);
        sources.foreground = source(1, 0, mosaic);
        put(
            &mut rows,
            1,
            column,
            b'A',
            None,
            TeletextBaseDisplayAttributes::new(values, sources).unwrap(),
        );
    }
    for (column, code, control) in [
        (1, 0x21, None),
        (2, 0x1e, Some(TeletextBaseSpacingControl::HoldMosaics)),
        (4, 0x09, Some(TeletextBaseSpacingControl::Steady)),
        (5, 0x26, None),
        (9, 0x09, Some(TeletextBaseSpacingControl::Steady)),
        (10, 0x29, None),
        (12, 0x09, Some(TeletextBaseSpacingControl::Steady)),
        (13, 0x2c, None),
    ] {
        let old = rows[1][usize::from(column)].attributes();
        put(&mut rows, 1, column, code, control, old);
    }
    for (column, control) in [
        (3, TeletextBaseSpacingControl::DoubleWidth),
        (6, TeletextBaseSpacingControl::DoubleWidth), // Same size: preserve glyph5.
        (8, TeletextBaseSpacingControl::DoubleSize),
        (11, TeletextBaseSpacingControl::DoubleHeight), // Width changes, height remains.
        (14, TeletextBaseSpacingControl::NormalHeight),
    ] {
        size_segment(&mut rows, 1, column, control);
    }
    let base = finish(rows, None);
    let mut state = TeletextBaseMosaicState::default();
    let held = base.cells()[1]
        .iter()
        .map(|&cell| state.advance(cell).unwrap())
        .collect::<Vec<_>>();
    for (column, pattern) in [(2, 1), (3, 1), (6, 6), (8, 6), (11, 9)] {
        assert_eq!(held[column].unwrap().mosaic().blocks(), pattern);
    }
    for (column, origin, control) in [
        (4, 3, TeletextBaseSpacingControl::DoubleWidth),
        (9, 8, TeletextBaseSpacingControl::DoubleSize),
        (12, 11, TeletextBaseSpacingControl::DoubleHeight),
        (14, 14, TeletextBaseSpacingControl::NormalHeight),
    ] {
        assert_eq!(held[column].unwrap().mosaic().blocks(), 0);
        assert_eq!(
            held[column].unwrap().source(),
            TeletextHeldMosaicSource::SpacingReset {
                source: source(1, origin, control),
            }
        );
    }
    // A literal G1 alphabetic at7 leaves the held register untouched.
    assert!(held[7].is_none());
    assert_eq!(
        held[8].unwrap().source(),
        TeletextHeldMosaicSource::Mosaic {
            position: TeletextEnhancementPosition::new(1, 5).unwrap(),
        }
    );
}

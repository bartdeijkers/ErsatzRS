//! Literal table 30/C.1 colour oracles, independent of glyph bitmap lookup.

use super::colour_composition::*;
use super::display_resolution_values::{
    Cell, Selection, Value, BG, BOXED, CONCEAL, FG, FIELDS, FLASH, INVERT,
};
use super::*;

fn colour(value: u8) -> TeletextColourReference {
    TeletextColourReference::new(value).unwrap()
}

fn display() -> TeletextDisplayResolution {
    let source = TeletextAttributeSource::Base {
        position: (1, 8),
        source: TeletextBaseAttributeSource::RowDefault {
            row: 1,
            start_column: 0,
        },
    };
    let writers = (0..FIELDS)
        .map(|field| {
            let value = match field {
                FG => Value::Colour(colour(1)),
                BG => Value::Colour(colour(4)),
                FLASH => Value::Flash(TeletextExecutedFlash {
                    mode: TeletextFlashMode::Steady,
                    rate: TeletextFlashRate::Slow1Hz,
                }),
                _ => Value::Flag(false),
            };
            let mut values = [None; FIELDS];
            values[field] = Some(value);
            TeletextDisplayWriter {
                source,
                requested: (1, 0),
                effective: (1, 0),
                placed: (1, 0),
                values,
                explicit: false,
            }
        })
        .collect();
    let mut cells = vec![None; 25 * 40];
    cells[48] = Some(Cell {
        origin: None,
        attributes: 0,
        fragment: TeletextDisplayFragment::Normal,
    });
    TeletextDisplayResolution {
        owner: super::attribute_execution_tests::owner(),
        width: 40,
        cells,
        states: vec![std::array::from_fn(|writer| Selection {
            writer,
            pending: None,
        })],
        origins: vec![],
        writers,
        pending: vec![],
    }
}

fn plan(display: &TeletextDisplayResolution, kind: TeletextPageKind) -> TeletextColourComposition {
    resolve_teletext_colour_composition(
        display,
        TeletextEffectivePresentation::Header {
            national_option: TeletextNationalOption::new(0).unwrap(),
        },
        &[colour(2); 25],
        colour(3),
        kind,
    )
}

fn options(phase: TeletextFlashPhase) -> TeletextColourPixelOptions {
    TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: phase,
    }
}

fn rgb(pixel: TeletextColourPixel) -> [u8; 3] {
    match pixel {
        TeletextColourPixel::Rgb(value) => value.get(),
        TeletextColourPixel::Video => panic!("expected literal RGB, received video"),
    }
}

fn set_value(display: &mut TeletextDisplayResolution, field: usize, value: Value) {
    display.writers[field].values[field] = Some(value);
}

#[test]
fn fixed_clut0_and_clut1_match_all_literal_rgb4_values() {
    let plan = plan(&display(), TeletextPageKind::FullPage);
    let literals = [
        Some([0, 0, 0]),
        Some([15, 0, 0]),
        Some([0, 15, 0]),
        Some([15, 15, 0]),
        Some([0, 0, 15]),
        Some([15, 0, 15]),
        Some([0, 15, 15]),
        Some([15, 15, 15]),
        None,
        Some([7, 0, 0]),
        Some([0, 7, 0]),
        Some([7, 7, 0]),
        Some([0, 0, 7]),
        Some([7, 0, 7]),
        Some([0, 7, 7]),
        Some([7, 7, 7]),
    ];
    for (reference, expected) in (0_u8..16).zip(literals) {
        match (plan.palette_colour(colour(reference)), expected) {
            (TeletextPaletteColour::Rgb(value), Some(expected)) => {
                assert_eq!(value.get(), expected);
            }
            (TeletextPaletteColour::Transparent, None) => {}
            _ => panic!("table 30 mismatch at {reference}"),
        }
    }
}

#[test]
fn absent_extension_uses_literal_table30_clut2_and_clut3() {
    let plan = plan(&display(), TeletextPageKind::FullPage);
    let expected = [
        [15, 0, 5],
        [15, 7, 0],
        [0, 15, 7],
        [15, 15, 11],
        [0, 12, 10],
        [5, 0, 0],
        [6, 5, 2],
        [12, 7, 7],
        [3, 3, 3],
        [15, 7, 7],
        [7, 15, 7],
        [15, 15, 7],
        [7, 7, 15],
        [15, 7, 15],
        [7, 15, 15],
        [13, 13, 13],
    ];
    for (reference, expected) in (16_u8..32).zip(expected) {
        let TeletextPaletteColour::Rgb(actual) = plan.palette_colour(colour(reference)) else {
            panic!("RGB required")
        };
        assert_eq!(actual.get(), expected);
    }
}

#[test]
fn selected_palette_changes_only_authored_clut2_and_clut3() {
    let defaults = TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(0).unwrap(),
        secondary: None,
        palette: TeletextLevel25Palette {
            clut2: [TeletextRgb::new([1, 2, 3]).unwrap(); 8],
            clut3: [TeletextRgb::new([4, 5, 6]).unwrap(); 8],
        },
        screen_colour: colour(31),
        row_colour: colour(16),
        black_background_substitution: true,
        remapping: TeletextColourRemapping::FgClut2BgClut3,
        panels: TeletextSidePanels {
            left: true,
            right: true,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::new(4).unwrap(),
        },
    };
    for source in [
        TeletextPresentationSource::Page,
        TeletextPresentationSource::Magazine,
    ] {
        let plan = resolve_teletext_colour_composition(
            &display(),
            TeletextEffectivePresentation::Extension {
                source,
                defaults: &defaults,
            },
            &[colour(16); 25],
            colour(31),
            TeletextPageKind::FullPage,
        );
        for reference in 16..24 {
            assert_eq!(
                rgb(plan.compose_plane_pixel(colour(reference), colour(0), false)),
                [1, 2, 3]
            );
        }
        for reference in 24..32 {
            assert_eq!(
                rgb(plan.compose_plane_pixel(colour(reference), colour(0), false)),
                [4, 5, 6]
            );
        }
        assert_eq!(
            rgb(plan.compose_plane_pixel(colour(1), colour(0), false)),
            [15, 0, 0]
        );
        assert_eq!(
            plan.palette_colour(colour(8)),
            TeletextPaletteColour::Transparent
        );
        assert_eq!(rgb(plan.screen_pixel()), [4, 5, 6]);
        assert_eq!(rgb(plan.row_pixel(24).unwrap()), [1, 2, 3]);
    }
}

#[test]
fn all_sixteen_c1_cases_match_the_normative_result_column() {
    // F/B, row, boxed, subtitle/newsflash, result: 0 video, 1 F/B, 2 row.
    let cases = [
        (1, 2, false, false, 1),
        (1, 2, false, true, 0),
        (1, 2, true, false, 1),
        (1, 2, true, true, 1),
        (1, 8, false, false, 1),
        (1, 8, false, true, 0),
        (1, 8, true, false, 1),
        (1, 8, true, true, 1),
        (8, 2, false, false, 2),
        (8, 2, false, true, 0),
        (8, 2, true, false, 0),
        (8, 2, true, true, 2),
        (8, 8, false, false, 0),
        (8, 8, false, true, 0),
        (8, 8, true, false, 0),
        (8, 8, true, true, 0),
    ];
    for subtitle_kind in [TeletextPageKind::Subtitle, TeletextPageKind::Newsflash] {
        for (fb, row, boxed, controlled, expected) in cases {
            let kind = if controlled {
                subtitle_kind
            } else {
                TeletextPageKind::FullPage
            };
            let pixel = plan(&display(), kind).compose_plane_pixel(colour(fb), colour(row), boxed);
            match expected {
                0 => assert_eq!(pixel, TeletextColourPixel::Video),
                1 => assert_eq!(rgb(pixel), [15, 0, 0]),
                2 => assert_eq!(rgb(pixel), [0, 15, 0]),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn conceal_reveal_invert_and_background_only_use_original_logical_colours() {
    let mut display = display();
    set_value(&mut display, CONCEAL, Value::Flag(true));
    let plan = plan(&display, TeletextPageKind::FullPage);
    let mut opts = options(TeletextFlashPhase::Original);
    assert_eq!(
        rgb(plan.pixel(&display, 1, 8, true, opts).unwrap()),
        [0, 0, 15]
    );
    opts.reveal_concealed = true;
    assert_eq!(
        rgb(plan.pixel(&display, 1, 8, true, opts).unwrap()),
        [15, 0, 0]
    );
    set_value(&mut display, INVERT, Value::Flag(true));
    assert_eq!(
        rgb(plan.pixel(&display, 1, 8, true, opts).unwrap()),
        [0, 0, 15]
    );
    assert_eq!(
        rgb(plan.pixel(&display, 1, 8, false, opts).unwrap()),
        [15, 0, 0]
    );
    assert_eq!(
        display
            .cell(1, 8)
            .unwrap()
            .attributes()
            .foreground()
            .value(),
        Some(colour(1))
    );
    display.cells[48].as_mut().unwrap().fragment = TeletextDisplayFragment::BackgroundOnly;
    assert_eq!(
        rgb(plan.pixel(&display, 1, 8, true, opts).unwrap()),
        [15, 0, 0]
    );
}

#[test]
fn every_flash_mode_and_rate_uses_the_explicit_phase_without_a_hidden_clock() {
    let mut display = display();
    let plan = plan(&display, TeletextPageKind::FullPage);
    let modes = [
        (TeletextFlashMode::Steady, [15, 0, 0], [15, 0, 0]),
        (TeletextFlashMode::Normal, [15, 0, 0], [0, 0, 15]),
        (TeletextFlashMode::Inverted, [0, 0, 15], [15, 0, 0]),
        (TeletextFlashMode::AdjacentClut, [15, 0, 0], [7, 0, 0]),
    ];
    for rate in [
        TeletextFlashRate::Slow1Hz,
        TeletextFlashRate::Fast2HzPhase1,
        TeletextFlashRate::Fast2HzPhase2,
        TeletextFlashRate::Fast2HzPhase3,
        TeletextFlashRate::Fast2HzIncremental,
        TeletextFlashRate::Fast2HzDecremental,
    ] {
        for (mode, original, alternate) in modes {
            set_value(
                &mut display,
                FLASH,
                Value::Flash(TeletextExecutedFlash { mode, rate }),
            );
            assert_eq!(
                rgb(plan
                    .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
                    .unwrap()),
                original
            );
            assert_eq!(
                rgb(plan
                    .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
                    .unwrap()),
                alternate
            );
            assert_eq!(
                rgb(plan
                    .pixel(
                        &display,
                        1,
                        8,
                        false,
                        options(TeletextFlashPhase::Alternate)
                    )
                    .unwrap()),
                [0, 0, 15]
            );
        }
    }
}

#[test]
fn adjacent_clut_flash_pairs_all_tables_including_transparency() {
    let mut display = display();
    set_value(
        &mut display,
        FLASH,
        Value::Flash(TeletextExecutedFlash {
            mode: TeletextFlashMode::AdjacentClut,
            rate: TeletextFlashRate::Slow1Hz,
        }),
    );
    let plan = plan(&display, TeletextPageKind::FullPage);
    for (input, expected) in [
        (0, [0, 15, 0]),
        (8, [0, 0, 0]),
        (16, [3, 3, 3]),
        (24, [15, 0, 5]),
    ] {
        set_value(&mut display, FG, Value::Colour(colour(input)));
        assert_eq!(
            rgb(plan
                .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
                .unwrap()),
            expected
        );
    }
}

#[test]
fn materialized_colour_indices_are_not_remapped_a_second_time() {
    let mut display = display();
    set_value(&mut display, FG, Value::Colour(colour(1)));
    set_value(&mut display, BG, Value::Colour(colour(17)));
    let plan = plan(&display, TeletextPageKind::FullPage);
    assert_eq!(
        rgb(plan
            .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
            .unwrap()),
        [15, 0, 0]
    );
    assert_eq!(
        rgb(plan
            .pixel(&display, 1, 8, false, options(TeletextFlashPhase::Original))
            .unwrap()),
        [15, 7, 0]
    );
}

#[test]
fn unresolved_attributes_keep_full_original_pending_provenance() {
    let mut display = display();
    let pending = TeletextDisplayPending {
        candidate: Some(48),
        position: (1, 8),
        location: Some(TeletextEnhancementSource {
            object: None,
            command: 7,
        }),
        root_source: Some(TeletextSpatialRootSource::DefaultSlot { slot: 3 }),
        invocation: None,
        reason: TeletextDisplayPendingReason::TransmissionOrder,
        first: Some(0),
        second: Some(1),
        previous: None,
        incoming_previous: None,
    };
    display.pending.push(pending);
    display.states[0][FG].pending = Some(0);
    let error = plan(&display, TeletextPageKind::FullPage)
        .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
        .unwrap_err();
    assert_eq!(error.position, (1, 8));
    assert_eq!(error.pending.as_deref(), Some(&pending));
    assert_eq!(error.location, pending.location());
    assert_eq!(
        error.reason,
        TeletextColourCompositionFailure::AttributePending {
            field: TeletextColourField::Foreground
        }
    );
}

#[test]
fn missing_cells_foreign_subcodes_and_joint_flash_invert_are_explicit_errors() {
    let mut display = display();
    let plan = plan(&display, TeletextPageKind::FullPage);
    assert_eq!(
        plan.pixel(&display, 0, 0, true, options(TeletextFlashPhase::Original))
            .unwrap_err()
            .reason,
        TeletextColourCompositionFailure::MissingCell
    );
    set_value(&mut display, INVERT, Value::Flag(true));
    set_value(
        &mut display,
        FLASH,
        Value::Flash(TeletextExecutedFlash {
            mode: TeletextFlashMode::Normal,
            rate: TeletextFlashRate::Slow1Hz,
        }),
    );
    assert_eq!(
        plan.pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
            .unwrap_err()
            .reason,
        TeletextColourCompositionFailure::UnverifiedInvertFlashOrder
    );
    display.owner.subcode = TeletextSubcode::new(0).unwrap();
    assert_eq!(
        plan.pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
            .unwrap_err()
            .reason,
        TeletextColourCompositionFailure::OwnerMismatch
    );
}

#[test]
fn subtitle_foreground_and_transparent_background_follow_the_same_c1_rule() {
    let mut display = display();
    set_value(&mut display, BG, Value::Colour(colour(8)));
    for kind in [TeletextPageKind::Subtitle, TeletextPageKind::Newsflash] {
        let plan = plan(&display, kind);
        assert_eq!(
            plan.pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
                .unwrap(),
            TeletextColourPixel::Video
        );
        set_value(&mut display, BOXED, Value::Flag(true));
        assert_eq!(
            rgb(plan
                .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
                .unwrap()),
            [15, 0, 0]
        );
        assert_eq!(
            rgb(plan
                .pixel(&display, 1, 8, false, options(TeletextFlashPhase::Original))
                .unwrap()),
            [0, 15, 0]
        );
        set_value(&mut display, BOXED, Value::Flag(false));
    }
}

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
fn missing_cells_and_foreign_subcodes_are_explicit_errors() {
    let mut display = display();
    let plan = plan(&display, TeletextPageKind::FullPage);
    assert_eq!(
        plan.pixel(&display, 0, 0, true, options(TeletextFlashPhase::Original))
            .unwrap_err()
            .reason,
        TeletextColourCompositionFailure::MissingCell
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

#[test]
fn joint_invert_flash_uses_the_exchanged_foreground_in_selected_cluts() {
    let mut display = display();
    set_value(&mut display, FG, Value::Colour(colour(17)));
    set_value(&mut display, BG, Value::Colour(colour(24)));
    set_value(&mut display, INVERT, Value::Flag(true));
    let mut palette = TeletextLevel25Palette {
        clut2: [TeletextRgb::new([1, 2, 3]).unwrap(); 8],
        clut3: [TeletextRgb::new([4, 5, 6]).unwrap(); 8],
    };
    palette.clut2[0] = TeletextRgb::new([13, 2, 7]).unwrap();
    let defaults = TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(0).unwrap(),
        secondary: None,
        palette,
        screen_colour: colour(0),
        row_colour: colour(2),
        black_background_substitution: false,
        remapping: TeletextColourRemapping::FgClut2BgClut3,
        panels: TeletextSidePanels {
            left: false,
            right: false,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::ZERO,
        },
    };
    let plan = resolve_teletext_colour_composition(
        &display,
        TeletextEffectivePresentation::Extension {
            source: TeletextPresentationSource::Page,
            defaults: &defaults,
        },
        &[colour(2); 25],
        colour(0),
        TeletextPageKind::FullPage,
    );
    let cases = [
        (TeletextFlashMode::Steady, [4, 5, 6], [4, 5, 6]),
        (TeletextFlashMode::Normal, [4, 5, 6], [1, 2, 3]),
        (TeletextFlashMode::Inverted, [1, 2, 3], [4, 5, 6]),
        (TeletextFlashMode::AdjacentClut, [4, 5, 6], [13, 2, 7]),
    ];
    for (mode, original, alternate) in cases {
        set_value(
            &mut display,
            FLASH,
            Value::Flash(TeletextExecutedFlash {
                mode,
                rate: TeletextFlashRate::Slow1Hz,
            }),
        );
        for (phase, expected) in [
            (TeletextFlashPhase::Original, original),
            (TeletextFlashPhase::Alternate, alternate),
        ] {
            assert_eq!(
                rgb(plan.pixel(&display, 1, 8, true, options(phase)).unwrap()),
                expected
            );
            assert_eq!(
                rgb(plan.pixel(&display, 1, 8, false, options(phase)).unwrap()),
                [1, 2, 3]
            );
        }
    }
    let attrs = display.cell(1, 8).unwrap().attributes();
    assert_eq!(attrs.foreground().value(), Some(colour(17)));
    assert_eq!(attrs.background().value(), Some(colour(24)));
}

#[test]
fn joint_invert_adjacent_flash_applies_transparency_after_pair_selection() {
    let mut display = display();
    set_value(&mut display, INVERT, Value::Flag(true));
    set_value(
        &mut display,
        FLASH,
        Value::Flash(TeletextExecutedFlash {
            mode: TeletextFlashMode::AdjacentClut,
            rate: TeletextFlashRate::Slow1Hz,
        }),
    );
    let full = plan(&display, TeletextPageKind::FullPage);
    // Original BG becomes displayed FG. Its paired entry, not logical FG's,
    // decides transparency. Imager 2b0125's raw-FG pairing cannot be this oracle.
    for (background, original, alternate) in
        [(0, [0, 0, 0], [0, 15, 0]), (8, [0, 15, 0], [0, 0, 0])]
    {
        set_value(&mut display, BG, Value::Colour(colour(background)));
        assert_eq!(
            rgb(full
                .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
                .unwrap()),
            original
        );
        assert_eq!(
            rgb(full
                .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
                .unwrap()),
            alternate
        );
    }
    set_value(&mut display, BG, Value::Colour(colour(0)));
    set_value(&mut display, BOXED, Value::Flag(true));
    assert_eq!(
        full.pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
            .unwrap(),
        TeletextColourPixel::Video
    );
    let subtitle = plan(&display, TeletextPageKind::Subtitle);
    assert_eq!(
        rgb(subtitle
            .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
            .unwrap()),
        [0, 15, 0]
    );
}

#[test]
fn concealed_joint_flash_has_only_exchanged_background_until_revealed() {
    let mut display = display();
    set_value(&mut display, BG, Value::Colour(colour(2)));
    set_value(&mut display, INVERT, Value::Flag(true));
    set_value(&mut display, CONCEAL, Value::Flag(true));
    let plan = plan(&display, TeletextPageKind::FullPage);
    let cases = [
        (TeletextFlashMode::Normal, [0, 15, 0], [15, 0, 0]),
        (TeletextFlashMode::Inverted, [15, 0, 0], [0, 15, 0]),
        (TeletextFlashMode::AdjacentClut, [0, 15, 0], [0, 7, 0]),
    ];
    for (mode, original, alternate) in cases {
        set_value(
            &mut display,
            FLASH,
            Value::Flash(TeletextExecutedFlash {
                mode,
                rate: TeletextFlashRate::Fast2HzPhase2,
            }),
        );
        for (phase, expected) in [
            (TeletextFlashPhase::Original, original),
            (TeletextFlashPhase::Alternate, alternate),
        ] {
            let mut opts = options(phase);
            assert_eq!(
                rgb(plan.pixel(&display, 1, 8, true, opts).unwrap()),
                [15, 0, 0]
            );
            opts.reveal_concealed = true;
            assert_eq!(
                rgb(plan.pixel(&display, 1, 8, true, opts).unwrap()),
                expected
            );
        }
    }
}

fn height_origin(candidate: usize, row: u8, column: u8) -> TeletextDisplayOrigin {
    TeletextDisplayOrigin {
        candidate,
        visual: Some((row, column)),
        requested: Some(TeletextDisplaySize::DoubleHeight),
        effective: Some(TeletextDisplaySize::DoubleHeight),
        attributes: 0,
        pending: None,
        obscured_by: None,
    }
}

#[test]
fn enlarged_bottom_parts_retain_top_row_plane_for_both_transparent_colours() {
    let mut display = display();
    set_value(&mut display, FG, Value::Colour(colour(8)));
    set_value(&mut display, BG, Value::Colour(colour(8)));
    display.origins.push(height_origin(48, 1, 8));
    let mut rows = [colour(0); 25];
    rows[1] = colour(1);
    rows[2] = colour(4);
    let plan = resolve_teletext_colour_composition(
        &display,
        TeletextEffectivePresentation::Header {
            national_option: TeletextNationalOption::new(0).unwrap(),
        },
        &rows,
        colour(0),
        TeletextPageKind::FullPage,
    );
    for (top, bottom, column) in [
        (
            TeletextDisplayFragment::DoubleHeightTop,
            TeletextDisplayFragment::DoubleHeightBottom,
            8_u8,
        ),
        (
            TeletextDisplayFragment::DoubleSizeTopLeft,
            TeletextDisplayFragment::DoubleSizeBottomLeft,
            8,
        ),
        (
            TeletextDisplayFragment::DoubleSizeTopRight,
            TeletextDisplayFragment::DoubleSizeBottomRight,
            9,
        ),
    ] {
        display.cells[40 + usize::from(column)] = Some(Cell {
            origin: Some(0),
            attributes: 0,
            fragment: top,
        });
        display.cells[80 + usize::from(column)] = Some(Cell {
            origin: Some(0),
            attributes: 0,
            fragment: bottom,
        });
        for foreground in [true, false] {
            let options = options(TeletextFlashPhase::Original);
            assert_eq!(
                rgb(plan
                    .pixel(&display, 1, column, foreground, options)
                    .unwrap()),
                [15, 0, 0]
            );
            assert_eq!(
                rgb(plan
                    .pixel(&display, 2, column, foreground, options)
                    .unwrap()),
                [15, 0, 0]
            );
        }
    }
    assert_eq!(rgb(plan.row_pixel(2).unwrap()), [0, 0, 15]);
    display.cells[88].as_mut().unwrap().origin = None;
    assert_eq!(
        plan.pixel(&display, 2, 8, true, options(TeletextFlashPhase::Original))
            .unwrap_err()
            .reason,
        TeletextColourCompositionFailure::MissingOrigin
    );
}

#[test]
fn appended_repaint_origin_selects_its_row_while_background_only_keeps_current_row() {
    let mut display = display();
    set_value(&mut display, FG, Value::Colour(colour(8)));
    set_value(&mut display, BG, Value::Colour(colour(8)));
    display.origins.push(height_origin(48, 1, 8));
    // Same original candidate, distinct re-realized origin/attribute context.
    display.origins.push(height_origin(48, 3, 8));
    display.cells[128] = Some(Cell {
        origin: Some(1),
        attributes: 0,
        fragment: TeletextDisplayFragment::DoubleHeightTop,
    });
    display.cells[168] = Some(Cell {
        origin: Some(1),
        attributes: 0,
        fragment: TeletextDisplayFragment::DoubleHeightBottom,
    });
    let mut rows = [colour(0); 25];
    rows[1] = colour(1);
    rows[3] = colour(2);
    rows[4] = colour(4);
    let plan = resolve_teletext_colour_composition(
        &display,
        TeletextEffectivePresentation::Header {
            national_option: TeletextNationalOption::new(0).unwrap(),
        },
        &rows,
        colour(0),
        TeletextPageKind::FullPage,
    );
    for row in [3, 4] {
        assert_eq!(
            rgb(plan
                .pixel(
                    &display,
                    row,
                    8,
                    true,
                    options(TeletextFlashPhase::Original)
                )
                .unwrap()),
            [0, 15, 0]
        );
    }
    display.cells[168].as_mut().unwrap().fragment = TeletextDisplayFragment::BackgroundOnly;
    assert_eq!(
        rgb(plan
            .pixel(&display, 4, 8, true, options(TeletextFlashPhase::Original))
            .unwrap()),
        [0, 0, 15]
    );
}

fn bbs_plan(
    display: &TeletextDisplayResolution,
    enabled: bool,
    kind: TeletextPageKind,
) -> TeletextColourComposition {
    let defaults = TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(0).unwrap(),
        secondary: None,
        palette: TeletextLevel25Palette::standard(),
        screen_colour: colour(3),
        row_colour: colour(8),
        black_background_substitution: enabled,
        remapping: TeletextColourRemapping::FgClut0BgClut0,
        panels: TeletextSidePanels {
            left: false,
            right: false,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::ZERO,
        },
    };
    resolve_teletext_colour_composition(
        display,
        TeletextEffectivePresentation::Extension {
            source: TeletextPresentationSource::Page,
            defaults: &defaults,
        },
        &[colour(8); 25],
        colour(3),
        kind,
    )
}

fn parameter_source(command: usize) -> TeletextAttributeSource {
    TeletextAttributeSource::Enhancement {
        event_ordinal: command,
        location: TeletextEnhancementSource {
            object: None,
            command,
        },
        root_source: TeletextSpatialRootSource::Local { command },
        invocation: None,
        effective_start: (1, 8),
        placed_position: (1, 7),
    }
}

fn background_spacing_source(control: TeletextBaseSpacingControl) -> TeletextAttributeSource {
    TeletextAttributeSource::Base {
        position: (1, 8),
        source: TeletextBaseAttributeSource::Spacing {
            position: TeletextEnhancementPosition::new(1, 7).unwrap(),
            control,
            timing: TeletextSpacingTiming::SetAt,
        },
    }
}

#[test]
fn window_rejects_only_consumed_substituted_background_transparency() {
    let mut display = display();
    set_value(&mut display, BG, Value::Colour(colour(8)));
    set_value(&mut display, BOXED, Value::Flag(true));
    display.writers[BOXED].source = parameter_source(12);
    for source in [
        display.writers[BG].source,
        background_spacing_source(TeletextBaseSpacingControl::BlackBackground),
    ] {
        display.writers[BG].source = source;
        // A transmitted 1C remains substituted; C2 explicitness is not proof.
        display.writers[BG].explicit = true;
        for (invert, coverage) in [(false, false), (true, true)] {
            set_value(&mut display, INVERT, Value::Flag(invert));
            let error = bbs_plan(&display, true, TeletextPageKind::FullPage)
                .pixel(
                    &display,
                    1,
                    8,
                    coverage,
                    options(TeletextFlashPhase::Original),
                )
                .unwrap_err();
            assert_eq!(
                error.reason,
                TeletextColourCompositionFailure::BackgroundParameterFromSubstitution {
                    usage: TeletextBackgroundParameterUse::WindowTransparency,
                }
            );
            let evidence = error.parameter.as_deref().unwrap();
            assert_eq!(evidence.background_source, source);
            assert_eq!(evidence.consumer_source, Some(parameter_source(12)));
            assert_eq!(
                error.location,
                Some(TeletextEnhancementSource {
                    object: None,
                    command: 12
                })
            );
            assert_eq!(error.position, (1, 8));
            assert!(error.pending.is_none());
        }
        for (invert, coverage) in [(false, true), (true, false)] {
            set_value(&mut display, INVERT, Value::Flag(invert));
            assert_eq!(
                rgb(bbs_plan(&display, true, TeletextPageKind::FullPage)
                    .pixel(
                        &display,
                        1,
                        8,
                        coverage,
                        options(TeletextFlashPhase::Original)
                    )
                    .unwrap()),
                [15, 0, 0]
            );
        }
    }
}

#[test]
fn window_background_selection_accounts_for_conceal_and_flash_phase() {
    let mut display = display();
    set_value(&mut display, BG, Value::Colour(colour(8)));
    set_value(&mut display, BOXED, Value::Flag(true));
    set_value(&mut display, CONCEAL, Value::Flag(true));
    let plan = bbs_plan(&display, true, TeletextPageKind::FullPage);
    assert!(matches!(
        plan.pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
            .unwrap_err()
            .reason,
        TeletextColourCompositionFailure::BackgroundParameterFromSubstitution {
            usage: TeletextBackgroundParameterUse::WindowTransparency,
        }
    ));
    assert_eq!(
        rgb(plan
            .pixel(
                &display,
                1,
                8,
                true,
                TeletextColourPixelOptions {
                    reveal_concealed: true,
                    flash_phase: TeletextFlashPhase::Original,
                }
            )
            .unwrap()),
        [15, 0, 0]
    );
    set_value(&mut display, CONCEAL, Value::Flag(false));
    set_value(
        &mut display,
        FLASH,
        Value::Flash(TeletextExecutedFlash {
            mode: TeletextFlashMode::Normal,
            rate: TeletextFlashRate::Slow1Hz,
        }),
    );
    assert!(plan
        .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
        .is_err());
    assert_eq!(
        rgb(plan
            .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
            .unwrap()),
        [15, 0, 0]
    );
}

#[test]
fn inverted_adjacent_clut_checks_original_background_source_in_both_phases() {
    let mut display = display();
    set_value(&mut display, BG, Value::Colour(colour(17)));
    set_value(&mut display, INVERT, Value::Flag(true));
    set_value(
        &mut display,
        FLASH,
        Value::Flash(TeletextExecutedFlash {
            mode: TeletextFlashMode::AdjacentClut,
            rate: TeletextFlashRate::Slow1Hz,
        }),
    );
    display.writers[FLASH].source = parameter_source(9);
    display.writers[INVERT].source = parameter_source(10);
    let background = display.writers[BG].source;
    for phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
        let error = bbs_plan(&display, true, TeletextPageKind::FullPage)
            .pixel(&display, 1, 8, true, options(phase))
            .unwrap_err();
        assert_eq!(
            error.reason,
            TeletextColourCompositionFailure::BackgroundParameterFromSubstitution {
                usage: TeletextBackgroundParameterUse::InvertedAdjacentClut,
            }
        );
        assert_eq!(
            error.parameter.as_deref(),
            Some(&TeletextBackgroundParameterEvidence {
                background_source: background,
                consumer_source: Some(parameter_source(9)),
                invert_source: Some(parameter_source(10)),
            })
        );
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: None,
                command: 9
            })
        );
    }
    display.writers[BG].source = parameter_source(7);
    assert_eq!(
        rgb(bbs_plan(&display, true, TeletextPageKind::FullPage)
            .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
            .unwrap()),
        [15, 7, 7]
    );
    assert_eq!(
        display
            .cell(1, 8)
            .unwrap()
            .attributes()
            .background()
            .value(),
        Some(colour(17))
    );
}

#[test]
fn explicit_background_sources_and_noninverted_pairing_preserve_literal_colours() {
    let mut display = display();
    set_value(&mut display, BG, Value::Colour(colour(17)));
    set_value(&mut display, INVERT, Value::Flag(true));
    set_value(
        &mut display,
        FLASH,
        Value::Flash(TeletextExecutedFlash {
            mode: TeletextFlashMode::AdjacentClut,
            rate: TeletextFlashRate::Slow1Hz,
        }),
    );
    for source in [
        parameter_source(7),
        background_spacing_source(TeletextBaseSpacingControl::NewBackground),
        TeletextAttributeSource::PanelDefault { row: 1 },
    ] {
        display.writers[BG].source = source;
        assert_eq!(
            rgb(bbs_plan(&display, true, TeletextPageKind::FullPage)
                .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Original))
                .unwrap()),
            [15, 7, 0]
        );
        assert_eq!(
            rgb(bbs_plan(&display, true, TeletextPageKind::FullPage)
                .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
                .unwrap()),
            [15, 7, 7]
        );
    }
    display.writers[BG].source =
        background_spacing_source(TeletextBaseSpacingControl::BlackBackground);
    set_value(&mut display, INVERT, Value::Flag(false));
    assert_eq!(
        rgb(bbs_plan(&display, true, TeletextPageKind::FullPage)
            .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
            .unwrap()),
        [7, 0, 0]
    );
    set_value(&mut display, INVERT, Value::Flag(true));
    assert_eq!(
        rgb(bbs_plan(&display, false, TeletextPageKind::FullPage)
            .pixel(&display, 1, 8, true, options(TeletextFlashPhase::Alternate))
            .unwrap()),
        [15, 7, 7]
    );
}

#[test]
fn window_checks_winning_writer_without_rejecting_other_cells_or_boxing() {
    let mut display = display();
    set_value(&mut display, BG, Value::Colour(colour(8)));
    set_value(&mut display, BOXED, Value::Flag(true));
    display.writers[BG].source = parameter_source(7);
    assert_eq!(
        bbs_plan(&display, true, TeletextPageKind::FullPage)
            .pixel(&display, 1, 8, false, options(TeletextFlashPhase::Original))
            .unwrap(),
        TeletextColourPixel::Video
    );
    display.writers[BG].source =
        background_spacing_source(TeletextBaseSpacingControl::BlackBackground);
    for kind in [TeletextPageKind::Subtitle, TeletextPageKind::Newsflash] {
        assert_eq!(
            bbs_plan(&display, true, kind)
                .pixel(&display, 1, 8, false, options(TeletextFlashPhase::Original))
                .unwrap(),
            TeletextColourPixel::Video
        );
    }
    assert_eq!(
        bbs_plan(&display, false, TeletextPageKind::FullPage)
            .pixel(&display, 1, 8, false, options(TeletextFlashPhase::Original))
            .unwrap(),
        TeletextColourPixel::Video
    );
    // Same colour/source but no Window on the neighbouring cell.
    let mut unboxed = display.writers[BOXED];
    unboxed.values[BOXED] = Some(Value::Flag(false));
    let writer = display.writers.len();
    display.writers.push(unboxed);
    let mut state = display.states[0];
    state[BOXED].writer = writer;
    display.states.push(state);
    display.cells[49] = Some(Cell {
        origin: None,
        attributes: 1,
        fragment: TeletextDisplayFragment::Normal,
    });
    assert!(bbs_plan(&display, true, TeletextPageKind::FullPage)
        .pixel(&display, 1, 8, false, options(TeletextFlashPhase::Original))
        .is_err());
    assert_eq!(
        bbs_plan(&display, true, TeletextPageKind::FullPage)
            .pixel(&display, 1, 9, false, options(TeletextFlashPhase::Original))
            .unwrap(),
        TeletextColourPixel::Video
    );
}

#[test]
fn unresolved_background_precedes_definite_parameter_check() {
    let mut display = display();
    set_value(&mut display, BG, Value::Colour(colour(8)));
    set_value(&mut display, BOXED, Value::Flag(true));
    let pending = TeletextDisplayPending {
        candidate: Some(48),
        position: (1, 8),
        location: Some(TeletextEnhancementSource {
            object: None,
            command: 7,
        }),
        root_source: Some(TeletextSpatialRootSource::Local { command: 7 }),
        invocation: None,
        reason: TeletextDisplayPendingReason::TransmissionOrder,
        first: Some(0),
        second: Some(1),
        previous: None,
        incoming_previous: None,
    };
    display.pending.push(pending);
    display.states[0][BG].pending = Some(0);
    let error = bbs_plan(&display, true, TeletextPageKind::FullPage)
        .pixel(&display, 1, 8, false, options(TeletextFlashPhase::Original))
        .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextColourCompositionFailure::AttributePending {
            field: TeletextColourField::Background,
        }
    );
    assert_eq!(error.pending.as_deref(), Some(&pending));
    assert!(error.parameter.is_none());
}

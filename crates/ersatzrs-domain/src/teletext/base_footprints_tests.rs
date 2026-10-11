//! Literal spacing-DH occupancy oracles; enhanced C.3 realization remains separate.

use super::*;

type Rows = Vec<[TeletextBaseDisplayCell; 40]>;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

fn position(row: u8, column: u8) -> TeletextEnhancementPosition {
    TeletextEnhancementPosition::new(row, column).unwrap()
}

fn default_source(row: u8) -> TeletextBaseAttributeSource {
    TeletextBaseAttributeSource::RowDefault {
        row,
        start_column: if row == 0 { 8 } else { 0 },
    }
}

fn source(row: u8, column: u8, control: TeletextBaseSpacingControl) -> TeletextBaseAttributeSource {
    TeletextBaseAttributeSource::Spacing {
        position: position(row, column),
        control,
        timing: control.timing(),
    }
}

fn attributes(row: u8, foreground: u8, background: u8) -> TeletextBaseDisplayAttributes {
    let source = default_source(row);
    TeletextBaseDisplayAttributes::new(
        TeletextBaseAttributeValues {
            raw_foreground: 7,
            raw_background: 0,
            foreground: TeletextColourReference::new(foreground).unwrap(),
            background: TeletextColourReference::new(background).unwrap(),
            graphics: false,
            separated: false,
            double_height: false,
            boxed: false,
            flash: false,
            conceal: false,
        },
        TeletextBaseAttributeSources {
            foreground: source,
            background: source,
            graphics: source,
            separated: source,
            height: source,
            boxed: source,
            flash: source,
            conceal: source,
        },
    )
    .unwrap()
}

fn empty_rows(foreground: u8, background: u8) -> Rows {
    (0_u8..25)
        .map(|row| {
            (0_u8..40)
                .map(|column| {
                    TeletextBaseDisplayCell::new(
                        position(row, column),
                        b' ',
                        if row == 0 && column < 8 {
                            TeletextBaseDisplayCellKind::HeaderOwned
                        } else {
                            TeletextBaseDisplayCellKind::Blank
                        },
                        None,
                        attributes(row, foreground, background),
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap()
        })
        .collect()
}

fn put(
    rows: &mut Rows,
    row: u8,
    column: u8,
    code: u8,
    control: Option<TeletextBaseSpacingControl>,
    attributes: TeletextBaseDisplayAttributes,
) {
    let kind = control.map_or(
        if code == b' ' {
            TeletextBaseDisplayCellKind::Blank
        } else {
            TeletextBaseDisplayCellKind::Glyph
        },
        |control| TeletextBaseDisplayCellKind::SpacingControl {
            control,
            timing: control.timing(),
        },
    );
    rows[usize::from(row)][usize::from(column)] =
        TeletextBaseDisplayCell::new(position(row, column), code, kind, None, attributes).unwrap();
}

fn finish(
    rows: Rows,
    presentation: Option<TeletextLevel25PresentationDefaults>,
) -> TeletextBaseDisplay {
    TeletextBaseDisplay::new(
        owner(),
        TeletextPageCharsetDesignation::new(0).unwrap(),
        TeletextNationalOption::new(0).unwrap(),
        presentation,
        rows.into_boxed_slice().try_into().unwrap(),
    )
    .unwrap()
}

fn height_segment(rows: &mut Rows, row: u8, control_column: u8, normal_column: Option<u8>) {
    let dh = TeletextBaseSpacingControl::DoubleHeight;
    let before = rows[usize::from(row)][usize::from(control_column)].attributes();
    put(rows, row, control_column, 0x0d, Some(dh), before);
    for column in control_column.saturating_add(1)..40 {
        let original = rows[usize::from(row)][usize::from(column)];
        let mut values = original.attributes().values();
        let mut sources = original.attributes().sources();
        values.double_height = normal_column.is_none_or(|normal| column < normal);
        sources.height = if let Some(normal) = normal_column.filter(|&normal| column >= normal) {
            source(row, normal, TeletextBaseSpacingControl::NormalHeight)
        } else {
            source(row, control_column, dh)
        };
        let attributes = TeletextBaseDisplayAttributes::new(values, sources).unwrap();
        let control = match original.kind() {
            TeletextBaseDisplayCellKind::SpacingControl { control, .. } => Some(control),
            _ => None,
        };
        if normal_column == Some(column) {
            put(
                rows,
                row,
                column,
                0x0c,
                Some(TeletextBaseSpacingControl::NormalHeight),
                attributes,
            );
        } else {
            put(rows, row, column, original.code(), control, attributes);
        }
    }
}

fn lower_attributes(column: u8, flashing: bool) -> TeletextBaseDisplayAttributes {
    let original = attributes(2, 7, 0);
    let mut values = original.values();
    let mut sources = original.sources();
    if column > 0 {
        values.raw_foreground = 2;
        values.foreground = TeletextColourReference::new(2).unwrap();
        sources.foreground = source(
            2,
            0,
            TeletextBaseSpacingControl::AlphaColour {
                colour: TeletextColour::Green,
            },
        );
        sources.graphics = sources.foreground;
        sources.conceal = sources.foreground;
    }
    if flashing && column >= 2 {
        values.flash = true;
        sources.flash = source(2, 1, TeletextBaseSpacingControl::Flash);
    }
    TeletextBaseDisplayAttributes::new(values, sources).unwrap()
}

fn mixed_base() -> TeletextBaseDisplay {
    let mut rows = empty_rows(7, 0);
    let blue = TeletextBaseSpacingControl::AlphaColour {
        colour: TeletextColour::Blue,
    };
    let red = TeletextBaseSpacingControl::AlphaColour {
        colour: TeletextColour::Red,
    };
    for column in 0_u8..40 {
        let mut values = attributes(1, 7, 0).values();
        let mut sources = attributes(1, 7, 0).sources();
        if column >= 1 {
            values.raw_foreground = 4;
            values.foreground = TeletextColourReference::new(4).unwrap();
            sources.foreground = source(1, 0, blue);
            sources.graphics = sources.foreground;
            sources.conceal = sources.foreground;
        }
        if column >= 2 {
            values.raw_background = 4;
            values.background = TeletextColourReference::new(4).unwrap();
            sources.background = source(1, 2, TeletextBaseSpacingControl::NewBackground);
        }
        if column >= 4 {
            values.raw_foreground = 1;
            values.foreground = TeletextColourReference::new(1).unwrap();
            sources.foreground = source(1, 3, red);
            sources.graphics = sources.foreground;
            sources.conceal = sources.foreground;
        }
        if column >= 7 {
            values.flash = true;
            sources.flash = source(1, 6, TeletextBaseSpacingControl::Flash);
        }
        let (code, control) = match column {
            0 => (0x04, Some(blue)),
            2 => (0x1d, Some(TeletextBaseSpacingControl::NewBackground)),
            3 => (0x01, Some(red)),
            6 => (0x08, Some(TeletextBaseSpacingControl::Flash)),
            8 => (b'A', None),
            11 => (b'B', None),
            _ => (b' ', None),
        };
        put(
            &mut rows,
            1,
            column,
            code,
            control,
            TeletextBaseDisplayAttributes::new(values, sources).unwrap(),
        );
    }
    height_segment(&mut rows, 1, 7, Some(10));
    // Real checked raw lower-row data is retained; authored-page validation is
    // intentionally not weakened merely to create this suppression oracle.
    let green = TeletextBaseSpacingControl::AlphaColour {
        colour: TeletextColour::Green,
    };
    for column in 0_u8..40 {
        let (code, control) = match column {
            0 => (0x02, Some(green)),
            1 => (0x08, Some(TeletextBaseSpacingControl::Flash)),
            3 => (b'X', None),
            39 => (b'Z', None),
            _ => (b' ', None),
        };
        put(
            &mut rows,
            2,
            column,
            code,
            control,
            lower_attributes(column, true),
        );
    }
    height_segment(&mut rows, 2, 2, Some(4));
    put(&mut rows, 3, 3, b'C', None, attributes(3, 7, 0));
    finish(rows, None)
}

fn resolve(base: &TeletextBaseDisplay) -> TeletextBaseFootprints {
    resolve_teletext_base_footprints(base, &[TeletextColourReference::BLACK; 25]).unwrap()
}

#[test]
fn double_height_set_after_and_normal_height_set_at_keep_exact_fragment_origins() {
    let base = mixed_base();
    let footprints = resolve(&base);
    assert_eq!(footprints.owner(), owner());
    assert_eq!(footprints.cells().len(), 1000);
    assert_eq!(footprints.rows().len(), 25);
    assert_eq!(
        footprints.cell(1, 7).unwrap().kind(),
        TeletextBaseFragmentKind::Normal
    );
    for column in [8, 9] {
        let top = footprints.cell(1, column).unwrap();
        let bottom = footprints.cell(2, column).unwrap();
        assert_eq!(top.kind(), TeletextBaseFragmentKind::DoubleHeightTop);
        assert_eq!(bottom.kind(), TeletextBaseFragmentKind::DoubleHeightBottom);
        assert_eq!(top.origin(), Some(position(1, column)));
        assert_eq!(bottom.origin(), Some(position(1, column)));
        assert_eq!(bottom.coded_position(), position(2, column));
    }
    assert_eq!(
        footprints.cell(1, 10).unwrap().kind(),
        TeletextBaseFragmentKind::Normal
    );
    assert_eq!(
        footprints.cell(2, 10).unwrap().kind(),
        TeletextBaseFragmentKind::BackgroundOnly
    );
    assert_eq!(
        footprints.cell(2, 10).unwrap().origin(),
        Some(position(1, 10))
    );
    let origin = base.cell(1, 8).unwrap().attributes();
    assert_eq!(
        (origin.foreground().get(), origin.background().get()),
        (1, 4)
    );
    assert!(origin.flash());
    assert_eq!(
        origin.sources().height,
        source(1, 7, TeletextBaseSpacingControl::DoubleHeight)
    );
}

#[test]
fn whole_lower_row_has_only_origin_fragments_or_upper_background_and_retains_coded_data() {
    let base = mixed_base();
    let original = base.clone();
    let footprints = resolve(&base);
    assert_eq!(
        footprints.row(2).unwrap().suppressed_by(),
        Some(position(1, 7))
    );
    for column in 0_u8..40 {
        let record = footprints.cell(2, column).unwrap();
        assert_eq!(
            record.kind(),
            if (8..10).contains(&column) {
                TeletextBaseFragmentKind::DoubleHeightBottom
            } else {
                TeletextBaseFragmentKind::BackgroundOnly
            }
        );
        assert_eq!(record.origin(), Some(position(1, column)));
        assert_eq!(
            base.cell(1, column)
                .unwrap()
                .attributes()
                .background()
                .get(),
            if column >= 2 { 4 } else { 0 }
        );
    }
    assert_eq!(base.cell(2, 39).unwrap().code(), b'Z');
    assert_eq!(
        footprints.cell(2, 39).unwrap().kind(),
        TeletextBaseFragmentKind::BackgroundOnly
    );
    assert_eq!(base, original);
}

#[test]
fn ignored_lower_double_height_and_colour_controls_do_not_cascade_to_the_next_row() {
    let base = mixed_base();
    let footprints = resolve(&base);
    assert_eq!(
        footprints.row(2).unwrap().first_double_height_control(),
        Some(position(2, 2))
    );
    assert_eq!(
        footprints.row(2).unwrap().suppressed_by(),
        Some(position(1, 7))
    );
    assert_eq!(footprints.row(3).unwrap().suppressed_by(), None);
    assert_eq!(
        footprints.cell(3, 3).unwrap().kind(),
        TeletextBaseFragmentKind::Normal
    );
    assert_eq!(
        footprints.cell(3, 3).unwrap().origin(),
        Some(position(3, 3))
    );
    assert_eq!(base.cell(3, 3).unwrap().code(), b'C');
    assert_eq!(
        footprints.row(3).unwrap().baseline().sources().height,
        default_source(3)
    );
    assert!(!footprints.row(3).unwrap().baseline().double_height());
    assert!(!footprints.row(3).unwrap().baseline().flash());
    let mut rows = empty_rows(7, 0);
    height_segment(&mut rows, 22, 7, Some(10));
    height_segment(&mut rows, 23, 7, None);
    put(&mut rows, 24, 8, b'Z', None, attributes(24, 7, 0));
    let edge = resolve(&finish(rows, None));
    assert_eq!(edge.row(23).unwrap().suppressed_by(), Some(position(22, 7)));
    assert_eq!(
        edge.row(23).unwrap().first_double_height_control(),
        Some(position(23, 7))
    );
    assert_eq!(edge.row(23).unwrap().pending(), None);
    assert_eq!(edge.row(24).unwrap().suppressed_by(), None);
    assert_eq!(edge.row(24).unwrap().pending(), None);
    assert_eq!(
        edge.cell(24, 8).unwrap().kind(),
        TeletextBaseFragmentKind::Normal
    );
}

#[test]
fn double_height_control_at_column_39_and_immediately_cancelled_height_still_suppress() {
    for (control_column, normal_column) in [(39, None), (7, Some(8))] {
        let mut rows = empty_rows(7, 0);
        height_segment(&mut rows, 1, control_column, normal_column);
        put(&mut rows, 2, 8, b'X', None, attributes(2, 7, 0));
        let base = finish(rows, None);
        let footprints = resolve(&base);
        assert_eq!(
            footprints.row(1).unwrap().first_double_height_control(),
            Some(position(1, control_column))
        );
        assert_eq!(
            footprints.row(2).unwrap().suppressed_by(),
            Some(position(1, control_column))
        );
        for column in 0_u8..40 {
            assert_eq!(
                footprints.cell(2, column).unwrap().kind(),
                TeletextBaseFragmentKind::BackgroundOnly
            );
        }
        assert_eq!(
            footprints.cell(1, control_column).unwrap().kind(),
            TeletextBaseFragmentKind::Normal
        );
        assert_eq!(footprints.row(3).unwrap().suppressed_by(), None);
    }
}

#[test]
fn base_without_spacing_double_height_preserves_lower_controls_and_glyphs() {
    let mut rows = empty_rows(7, 0);
    put(&mut rows, 1, 8, b'A', None, attributes(1, 7, 0));
    for column in 0_u8..40 {
        put(
            &mut rows,
            2,
            column,
            if column == 0 {
                0x02
            } else if column == 8 {
                b'B'
            } else {
                b' '
            },
            if column == 0 {
                Some(TeletextBaseSpacingControl::AlphaColour {
                    colour: TeletextColour::Green,
                })
            } else {
                None
            },
            lower_attributes(column, false),
        );
    }
    let base = finish(rows, None);
    let footprints = resolve(&base);
    for row in footprints.rows() {
        assert_eq!(row.suppressed_by(), None);
    }
    for (row, column) in [(1, 8), (2, 0), (2, 8)] {
        let record = footprints.cell(row, column).unwrap();
        assert_eq!(record.kind(), TeletextBaseFragmentKind::Normal);
        assert_eq!(record.origin(), Some(position(row, column)));
    }
    assert_eq!(base.cell(2, 8).unwrap().code(), b'B');
    // Enhanced nonspacing DH does not enter this Base-only input; the consumed
    // factory must retain this unsuppressed Base outside its true footprint.
}

#[test]
fn illegal_double_height_rows_keep_located_pending_and_never_guess_receiver_clipping() {
    for (row, control_column) in [(0, 8), (23, 7), (24, 7)] {
        let mut rows = empty_rows(7, 0);
        height_segment(&mut rows, row, control_column, Some(control_column + 2));
        let base = finish(rows, None);
        let original = base.clone();
        let footprints = resolve(&base);
        let cause = position(row, control_column);
        let pending = footprints.row(row).unwrap().pending().unwrap();
        assert_eq!(pending.cause(), cause);
        assert_eq!(
            pending.reason(),
            TeletextBaseFootprintPendingReason::IllegalDoubleHeightRow
        );
        assert_eq!(footprints.row(row).unwrap().suppressed_by(), None);
        for column in control_column..40 {
            let cell = footprints.cell(row, column).unwrap();
            assert_eq!(cell.kind(), TeletextBaseFragmentKind::PendingHeightBoundary);
            assert_eq!(cell.origin(), None);
            assert_eq!(cell.pending().unwrap().cause(), cause);
        }
        if row < 24 {
            for column in 0_u8..40 {
                let affected = footprints.cell(row + 1, column).unwrap();
                assert_eq!(
                    affected.kind(),
                    TeletextBaseFragmentKind::PendingHeightBoundary
                );
                assert_eq!(affected.pending().unwrap().cause(), cause);
            }
            assert_eq!(footprints.row(row + 1).unwrap().suppressed_by(), None);
        }
        if row == 0 {
            for column in 0_u8..8 {
                assert_eq!(
                    footprints.cell(0, column).unwrap().kind(),
                    TeletextBaseFragmentKind::HeaderOwned
                );
                assert_eq!(footprints.cell(0, column).unwrap().origin(), None);
            }
            assert_eq!(footprints.row(2).unwrap().pending(), None);
        } else {
            assert_eq!(
                footprints.cell(row, 6).unwrap().kind(),
                TeletextBaseFragmentKind::Normal
            );
        }
        assert_eq!(base, original);
    }
    // An uncertain lower row may itself be processed: retain the original
    // illegal cause for that row's possible downstream suppression too.
    let mut rows = empty_rows(7, 0);
    height_segment(&mut rows, 0, 8, Some(10));
    height_segment(&mut rows, 1, 7, Some(10));
    let uncertain = resolve(&finish(rows, None));
    assert_eq!(
        uncertain.row(1).unwrap().first_double_height_control(),
        Some(position(1, 7))
    );
    assert_eq!(
        uncertain.row(2).unwrap().pending().unwrap().cause(),
        position(0, 8)
    );
    assert_eq!(
        uncertain.cell(2, 39).unwrap().kind(),
        TeletextBaseFragmentKind::PendingHeightBoundary
    );
    assert_eq!(uncertain.row(2).unwrap().suppressed_by(), None);
    assert_eq!(uncertain.row(3).unwrap().pending(), None);
}

fn presentation(substitution: bool) -> TeletextLevel25PresentationDefaults {
    TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(0).unwrap(),
        secondary: None,
        palette: TeletextLevel25Palette::standard(),
        screen_colour: TeletextColourReference::new(31).unwrap(),
        row_colour: TeletextColourReference::new(5).unwrap(),
        black_background_substitution: substitution,
        remapping: TeletextColourRemapping::FgClut2BgClut3,
        panels: TeletextSidePanels {
            left: false,
            right: false,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::ZERO,
        },
    }
}

#[test]
fn baseline_uses_original_remapping_and_row_colour_when_no_coded_default_background_survives() {
    for (substitution, expected_background) in [(false, 24), (true, 13)] {
        let mut rows = empty_rows(23, 24);
        for column in 0_u8..40 {
            let original = attributes(2, 23, 31);
            let mut values = original.values();
            let mut sources = original.sources();
            values.raw_background = 7;
            sources.background = source(2, 0, TeletextBaseSpacingControl::NewBackground);
            put(
                &mut rows,
                2,
                column,
                if column == 0 { 0x1d } else { b' ' },
                if column == 0 {
                    Some(TeletextBaseSpacingControl::NewBackground)
                } else {
                    None
                },
                TeletextBaseDisplayAttributes::new(values, sources).unwrap(),
            );
        }
        let base = finish(rows, Some(presentation(substitution)));
        let before = base.clone();
        let mut colours = [TeletextColourReference::BLACK; 25];
        colours[2] = TeletextColourReference::new(13).unwrap();
        let footprints = resolve_teletext_base_footprints(&base, &colours).unwrap();
        let baseline = footprints.row(2).unwrap().baseline();
        assert_eq!(
            (baseline.raw_foreground(), baseline.raw_background()),
            (7, 0)
        );
        assert_eq!(
            (baseline.foreground().get(), baseline.background().get()),
            (23, expected_background)
        );
        assert_eq!(baseline.sources().foreground, default_source(2));
        assert_eq!(baseline.sources().background, default_source(2));
        assert!(!baseline.flash());
        assert!(!baseline.double_height());
        for column in 0_u8..40 {
            assert_eq!(
                base.cell(2, column)
                    .unwrap()
                    .attributes()
                    .background()
                    .get(),
                31
            );
            assert_eq!(
                base.cell(2, column)
                    .unwrap()
                    .attributes()
                    .sources()
                    .background,
                source(2, 0, TeletextBaseSpacingControl::NewBackground)
            );
        }
        assert_eq!(footprints.row(2).unwrap().suppressed_by(), None);
        assert_eq!(base, before);
    }
    let base = finish(empty_rows(7, 0), None);
    let footprints =
        resolve_teletext_base_footprints(&base, &[TeletextColourReference::new(13).unwrap(); 25])
            .unwrap();
    assert_eq!(
        footprints.row(2).unwrap().baseline().background(),
        TeletextColourReference::BLACK
    );
}

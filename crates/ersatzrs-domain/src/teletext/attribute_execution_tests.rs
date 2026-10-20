//! Fixed EN 300 706 §13.9 logical attribute oracles, not raster acceptance.

use super::*;

pub(super) fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

fn position(row: u8, column: u8) -> TeletextEnhancementPosition {
    TeletextEnhancementPosition::new(row, column).unwrap()
}

fn row_default(row: u8) -> TeletextBaseAttributeSource {
    TeletextBaseAttributeSource::RowDefault {
        row,
        start_column: if row == 0 { 8 } else { 0 },
    }
}

fn colour_source(column: u8, colour: TeletextColour) -> TeletextBaseAttributeSource {
    TeletextBaseAttributeSource::Spacing {
        position: position(1, column),
        control: TeletextBaseSpacingControl::AlphaColour { colour },
        timing: TeletextSpacingTiming::SetAfter,
    }
}

fn base_attributes(row: u8, column: u8) -> TeletextBaseDisplayAttributes {
    let default = row_default(row);
    let (raw_foreground, foreground_source) = if row != 1 || column == 0 {
        (7, default)
    } else if column <= 17 {
        (4, colour_source(0, TeletextColour::Blue))
    } else if column <= 21 {
        (1, colour_source(17, TeletextColour::Red))
    } else {
        (2, colour_source(21, TeletextColour::Green))
    };
    TeletextBaseDisplayAttributes::new(
        TeletextBaseAttributeValues {
            raw_foreground,
            raw_background: 0,
            foreground: TeletextColourReference::new(raw_foreground).unwrap(),
            background: TeletextColourReference::new(0).unwrap(),
            graphics: false,
            separated: false,
            double_height: false,
            double_width: false,
            boxed: false,
            flash: false,
            conceal: false,
        },
        TeletextBaseAttributeSources {
            foreground: foreground_source,
            background: default,
            graphics: foreground_source,
            separated: default,
            height: default,
            width: default,
            boxed: default,
            flash: default,
            conceal: foreground_source,
        },
    )
    .unwrap()
}

fn base_code(row: u8, column: u8) -> u8 {
    // Figure's exact cell positions: spacing controls are separate coded cells.
    const ROW: [u8; 28] = [
        0x04, b'T', b'H', b'E', b' ', b'F', b'A', b'S', b'T', b' ', b'D', b'O', b'G', b' ', b'A',
        b'N', b'D', 0x01, b'C', b'A', b'T', 0x02, b'R', b'A', b'N', b' ', b'I', b'N',
    ];
    if row != 1 {
        return b' ';
    }
    ROW.get(usize::from(column)).copied().unwrap_or(b' ')
}

pub(super) fn base() -> TeletextBaseDisplay {
    let rows: Vec<[TeletextBaseDisplayCell; 40]> = (0_u8..25)
        .map(|row| {
            let cells: Vec<_> = (0_u8..40)
                .map(|column| {
                    let code = base_code(row, column);
                    let control = match (row, column) {
                        (1, 0) => Some(TeletextBaseSpacingControl::AlphaColour {
                            colour: TeletextColour::Blue,
                        }),
                        (1, 17) => Some(TeletextBaseSpacingControl::AlphaColour {
                            colour: TeletextColour::Red,
                        }),
                        (1, 21) => Some(TeletextBaseSpacingControl::AlphaColour {
                            colour: TeletextColour::Green,
                        }),
                        _ => None,
                    };
                    let kind = if row == 0 && column < 8 {
                        TeletextBaseDisplayCellKind::HeaderOwned
                    } else if let Some(control) = control {
                        TeletextBaseDisplayCellKind::SpacingControl {
                            control,
                            timing: TeletextSpacingTiming::SetAfter,
                        }
                    } else if code == b' ' {
                        TeletextBaseDisplayCellKind::Blank
                    } else {
                        TeletextBaseDisplayCellKind::Glyph
                    };
                    TeletextBaseDisplayCell::new(
                        position(row, column),
                        code,
                        kind,
                        None,
                        base_attributes(row, column),
                    )
                    .unwrap()
                })
                .collect();
            cells.try_into().unwrap()
        })
        .collect();
    let rows: Box<[[TeletextBaseDisplayCell; 40]; 25]> =
        rows.into_boxed_slice().try_into().unwrap();
    TeletextBaseDisplay::new(
        owner(),
        TeletextPageCharsetDesignation::new(0).unwrap(),
        TeletextNationalOption::new(0).unwrap(),
        None,
        rows,
    )
    .unwrap()
}

fn separation_base() -> TeletextBaseDisplay {
    let original = base();
    let mut rows: Vec<_> = original.cells().to_vec();
    let default = row_default(2);
    let blue = TeletextBaseAttributeSource::Spacing {
        position: position(2, 0),
        control: TeletextBaseSpacingControl::MosaicColour {
            colour: TeletextColour::Blue,
        },
        timing: TeletextSpacingTiming::SetAfter,
    };
    let cells: Vec<_> = (0_u8..40)
        .map(|column| {
            let (code, control) = match column {
                0 => (
                    0x14,
                    Some(TeletextBaseSpacingControl::MosaicColour {
                        colour: TeletextColour::Blue,
                    }),
                ),
                3 => (0x1a, Some(TeletextBaseSpacingControl::SeparatedMosaics)),
                20 => (0x19, Some(TeletextBaseSpacingControl::ContiguousMosaics)),
                _ => (b' ', None),
            };
            let colour = if column == 0 { default } else { blue };
            let separated_source = if column < 3 {
                default
            } else {
                TeletextBaseAttributeSource::Spacing {
                    position: position(2, if column < 20 { 3 } else { 20 }),
                    control: if column < 20 {
                        TeletextBaseSpacingControl::SeparatedMosaics
                    } else {
                        TeletextBaseSpacingControl::ContiguousMosaics
                    },
                    timing: TeletextSpacingTiming::SetAt,
                }
            };
            let attributes = TeletextBaseDisplayAttributes::new(
                TeletextBaseAttributeValues {
                    raw_foreground: if column == 0 { 7 } else { 4 },
                    raw_background: 0,
                    foreground: TeletextColourReference::new(if column == 0 { 7 } else { 4 })
                        .unwrap(),
                    background: TeletextColourReference::new(0).unwrap(),
                    graphics: column != 0,
                    separated: (3..20).contains(&column),
                    double_height: false,
                    double_width: false,
                    boxed: false,
                    flash: false,
                    conceal: false,
                },
                TeletextBaseAttributeSources {
                    foreground: colour,
                    background: default,
                    graphics: colour,
                    separated: separated_source,
                    height: default,
                    width: default,
                    boxed: default,
                    flash: default,
                    conceal: colour,
                },
            )
            .unwrap();
            let kind = control.map_or(TeletextBaseDisplayCellKind::Blank, |control| {
                TeletextBaseDisplayCellKind::SpacingControl {
                    control,
                    timing: control.timing(),
                }
            });
            TeletextBaseDisplayCell::new(position(2, column), code, kind, None, attributes).unwrap()
        })
        .collect();
    rows[2] = cells.try_into().unwrap();
    let rows: Box<[[TeletextBaseDisplayCell; 40]; 25]> =
        rows.into_boxed_slice().try_into().unwrap();
    TeletextBaseDisplay::new(
        owner(),
        original.primary(),
        original.national_option(),
        None,
        rows,
    )
    .unwrap()
}

fn spacing(
    row: u8,
    column: u8,
    control: TeletextBaseSpacingControl,
) -> TeletextBaseAttributeSource {
    TeletextBaseAttributeSource::Spacing {
        position: position(row, column),
        control,
        timing: control.timing(),
    }
}

fn group_attributes(column: u8) -> TeletextBaseDisplayAttributes {
    let default = row_default(2);
    let (raw_foreground, foreground_source) = match column {
        0..=1 => (7, default),
        2..=3 => (
            4,
            spacing(
                2,
                1,
                TeletextBaseSpacingControl::AlphaColour {
                    colour: TeletextColour::Blue,
                },
            ),
        ),
        4..=11 => (
            7,
            spacing(
                2,
                3,
                TeletextBaseSpacingControl::AlphaColour {
                    colour: TeletextColour::White,
                },
            ),
        ),
        12..=13 => (
            1,
            spacing(
                2,
                11,
                TeletextBaseSpacingControl::AlphaColour {
                    colour: TeletextColour::Red,
                },
            ),
        ),
        _ => (
            7,
            spacing(
                2,
                13,
                TeletextBaseSpacingControl::AlphaColour {
                    colour: TeletextColour::White,
                },
            ),
        ),
    };
    let (raw_background, background_source) = match column {
        0..=1 => (0, default),
        2..=11 => (4, spacing(2, 2, TeletextBaseSpacingControl::NewBackground)),
        _ => (1, spacing(2, 12, TeletextBaseSpacingControl::NewBackground)),
    };
    let flash_source = match column {
        0..=4 => default,
        5..=13 => spacing(2, 4, TeletextBaseSpacingControl::Flash),
        _ => spacing(2, 14, TeletextBaseSpacingControl::Steady),
    };
    let height_source = match column {
        0..=6 => default,
        7..=15 => spacing(2, 6, TeletextBaseSpacingControl::DoubleHeight),
        _ => spacing(2, 16, TeletextBaseSpacingControl::NormalHeight),
    };
    let conceal_source = if (7..12).contains(&column) {
        spacing(2, 7, TeletextBaseSpacingControl::Conceal)
    } else {
        foreground_source
    };
    TeletextBaseDisplayAttributes::new(
        TeletextBaseAttributeValues {
            raw_foreground,
            raw_background,
            foreground: TeletextColourReference::new(raw_foreground).unwrap(),
            background: TeletextColourReference::new(raw_background).unwrap(),
            graphics: false,
            separated: false,
            double_height: (7..16).contains(&column),
            double_width: false,
            boxed: false,
            flash: (5..14).contains(&column),
            conceal: (7..12).contains(&column),
        },
        TeletextBaseAttributeSources {
            foreground: foreground_source,
            background: background_source,
            graphics: foreground_source,
            separated: default,
            height: height_source,
            width: height_source,
            boxed: default,
            flash: flash_source,
            conceal: conceal_source,
        },
    )
    .unwrap()
}

fn group_base() -> TeletextBaseDisplay {
    let original = base();
    let mut rows: Vec<_> = original.cells().to_vec();
    let cells: Vec<_> = (0_u8..40)
        .map(|column| {
            let control = match column {
                1 => Some(TeletextBaseSpacingControl::AlphaColour {
                    colour: TeletextColour::Blue,
                }),
                2 | 12 => Some(TeletextBaseSpacingControl::NewBackground),
                3 | 13 => Some(TeletextBaseSpacingControl::AlphaColour {
                    colour: TeletextColour::White,
                }),
                4 => Some(TeletextBaseSpacingControl::Flash),
                6 => Some(TeletextBaseSpacingControl::DoubleHeight),
                7 => Some(TeletextBaseSpacingControl::Conceal),
                11 => Some(TeletextBaseSpacingControl::AlphaColour {
                    colour: TeletextColour::Red,
                }),
                14 => Some(TeletextBaseSpacingControl::Steady),
                16 => Some(TeletextBaseSpacingControl::NormalHeight),
                _ => None,
            };
            let code = control.map_or(b' ', TeletextBaseSpacingControl::code);
            let kind = control.map_or(TeletextBaseDisplayCellKind::Blank, |control| {
                TeletextBaseDisplayCellKind::SpacingControl {
                    control,
                    timing: control.timing(),
                }
            });
            TeletextBaseDisplayCell::new(
                position(2, column),
                code,
                kind,
                None,
                group_attributes(column),
            )
            .unwrap()
        })
        .collect();
    rows[2] = cells.try_into().unwrap();
    let rows: Box<[[TeletextBaseDisplayCell; 40]; 25]> =
        rows.into_boxed_slice().try_into().unwrap();
    TeletextBaseDisplay::new(
        owner(),
        original.primary(),
        original.national_option(),
        None,
        rows,
    )
    .unwrap()
}

pub(super) fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}

pub(super) fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}

pub(super) fn foreground(column: u8, index: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(index).unwrap(),
        },
    )
}

pub(super) fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}

pub(super) fn set(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: position(row, column),
    }
}

pub(super) fn invoke(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    offset: Option<(u8, u8)>,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: offset.map(|(row, column)| TeletextObjectOffset::new(row, column).unwrap()),
    }
}

pub(super) fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}

fn object_example() -> Vec<TeletextEnhancementCommand> {
    vec![
        glyph(5, b'L'),
        glyph(6, b'A'),
        glyph(7, b'Z'),
        glyph(8, b'Y'),
        foreground(10, 13),
        glyph(10, b'C'),
        glyph(11, b'O'),
        glyph(12, b'W'),
        glyph(18, b'F'),
        glyph(19, b'O'),
        glyph(20, b'X'),
        foreground(23, 3),
        glyph(23, b'S'),
        glyph(24, b'A'),
        glyph(25, b'T'),
    ]
}

pub(super) fn service(
    main: Vec<TeletextEnhancementCommand>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextSemanticService {
    let language = TeletextLanguage::new("eng".into()).unwrap();
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: language.clone(),
                initial_page: owner().page,
                pages: vec![TeletextPageDraft {
                    address: owner(),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(0).unwrap(),
                    rows: vec![],
                    navigation: vec![],
                }],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation: None,
                compositional_links: None,
                local_program: Some(TeletextEnhancementProgram::new(main).unwrap()),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [None; 4],
                },
            }],
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 16,
            max_objects: 8,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 4096,
            max_collected_commands: 4096,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}

fn execute(
    service: &TeletextSemanticService,
    base: &TeletextBaseDisplay,
    max_commands: usize,
) -> Result<TeletextAttributeExecution, TeletextAttributeExecutionError> {
    let original = resolve_teletext_spatial_trace(
        service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 4096,
            max_invocations: 32,
        },
    )
    .unwrap();
    let glyphs = build_teletext_glyph_layers(base, &original, 4096).unwrap();
    resolve_teletext_attribute_execution(
        base,
        &original,
        &glyphs,
        &[TeletextColourReference::BLACK; 25],
        max_commands,
    )
}

fn foreground_value(execution: &TeletextAttributeExecution, row: u8, column: u8) -> u8 {
    execution
        .cell(row, column)
        .unwrap()
        .foreground()
        .value()
        .unwrap()
        .get()
}

fn example(object_type: TeletextObjectType) -> TeletextAttributeExecution {
    let target = local(0xffff);
    let service = service(
        vec![set(1, 0), invoke(target, object_type, None)],
        vec![definition(target, object_type, object_example())],
    );
    execute(&service, &base(), 4096).unwrap()
}

#[test]
fn official_active_example_keeps_pink_gaps_but_later_red_spacing_wins_fox() {
    let execution = example(TeletextObjectType::Active);
    for (columns, colour) in [
        (&[1, 2, 3][..], 4),
        (&[5, 6, 7, 8][..], 4),
        (&[10, 11, 12][..], 13),
        (&[13, 14, 15, 16][..], 13),
        (&[18, 19, 20][..], 1),
        (&[23, 24, 25][..], 3),
        (&[26, 27][..], 3),
    ] {
        for &column in columns {
            assert_eq!(
                foreground_value(&execution, 1, column),
                colour,
                "column {column}"
            );
        }
    }
}

#[test]
fn official_adaptive_example_keeps_pink_inside_addressed_extent_and_restores_outside_green() {
    let execution = example(TeletextObjectType::Adaptive);
    for (columns, colour) in [
        (&[1, 2, 3][..], 4),
        (&[5, 6, 7, 8][..], 4),
        (&[10, 11, 12][..], 13),
        (&[13, 14, 15, 16][..], 13),
        (&[18, 19, 20][..], 13),
        (&[23, 24, 25][..], 3),
        (&[26, 27][..], 2),
    ] {
        for &column in columns {
            assert_eq!(
                foreground_value(&execution, 1, column),
                colour,
                "column {column}"
            );
        }
    }
}

#[test]
fn official_passive_example_private_white_pink_yellow_changes_only_written_glyphs() {
    let execution = example(TeletextObjectType::Passive);
    for (columns, colour) in [
        (&[1, 2, 3][..], 4),
        (&[5, 6, 7, 8][..], 7),
        (&[10, 11, 12][..], 13),
        (&[13, 14, 15, 16][..], 4),
        (&[18, 19, 20][..], 13),
        (&[23, 24, 25][..], 3),
        (&[26, 27][..], 2),
    ] {
        for &column in columns {
            assert_eq!(
                foreground_value(&execution, 1, column),
                colour,
                "column {column}"
            );
        }
    }
}

#[test]
fn adaptive_inherits_unsupplied_background_flash_and_size_per_cell_not_at_call_origin() {
    let target = local(0xffff);
    let service = service(
        vec![
            set(2, 0),
            invoke(target, TeletextObjectType::Adaptive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Adaptive,
            vec![foreground(5, 13), glyph(5, b'A'), glyph(15, b'B')],
        )],
    );
    let execution = execute(&service, &group_base(), 4096).unwrap();
    let first = execution.cell(2, 5).unwrap();
    let last = execution.cell(2, 15).unwrap();
    assert_eq!(first.foreground().value().unwrap().get(), 13);
    assert_eq!(last.foreground().value().unwrap().get(), 13);
    assert_eq!(first.background().value().unwrap().get(), 4);
    assert_eq!(last.background().value().unwrap().get(), 1);
    assert_eq!(
        first.flash().value().unwrap().mode(),
        TeletextFlashMode::Normal
    );
    assert_eq!(
        last.flash().value().unwrap().mode(),
        TeletextFlashMode::Steady
    );
    assert_eq!(first.double_height().value(), Some(false));
    assert_eq!(last.double_height().value(), Some(true));
}

#[test]
fn passive_private_state_persists_across_relative_rows_but_each_invocation_is_fresh() {
    let target = local(0xffff);
    let service = service(
        vec![
            set(2, 0),
            invoke(target, TeletextObjectType::Passive, None),
            set(4, 0),
            invoke(target, TeletextObjectType::Passive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![
                glyph(5, b'A'),
                foreground(10, 13),
                glyph(10, b'B'),
                set(1, 0),
                glyph(1, b'C'),
            ],
        )],
    );
    let execution = execute(&service, &group_base(), 4096).unwrap();
    for (row, column, colour) in [
        (2, 5, 7),
        (2, 10, 13),
        (3, 1, 13),
        (4, 5, 7),
        (4, 10, 13),
        (5, 1, 13),
    ] {
        assert_eq!(foreground_value(&execution, row, column), colour);
        let cell = execution.cell(row, column).unwrap();
        assert_eq!(cell.background().value().unwrap().get(), 0);
        assert_eq!(
            cell.flash().value().unwrap().mode(),
            TeletextFlashMode::Steady
        );
        assert_eq!(cell.double_height().value(), Some(false));
        assert_eq!(cell.conceal().value(), Some(false));
    }
    assert_eq!(
        execution
            .cell(2, 14)
            .unwrap()
            .background()
            .value()
            .unwrap()
            .get(),
        1
    );
}

#[test]
fn active_attribute_without_any_glyph_changes_underlying_base_cells() {
    let target = local(0xffff);
    let service = service(
        vec![set(1, 0), invoke(target, TeletextObjectType::Active, None)],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![foreground(10, 13)],
        )],
    );
    let execution = execute(&service, &base(), 4096).unwrap();
    assert_eq!(foreground_value(&execution, 1, 9), 4);
    assert_eq!(foreground_value(&execution, 1, 10), 13);
    assert_eq!(foreground_value(&execution, 1, 14), 13);
    assert_eq!(foreground_value(&execution, 1, 18), 1);
}

#[test]
fn c2_effective_start_and_placement_compare_absolute_invocation_origins() {
    let target = local(0xffff);
    for (offset, glyph_column, expected) in [(5, 1, 1), (6, 0, 4)] {
        let service = service(
            vec![
                set(1, 12),
                invoke(target, TeletextObjectType::Active, Some((0, offset))),
            ],
            vec![definition(
                target,
                TeletextObjectType::Active,
                vec![foreground(0, 4), glyph(glyph_column, b'A')],
            )],
        );
        let execution = execute(&service, &base(), 4096).unwrap();
        assert_eq!(foreground_value(&execution, 1, 18), expected);
        let source = execution
            .cell(1, 18)
            .unwrap()
            .foreground()
            .source()
            .unwrap();
        if offset == 5 {
            assert!(
                matches!(source,TeletextAttributeSource::Base {source:TeletextBaseAttributeSource::Spacing {
                position:origin,control:TeletextBaseSpacingControl::AlphaColour{colour:TeletextColour::Red},timing:TeletextSpacingTiming::SetAfter},..} if origin==position(1,17))
            );
        } else {
            assert!(
                matches!(source,TeletextAttributeSource::Enhancement{location:TeletextEnhancementSource{object:Some(identity),command:0},
                effective_start:(1,18),placed_position:(1,18),..} if identity==target)
            );
        }
    }
}

#[test]
fn transmission_position_tie_retains_both_original_sources_without_event_time_guess() {
    let service = service(
        vec![
            set(1, 18),
            foreground(18, 4),
            foreground(18, 13),
            glyph(18, b'A'),
        ],
        vec![],
    );
    let execution = execute(&service, &base(), 4096).unwrap();
    let foreground = execution.cell(1, 18).unwrap().foreground();
    assert_eq!(foreground.value(), None);
    let pending = &execution.pending()[foreground.pending().unwrap()];
    assert_eq!(
        pending.reason(),
        TeletextAttributePendingReason::TransmissionOrder
    );
    assert_eq!(pending.position(), (1, 18));
    assert_eq!(
        pending.location(),
        TeletextEnhancementSource {
            object: None,
            command: 2
        }
    );
    assert_eq!(
        pending.root_source(),
        TeletextSpatialRootSource::Local { command: 2 }
    );
    assert_eq!(pending.invocation(), None);
    assert!(matches!(
        pending.first_source(),
        Some(TeletextAttributeSource::Enhancement {
            location: TeletextEnhancementSource {
                object: None,
                command: 1
            },
            ..
        })
    ));
    assert!(matches!(
        pending.second_source(),
        Some(TeletextAttributeSource::Enhancement {
            location: TeletextEnhancementSource {
                object: None,
                command: 2
            },
            ..
        })
    ));
}

fn flags(column: u8, d5: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_height: true,
            double_width: true,
            boxed: true,
            conceal: true,
            invert: true,
            underline_or_separated_mosaics: d5,
        },
    )
}

#[test]
fn partial_spacing_cancellation_preserves_other_display_fields_and_logical_invert() {
    let target = local(0xffff);
    let commands = vec![
        foreground(5, 13),
        at(
            5,
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(6).unwrap(),
            },
        ),
        at(
            5,
            TeletextColumnOperation::Flash {
                mode: TeletextFlashMode::Normal,
                rate: TeletextFlashRate::Fast2HzPhase2,
            },
        ),
        flags(5, true),
        glyph(5, b'A'),
        glyph(8, b'B'),
        glyph(18, b'C'),
    ];
    let service = service(
        vec![set(2, 0), invoke(target, TeletextObjectType::Active, None)],
        vec![definition(target, TeletextObjectType::Active, commands)],
    );
    let execution = execute(&service, &group_base(), 4096).unwrap();
    let first = execution.cell(2, 5).unwrap();
    assert_eq!(first.foreground().value().unwrap().get(), 13);
    assert_eq!(first.background().value().unwrap().get(), 6);
    assert_eq!(
        first.flash().value().unwrap().rate(),
        TeletextFlashRate::Fast2HzPhase2
    );
    assert_eq!(first.double_width().value(), Some(true));
    assert_eq!(first.invert().value(), Some(true));
    assert_eq!(
        first.rendered_colours(),
        Some((
            TeletextColourReference::new(6).unwrap(),
            TeletextColourReference::new(13).unwrap()
        ))
    );
    let middle = execution.cell(2, 8).unwrap();
    assert_eq!(middle.double_height().value(), Some(true));
    assert_eq!(middle.double_width().value(), Some(false));
    let last = execution.cell(2, 18).unwrap();
    assert_eq!(last.foreground().value().unwrap().get(), 7);
    assert_eq!(last.background().value().unwrap().get(), 1);
    assert_eq!(
        last.flash().value().unwrap().mode(),
        TeletextFlashMode::Steady
    );
    assert_eq!(last.double_height().value(), Some(false));
    assert_eq!(last.double_width().value(), Some(false));
    assert_eq!(last.conceal().value(), Some(false));
    assert_eq!(last.boxed().value(), Some(true));
    assert_eq!(last.invert().value(), Some(true));
    assert_eq!(last.underline_or_separated_mosaics().value(), Some(true));
}

#[test]
fn nonspacing_d5_survives_later_contiguous_and_explicit_clear_removes_prior_separation() {
    let clear = TeletextColumnOperation::Attributes {
        double_height: false,
        double_width: false,
        boxed: false,
        conceal: false,
        invert: false,
        underline_or_separated_mosaics: false,
    };
    let clear_service = service(
        vec![
            set(2, 8),
            at(8, clear),
            at(
                8,
                TeletextColumnOperation::G1 {
                    mosaic: TeletextMosaic::new(1).unwrap(),
                },
            ),
        ],
        vec![],
    );
    let execution = execute(&clear_service, &separation_base(), 4096).unwrap();
    assert_eq!(
        execution
            .cell(2, 8)
            .unwrap()
            .underline_or_separated_mosaics()
            .value(),
        Some(false)
    );
    assert_eq!(
        execution.cell(2, 8).unwrap().separated_mosaics(),
        Some(false)
    );
    let set_service = service(
        vec![
            set(2, 8),
            flags(8, true),
            at(
                8,
                TeletextColumnOperation::G1 {
                    mosaic: TeletextMosaic::new(1).unwrap(),
                },
            ),
            at(
                21,
                TeletextColumnOperation::G1 {
                    mosaic: TeletextMosaic::new(2).unwrap(),
                },
            ),
        ],
        vec![],
    );
    let execution = execute(&set_service, &separation_base(), 4096).unwrap();
    assert_eq!(
        execution
            .cell(2, 21)
            .unwrap()
            .underline_or_separated_mosaics()
            .value(),
        Some(true)
    );
    assert_eq!(
        execution.cell(2, 21).unwrap().separated_mosaics(),
        Some(true)
    );
}

#[test]
fn exact_original_command_budget_is_located_and_rejection_does_not_mutate_inputs() {
    let target = local(0xffff);
    let service = service(
        vec![set(1, 0), invoke(target, TeletextObjectType::Active, None)],
        vec![definition(
            target,
            TeletextObjectType::Active,
            object_example(),
        )],
    );
    let before = service.clone();
    let base = base();
    let before_base = base.clone();
    let exact = execute(&service, &base, 17).unwrap();
    assert_eq!(foreground_value(&exact, 1, 18), 1);
    let error = execute(&service, &base, 16).unwrap_err();
    assert_eq!(error.owner, owner());
    assert_eq!(
        error.reason,
        TeletextAttributeExecutionFailure::CommandBudget { maximum: 16 }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 14
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(
        error.invocation.map(TeletextSpatialInvocationId::get),
        Some(0)
    );
    assert_eq!(base, before_base);
    assert_eq!(service, before);
}

#[test]
fn side_panel_areas_have_independent_white_foreground_and_original_row_background() {
    let target = local(0xffff);
    let service = service(
        vec![
            set(1, 39),
            foreground(39, 13),
            glyph(39, b'A'),
            invoke(target, TeletextObjectType::Active, Some((0, 1))),
            invoke(target, TeletextObjectType::Active, Some((0, 17))),
        ],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![glyph(0, b'B')],
        )],
    );
    let base = base();
    let original = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 128,
            max_invocations: 16,
        },
    )
    .unwrap();
    let glyphs = build_teletext_glyph_layers(&base, &original, 16).unwrap();
    let mut rows = [TeletextColourReference::BLACK; 25];
    rows[1] = TeletextColourReference::new(6).unwrap();
    let execution =
        resolve_teletext_attribute_execution(&base, &original, &glyphs, &rows, 128).unwrap();
    assert_eq!(foreground_value(&execution, 1, 39), 13);
    for column in [40, 55, 56, 71] {
        let panel = execution.cell(1, column).unwrap();
        assert_eq!(panel.foreground().value().unwrap().get(), 7);
        assert_eq!(panel.background().value().unwrap().get(), 6);
        assert_eq!(
            panel.background().source(),
            Some(TeletextAttributeSource::PanelDefault { row: 1 })
        );
    }
    assert!(execution.cell(1, 72).is_none());
    assert!(execution.cell(25, 0).is_none());
    // These are logical areas; actual panel enablement/width remains unresolved.
}

#[test]
fn out_of_area_events_keep_full_original_position_source_and_instance_as_pending() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let service = service(
        vec![
            set(24, 39),
            invoke(active, TeletextObjectType::Active, Some((23, 71))),
        ],
        vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![
                    set(24, 39),
                    invoke(adaptive, TeletextObjectType::Adaptive, Some((23, 71))),
                ],
            ),
            definition(
                adaptive,
                TeletextObjectType::Adaptive,
                vec![
                    set(24, 39),
                    invoke(passive, TeletextObjectType::Passive, Some((23, 71))),
                ],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                vec![set(24, 39), foreground(39, 13), glyph(39, b'A')],
            ),
        ],
    );
    let execution = execute(&service, &base(), 128).unwrap();
    let pending = execution
        .pending()
        .iter()
        .find(|pending| {
            pending.position() == (165, 369)
                && pending.location()
                    == TeletextEnhancementSource {
                        object: Some(passive),
                        command: 1,
                    }
        })
        .unwrap();
    assert_eq!(
        pending.reason(),
        TeletextAttributePendingReason::Unaddressable
    );
    assert_eq!(
        pending.root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert_eq!(
        pending.invocation().map(TeletextSpatialInvocationId::get),
        Some(2)
    );
}

#[test]
fn exact_owner_mismatch_is_rejected_before_execution_with_no_input_mutation() {
    let service = service(vec![foreground(1, 13)], vec![]);
    let original = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 16,
            max_invocations: 4,
        },
    )
    .unwrap();
    let base = base();
    let glyphs = build_teletext_glyph_layers(&base, &original, 16).unwrap();
    let mut other = owner();
    other.subcode = TeletextSubcode::new(0x1235).unwrap();
    let rows: Vec<_> = base.cells().to_vec();
    let rows: Box<[[TeletextBaseDisplayCell; 40]; 25]> =
        rows.into_boxed_slice().try_into().unwrap();
    let foreign =
        TeletextBaseDisplay::new(other, base.primary(), base.national_option(), None, rows)
            .unwrap();
    let before = foreign.clone();
    let before_original = original.clone();
    let before_glyphs = glyphs.clone();
    let error = resolve_teletext_attribute_execution(
        &foreign,
        &original,
        &glyphs,
        &[TeletextColourReference::BLACK; 25],
        16,
    )
    .unwrap_err();
    assert_eq!(error.owner, other);
    assert_eq!(
        error.reason,
        TeletextAttributeExecutionFailure::OwnerMismatch { other: owner() }
    );
    assert_eq!(error.location, None);
    assert_eq!(error.root_source, None);
    assert_eq!(error.invocation, None);
    let (result, admitted) = resolve_teletext_attribute_execution_with_usage(
        &foreign,
        &original,
        &glyphs,
        &[TeletextColourReference::BLACK; 25],
        16,
        Some(0),
    );
    assert_eq!(result.unwrap_err(), error);
    assert_eq!(admitted, 0);
    assert_eq!(foreign, before);
    assert_eq!(original, before_original);
    assert_eq!(glyphs, before_glyphs);
}

#[test]
fn original_header_panel_attribute_event_is_retained_pending_without_a_header_panel_cell() {
    let target = local(0xffff);
    let service = service(
        vec![
            set(0, 39),
            invoke(target, TeletextObjectType::Active, Some((0, 32))),
        ],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![foreground(0, 13), glyph(0, b'A')],
        )],
    );
    let execution = execute(&service, &base(), 128).unwrap();
    assert!(execution.cell(0, 71).is_none());
    let pending = execution
        .pending()
        .iter()
        .find(|pending| {
            pending.position() == (0, 71)
                && pending.location()
                    == TeletextEnhancementSource {
                        object: Some(target),
                        command: 0,
                    }
        })
        .unwrap();
    assert_eq!(
        pending.reason(),
        TeletextAttributePendingReason::Unaddressable
    );
    assert_eq!(
        pending.root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert_eq!(
        pending.invocation().map(TeletextSpatialInvocationId::get),
        Some(0)
    );
}

#[test]
fn adaptive_ap_only_tail_retains_endpoint_uncertainty_without_erasing_unmodified_groups() {
    let target = local(0xffff);
    let service = service(
        vec![
            set(1, 0),
            invoke(target, TeletextObjectType::Adaptive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Adaptive,
            vec![foreground(5, 13), glyph(10, b'A'), set(0, 20)],
        )],
    );
    let execution = execute(&service, &base(), 128).unwrap();
    assert_eq!(foreground_value(&execution, 1, 5), 13);
    assert_eq!(foreground_value(&execution, 1, 10), 13);
    let foreground = execution.cell(1, 11).unwrap().foreground();
    assert_eq!(foreground.value(), None);
    let pending = &execution.pending()[foreground.pending().unwrap()];
    assert_eq!(
        pending.reason(),
        TeletextAttributePendingReason::AdaptiveExtent
    );
    assert_eq!(pending.position(), (1, 11));
    assert_eq!(
        pending.location(),
        TeletextEnhancementSource {
            object: Some(target),
            command: 2
        }
    );
    assert_eq!(
        pending.root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert_eq!(
        pending.invocation().map(TeletextSpatialInvocationId::get),
        Some(0)
    );
    assert!(
        matches!(pending.first_source(), Some(TeletextAttributeSource::Enhancement {
        location: TeletextEnhancementSource { object: Some(object), command: 0 },
        root_source: TeletextSpatialRootSource::Local { command: 1 },
        invocation: Some(id),
        effective_start: (1, 5),
        placed_position: (1, 5),
        ..
    }) if object == target && id.get() == 0)
    );
    assert_eq!(execution.cell(1, 20).unwrap().foreground().value(), None);
    assert_eq!(
        execution
            .cell(1, 16)
            .unwrap()
            .background()
            .value()
            .unwrap()
            .get(),
        0
    );
}

#[test]
fn legal_noncell_setup_does_not_create_pending_but_real_header_column_targets_do() {
    let target = local(0xffff);
    let setup = service(
        vec![
            TeletextEnhancementCommand::SetScreenColour {
                colour: TeletextColourReference::BLACK,
            },
            invoke(target, TeletextObjectType::Active, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![set(1, 0), foreground(8, 13), glyph(8, b'A')],
        )],
    );
    let execution = execute(&setup, &base(), 128).unwrap();
    assert!(execution
        .pending()
        .iter()
        .all(|pending| pending.reason() != TeletextAttributePendingReason::Unaddressable));
    assert_eq!(foreground_value(&execution, 1, 8), 13);

    let valid_header = service(
        vec![set(0, 8), invoke(target, TeletextObjectType::Active, None)],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![foreground(0, 13), glyph(0, b'A')],
        )],
    );
    let execution = execute(&valid_header, &base(), 128).unwrap();
    assert_eq!(foreground_value(&execution, 0, 8), 13);
    assert!(execution.pending().is_empty());

    let invalid_header = service(vec![foreground(0, 13)], vec![]);
    let execution = execute(&invalid_header, &base(), 128).unwrap();
    assert!(execution.pending().iter().any(|pending| pending.reason()
        == TeletextAttributePendingReason::Unaddressable
        && pending.position() == (0, 0)
        && pending.location()
            == TeletextEnhancementSource {
                object: None,
                command: 0
            }));
}

#[test]
fn adaptive_nonglyph_column_operation_establishes_a_firm_final_endpoint() {
    let target = local(0xffff);
    let service = service(
        vec![
            set(1, 0),
            invoke(target, TeletextObjectType::Adaptive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Adaptive,
            vec![
                foreground(5, 13),
                at(
                    15,
                    TeletextColumnOperation::Flash {
                        mode: TeletextFlashMode::Normal,
                        rate: TeletextFlashRate::Fast2HzPhase2,
                    },
                ),
            ],
        )],
    );
    let execution = execute(&service, &base(), 128).unwrap();
    for column in 5..=15 {
        assert_eq!(foreground_value(&execution, 1, column), 13);
    }
    assert_eq!(foreground_value(&execution, 1, 16), 4);
    assert_eq!(
        execution
            .cell(1, 15)
            .unwrap()
            .flash()
            .value()
            .unwrap()
            .mode(),
        TeletextFlashMode::Normal
    );
    assert_eq!(
        execution
            .cell(1, 16)
            .unwrap()
            .flash()
            .value()
            .unwrap()
            .mode(),
        TeletextFlashMode::Steady
    );
    assert!(execution
        .pending()
        .iter()
        .all(|pending| pending.reason() != TeletextAttributePendingReason::AdaptiveExtent));
}

#[test]
fn adaptive_uncertain_tail_retains_original_override_transmission_tie_chain() {
    let target = local(0xffff);
    let service = service(
        vec![
            set(1, 0),
            invoke(target, TeletextObjectType::Adaptive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Adaptive,
            vec![
                foreground(5, 13),
                foreground(5, 3),
                glyph(10, b'A'),
                set(0, 20),
            ],
        )],
    );
    let execution = execute(&service, &base(), 128).unwrap();
    let foreground = execution.cell(1, 11).unwrap().foreground();
    assert_eq!(foreground.value(), None);
    let tail = &execution.pending()[foreground.pending().unwrap()];
    assert_eq!(
        tail.reason(),
        TeletextAttributePendingReason::AdaptiveExtent
    );
    assert_eq!(
        tail.location(),
        TeletextEnhancementSource {
            object: Some(target),
            command: 3
        }
    );
    assert_eq!(
        tail.root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert_eq!(tail.invocation().unwrap().get(), 0);
    let tie = &execution.pending()[tail.previous().unwrap()];
    assert_eq!(
        tie.reason(),
        TeletextAttributePendingReason::TransmissionOrder
    );
    assert!(
        matches!(tie.first_source(), Some(TeletextAttributeSource::Enhancement {
        location: TeletextEnhancementSource { object: Some(object), command: 0 }, ..
    }) if object == target)
    );
    assert!(
        matches!(tie.second_source(), Some(TeletextAttributeSource::Enhancement {
        location: TeletextEnhancementSource { object: Some(object), command: 1 }, ..
    }) if object == target)
    );
    assert_eq!(
        tie.root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert_eq!(tie.invocation().unwrap().get(), 0);
}

#[test]
fn passive_g1_alpha_glyph_uses_private_colours_without_inheriting_local_attributes() {
    let target = local(0xffff);
    let alpha = TeletextColumnOperation::G1Alpha {
        code: TeletextG1AlphaCode::new(0x40).unwrap(),
    };
    let service = service(
        vec![
            set(1, 5),
            foreground(5, 13),
            at(
                5,
                TeletextColumnOperation::Background {
                    colour: TeletextColourReference::new(6).unwrap(),
                },
            ),
            invoke(target, TeletextObjectType::Passive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![at(0, alpha), foreground(1, 3), at(1, alpha)],
        )],
    );
    let execution = execute(&service, &base(), 128).unwrap();
    let first = execution.cell(1, 5).unwrap();
    assert_eq!(first.foreground().value().unwrap().get(), 7);
    assert_eq!(first.background().value().unwrap().get(), 0);
    assert!(
        matches!(first.foreground().source(), Some(TeletextAttributeSource::PassiveDefault { invocation }) if invocation.get() == 0)
    );
    let second = execution.cell(1, 6).unwrap();
    assert_eq!(second.foreground().value().unwrap().get(), 3);
    assert_eq!(second.background().value().unwrap().get(), 0);
    assert!(
        matches!(second.foreground().source(), Some(TeletextAttributeSource::Enhancement {
        location: TeletextEnhancementSource { object: Some(object), command: 1 },
        invocation: Some(id), ..
    }) if object == target && id.get() == 0)
    );
    let gap = execution.cell(1, 7).unwrap();
    assert_eq!(gap.foreground().value().unwrap().get(), 13);
    assert_eq!(gap.background().value().unwrap().get(), 6);
}

fn update_meter_inputs(
    service: &TeletextSemanticService,
) -> (
    TeletextBaseDisplay,
    TeletextSpatialTrace,
    TeletextGlyphLayers,
) {
    let base = base();
    let original = resolve_teletext_spatial_trace(
        service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 4096,
            max_invocations: 32,
        },
    )
    .unwrap();
    let glyphs = build_teletext_glyph_layers(&base, &original, 4096).unwrap();
    (base, original, glyphs)
}

fn update_meter_execution(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    glyphs: &TeletextGlyphLayers,
    transmission: Option<&TeletextAttributeTransmissionEvidence>,
    maximum: Option<usize>,
) -> (
    Result<TeletextAttributeExecution, TeletextAttributeExecutionError>,
    usize,
) {
    let rows = [TeletextColourReference::BLACK; 25];
    match transmission {
        None => resolve_teletext_attribute_execution_with_usage(
            base, original, glyphs, &rows, 4096, maximum,
        ),
        Some(evidence) => resolve_teletext_attribute_execution_with_transmission_and_usage(
            base, original, glyphs, &rows, 4096, evidence, maximum,
        ),
    }
}

fn update_meter_legacy_execution(
    base: &TeletextBaseDisplay,
    original: &TeletextSpatialTrace,
    glyphs: &TeletextGlyphLayers,
    transmission: Option<&TeletextAttributeTransmissionEvidence>,
) -> TeletextAttributeExecution {
    let rows = [TeletextColourReference::BLACK; 25];
    match transmission {
        None => resolve_teletext_attribute_execution(base, original, glyphs, &rows, 4096),
        Some(evidence) => resolve_teletext_attribute_execution_with_transmission(
            base, original, glyphs, &rows, 4096, evidence,
        ),
    }
    .unwrap()
}

#[test]
fn attribute_update_exact_and_one_less_limits_preserve_all_object_models_and_provenance() {
    let target = local(0xffff);
    for (object_type, expected_updates, denied_command) in [
        (TeletextObjectType::Active, 32, 1),
        (TeletextObjectType::Adaptive, 2, 2),
        (TeletextObjectType::Passive, 1, 2),
    ] {
        let service = service(
            vec![set(1, 0), invoke(target, object_type, None)],
            vec![definition(
                target,
                object_type,
                vec![set(0, 8), foreground(8, 13), glyph(8, b'A')],
            )],
        );
        let (base, original, glyphs) = update_meter_inputs(&service);
        let positions = original
            .events()
            .iter()
            .enumerate()
            .map(|(ordinal, event)| TeletextAttributeTransmissionPosition {
                event_ordinal: ordinal,
                source: event.source(),
                invocation: event.invocation(),
                word: ordinal,
            })
            .collect::<Vec<_>>();
        let evidence =
            TeletextAttributeTransmissionEvidence::new(&original, &positions, 4096).unwrap();
        let saved = (
            base.clone(),
            original.clone(),
            glyphs.clone(),
            evidence.clone(),
        );
        for transmission in [None, Some(&evidence)] {
            let expected = update_meter_legacy_execution(&base, &original, &glyphs, transmission);
            for maximum in [None, Some(expected_updates), Some(usize::MAX)] {
                let (result, admitted) =
                    update_meter_execution(&base, &original, &glyphs, transmission, maximum);
                assert_eq!(result.unwrap(), expected);
                assert_eq!(admitted, expected_updates);
            }
            let (result, admitted) = update_meter_execution(
                &base,
                &original,
                &glyphs,
                transmission,
                Some(expected_updates - 1),
            );
            let error = result.unwrap_err();
            assert_eq!(admitted, expected_updates - 1);
            assert_eq!(error.owner, owner());
            assert_eq!(error.reason, TeletextAttributeExecutionFailure::WorkBudget);
            assert_eq!(
                error.location,
                Some(TeletextEnhancementSource {
                    object: Some(target),
                    command: denied_command
                })
            );
            assert_eq!(
                error.root_source,
                Some(TeletextSpatialRootSource::Local { command: 1 })
            );
            assert_eq!(
                error.invocation.map(TeletextSpatialInvocationId::get),
                Some(0)
            );
        }
        assert_eq!((base, original, glyphs, evidence), saved);
    }
}

#[test]
fn zero_attribute_update_credit_preserves_empty_and_glyph_only_execution() {
    for main in [vec![], vec![set(1, 8), glyph(8, b'A')]] {
        let service = service(main, vec![]);
        let (base, original, glyphs) = update_meter_inputs(&service);
        let evidence = TeletextAttributeTransmissionEvidence::new(&original, &[], 4096).unwrap();
        let expected = execute(&service, &base, 4096).unwrap();
        let (result, admitted) = update_meter_execution(&base, &original, &glyphs, None, Some(0));
        assert_eq!(admitted, 0);
        assert_eq!(result.unwrap(), expected);
        let expected = resolve_teletext_attribute_execution_with_transmission(
            &base,
            &original,
            &glyphs,
            &[TeletextColourReference::BLACK; 25],
            4096,
            &evidence,
        )
        .unwrap();
        let (result, admitted) =
            update_meter_execution(&base, &original, &glyphs, Some(&evidence), Some(0));
        assert_eq!(admitted, 0);
        assert_eq!(result.unwrap(), expected);
    }
}

#[test]
fn attribute_update_usage_is_zero_before_command_or_transmission_admission() {
    let service = service(vec![set(1, 8), foreground(8, 13)], vec![]);
    let (base, original, glyphs) = update_meter_inputs(&service);
    let rows = [TeletextColourReference::BLACK; 25];
    let saved = (base.clone(), original.clone(), glyphs.clone());
    let expected =
        resolve_teletext_attribute_execution(&base, &original, &glyphs, &rows, 1).unwrap_err();
    let (result, admitted) = resolve_teletext_attribute_execution_with_usage(
        &base,
        &original,
        &glyphs,
        &rows,
        1,
        Some(0),
    );
    assert_eq!(result.unwrap_err(), expected);
    assert_eq!(admitted, 0);
    assert_eq!(
        expected.reason,
        TeletextAttributeExecutionFailure::CommandBudget { maximum: 1 }
    );
    let other_service = self::service(vec![], vec![]);
    let (_, other, _) = update_meter_inputs(&other_service);
    let foreign = TeletextAttributeTransmissionEvidence::new(&other, &[], 4096).unwrap();
    let expected = resolve_teletext_attribute_execution_with_transmission(
        &base, &original, &glyphs, &rows, 4096, &foreign,
    )
    .unwrap_err();
    let (result, admitted) = resolve_teletext_attribute_execution_with_transmission_and_usage(
        &base,
        &original,
        &glyphs,
        &rows,
        4096,
        &foreign,
        Some(0),
    );
    assert_eq!(result.unwrap_err(), expected);
    assert_eq!(admitted, 0);
    assert_eq!(
        expected.reason,
        TeletextAttributeExecutionFailure::TransmissionMismatch
    );
    assert_eq!((base, original, glyphs), saved);
}

#[test]
fn attribute_update_usage_preserves_real_transmission_tie_settlement() {
    let service = service(
        vec![
            set(1, 18),
            foreground(18, 4),
            foreground(18, 13),
            glyph(18, b'A'),
        ],
        vec![],
    );
    let (base, original, glyphs) = update_meter_inputs(&service);
    let positions = [1, 2].map(|ordinal| {
        let event = original.events()[ordinal];
        TeletextAttributeTransmissionPosition {
            event_ordinal: ordinal,
            source: event.source(),
            invocation: event.invocation(),
            word: ordinal,
        }
    });
    let evidence = TeletextAttributeTransmissionEvidence::new(&original, &positions, 4096).unwrap();
    let (unsettled, admitted) = update_meter_execution(&base, &original, &glyphs, None, Some(44));
    let unsettled = unsettled.unwrap();
    assert_eq!(admitted, 44);
    assert_eq!(unsettled.cell(1, 18).unwrap().foreground().value(), None);
    assert!(!unsettled.pending().is_empty());
    let expected = update_meter_legacy_execution(&base, &original, &glyphs, Some(&evidence));
    let (settled, admitted) =
        update_meter_execution(&base, &original, &glyphs, Some(&evidence), Some(44));
    let settled = settled.unwrap();
    assert_eq!(admitted, 44);
    assert_eq!(settled, expected);
    assert_eq!(foreground_value(&settled, 1, 18), 13);
    assert!(settled.pending().is_empty());
    for transmission in [None, Some(&evidence)] {
        let (result, admitted) =
            update_meter_execution(&base, &original, &glyphs, transmission, Some(43));
        let error = result.unwrap_err();
        assert_eq!(admitted, 43);
        assert_eq!(error.reason, TeletextAttributeExecutionFailure::WorkBudget);
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: None,
                command: 2
            })
        );
        assert_eq!(
            error.root_source,
            Some(TeletextSpatialRootSource::Local { command: 2 })
        );
        assert_eq!(error.invocation, None);
    }
}

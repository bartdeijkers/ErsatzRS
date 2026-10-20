//! Original repertoire-context oracles; no Unicode/font/rendering acceptance.

use super::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn set(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn designate(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
        },
    )
}
fn g0(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}
fn g2(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}
fn invoke(
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
fn definition(
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
fn presentation(primary: u8, secondary: Option<u8>) -> TeletextLevel25PresentationDefaults {
    TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(primary).unwrap(),
        secondary: secondary.map(|code| TeletextPageCharsetDesignation::new(code).unwrap()),
        palette: TeletextLevel25Palette::standard(),
        screen_colour: TeletextColourReference::BLACK,
        row_colour: TeletextColourReference::BLACK,
        black_background_substitution: false,
        remapping: TeletextColourRemapping::FgClut0BgClut0,
        panels: TeletextSidePanels {
            left: false,
            right: false,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::ZERO,
        },
    }
}

fn base(presentation: TeletextLevel25PresentationDefaults) -> TeletextBaseDisplay {
    let rows: Vec<[TeletextBaseDisplayCell; 40]> = (0..25)
        .map(|row| {
            let source = TeletextBaseAttributeSource::RowDefault {
                row,
                start_column: if row == 0 { 8 } else { 0 },
            };
            let attributes = TeletextBaseDisplayAttributes::new(
                TeletextBaseAttributeValues {
                    raw_foreground: 7,
                    raw_background: 0,
                    foreground: TeletextColourReference::new(7).unwrap(),
                    background: TeletextColourReference::BLACK,
                    graphics: false,
                    separated: false,
                    double_height: false,
                    double_width: false,
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
                    width: source,
                    boxed: source,
                    flash: source,
                    conceal: source,
                },
            )
            .unwrap();
            let cells: Vec<_> = (0..40)
                .map(|column| {
                    let header = row == 0 && column < 8;
                    let code = if row == 1 && column == 8 { 0x23 } else { b' ' };
                    let kind = if header {
                        TeletextBaseDisplayCellKind::HeaderOwned
                    } else if code == b' ' {
                        TeletextBaseDisplayCellKind::Blank
                    } else {
                        TeletextBaseDisplayCellKind::Glyph
                    };
                    TeletextBaseDisplayCell::new(
                        TeletextEnhancementPosition::new(row, column).unwrap(),
                        code,
                        kind,
                        None,
                        attributes,
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
        presentation.primary,
        TeletextNationalOption::new(0).unwrap(),
        Some(presentation),
        rows,
    )
    .unwrap()
}

struct Fixture {
    service: TeletextSemanticService,
    base: TeletextBaseDisplay,
    spatial: TeletextSpatialTrace,
    layers: TeletextGlyphLayers,
}
fn fixture(
    main: Vec<TeletextEnhancementCommand>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    primary: u8,
    secondary: Option<u8>,
    with_drcs: bool,
) -> Fixture {
    let presentation = presentation(primary, secondary);
    let language = TeletextLanguage::new("nld".into()).unwrap();
    let drcs_resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Gdrcs,
        TeletextRawDataPageNumber::new(2, 0xb1).unwrap(),
        TeletextResourceSubpageIndex::new(9).unwrap(),
    )
    .unwrap();
    let drcs = if with_drcs {
        vec![TeletextDrcsSubtable::new(
            drcs_resource,
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
                bitmap: TeletextDrcsBitmap::new([1; 10]).unwrap(),
            }],
        )
        .unwrap()]
    } else {
        vec![]
    };
    let service = TeletextSemanticService::new(
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
                presentation: Some(presentation),
                compositional_links: None,
                local_program: Some(TeletextEnhancementProgram::new(main).unwrap()),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    defaults: [None; 4],
                    drcs: TeletextDrcsPageLinks::new(None, with_drcs.then(|| drcs_resource.page()))
                        .unwrap(),
                },
            }],
            magazines: vec![],
            objects,
            drcs,
        },
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 4096,
            max_collected_commands: 4096,
            max_collected_drcs_references: 128,
        },
    )
    .unwrap();
    let base = base(presentation);
    let spatial = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 128,
            max_invocations: 16,
        },
    )
    .unwrap();
    let layers = build_teletext_glyph_layers(&base, &spatial, 128).unwrap();
    Fixture {
        service,
        base,
        spatial,
        layers,
    }
}

fn execute(fixture: &Fixture, maximum: usize) -> TeletextCharsetExecution {
    resolve_teletext_charset_execution(&fixture.base, &fixture.spatial, &fixture.layers, maximum)
        .unwrap()
}
fn record(
    execution: &TeletextCharsetExecution,
    object: Option<TeletextObjectIdentity>,
    command: usize,
    instance: Option<usize>,
) -> &TeletextExecutedGlyphCharset {
    execution
        .records()
        .iter()
        .find(|record| {
            record.candidate().source() == Some(TeletextEnhancementSource { object, command })
                && record
                    .candidate()
                    .invocation()
                    .map(TeletextSpatialInvocationId::get)
                    == instance
        })
        .unwrap()
}
fn selection(record: &TeletextExecutedGlyphCharset) -> TeletextCharsetSelection {
    record.selection().unwrap().value().unwrap()
}

fn replace_base_codes(
    fixture: &mut Fixture,
    presentation: Option<TeletextLevel25PresentationDefaults>,
    replacements: &[(u8, u8, u8)],
) {
    let mut rows = fixture.base.cells().to_vec();
    for &(row, column, code) in replacements {
        let original = rows[usize::from(row)][usize::from(column)];
        let kind = if code == 0x1b {
            TeletextBaseDisplayCellKind::SpacingControl {
                control: TeletextBaseSpacingControl::Escape,
                timing: TeletextSpacingTiming::SetAfter,
            }
        } else {
            TeletextBaseDisplayCellKind::Glyph
        };
        rows[usize::from(row)][usize::from(column)] = TeletextBaseDisplayCell::new(
            original.position(),
            code,
            kind,
            None,
            original.attributes(),
        )
        .unwrap();
    }
    fixture.base = TeletextBaseDisplay::new(
        owner(),
        fixture.base.primary(),
        fixture.base.national_option(),
        presentation,
        rows.into_boxed_slice().try_into().unwrap(),
    )
    .unwrap();
    fixture.layers = build_teletext_glyph_layers(&fixture.base, &fixture.spatial, 128).unwrap();
}

fn base_record(
    execution: &TeletextCharsetExecution,
    position: (u16, u16),
) -> &TeletextExecutedGlyphCharset {
    execution
        .records()
        .iter()
        .find(|record| {
            record.candidate().base().is_some() && record.candidate().position() == position
        })
        .unwrap()
}

#[test]
fn base_escape_keeps_set_after_sources_and_resets_each_row() {
    let mut fixture = fixture(vec![], vec![], 0x40, Some(0x37), false);
    let presentation = fixture.base.presentation();
    replace_base_codes(
        &mut fixture,
        presentation,
        &[(0, 8, 0x23), (1, 7, 0x1b), (1, 10, 0x1b), (1, 39, 0x1b)],
    );
    let execution = execute(&fixture, 128);
    for (position, designation, origin) in [
        ((1, 7), 0x40, TeletextBaseG0Origin::RowPrimary { row: 1 }),
        (
            (1, 8),
            0x37,
            TeletextBaseG0Origin::EscapeSecondary {
                position: TeletextEnhancementPosition::new(1, 7).unwrap(),
            },
        ),
        (
            (1, 10),
            0x37,
            TeletextBaseG0Origin::EscapeSecondary {
                position: TeletextEnhancementPosition::new(1, 7).unwrap(),
            },
        ),
        (
            (1, 11),
            0x40,
            TeletextBaseG0Origin::EscapePrimary {
                position: TeletextEnhancementPosition::new(1, 10).unwrap(),
            },
        ),
        (
            (1, 39),
            0x40,
            TeletextBaseG0Origin::EscapePrimary {
                position: TeletextEnhancementPosition::new(1, 10).unwrap(),
            },
        ),
        ((2, 0), 0x40, TeletextBaseG0Origin::RowPrimary { row: 2 }),
    ] {
        let record = base_record(&execution, position);
        let selected = record.base_g0().unwrap();
        assert_eq!(selected.designation().get(), designation);
        assert_eq!(selected.origin(), origin);
        assert_eq!(
            selection(record),
            TeletextCharsetSelection::PagePrimary(execution.primary())
        );
    }
    let escape = base_record(&execution, (1, 7)).candidate().base().unwrap();
    assert_eq!(escape.code(), 0x1b);
    assert_eq!(
        escape.kind(),
        TeletextBaseDisplayCellKind::SpacingControl {
            control: TeletextBaseSpacingControl::Escape,
            timing: TeletextSpacingTiming::SetAfter,
        }
    );
    assert!(!base_record(&execution, (0, 8)).national_substitution());
    assert!(base_record(&execution, (1, 8)).national_substitution());
}

#[test]
fn base_escape_explicit_absence_is_noop_but_missing_presentation_rejects() {
    for (row, column) in [(0, 8), (1, 7)] {
        let mut fixture = fixture(vec![], vec![], 0, None, false);
        let presentation = fixture.base.presentation();
        replace_base_codes(&mut fixture, presentation, &[(row, column, 0x1b)]);
        let execution = execute(&fixture, 128);
        for current in [column, column + 1] {
            let selected = base_record(&execution, (u16::from(row), u16::from(current)))
                .base_g0()
                .unwrap();
            assert_eq!(selected.designation(), execution.primary());
            assert_eq!(selected.origin(), TeletextBaseG0Origin::RowPrimary { row });
        }
        replace_base_codes(&mut fixture, None, &[]);
        let before = (
            fixture.base.clone(),
            fixture.spatial.clone(),
            fixture.layers.clone(),
        );
        let error = resolve_teletext_charset_execution(
            &fixture.base,
            &fixture.spatial,
            &fixture.layers,
            128,
        )
        .unwrap_err();
        assert_eq!(error.owner, owner());
        assert_eq!(
            error.reason,
            TeletextCharsetExecutionFailure::DecoderDependentBaseG0 { row, column }
        );
        assert_eq!(error.location, None);
        assert_eq!(error.root_source, None);
        assert_eq!(error.invocation, None);
        assert_eq!(before, (fixture.base, fixture.spatial, fixture.layers));
    }
}

#[test]
fn base_escape_preserves_enhancement_g2_sources_and_pending_order() {
    let target = local(0xffff);
    let mut fixture = fixture(
        vec![
            set(1, 0),
            g2(1, 0x23),
            designate(5, 0x40),
            set(1, 5),
            invoke(target, TeletextObjectType::Active, None),
            g2(10, 0x23),
        ],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![designate(0, 0x37)],
        )],
        0x40,
        Some(0x37),
        false,
    );
    let original = execute(&fixture, 128);
    let spatial = fixture.spatial.clone();
    let presentation = fixture.base.presentation();
    replace_base_codes(&mut fixture, presentation, &[(1, 0, 0x1b)]);
    let execution = execute(&fixture, 128);
    assert_eq!(fixture.spatial, spatial);
    assert_eq!(execution.declarations(), original.declarations());
    assert_eq!(execution.pending(), original.pending());
    assert!(!execution.pending().is_empty());
    for index in 0..execution.pending().len() {
        assert_eq!(
            execution.pending_contenders(index).collect::<Vec<_>>(),
            original.pending_contenders(index).collect::<Vec<_>>()
        );
    }
    let enhanced = |execution: &TeletextCharsetExecution| {
        execution
            .records()
            .iter()
            .filter(|record| record.candidate().source().is_some())
            .copied()
            .collect::<Vec<_>>()
    };
    assert_eq!(enhanced(&execution), enhanced(&original));
    let known = record(&execution, None, 1, None);
    assert_eq!(known.usage(), TeletextGlyphCharsetUsage::G2);
    assert_eq!(selection(known).pair(), TeletextCharsetPair::LatinArabic);
    assert_eq!(known.base_g0(), None);
    let unresolved = record(&execution, None, 5, None);
    assert_eq!(unresolved.usage(), TeletextGlyphCharsetUsage::G2);
    assert_eq!(unresolved.selection().unwrap().value(), None);
    assert!(unresolved.selection().unwrap().pending().is_some());
    assert_eq!(unresolved.base_g0(), None);
    assert_eq!(
        base_record(&execution, (1, 1))
            .base_g0()
            .unwrap()
            .designation()
            .get(),
        0x37
    );
}

#[test]
fn shared_absolute_row_designations_follow_spatial_order_not_recursive_event_preorder() {
    let target = local(0xffff);
    let fixture = fixture(
        vec![
            set(1, 0),
            designate(5, 0x37),
            invoke(target, TeletextObjectType::Active, Some((0, 3))),
            designate(10, 0),
        ],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![g0(12, b'A')],
        )],
        0,
        None,
        false,
    );
    assert!(
        fixture
            .spatial
            .events()
            .iter()
            .position(|event| event.source().object == Some(target))
            .unwrap()
            < fixture
                .spatial
                .events()
                .iter()
                .position(|event| event.source().object.is_none() && event.source().command == 3)
                .unwrap()
    );
    let execution = execute(&fixture, 128);
    let glyph = record(&execution, Some(target), 0, Some(0));
    assert_eq!(glyph.candidate().position(), (1, 20));
    assert_eq!(
        selection(glyph),
        TeletextCharsetSelection::Enhancement(
            TeletextEnhancementCharsetDesignation::new(0).unwrap()
        )
    );
    assert_eq!(selection(glyph).pair(), TeletextCharsetPair::LatinLatin);
    let writer = &execution.declarations()[glyph.selection().unwrap().declaration().unwrap()];
    assert_eq!(
        writer.source(),
        TeletextEnhancementSource {
            object: None,
            command: 3
        }
    );
    assert_eq!(
        writer.root_source(),
        TeletextSpatialRootSource::Local { command: 3 }
    );
    assert_eq!(writer.position(), (1, 10));
    assert_eq!(writer.invocation(), None);
}

#[test]
fn active_designation_affects_later_main_glyphs_and_resets_at_absolute_row_end() {
    let target = local(0xffff);
    let fixture = fixture(
        vec![
            set(1, 8),
            invoke(target, TeletextObjectType::Active, None),
            set(1, 12),
            g0(12, b'A'),
            set(2, 12),
            g0(12, b'A'),
        ],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![designate(2, 0x37)],
        )],
        0,
        None,
        false,
    );
    let execution = execute(&fixture, 128);
    assert_eq!(
        selection(record(&execution, None, 3, None)).pair(),
        TeletextCharsetPair::GreekGreek
    );
    assert_eq!(
        selection(record(&execution, None, 5, None)),
        TeletextCharsetSelection::PagePrimary(TeletextPageCharsetDesignation::new(0).unwrap())
    );
    let declaration = &execution.declarations()[0];
    assert_eq!(declaration.position(), (1, 10));
    assert_eq!(
        declaration.source(),
        TeletextEnhancementSource {
            object: Some(target),
            command: 0
        }
    );
    assert_eq!(
        declaration.root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert_eq!(declaration.invocation().unwrap().get(), 0);
}

#[test]
fn adaptive_and_passive_instances_start_primary_keep_private_rows_and_cannot_leak() {
    for object_type in [TeletextObjectType::Adaptive, TeletextObjectType::Passive] {
        let target = local(0xffff);
        let fixture = fixture(
            vec![
                set(1, 8),
                designate(8, 0x37),
                invoke(target, object_type, None),
                g0(16, b'P'),
                set(3, 8),
                invoke(target, object_type, None),
                g0(16, b'P'),
            ],
            vec![definition(
                target,
                object_type,
                vec![
                    g0(0, b'A'),
                    designate(5, 0x37),
                    g0(6, b'B'),
                    set(1, 0),
                    g0(0, b'C'),
                ],
            )],
            0,
            None,
            false,
        );
        let execution = execute(&fixture, 128);
        for instance in [0, 1] {
            assert_eq!(
                selection(record(&execution, Some(target), 0, Some(instance))),
                TeletextCharsetSelection::PagePrimary(
                    TeletextPageCharsetDesignation::new(0).unwrap()
                )
            );
            for command in [2, 4] {
                assert_eq!(
                    selection(record(&execution, Some(target), command, Some(instance))).pair(),
                    TeletextCharsetPair::GreekGreek
                );
            }
        }
        assert_eq!(
            selection(record(&execution, None, 3, None)).pair(),
            TeletextCharsetPair::GreekGreek
        );
        assert_eq!(
            selection(record(&execution, None, 6, None)).pair(),
            TeletextCharsetPair::LatinLatin
        );
    }
}

#[test]
fn same_coordinate_private_sequence_preserves_each_losing_glyphs_own_context() {
    let target = local(0xffff);
    let fixture = fixture(
        vec![set(1, 8), invoke(target, TeletextObjectType::Passive, None)],
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![
                designate(0, 0x37),
                g0(0, b'A'),
                designate(0, 0),
                g0(0, b'B'),
            ],
        )],
        0,
        None,
        false,
    );
    let execution = execute(&fixture, 128);
    let first = record(&execution, Some(target), 1, Some(0));
    let last = record(&execution, Some(target), 3, Some(0));
    assert_eq!(first.candidate().position(), (1, 8));
    assert_eq!(last.candidate().position(), (1, 8));
    assert_eq!(selection(first).pair(), TeletextCharsetPair::GreekGreek);
    assert_eq!(selection(last).pair(), TeletextCharsetPair::LatinLatin);
    assert_eq!(execution.records().len(), fixture.layers.candidates().len());
    for (record, candidate) in execution.records().iter().zip(fixture.layers.candidates()) {
        assert_eq!(record.candidate(), candidate);
    }
}

#[test]
fn secondary_g0_does_not_designate_g2_or_invent_a_base_escape_switch() {
    let fixture = fixture(
        vec![set(1, 8), g2(8, 0x23)],
        vec![],
        0x40,
        Some(0x37),
        false,
    );
    let execution = execute(&fixture, 128);
    assert_eq!(execution.primary().get(), 0x40);
    assert_eq!(execution.secondary().unwrap().get(), 0x37);
    assert_eq!(execution.national_option().get(), 0);
    let enhanced = record(&execution, None, 1, None);
    assert_eq!(enhanced.usage(), TeletextGlyphCharsetUsage::G2);
    assert_eq!(selection(enhanced).pair(), TeletextCharsetPair::LatinArabic);
    assert!(!enhanced.national_substitution());
    let coded = execution
        .records()
        .iter()
        .find(|record| {
            record.candidate().base().is_some() && record.candidate().position() == (1, 8)
        })
        .unwrap();
    assert_eq!(coded.usage(), TeletextGlyphCharsetUsage::BaseCoded);
    assert_eq!(
        selection(coded),
        TeletextCharsetSelection::PagePrimary(TeletextPageCharsetDesignation::new(0x40).unwrap())
    );
    assert!(coded.national_substitution());
    assert!(execution.declarations().is_empty());
}

#[test]
fn latin_alias_and_g1_alpha_preserve_exact_designation_without_national_substitution() {
    let fixture = fixture(
        vec![
            set(1, 8),
            designate(8, 0x1f),
            at(
                9,
                TeletextColumnOperation::G1Alpha {
                    code: TeletextG1AlphaCode::new(0x40).unwrap(),
                },
            ),
            designate(10, 0x37),
            at(
                11,
                TeletextColumnOperation::G1Alpha {
                    code: TeletextG1AlphaCode::new(0x5f).unwrap(),
                },
            ),
        ],
        vec![],
        1,
        None,
        false,
    );
    let execution = execute(&fixture, 128);
    let alpha = record(&execution, None, 2, None);
    assert_eq!(alpha.usage(), TeletextGlyphCharsetUsage::G1Alpha);
    assert_eq!(
        selection(alpha),
        TeletextCharsetSelection::Enhancement(
            TeletextEnhancementCharsetDesignation::new(0x1f).unwrap()
        )
    );
    assert_eq!(selection(alpha).pair(), TeletextCharsetPair::LatinLatin);
    assert!(!alpha.national_substitution());
    assert_eq!(
        alpha.candidate().operation(),
        Some(TeletextColumnOperation::G1Alpha {
            code: TeletextG1AlphaCode::new(0x40).unwrap()
        })
    );
    let greek = record(&execution, None, 4, None);
    assert_eq!(greek.usage(), TeletextGlyphCharsetUsage::G1Alpha);
    assert_eq!(selection(greek).pair(), TeletextCharsetPair::GreekGreek);
    assert!(!greek.national_substitution());
}

#[test]
fn diacritic_usage_keeps_current_pair_and_only_zero_asterisk_selects_special_at() {
    let operations = [
        TeletextColumnOperation::G0Diacritic {
            code: TeletextGlyphCode::new(0x2a).unwrap(),
            diacritic: TeletextDiacriticIndex::new(0).unwrap(),
        },
        TeletextColumnOperation::G0Diacritic {
            code: TeletextGlyphCode::new(0x41).unwrap(),
            diacritic: TeletextDiacriticIndex::new(0).unwrap(),
        },
        TeletextColumnOperation::G0Diacritic {
            code: TeletextGlyphCode::new(0x41).unwrap(),
            diacritic: TeletextDiacriticIndex::new(15).unwrap(),
        },
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(0x2a).unwrap(),
        },
    ];
    let mut commands = vec![set(1, 8), designate(8, 0x37)];
    commands.extend(
        (9..)
            .zip(operations)
            .map(|(column, operation)| at(column, operation)),
    );
    let fixture = fixture(commands, vec![], 0, None, false);
    let execution = execute(&fixture, 128);
    for (index, operation) in operations.into_iter().enumerate() {
        let glyph = record(&execution, None, index + 2, None);
        assert_eq!(selection(glyph).pair(), TeletextCharsetPair::GreekGreek);
        assert_eq!(glyph.candidate().operation(), Some(operation));
        assert!(!glyph.national_substitution());
        let expected = match index {
            0 => TeletextGlyphCharsetUsage::G0Diacritic {
                diacritic: TeletextDiacriticIndex::new(0).unwrap(),
                special_at: true,
            },
            1 => TeletextGlyphCharsetUsage::G0Diacritic {
                diacritic: TeletextDiacriticIndex::new(0).unwrap(),
                special_at: false,
            },
            2 => TeletextGlyphCharsetUsage::G0Diacritic {
                diacritic: TeletextDiacriticIndex::new(15).unwrap(),
                special_at: false,
            },
            _ => TeletextGlyphCharsetUsage::G0,
        };
        assert_eq!(glyph.usage(), expected);
    }
}

#[test]
fn mosaic_g3_and_drcs_keep_fixed_raw_usage_without_a_fictitious_charset_selection() {
    let reference = TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Global,
        subtable: TeletextDrcsSubtableIndex::new(9).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
    };
    let operations = [
        TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(0x21).unwrap(),
        },
        TeletextColumnOperation::G3 {
            code: TeletextGlyphCode::new(0x40).unwrap(),
            compatibility: TeletextG3Compatibility::Level25,
        },
        TeletextColumnOperation::Drcs { reference },
    ];
    let mut commands = vec![set(1, 8), designate(8, 0x37)];
    commands.extend(
        (9..)
            .zip(operations)
            .map(|(column, operation)| at(column, operation)),
    );
    let fixture = fixture(commands, vec![], 0, None, true);
    let execution = execute(&fixture, 128);
    for (index, usage) in [
        TeletextGlyphCharsetUsage::G1Mosaic,
        TeletextGlyphCharsetUsage::G3,
        TeletextGlyphCharsetUsage::Drcs,
    ]
    .into_iter()
    .enumerate()
    {
        let glyph = record(&execution, None, index + 2, None);
        assert_eq!(glyph.usage(), usage);
        assert_eq!(glyph.selection(), None);
        assert_eq!(glyph.candidate().operation(), Some(operations[index]));
    }
}

#[test]
fn cross_source_equal_coordinate_designations_remain_pending_with_full_identity() {
    let target = local(0xffff);
    let fixture = fixture(
        vec![
            set(1, 5),
            designate(5, 0),
            invoke(target, TeletextObjectType::Active, None),
            g0(10, b'A'),
        ],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![designate(0, 0x37)],
        )],
        0,
        None,
        false,
    );
    let execution = execute(&fixture, 128);
    let choice = record(&execution, None, 3, None).selection().unwrap();
    assert_eq!(choice.value(), None);
    let pending = choice.pending().unwrap();
    assert_eq!(execution.pending()[pending].position(), (1, 5));
    let contenders: Vec<_> = execution.pending_contenders(pending).collect();
    assert_eq!(contenders.len(), 2);
    assert!(contenders.iter().any(|declaration| declaration.source()
        == TeletextEnhancementSource {
            object: None,
            command: 1
        }
        && declaration.root_source() == TeletextSpatialRootSource::Local { command: 1 }
        && declaration.invocation().is_none()));
    assert!(contenders.iter().any(|declaration| declaration.source()
        == TeletextEnhancementSource {
            object: Some(target),
            command: 0
        }
        && declaration.root_source() == TeletextSpatialRootSource::Local { command: 2 }
        && declaration.invocation().unwrap().get() == 0));
    assert!(contenders
        .iter()
        .any(|declaration| declaration.designation().get() == 0));
    assert!(contenders
        .iter()
        .any(|declaration| declaration.designation().get() == 0x37));
}

#[test]
fn private_repertoire_and_original_provenance_survive_wide_unplaced_coordinates() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let fixture = fixture(
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
                vec![set(24, 39), designate(39, 0x37), g0(39, b'A')],
            ),
        ],
        0,
        None,
        false,
    );
    let execution = execute(&fixture, 128);
    let glyph = record(&execution, Some(passive), 2, Some(2));
    assert_eq!(glyph.candidate().position(), (165, 369));
    assert_eq!(selection(glyph).pair(), TeletextCharsetPair::GreekGreek);
    assert_eq!(
        glyph.candidate().root_source(),
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
}

#[test]
fn exact_original_command_budget_retains_repeated_instance_blame_and_inputs() {
    let target = local(0xffff);
    let fixture = fixture(
        vec![
            set(1, 8),
            invoke(target, TeletextObjectType::Passive, None),
            set(2, 8),
            invoke(target, TeletextObjectType::Passive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![designate(0, 0x37), g0(1, b'A')],
        )],
        0,
        None,
        false,
    );
    let before = (
        fixture.service.clone(),
        fixture.base.clone(),
        fixture.spatial.clone(),
        fixture.layers.clone(),
    );
    assert_eq!(fixture.spatial.events().len(), 8);
    assert!(resolve_teletext_charset_execution(
        &fixture.base,
        &fixture.spatial,
        &fixture.layers,
        8
    )
    .is_ok());
    let error =
        resolve_teletext_charset_execution(&fixture.base, &fixture.spatial, &fixture.layers, 7)
            .unwrap_err();
    assert_eq!(error.owner, owner());
    assert_eq!(
        error.reason,
        TeletextCharsetExecutionFailure::CommandBudget {
            count: 8,
            maximum: 7
        }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 1
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 3 })
    );
    assert_eq!(error.invocation.unwrap().get(), 1);
    assert_eq!(
        before,
        (
            fixture.service,
            fixture.base,
            fixture.spatial,
            fixture.layers
        )
    );
}

#[test]
fn exact_owner_and_same_owner_candidate_event_correspondence_are_checked() {
    let original = fixture(vec![set(1, 8), g0(8, b'A')], vec![], 0, None, false);
    let mut foreign = owner();
    foreign.subcode = TeletextSubcode::new(0x1235).unwrap();
    let rows: Vec<_> = original.base.cells().to_vec();
    let foreign_base = TeletextBaseDisplay::new(
        foreign,
        original.base.primary(),
        original.base.national_option(),
        original.base.presentation(),
        rows.into_boxed_slice().try_into().unwrap(),
    )
    .unwrap();
    let error =
        resolve_teletext_charset_execution(&foreign_base, &original.spatial, &original.layers, 128)
            .unwrap_err();
    assert_eq!(error.owner, foreign);
    assert_eq!(
        error.reason,
        TeletextCharsetExecutionFailure::OwnerMismatch { original: owner() }
    );
    let other = fixture(vec![set(1, 8), g0(8, b'B')], vec![], 0, None, false);
    let error =
        resolve_teletext_charset_execution(&original.base, &original.spatial, &other.layers, 128)
            .unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextCharsetExecutionFailure::CandidateMismatch { .. }
    ));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(error.invocation, None);
}

#[test]
fn shared_own_sequence_at_one_coordinate_uses_only_earlier_own_designations() {
    let fixture = fixture(
        vec![
            set(1, 8),
            g0(8, b'A'),
            designate(8, 0x37),
            g0(8, b'B'),
            designate(8, 0),
            g0(8, b'C'),
        ],
        vec![],
        0,
        None,
        false,
    );
    let execution = execute(&fixture, 128);
    let before = record(&execution, None, 1, None);
    assert_eq!(
        selection(before),
        TeletextCharsetSelection::PagePrimary(TeletextPageCharsetDesignation::new(0).unwrap())
    );
    assert_eq!(before.selection().unwrap().declaration(), None);
    let greek = record(&execution, None, 3, None);
    let latin = record(&execution, None, 5, None);
    assert_eq!(selection(greek).pair(), TeletextCharsetPair::GreekGreek);
    assert_eq!(selection(latin).pair(), TeletextCharsetPair::LatinLatin);
    for (glyph, command) in [(greek, 2), (latin, 4)] {
        assert_eq!(glyph.candidate().position(), (1, 8));
        let writer = &execution.declarations()[glyph.selection().unwrap().declaration().unwrap()];
        assert_eq!(
            writer.source(),
            TeletextEnhancementSource {
                object: None,
                command
            }
        );
        assert_eq!(
            writer.root_source(),
            TeletextSpatialRootSource::Local { command }
        );
        assert_eq!(writer.invocation(), None);
    }
    assert_eq!(
        execution
            .records()
            .iter()
            .filter(|record| record.candidate().source().is_some()
                && record.candidate().position() == (1, 8))
            .count(),
        3
    );
}

#[test]
fn same_coordinate_cross_source_glyph_excludes_own_future_writer_and_keeps_prior_pending() {
    let earlier = local(0xffff);
    let later = local(0x0100);
    let fixture = fixture(
        vec![
            set(1, 5),
            designate(5, 0x37),
            invoke(earlier, TeletextObjectType::Active, None),
            set(1, 10),
            invoke(later, TeletextObjectType::Active, None),
            g0(10, b'A'),
            designate(10, 0),
        ],
        vec![
            definition(earlier, TeletextObjectType::Active, vec![designate(0, 0)]),
            definition(later, TeletextObjectType::Active, vec![designate(0, 0x37)]),
        ],
        0,
        None,
        false,
    );
    let execution = execute(&fixture, 128);
    let glyph = record(&execution, None, 5, None);
    let unresolved = glyph.selection().unwrap();
    assert_eq!(unresolved.value(), None);
    assert_eq!(unresolved.declaration(), None);
    let pending = unresolved.pending().unwrap();
    assert_eq!(execution.pending()[pending].position(), (1, 10));
    let contenders: Vec<_> = execution.pending_contenders(pending).collect();
    assert_eq!(contenders.len(), 1);
    assert_eq!(
        contenders[0].source(),
        TeletextEnhancementSource {
            object: Some(later),
            command: 0
        }
    );
    assert_eq!(
        contenders[0].root_source(),
        TeletextSpatialRootSource::Local { command: 4 }
    );
    assert_eq!(contenders[0].invocation().unwrap().get(), 1);
    assert_eq!(contenders[0].designation().get(), 0x37);
    // The future main declaration is retained globally, but cannot precede this glyph.
    assert!(execution.declarations().iter().any(|writer| writer.source()
        == TeletextEnhancementSource {
            object: None,
            command: 6
        }));
    let previous = execution.pending()[pending].previous();
    assert_eq!(previous.value(), None);
    let previous = previous.pending().unwrap();
    assert_eq!(execution.pending()[previous].position(), (1, 5));
    let contenders: Vec<_> = execution.pending_contenders(previous).collect();
    assert_eq!(contenders.len(), 2);
    assert!(contenders.iter().any(|writer| writer.source()
        == TeletextEnhancementSource {
            object: None,
            command: 1
        }
        && writer.root_source() == TeletextSpatialRootSource::Local { command: 1 }
        && writer.invocation().is_none()
        && writer.designation().get() == 0x37));
    assert!(contenders.iter().any(|writer| writer.source()
        == TeletextEnhancementSource {
            object: Some(earlier),
            command: 0
        }
        && writer.root_source() == TeletextSpatialRootSource::Local { command: 2 }
        && writer.invocation().unwrap().get() == 0
        && writer.designation().get() == 0));
}

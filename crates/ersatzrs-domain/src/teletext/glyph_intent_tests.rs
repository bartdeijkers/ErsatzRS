//! Primary-table identities and assignment evidence, not Unicode/font oracles.

use super::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn g0(code: u8) -> TeletextEnhancementCommand {
    at(
        8,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}
fn set(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn designate(code: u8) -> TeletextEnhancementCommand {
    at(
        8,
        TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
        },
    )
}
fn presentation(primary: u8) -> TeletextLevel25PresentationDefaults {
    TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(primary).unwrap(),
        secondary: Some(TeletextPageCharsetDesignation::new(0x37).unwrap()),
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

// Heap-backed checked cells: all96 raw G0 positions occupy rows1..3. Explicit
// presentation differs from header national6 and language nld deliberately.
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
            .unwrap();
            let cells: Vec<_> = (0..40)
                .map(|column| {
                    let position = TeletextEnhancementPosition::new(row, column).unwrap();
                    let index = if (1..=3).contains(&row) {
                        Some(usize::from(row - 1) * 40 + usize::from(column))
                    } else {
                        None
                    };
                    let (code, kind) = if row == 0 && column < 8 {
                        (b' ', TeletextBaseDisplayCellKind::HeaderOwned)
                    } else if let Some(index) = index.filter(|&index| index < 96) {
                        (
                            0x20 + u8::try_from(index).unwrap(),
                            TeletextBaseDisplayCellKind::Glyph,
                        )
                    } else if row == 4 && column == 0 {
                        (
                            0x0c,
                            TeletextBaseDisplayCellKind::SpacingControl {
                                control: TeletextBaseSpacingControl::NormalHeight,
                                timing: TeletextSpacingTiming::SetAt,
                            },
                        )
                    } else {
                        (b' ', TeletextBaseDisplayCellKind::Blank)
                    };
                    let authored = (code == b'e' && kind == TeletextBaseDisplayCellKind::Glyph)
                        .then(|| {
                            TeletextBaseAuthoredGlyph::new(
                                position,
                                column,
                                TeletextStyle::default(),
                                TeletextBaseAuthoredContent::Text { character: 'é' },
                            )
                            .unwrap()
                        });
                    TeletextBaseDisplayCell::new(position, code, kind, authored, attributes)
                        .unwrap()
                })
                .collect();
            cells.try_into().unwrap()
        })
        .collect();
    TeletextBaseDisplay::new(
        owner(),
        presentation.primary,
        TeletextNationalOption::new(6).unwrap(),
        Some(presentation),
        rows.into_boxed_slice().try_into().unwrap(),
    )
    .unwrap()
}

fn context(
    primary: u8,
    commands: Vec<TeletextEnhancementCommand>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextCharsetExecution {
    let presentation = presentation(primary);
    let language = TeletextLanguage::new("nld".into()).unwrap();
    let service = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: language.clone(),
                initial_page: owner().page,
                pages: vec![TeletextPageDraft {
                    address: owner(),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(6).unwrap(),
                    rows: vec![],
                    navigation: vec![],
                }],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation: Some(presentation),
                compositional_links: None,
                local_program: Some(TeletextEnhancementProgram::new(commands).unwrap()),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    defaults: [None; 4],
                    drcs: TeletextDrcsPageLinks::unlinked(),
                },
            }],
            magazines: vec![],
            objects,
            drcs: vec![],
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
    let original = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 512,
            max_invocations: 16,
        },
    )
    .unwrap();
    let layers = build_teletext_glyph_layers(&base, &original, 512).unwrap();
    resolve_teletext_charset_execution(&base, &original, &layers, 512).unwrap()
}
fn intent(context: &TeletextCharsetExecution) -> TeletextGlyphIntent {
    resolve_teletext_glyph_intent(context, context.records().len()).unwrap()
}
fn enhanced(intent: &TeletextGlyphIntent) -> Vec<&TeletextExecutedGlyphIntent> {
    intent
        .records()
        .iter()
        .filter(|record| record.context().candidate().operation().is_some())
        .collect()
}
fn coded(intent: &TeletextGlyphIntent, code: u8) -> &TeletextExecutedGlyphIntent {
    intent
        .records()
        .iter()
        .find(|record| {
            record.context().candidate().base().is_some_and(|cell| {
                cell.kind() == TeletextBaseDisplayCellKind::Glyph && cell.code() == code
            })
        })
        .unwrap()
}
fn g0_identity(record: &TeletextExecutedGlyphIntent) -> TeletextG0Glyph {
    match record.glyph().unwrap() {
        TeletextGlyphIdentity::G0 { glyph } => glyph,
        other => panic!("expected G0, got {other:?}"),
    }
}

#[test]
fn all_g0_table_positions_retain_exact_family_and_unmarked_arabic_cell() {
    // EN tables35,38,39,40,42,44,46: 96 positions each. Arabic7E is
    // unmarked without a prohibition note; a decoder placeholder is no glyph.
    for (designation, family) in [
        (0, TeletextG0Repertoire::Latin),
        (0x20, TeletextG0Repertoire::Cyrillic1),
        (0x24, TeletextG0Repertoire::Cyrillic2),
        (0x25, TeletextG0Repertoire::Cyrillic3),
        (0x37, TeletextG0Repertoire::Greek),
        (0x47, TeletextG0Repertoire::Arabic),
        (0x55, TeletextG0Repertoire::Hebrew),
    ] {
        let mut commands = vec![set(1, 8)];
        commands.extend((0x20..=0x7f).map(g0));
        let result = intent(&context(designation, commands, vec![]));
        let records = enhanced(&result);
        assert_eq!(records.len(), 96);
        let mut assigned = 0;
        for (code, record) in (0x20..=0x7f).zip(records) {
            let glyph = g0_identity(record);
            assert_eq!(
                (glyph.repertoire(), glyph.code(), glyph.national_subset()),
                (family, code, None)
            );
            if family == TeletextG0Repertoire::Arabic && code == 0x7e {
                assert_eq!(record.disposition(), TeletextGlyphDisposition::Pending);
                assert_eq!(
                    record.reason(),
                    Some(TeletextGlyphIntentReason::UnmarkedTableCell)
                );
            } else {
                assigned += 1;
                assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
                assert_eq!(record.reason(), None);
            }
        }
        assert_eq!(
            assigned,
            if family == TeletextG0Repertoire::Arabic {
                95
            } else {
                96
            }
        );
    }
}

#[test]
fn every_page_alias_uses_its_actual_table36_row_only_at_thirteen_positions() {
    use TeletextNationalSubset as N;
    // Literal Table32 aliases; neither code&7, header6 nor language nld owns NOS.
    let aliases: &[(N, &[u8])] = &[
        (N::English, &[0, 0x10, 0x40]),
        (N::German, &[1, 9, 0x11, 0x21]),
        (N::SwedishFinnishHungarian, &[2, 0x0a, 0x12]),
        (N::Italian, &[3, 0x0b, 0x13]),
        (N::French, &[4, 0x0c, 0x14, 0x44]),
        (N::PortugueseSpanish, &[5, 0x15]),
        (N::CzechSlovak, &[6, 0x0e, 0x26]),
        (N::Polish, &[8]),
        (N::Turkish, &[0x16, 0x36]),
        (N::SerbianCroatianSlovenian, &[0x1d]),
        (N::Romanian, &[0x1f]),
        (N::Estonian, &[0x22]),
        (N::LatvianLithuanian, &[0x23]),
    ];
    let positions = [
        0x23, 0x24, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e,
    ];
    assert_eq!(
        aliases.iter().map(|(_, codes)| codes.len()).sum::<usize>(),
        29
    );
    for &(subset, codes) in aliases {
        for &designation in codes {
            let result = intent(&context(designation, vec![], vec![]));
            for code in 0x20..=0x7f {
                let glyph = g0_identity(coded(&result, code));
                assert_eq!(
                    glyph.national_subset(),
                    positions.contains(&code).then_some(subset),
                    "designation{designation:02x} code{code:02x}"
                );
                assert_eq!(
                    (glyph.repertoire(), glyph.code()),
                    (TeletextG0Repertoire::Latin, code)
                );
            }
        }
    }
    for designation in [0x20, 0x24, 0x25, 0x37, 0x47, 0x55, 0x57] {
        let result = intent(&context(designation, vec![], vec![]));
        for code in positions {
            assert_eq!(g0_identity(coded(&result, code)).national_subset(), None);
        }
    }
}

#[test]
fn enhanced_g0_and_g1_alpha_never_apply_national_substitution() {
    let result = intent(&context(
        1,
        vec![
            set(1, 8),
            g0(0x23),
            g0(0x40),
            at(
                8,
                TeletextColumnOperation::G1Alpha {
                    code: TeletextG1AlphaCode::new(0x40).unwrap(),
                },
            ),
        ],
        vec![],
    ));
    assert_eq!(
        g0_identity(coded(&result, 0x40)).national_subset(),
        Some(TeletextNationalSubset::German)
    );
    for record in enhanced(&result) {
        assert_eq!(g0_identity(record).national_subset(), None);
        assert!(!record.context().national_substitution());
    }
}

#[test]
fn special_zero_diacritic_at_is_distinct_from_raw_asterisk_and_nonzero_composition() {
    let diacritic = |code, index| {
        at(
            8,
            TeletextColumnOperation::G0Diacritic {
                code: TeletextGlyphCode::new(code).unwrap(),
                diacritic: TeletextDiacriticIndex::new(index).unwrap(),
            },
        )
    };
    let result = intent(&context(
        0,
        vec![
            set(1, 8),
            g0(0x2a),
            diacritic(0x2a, 0),
            diacritic(0x41, 0),
            diacritic(0x41, 1),
            diacritic(0x23, 1),
            designate(0x37),
            diacritic(0x41, 1),
            diacritic(0x52, 1),
        ],
        vec![],
    ));
    let records = enhanced(&result);
    assert_eq!(g0_identity(records[0]).code(), 0x2a);
    assert_eq!(records[1].glyph(), Some(TeletextGlyphIdentity::AtSymbol));
    assert_eq!(g0_identity(records[2]).code(), 0x41);
    match records[3].glyph().unwrap() {
        TeletextGlyphIdentity::Composite { base, mark } => {
            assert_eq!(
                (base.repertoire(), base.code()),
                (TeletextG0Repertoire::Latin, 0x41)
            );
            assert_eq!(
                (mark.repertoire(), mark.code()),
                (TeletextG2Repertoire::Latin, 0x41)
            );
        }
        other => panic!("expected composite, got{other:?}"),
    }
    assert_eq!(records[3].disposition(), TeletextGlyphDisposition::Assigned);
    assert_eq!(
        records[4].reason(),
        Some(TeletextGlyphIntentReason::NonAlphabeticBase)
    );
    assert_eq!(records[5].reason(), None);
    assert_eq!(records[5].disposition(), TeletextGlyphDisposition::Assigned);
    match records[5].glyph().unwrap() {
        TeletextGlyphIdentity::Composite { base, mark } => {
            assert_eq!(base.repertoire(), TeletextG0Repertoire::Greek);
            assert_eq!(mark.repertoire(), TeletextG2Repertoire::Greek);
        }
        other => panic!("expected current Greek pair, got {other:?}"),
    }
    assert_eq!(
        records[6].reason(),
        Some(TeletextGlyphIntentReason::NonAlphabeticBase)
    );
}

#[test]
fn alphabetic_composites_retain_each_current_pair_for_all_fifteen_marks() {
    // Literal Table 34 designations and independently audited alphabetic cells.
    for (primary, code, base_family, mark_family) in [
        (
            0,
            0x41,
            TeletextG0Repertoire::Latin,
            TeletextG2Repertoire::Latin,
        ),
        (
            0x20,
            0x40,
            TeletextG0Repertoire::Cyrillic1,
            TeletextG2Repertoire::Cyrillic,
        ),
        (
            0x24,
            0x26,
            TeletextG0Repertoire::Cyrillic2,
            TeletextG2Repertoire::Cyrillic,
        ),
        (
            0x25,
            0x26,
            TeletextG0Repertoire::Cyrillic3,
            TeletextG2Repertoire::Cyrillic,
        ),
        (
            0x37,
            0x40,
            TeletextG0Repertoire::Greek,
            TeletextG2Repertoire::Greek,
        ),
    ] {
        let mut commands = vec![set(1, 8)];
        commands.extend((1..=15).map(|index| {
            at(
                8,
                TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(code).unwrap(),
                    diacritic: TeletextDiacriticIndex::new(index).unwrap(),
                },
            )
        }));
        let original = context(primary, commands, vec![]);
        let unchanged = original.clone();
        let result = intent(&original);
        for (index, record) in (1..=15).zip(enhanced(&result)) {
            assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
            assert_eq!(record.reason(), None);
            assert!(!record.context().national_substitution());
            match record.glyph().unwrap() {
                TeletextGlyphIdentity::Composite { base, mark } => {
                    assert_eq!(
                        (base.repertoire(), base.code(), base.national_subset()),
                        (base_family, code, None)
                    );
                    assert_eq!(
                        (mark.repertoire(), mark.code()),
                        (mark_family, 0x40 | index)
                    );
                }
                other => panic!("expected composite, got {other:?}"),
            }
        }
        assert_eq!(original, unchanged);
    }
}

#[test]
fn arabic_supplementary_letters_never_become_substituted_latin_accents() {
    for (primary, code, base_family) in [
        (0x40, 0x41, TeletextG0Repertoire::Latin),
        (0x47, 0x41, TeletextG0Repertoire::Arabic),
        (0x55, 0x60, TeletextG0Repertoire::Hebrew),
    ] {
        let mut commands = vec![set(1, 8)];
        commands.extend((0..=15).map(|index| {
            at(
                8,
                TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(code).unwrap(),
                    diacritic: TeletextDiacriticIndex::new(index).unwrap(),
                },
            )
        }));
        let result = intent(&context(primary, commands, vec![]));
        let records = enhanced(&result);
        assert_eq!(records[0].disposition(), TeletextGlyphDisposition::Assigned);
        assert_eq!(g0_identity(records[0]).repertoire(), base_family);
        for (index, record) in (1..=15).zip(&records[1..]) {
            assert_eq!(record.disposition(), TeletextGlyphDisposition::Pending);
            assert_eq!(
                record.reason(),
                Some(TeletextGlyphIntentReason::UnverifiedComposition)
            );
            match record.glyph().unwrap() {
                TeletextGlyphIdentity::Composite { base, mark } => {
                    assert_eq!((base.repertoire(), base.code()), (base_family, code));
                    assert_eq!(
                        (mark.repertoire(), mark.code()),
                        (TeletextG2Repertoire::Arabic, 0x40 | index)
                    );
                }
                other => panic!("expected unresolved actual pair, got {other:?}"),
            }
        }
    }
}

#[test]
fn base_blank_spacing_role_and_original_unicode_intent_survive_lookup() {
    let original = context(0, vec![], vec![]);
    let result = intent(&original);
    assert_eq!(result.records().len(), 992);
    let blank = result
        .records()
        .iter()
        .find(|record| record.context().candidate().position() == (5, 0))
        .unwrap();
    assert_eq!(blank.glyph(), Some(TeletextGlyphIdentity::Blank));
    let control = result
        .records()
        .iter()
        .find(|record| record.context().candidate().position() == (4, 0))
        .unwrap();
    assert_eq!(
        control.glyph(),
        Some(TeletextGlyphIdentity::SpacingControl { code: 0x0c })
    );
    let e = coded(&result, b'e');
    assert_eq!(g0_identity(e).code(), b'e');
    assert_eq!(
        e.context()
            .candidate()
            .base()
            .unwrap()
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    for (record, context) in result.records().iter().zip(original.records()) {
        assert_eq!(record.context(), context);
    }
}

#[test]
fn supplementary_families_use_primary_pair_not_secondary_g0() {
    for (primary, family) in [
        (0, TeletextG2Repertoire::Latin),
        (0x20, TeletextG2Repertoire::Cyrillic),
        (0x37, TeletextG2Repertoire::Greek),
        (0x40, TeletextG2Repertoire::Arabic),
        (0x47, TeletextG2Repertoire::Arabic),
        (0x55, TeletextG2Repertoire::Arabic),
    ] {
        let result = intent(&context(
            primary,
            vec![
                set(1, 8),
                at(
                    8,
                    TeletextColumnOperation::G2 {
                        code: TeletextGlyphCode::new(0x21).unwrap(),
                    },
                ),
            ],
            vec![],
        ));
        match enhanced(&result)[0].glyph().unwrap() {
            TeletextGlyphIdentity::G2 { glyph } => {
                assert_eq!((glyph.repertoire(), glyph.code()), (family, 0x21));
            }
            other => panic!("expected G2, got{other:?}"),
        }
    }
}

#[test]
fn complete_supplementary_tables_distinguish_explicit_bans_from_unmarked_cells() {
    // Printed tables37/41/43/45 and their own notes, not decoder font coverage.
    for (primary, family, forbidden, unmarked, assigned) in [
        (
            0,
            TeletextG2Repertoire::Latin,
            &[0x59, 0x5a, 0x5b, 0x65][..],
            0x40,
            91,
        ),
        (
            0x20,
            TeletextG2Repertoire::Cyrillic,
            &[0x26, 0x28][..],
            0x40,
            93,
        ),
        (0x37, TeletextG2Repertoire::Greek, &[][..], 0x40, 95),
        (0x47, TeletextG2Repertoire::Arabic, &[][..], 0x7f, 95),
    ] {
        let mut commands = vec![set(1, 8)];
        commands.extend((0x20..=0x7f).map(|code| {
            at(
                8,
                TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(code).unwrap(),
                },
            )
        }));
        let result = intent(&context(primary, commands, vec![]));
        let mut assigned_count = 0;
        for (code, record) in (0x20..=0x7f).zip(enhanced(&result)) {
            match record.glyph().unwrap() {
                TeletextGlyphIdentity::G2 { glyph } => {
                    assert_eq!((glyph.repertoire(), glyph.code()), (family, code));
                }
                other => panic!("expected G2, got{other:?}"),
            }
            if forbidden.contains(&code) {
                assert_eq!(
                    record.disposition(),
                    TeletextGlyphDisposition::ShouldNotTransmit
                );
                assert_eq!(
                    record.reason(),
                    Some(TeletextGlyphIntentReason::ProhibitedTablePosition)
                );
            } else if code == unmarked {
                assert_eq!(record.disposition(), TeletextGlyphDisposition::Pending);
                assert_eq!(
                    record.reason(),
                    Some(TeletextGlyphIntentReason::UnmarkedTableCell)
                );
            } else {
                assigned_count += 1;
                assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
                assert_eq!(record.reason(), None);
            }
        }
        assert_eq!(assigned_count, assigned);
    }
}

#[test]
fn g3_explicit_unassigned_codes_do_not_erase_background_only_glyph_or_mosaic() {
    let mut commands = vec![set(1, 8)];
    commands.extend((0x20..=0x7f).map(|code| {
        at(
            8,
            TeletextColumnOperation::G3 {
                code: TeletextGlyphCode::new(code).unwrap(),
                compatibility: TeletextG3Compatibility::Level25,
            },
        )
    }));
    let mosaic = TeletextMosaic::new(0x21).unwrap();
    commands.push(at(8, TeletextColumnOperation::G1 { mosaic }));
    let result = intent(&context(0, commands, vec![]));
    let records = enhanced(&result);
    let mut assigned = 0;
    for (code, record) in (0x20..=0x7f).zip(&records) {
        assert_eq!(
            record.glyph(),
            Some(TeletextGlyphIdentity::G3 {
                code: TeletextGlyphCode::new(code).unwrap()
            })
        );
        if [0x6e, 0x6f, 0x7e, 0x7f].contains(&code) {
            assert_eq!(
                record.disposition(),
                TeletextGlyphDisposition::ShouldNotTransmit
            );
        } else {
            assigned += 1;
            assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
        }
    }
    assert_eq!(assigned, 92);
    assert_eq!(
        records[0x5f - 0x20].glyph(),
        Some(TeletextGlyphIdentity::G3 {
            code: TeletextGlyphCode::new(0x5f).unwrap()
        })
    );
    assert_eq!(
        records[96].glyph(),
        Some(TeletextGlyphIdentity::G1Mosaic { mosaic })
    );
}

fn ambiguous_context() -> TeletextCharsetExecution {
    let target = TeletextObjectIdentity::local(owner(), TeletextObjectId::new(0xffff));
    let objects = vec![TeletextEnhancementObjectDefinition {
        identity: target,
        object_type: TeletextObjectType::Active,
        program: TeletextEnhancementProgram::new(vec![
            at(
                0,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
                },
            ),
            at(
                0,
                TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            ),
        ])
        .unwrap(),
    }];
    context(
        0,
        vec![
            set(1, 8),
            designate(0x37),
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Active,
                offset: None,
            },
            g0(b'B'),
            at(
                8,
                TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(0x2a).unwrap(),
                    diacritic: TeletextDiacriticIndex::new(0).unwrap(),
                },
            ),
        ],
        objects,
    )
}

#[test]
fn designation_ambiguity_keeps_original_sources_and_losers_but_cannot_obscure_special_at() {
    let original = ambiguous_context();
    let result = intent(&original);
    assert_eq!(result.records().len(), 995);
    for (record, context) in result.records().iter().zip(original.records()) {
        assert_eq!(record.context(), context);
    }
    let main = result
        .records()
        .iter()
        .find(|record| {
            record.context().candidate().source()
                == Some(TeletextEnhancementSource {
                    object: None,
                    command: 3,
                })
        })
        .unwrap();
    let pending = main.context().selection().unwrap().pending().unwrap();
    assert_eq!(main.disposition(), TeletextGlyphDisposition::Pending);
    assert_eq!(main.glyph(), None);
    assert_eq!(
        main.reason(),
        Some(TeletextGlyphIntentReason::CharsetOrder { pending })
    );
    assert_eq!(original.pending_contenders(pending).count(), 2);
    let object = result
        .records()
        .iter()
        .find(|record| {
            record
                .context()
                .candidate()
                .source()
                .is_some_and(|source| source.object.is_some() && source.command == 1)
        })
        .unwrap();
    assert_eq!(object.context().candidate().invocation().unwrap().get(), 0);
    assert_eq!(
        object.context().candidate().root_source(),
        Some(TeletextSpatialRootSource::Local { command: 2 })
    );
    let special = result
        .records()
        .iter()
        .find(|record| {
            record.context().candidate().source()
                == Some(TeletextEnhancementSource {
                    object: None,
                    command: 4,
                })
        })
        .unwrap();
    assert!(special.context().selection().unwrap().pending().is_some());
    assert_eq!(special.glyph(), Some(TeletextGlyphIdentity::AtSymbol));
    assert_eq!(special.disposition(), TeletextGlyphDisposition::Assigned);
    assert_eq!(special.reason(), None);
}

#[test]
fn whole_candidate_budget_includes_all_base_roles_and_blames_original_object_instance() {
    let original = ambiguous_context();
    let before = original.clone();
    let count = original.records().len();
    assert_eq!(
        resolve_teletext_glyph_intent(&original, count)
            .unwrap()
            .records()
            .len(),
        count
    );
    let last = resolve_teletext_glyph_intent(&original, count - 1).unwrap_err();
    assert_eq!(
        last.reason,
        TeletextGlyphIntentFailure::CandidateBudget {
            count,
            maximum: count - 1
        }
    );
    assert_eq!(last.position, Some((24, 39)));
    assert_eq!(last.location, None);
    let target = TeletextObjectIdentity::local(owner(), TeletextObjectId::new(0xffff));
    let ordinal = original
        .records()
        .iter()
        .position(|record| {
            record.candidate().source()
                == Some(TeletextEnhancementSource {
                    object: Some(target),
                    command: 1,
                })
        })
        .unwrap();
    let error = resolve_teletext_glyph_intent(&original, ordinal).unwrap_err();
    assert_eq!(error.owner, owner());
    assert_eq!(error.position, Some((1, 8)));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 1
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 2 })
    );
    assert_eq!(error.invocation.unwrap().get(), 0);
    assert_eq!(original, before);
}

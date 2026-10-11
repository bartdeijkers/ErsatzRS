//! Literal pinned-font masks plus independent repertoire and six-block oracles.

use super::*;

fn g0(repertoire: TeletextG0Repertoire, code: u8) -> TeletextGlyphIdentity {
    TeletextGlyphIdentity::G0 {
        glyph: TeletextG0Glyph {
            repertoire,
            code,
            national_subset: None,
        },
    }
}
fn g2(repertoire: TeletextG2Repertoire, code: u8) -> TeletextGlyphIdentity {
    TeletextGlyphIdentity::G2 {
        glyph: TeletextG2Glyph { repertoire, code },
    }
}
fn rows(identity: TeletextGlyphIdentity) -> [u16; 10] {
    resolve_teletext_glyph_mask(identity, None, false)
        .unwrap()
        .rows()
}

fn drcs_reference(scope: TeletextDrcsTableScope) -> TeletextDrcsGlyphReference {
    TeletextDrcsGlyphReference {
        table: scope,
        subtable: TeletextDrcsSubtableIndex::new(3).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(7).unwrap(),
    }
}

fn drcs_resource(scope: TeletextDrcsTableScope) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        if scope == TeletextDrcsTableScope::Normal {
            TeletextResourceKind::Drcs
        } else {
            TeletextResourceKind::Gdrcs
        },
        TeletextRawDataPageNumber::new(
            8,
            if scope == TeletextDrcsTableScope::Normal {
                0xb3
            } else {
                0xa4
            },
        )
        .unwrap(),
        TeletextResourceSubpageIndex::new(3).unwrap(),
    )
    .unwrap()
}

fn drcs_service() -> TeletextSemanticService {
    let owner = super::attribute_execution_tests::owner();
    let language = TeletextLanguage::new("eng".into()).unwrap();
    let scopes = [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ];
    let mut commands = vec![TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(1, 8).unwrap(),
    }];
    commands.extend(scopes.iter().enumerate().map(|(index, &scope)| {
        TeletextEnhancementCommand::AtColumn {
            column: 8 + u8::try_from(index).unwrap(),
            operation: TeletextColumnOperation::Drcs {
                reference: drcs_reference(scope),
            },
        }
    }));
    let draft = TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language: language.clone(),
            initial_page: owner.page,
            pages: vec![TeletextPageDraft {
                address: owner,
                kind: TeletextPageKind::FullPage,
                language,
                national_option: TeletextNationalOption::new(0).unwrap(),
                rows: vec![],
                navigation: vec![],
            }],
        },
        pages: vec![TeletextResolvedPageEnhancementDraft {
            address: owner,
            presentation: None,
            compositional_links: None,
            local_program: Some(TeletextEnhancementProgram::new(commands).unwrap()),
            resources: TeletextResolvedPageResourceSelection {
                objects: TeletextResolvedObjectPageLinks {
                    public: None,
                    global: None,
                },
                defaults: [None; 4],
                drcs: TeletextDrcsPageLinks::new(
                    Some(drcs_resource(scopes[0]).page()),
                    Some(drcs_resource(scopes[1]).page()),
                )
                .unwrap(),
            },
        }],
        magazines: vec![],
        objects: vec![],
        drcs: scopes
            .into_iter()
            .map(|scope| {
                TeletextDrcsSubtable::new(
                    drcs_resource(scope),
                    vec![TeletextDrcsGlyphDefinition {
                        slot: drcs_reference(scope).slot,
                        bitmap: TeletextDrcsBitmap::new(
                            [if scope == TeletextDrcsTableScope::Normal {
                                0x800
                            } else {
                                1
                            }; 10],
                        )
                        .unwrap(),
                    }],
                )
                .unwrap()
            })
            .collect(),
    };
    TeletextSemanticService::new(
        draft,
        TeletextServiceBudget {
            max_records: 16,
            max_objects: 0,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 16,
            max_collected_commands: 16,
            max_collected_drcs_references: 16,
        },
    )
    .unwrap()
}

#[test]
fn literal_alphabet_masks_preserve_all_seven_table_identities() {
    for (repertoire, code, expected) in [
        (
            TeletextG0Repertoire::Latin,
            0x41,
            [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0],
        ),
        (
            TeletextG0Repertoire::Cyrillic1,
            0x40,
            [0, 0x306, 0x306, 0x306, 0x1fe, 6, 6, 6, 0, 0],
        ),
        (
            TeletextG0Repertoire::Cyrillic2,
            0x26,
            [0, 0, 0, 0x306, 0x306, 0x3e6, 0x336, 0x3e6, 0, 0],
        ),
        (
            TeletextG0Repertoire::Cyrillic3,
            0x26,
            [0, 0x198, 0, 0x0e0, 0x060, 0x060, 0x060, 0x1f8, 0, 0],
        ),
        (
            TeletextG0Repertoire::Greek,
            0x37,
            [0, 0x3fc, 0x30c, 0x018, 0x030, 0x060, 0x0c0, 0x0c0, 0, 0],
        ),
        (
            TeletextG0Repertoire::Arabic,
            0x41,
            [0, 0, 0, 0, 0x0e0, 0x180, 0x3e0, 0, 0, 0],
        ),
        (
            TeletextG0Repertoire::Hebrew,
            0x60,
            [0, 0x31c, 0x18c, 0x1cc, 0x36c, 0x338, 0x318, 0x30c, 0, 0],
        ),
    ] {
        assert_eq!(rows(g0(repertoire, code)), expected, "{repertoire:?}");
    }
}

#[test]
fn national_subsets_are_explicit_and_never_applied_to_enhanced_g0() {
    let glyph = TeletextG0Glyph {
        repertoire: TeletextG0Repertoire::Latin,
        code: 0x40,
        national_subset: Some(TeletextNationalSubset::German),
    };
    assert_eq!(
        rows(TeletextGlyphIdentity::G0 { glyph }),
        [0x1f8, 0x30c, 0x300, 0x1f8, 0x30c, 0x1f8, 0x00c, 0x30c, 0x1f8, 0]
    );
    let literal_at = [0, 0x1fc, 0x306, 0x63e, 0x666, 0x666, 0x63e, 0x300, 0x1fc, 0];
    assert_eq!(rows(g0(TeletextG0Repertoire::Latin, 0x40)), literal_at);
    assert_eq!(rows(TeletextGlyphIdentity::AtSymbol), literal_at);
    for invalid in [
        TeletextG0Glyph {
            code: 0x41,
            ..glyph
        },
        TeletextG0Glyph {
            repertoire: TeletextG0Repertoire::Greek,
            ..glyph
        },
    ] {
        assert_eq!(
            resolve_teletext_glyph_mask(TeletextGlyphIdentity::G0 { glyph: invalid }, None, false),
            Err(TeletextGlyphMaskError::InvalidNationalSubset)
        );
    }
    for subset in [
        TeletextNationalSubset::English,
        TeletextNationalSubset::German,
        TeletextNationalSubset::SwedishFinnishHungarian,
        TeletextNationalSubset::Italian,
        TeletextNationalSubset::French,
        TeletextNationalSubset::PortugueseSpanish,
        TeletextNationalSubset::CzechSlovak,
        TeletextNationalSubset::Polish,
        TeletextNationalSubset::Turkish,
        TeletextNationalSubset::SerbianCroatianSlovenian,
        TeletextNationalSubset::Romanian,
        TeletextNationalSubset::Estonian,
        TeletextNationalSubset::LatvianLithuanian,
    ] {
        for code in [
            0x23, 0x24, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e,
        ] {
            let mask = rows(TeletextGlyphIdentity::G0 {
                glyph: TeletextG0Glyph {
                    code,
                    national_subset: Some(subset),
                    ..glyph
                },
            });
            assert!(mask.iter().all(|row| *row <= 0x0fff));
        }
    }
}

#[test]
fn current_g2_shapes_are_distinct_and_composites_use_only_verified_repertoires() {
    assert_eq!(
        rows(g2(TeletextG2Repertoire::Latin, 0x42)),
        [0x060, 0x0c0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(
        rows(g2(TeletextG2Repertoire::Arabic, 0x42)),
        [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x306, 0x306, 0x3fc, 0, 0]
    );
    let base = TeletextG0Glyph {
        repertoire: TeletextG0Repertoire::Latin,
        code: b'e',
        national_subset: None,
    };
    let mark = TeletextG2Glyph {
        repertoire: TeletextG2Repertoire::Latin,
        code: 0x42,
    };
    assert_eq!(
        rows(TeletextGlyphIdentity::Composite { base, mark }),
        [0x060, 0x0c0, 0, 0x1fc, 0x306, 0x3fe, 0x300, 0x1fc, 0, 0]
    );
    // Capital E uses the separately supplied reduced LATIN font, leaving accent room.
    assert_eq!(
        rows(TeletextGlyphIdentity::Composite {
            base: TeletextG0Glyph { code: b'E', ..base },
            mark,
        }),
        [0x060, 0x0c0, 0x3fe, 0x300, 0x3fc, 0x300, 0x300, 0x3fe, 0, 0]
    );
    for (base, mark) in [
        (
            TeletextG0Glyph {
                repertoire: TeletextG0Repertoire::Arabic,
                ..base
            },
            mark,
        ),
        (
            base,
            TeletextG2Glyph {
                repertoire: TeletextG2Repertoire::Arabic,
                ..mark
            },
        ),
        (TeletextG0Glyph { code: b'1', ..base }, mark),
        (base, TeletextG2Glyph { code: 0x50, ..mark }),
    ] {
        assert_eq!(
            resolve_teletext_glyph_mask(
                TeletextGlyphIdentity::Composite { base, mark },
                None,
                false
            ),
            Err(TeletextGlyphMaskError::UnverifiedComposition { base, mark })
        );
    }
}

fn composite(
    base_family: TeletextG0Repertoire,
    code: u8,
    mark_family: TeletextG2Repertoire,
    mark_code: u8,
) -> TeletextGlyphIdentity {
    TeletextGlyphIdentity::Composite {
        base: TeletextG0Glyph {
            repertoire: base_family,
            code,
            national_subset: None,
        },
        mark: TeletextG2Glyph {
            repertoire: mark_family,
            code: mark_code,
        },
    }
}

#[test]
fn non_latin_composite_literals_translate_or_fit_the_actual_glyph_without_font_borrowing() {
    for (family, code, mark_code, expected) in [
        (
            TeletextG0Repertoire::Cyrillic1,
            0x40,
            0x47,
            [0, 0x060, 0x306, 0x306, 0x306, 0x1fe, 6, 6, 6, 0],
        ),
        (
            TeletextG0Repertoire::Cyrillic2,
            0x26,
            0x42,
            [0x060, 0x0c0, 0x306, 0x306, 0x3e6, 0x336, 0x3e6, 0, 0, 0],
        ),
        (
            TeletextG0Repertoire::Cyrillic3,
            0x26,
            0x4a,
            [
                0x060, 0x090, 0x060, 0x198, 0, 0x0e0, 0x060, 0x060, 0x060, 0x1f8,
            ],
        ),
        // Greek 4/0 already has an eight-row glyph. The ring needs THREE rows;
        // its seven-row base fit retains all horizontal bits (36C union row).
        (
            TeletextG0Repertoire::Greek,
            0x40,
            0x4a,
            [
                0x060, 0x090, 0x060, 0x36c, 0, 0x0e0, 0x060, 0x060, 0x060, 0x1f8,
            ],
        ),
        (
            TeletextG0Repertoire::Greek,
            0x41,
            0x47,
            [0, 0x060, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0],
        ),
    ] {
        let mark_family = if family == TeletextG0Repertoire::Greek {
            TeletextG2Repertoire::Greek
        } else {
            TeletextG2Repertoire::Cyrillic
        };
        assert_eq!(
            rows(composite(family, code, mark_family, mark_code)),
            expected
        );
    }
    // This exact current-Cyrillic capital A previously swallowed dot-above.
    assert_eq!(
        rows(composite(
            TeletextG0Repertoire::Cyrillic2,
            0x41,
            TeletextG2Repertoire::Cyrillic,
            0x47
        )),
        [0, 0x060, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0]
    );
}

#[test]
fn supplied_mixed_composite_identities_keep_their_base_family_and_actual_mark() {
    // A font primitive may shape a supplied identity without inventing a
    // reachable Table 34 designation; the semantic factory retains that duty.
    for (base_family, code, mark_family, mark_code, expected) in [
        (
            TeletextG0Repertoire::Hebrew,
            0x60,
            TeletextG2Repertoire::Latin,
            0x4a,
            [
                0x060, 0x090, 0x060, 0x31c, 0x18c, 0x1cc, 0x36c, 0x338, 0x318, 0x30c,
            ],
        ),
        (
            TeletextG0Repertoire::Hebrew,
            0x41,
            TeletextG2Repertoire::Greek,
            0x47,
            [0, 0x060, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0],
        ),
        (
            TeletextG0Repertoire::Cyrillic1,
            0x40,
            TeletextG2Repertoire::Greek,
            0x4a,
            [0x060, 0x090, 0x060, 0x306, 0x306, 0x306, 0x1fe, 6, 6, 6],
        ),
    ] {
        assert_eq!(
            rows(composite(base_family, code, mark_family, mark_code)),
            expected
        );
    }
}

#[test]
fn all_mark_classes_preserve_their_actual_masks_and_below_marks_keep_full_capitals() {
    let expected_below = [
        (
            0x49,
            [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0x060],
        ),
        (
            0x4b,
            [
                0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x646, 0x040, 0x1c0,
            ],
        ),
        (
            0x4c,
            [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0x7fe],
        ),
        (
            0x4e,
            [
                0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x646, 0x040, 0x070,
            ],
        ),
    ];
    for (mark_code, expected) in expected_below {
        assert_eq!(
            rows(composite(
                TeletextG0Repertoire::Latin,
                0x41,
                TeletextG2Repertoire::Latin,
                mark_code
            )),
            expected
        );
    }
    for (family, code, mark_family) in [
        (
            TeletextG0Repertoire::Cyrillic1,
            0x40,
            TeletextG2Repertoire::Cyrillic,
        ),
        (
            TeletextG0Repertoire::Cyrillic2,
            0x26,
            TeletextG2Repertoire::Cyrillic,
        ),
        (
            TeletextG0Repertoire::Cyrillic3,
            0x26,
            TeletextG2Repertoire::Cyrillic,
        ),
        (
            TeletextG0Repertoire::Greek,
            0x40,
            TeletextG2Repertoire::Greek,
        ),
        (
            TeletextG0Repertoire::Hebrew,
            0x60,
            TeletextG2Repertoire::Latin,
        ),
    ] {
        let base_rows = rows(g0(family, code));
        for mark_code in 0x41..=0x4f {
            let mark_rows = rows(g2(mark_family, mark_code));
            let result = rows(composite(family, code, mark_family, mark_code));
            assert!(result.iter().all(|&row| row <= 0xfff));
            assert_ne!(result, base_rows, "{family:?}/{code:02x}/{mark_code:02x}");
            if matches!(mark_code, 0x49 | 0x4b | 0x4c | 0x4e) {
                for ((actual, base), mark) in result.into_iter().zip(base_rows).zip(mark_rows) {
                    assert_eq!(actual, base | mark);
                }
            } else {
                // The complete mark zone is unchanged and disjoint from fitted
                // base pixels; this checks every above-mark class, including ring.
                let mark_end = mark_rows.iter().rposition(|&row| row != 0).unwrap();
                assert_eq!(&result[..=mark_end], &mark_rows[..=mark_end]);
                assert_ne!(
                    result[mark_end + 1..]
                        .iter()
                        .fold(0, |union, row| union | row),
                    0
                );
            }
        }
    }
}

#[test]
fn composite_font_policy_rejects_letter_marks_nonletters_and_unknown_positions() {
    for (family, code, mark_family, mark_code) in [
        (
            TeletextG0Repertoire::Latin,
            0x41,
            TeletextG2Repertoire::Arabic,
            0x41,
        ),
        (
            TeletextG0Repertoire::Arabic,
            0x41,
            TeletextG2Repertoire::Arabic,
            0x41,
        ),
        (
            TeletextG0Repertoire::Hebrew,
            0x60,
            TeletextG2Repertoire::Arabic,
            0x41,
        ),
        (
            TeletextG0Repertoire::Greek,
            0x52,
            TeletextG2Repertoire::Greek,
            0x41,
        ),
        (
            TeletextG0Repertoire::Cyrillic1,
            0x26,
            TeletextG2Repertoire::Cyrillic,
            0x41,
        ),
        (
            TeletextG0Repertoire::Cyrillic2,
            0x20,
            TeletextG2Repertoire::Cyrillic,
            0x41,
        ),
        (
            TeletextG0Repertoire::Cyrillic2,
            0x80,
            TeletextG2Repertoire::Cyrillic,
            0x41,
        ),
        (
            TeletextG0Repertoire::Greek,
            0x41,
            TeletextG2Repertoire::Greek,
            0x40,
        ),
        (
            TeletextG0Repertoire::Greek,
            0x41,
            TeletextG2Repertoire::Greek,
            0x50,
        ),
    ] {
        let identity = composite(family, code, mark_family, mark_code);
        let TeletextGlyphIdentity::Composite { base, mark } = identity else {
            unreachable!()
        };
        assert_eq!(
            resolve_teletext_glyph_mask(identity, None, false),
            Err(TeletextGlyphMaskError::UnverifiedComposition { base, mark })
        );
    }
    // Current G0 alphabetic designation never adds a Base national subset.
    let base = TeletextG0Glyph {
        repertoire: TeletextG0Repertoire::Latin,
        code: 0x41,
        national_subset: Some(TeletextNationalSubset::English),
    };
    let mark = TeletextG2Glyph {
        repertoire: TeletextG2Repertoire::Latin,
        code: 0x42,
    };
    assert_eq!(
        resolve_teletext_glyph_mask(TeletextGlyphIdentity::Composite { base, mark }, None, false),
        Err(TeletextGlyphMaskError::UnverifiedComposition { base, mark })
    );
}

#[test]
fn all_mosaic_patterns_match_independent_six_block_geometry_at_both_spacings() {
    for blocks in 0..64 {
        let identity = TeletextGlyphIdentity::G1Mosaic {
            mosaic: TeletextMosaic::new(blocks).unwrap(),
        };
        for separated in [false, true] {
            let mask = resolve_teletext_glyph_mask(identity, None, separated).unwrap();
            for row in 0..10 {
                let band = if row < 3 {
                    0
                } else if row < 7 {
                    1
                } else {
                    2
                };
                let hidden_row = separated && matches!(row, 0 | 3 | 7);
                let left = if blocks & (1 << (band * 2)) != 0 {
                    if separated {
                        0x780
                    } else {
                        0xfc0
                    }
                } else {
                    0
                };
                let right = if blocks & (1 << (band * 2 + 1)) != 0 {
                    if separated {
                        0x01e
                    } else {
                        0x03f
                    }
                } else {
                    0
                };
                assert_eq!(
                    mask.rows()[row],
                    if hidden_row { 0 } else { left | right },
                    "blocks={blocks}, separated={separated}, row={row}"
                );
            }
        }
    }
}

#[test]
fn literal_smooth_mosaic_blank_and_underline_policy_are_independent_of_sizing() {
    assert_eq!(
        rows(TeletextGlyphIdentity::G3 {
            code: TeletextGlyphCode::new(0x2f).unwrap()
        }),
        [0xaaa, 0x555, 0xaaa, 0x555, 0xaaa, 0x555, 0xaaa, 0x555, 0xaaa, 0x555]
    );
    assert_eq!(
        rows(TeletextGlyphIdentity::G3 {
            code: TeletextGlyphCode::new(0x5f).unwrap()
        }),
        [0; 10]
    );
    for identity in [
        TeletextGlyphIdentity::Blank,
        TeletextGlyphIdentity::SpacingControl { code: 0x0d },
    ] {
        assert_eq!(rows(identity), [0; 10]);
    }
    let original =
        resolve_teletext_glyph_mask(g0(TeletextG0Repertoire::Latin, b'A'), None, false).unwrap();
    let underlined = original.with_underline();
    assert_eq!(&original.rows()[..9], &underlined.rows()[..9]);
    assert_eq!(underlined.rows()[9], 0xfff);
    assert_eq!(original.rows()[9], 0);
}

#[test]
fn all_table_positions_preserve_assignment_errors_and_masks_fit_twelve_bits() {
    let mut g0_count = 0;
    for repertoire in [
        TeletextG0Repertoire::Latin,
        TeletextG0Repertoire::Cyrillic1,
        TeletextG0Repertoire::Cyrillic2,
        TeletextG0Repertoire::Cyrillic3,
        TeletextG0Repertoire::Greek,
        TeletextG0Repertoire::Arabic,
        TeletextG0Repertoire::Hebrew,
    ] {
        for code in 0x20..=0x7f {
            let result = resolve_teletext_glyph_mask(g0(repertoire, code), None, false);
            if repertoire == TeletextG0Repertoire::Arabic && code == 0x7e {
                assert!(matches!(
                    result,
                    Err(TeletextGlyphMaskError::UnmarkedTableCell { .. })
                ));
            } else {
                assert!(result.unwrap().rows().iter().all(|row| *row <= 0xfff));
                g0_count += 1;
            }
        }
    }
    assert_eq!(g0_count, 671);
    let mut g2_count = 0;
    for repertoire in [
        TeletextG2Repertoire::Latin,
        TeletextG2Repertoire::Cyrillic,
        TeletextG2Repertoire::Greek,
        TeletextG2Repertoire::Arabic,
    ] {
        for code in 0x20..=0x7f {
            let result = resolve_teletext_glyph_mask(g2(repertoire, code), None, false);
            let prohibited = (repertoire == TeletextG2Repertoire::Latin
                && matches!(code, 0x59 | 0x5a | 0x5b | 0x65))
                || (repertoire == TeletextG2Repertoire::Cyrillic && matches!(code, 0x26 | 0x28));
            let unmarked = (repertoire != TeletextG2Repertoire::Arabic && code == 0x40)
                || (repertoire == TeletextG2Repertoire::Arabic && code == 0x7f);
            if prohibited {
                assert!(matches!(
                    result,
                    Err(TeletextGlyphMaskError::ProhibitedTablePosition { .. })
                ));
            } else if unmarked {
                assert!(matches!(
                    result,
                    Err(TeletextGlyphMaskError::UnmarkedTableCell { .. })
                ));
            } else {
                assert!(result.unwrap().rows().iter().all(|row| *row <= 0xfff));
                g2_count += 1;
            }
        }
    }
    assert_eq!(g2_count, 374);
    for code in 0x20..=0x7f {
        let result = resolve_teletext_glyph_mask(
            TeletextGlyphIdentity::G3 {
                code: TeletextGlyphCode::new(code).unwrap(),
            },
            None,
            false,
        );
        assert_eq!(result.is_err(), matches!(code, 0x6e | 0x6f | 0x7e | 0x7f));
    }
    for bytes in include_bytes!("font_data/masks.bin").chunks_exact(2) {
        assert!(u16::from_le_bytes([bytes[0], bytes[1]]) <= 0xfff);
    }
}

#[test]
fn checked_drcs_bitmap_preserves_corners_blank_and_pixel_bounds() {
    let bitmap =
        TeletextDrcsBitmap::new([0x800, 1, 0x401, 0x202, 0, 0x104, 0x088, 0x050, 0x020, 0xfff])
            .unwrap();
    let mask = TeletextGlyphMask::from_drcs_bitmap(bitmap);
    assert_eq!(mask.rows(), bitmap.get());
    assert_eq!(mask.pixel(0, 0), Some(true));
    assert_eq!(mask.pixel(11, 0), Some(false));
    assert_eq!(mask.pixel(11, 1), Some(true));
    for column in 0..12 {
        assert_eq!(mask.pixel(column, 9), Some(true));
    }
    assert_eq!(mask.pixel(12, 0), None);
    assert_eq!(mask.pixel(0, 10), None);
    assert_eq!(mask.pixel(u8::MAX, u8::MAX), None);
    assert_eq!(
        TeletextGlyphMask::from_drcs_bitmap(TeletextDrcsBitmap::new([0; 10]).unwrap()).rows(),
        [0; 10]
    );
    let identity = TeletextGlyphIdentity::Drcs {
        reference: TeletextDrcsGlyphReference {
            table: TeletextDrcsTableScope::Normal,
            subtable: TeletextDrcsSubtableIndex::new(15).unwrap(),
            slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
        },
    };
    assert_eq!(
        resolve_teletext_glyph_mask(identity, None, false),
        Err(TeletextGlyphMaskError::MissingDrcs)
    );
}

#[test]
fn legacy_latin_replacement_seam_preserves_exact_acute_pound_and_at_masks() {
    let acute = resolve_teletext_latin_replacement_mask(TeletextColumnOperation::G0Diacritic {
        code: TeletextGlyphCode::new(b'e').unwrap(),
        diacritic: TeletextDiacriticIndex::new(2).unwrap(),
    })
    .unwrap();
    assert_eq!(
        acute.rows(),
        [0x060, 0x0c0, 0, 0x1fc, 0x306, 0x3fe, 0x300, 0x1fc, 0, 0]
    );
    let pound = resolve_teletext_latin_replacement_mask(TeletextColumnOperation::G2 {
        code: TeletextGlyphCode::new(0x23).unwrap(),
    })
    .unwrap();
    assert_eq!(
        pound.rows(),
        [0, 0x0fc, 0x186, 0x0c0, 0x3f0, 0x0c0, 0x3e6, 0x33c, 0, 0]
    );
    let at = resolve_teletext_latin_replacement_mask(TeletextColumnOperation::G0Diacritic {
        code: TeletextGlyphCode::new(0x2a).unwrap(),
        diacritic: TeletextDiacriticIndex::new(0).unwrap(),
    })
    .unwrap();
    assert_eq!(at.rows(), rows(TeletextGlyphIdentity::AtSymbol));
    for operation in [
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(0x41).unwrap(),
        },
        TeletextColumnOperation::G1Alpha {
            code: TeletextG1AlphaCode::new(0x40).unwrap(),
        },
    ] {
        assert_eq!(
            resolve_teletext_latin_replacement_mask(operation),
            Err(TeletextGlyphMaskError::UnsupportedReplacement)
        );
    }
    for code in [0, 0x1f, 0x80, 0xff] {
        assert_eq!(
            resolve_teletext_glyph_mask(g0(TeletextG0Repertoire::Latin, code), None, false),
            Err(TeletextGlyphMaskError::InvalidGlyphCode { code })
        );
    }
}

#[test]
fn exact_resolved_drcs_scope_subtable_and_slot_are_required_before_using_pixels() {
    let service = drcs_service();
    let base = super::attribute_execution_tests::base();
    let original = resolve_teletext_spatial_trace(
        &service,
        base.owner(),
        TeletextSpatialTraceBudget {
            max_events: 16,
            max_invocations: 0,
        },
    )
    .unwrap();
    let layers = build_teletext_glyph_layers(&base, &original, 2048).unwrap();
    let charset = resolve_teletext_charset_execution(&base, &original, &layers, 2048).unwrap();
    let intent = resolve_teletext_glyph_intent(&charset, 2048).unwrap();
    let execution = resolve_teletext_drcs_execution(
        &service,
        &intent,
        TeletextDrcsExecutionBudget {
            max_candidates: 2048,
            max_references: 2,
        },
    )
    .unwrap();
    assert_eq!(execution.records().len(), 2);
    for (record, scope, expected) in [
        (
            &execution.records()[0],
            TeletextDrcsTableScope::Normal,
            0x800,
        ),
        (&execution.records()[1], TeletextDrcsTableScope::Global, 1),
    ] {
        let reference = drcs_reference(scope);
        let identity = TeletextGlyphIdentity::Drcs { reference };
        assert_eq!(
            resolve_teletext_glyph_mask(identity, Some(record), false)
                .unwrap()
                .rows(),
            [expected; 10]
        );
        for mismatch in [
            TeletextDrcsGlyphReference {
                table: if scope == TeletextDrcsTableScope::Normal {
                    TeletextDrcsTableScope::Global
                } else {
                    TeletextDrcsTableScope::Normal
                },
                ..reference
            },
            TeletextDrcsGlyphReference {
                subtable: TeletextDrcsSubtableIndex::new(4).unwrap(),
                ..reference
            },
            TeletextDrcsGlyphReference {
                slot: TeletextDrcsGlyphSlot::new(8).unwrap(),
                ..reference
            },
        ] {
            assert_eq!(
                resolve_teletext_glyph_mask(
                    TeletextGlyphIdentity::Drcs {
                        reference: mismatch
                    },
                    Some(record),
                    false
                ),
                Err(TeletextGlyphMaskError::DrcsReferenceMismatch)
            );
        }
    }
}

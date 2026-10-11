//! Literal primary-table operands; no Unicode or font-mask comparisons.
use super::attribute_execution_tests::{at, base, owner, service, set};
use super::*;
const CODES: [u8; 13] = [
    0x23, 0x24, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e,
];
// Literal Table29 mode/data words, Tables35–37/46; 0=no operand, FFFF=ambiguous mark.
const EXPECTED: [(TeletextNationalSubset, [u16; 13]); 13] = [
    (
        TeletextNationalSubset::English,
        [
            0x0f23, 0x0f24, 0x0940, 0x0f2c, 0x0f3d, 0x0f2e, 0x0f2d, 0x0923, 0x0f50, 0x0f3c, 0x097c,
            0x0f3e, 0x0f38,
        ],
    ),
    (
        TeletextNationalSubset::German,
        [
            0x0923, 0x0f24, 0x0f27, 0x1841, 0x184f, 0x1855, 0x095e, 0x095f, 0x0f30, 0x1861, 0x186f,
            0x1875, 0x0f7b,
        ],
    ),
    (
        TeletextNationalSubset::SwedishFinnishHungarian,
        [
            0x0923, 0x0924, 0x1245, 0x1841, 0x184f, 0x1a41, 0x1855, 0x095f, 0x1265, 0x1861, 0x186f,
            0x1a61, 0x1875,
        ],
    ),
    (
        TeletextNationalSubset::Italian,
        [
            0x0f23, 0x0f24, 0x1265, 0x0f30, 0x1b63, 0x0f2e, 0x0f2d, 0x0923, 0x1175, 0x1161, 0x116f,
            0x1165, 0x1169,
        ],
    ),
    (
        TeletextNationalSubset::French,
        [
            0x1265, 0x1869, 0x1161, 0x1865, 0x1365, 0x1175, 0x1369, 0x0923, 0x1165, 0x1361, 0x136f,
            0x1375, 0x1b63,
        ],
    ),
    (
        TeletextNationalSubset::PortugueseSpanish,
        [
            0x1b63, 0x0f24, 0x0f21, 0x1261, 0x1265, 0x1269, 0x126f, 0x1275, 0x0f3f, 0x1875, 0x146e,
            0x1165, 0x1161,
        ],
    ),
    (
        TeletextNationalSubset::CzechSlovak,
        [
            0x0923, 0x1a75, 0x1f63, 0x1f74, 0x1f7a, 0x1279, 0x1269, 0x1f72, 0x1265, 0x1261, 0x1f65,
            0x1275, 0x1f73,
        ],
    ),
    (
        TeletextNationalSubset::Polish,
        [
            0x0923, 0x126e, 0x1e61, 0x0000, 0x1253, 0x0f68, 0x1263, 0x126f, 0x1e65, 0x177a, 0x1273,
            0x0f78, 0x127a,
        ],
    ),
    (
        TeletextNationalSubset::Turkish,
        [
            0x0000, 0x1667, 0x1749, 0x1b53, 0x184f, 0x1b43, 0x1855, 0x1647, 0x0f75, 0x1b73, 0x186f,
            0x1b63, 0x1875,
        ],
    ),
    (
        TeletextNationalSubset::SerbianCroatianSlovenian,
        [
            0x0923, 0x1845, 0x1f43, 0x1243, 0x1f5a, 0x0f62, 0x1f53, 0x1865, 0x1f63, 0x1263, 0x1f7a,
            0x0f72, 0x1f73,
        ],
    ),
    (
        TeletextNationalSubset::Romanian,
        [
            0x0923, 0x0924, 0x1b54, 0x1341, 0x1b53, 0xffff, 0x1349, 0x0f75, 0x1b74, 0x1361, 0x1b73,
            0xffff, 0x1369,
        ],
    ),
    (
        TeletextNationalSubset::Estonian,
        [
            0x0923, 0x146f, 0x1f53, 0x1841, 0x184f, 0x1f5a, 0x1855, 0x144f, 0x1f73, 0x1861, 0x186f,
            0x1f7a, 0x1875,
        ],
    ),
    (
        TeletextNationalSubset::LatvianLithuanian,
        [
            0x0923, 0x0f24, 0x1f53, 0x1765, 0x1b65, 0x1f5a, 0x1f63, 0x1575, 0x1f73, 0x1e61, 0x1e75,
            0x1f7a, 0x1e69,
        ],
    ),
];
fn original(subset: TeletextNationalSubset, code: u8) -> TeletextGlyphIdentity {
    TeletextGlyphIdentity::G0 {
        glyph: TeletextG0Glyph {
            repertoire: TeletextG0Repertoire::Latin,
            code,
            national_subset: Some(subset),
        },
    }
}
fn operand_word(operation: TeletextColumnOperation) -> u16 {
    match operation {
        TeletextColumnOperation::G0 { code } => 0x0900 | u16::from(code.get()),
        TeletextColumnOperation::G2 { code } => 0x0f00 | u16::from(code.get()),
        TeletextColumnOperation::G0Diacritic { code, diacritic } => {
            (u16::from(0x10 | diacritic.get()) << 8) | u16::from(code.get())
        }
        _ => panic!("unexpected national conversion operation"),
    }
}
fn conversions() -> Vec<TeletextNationalConversion> {
    EXPECTED
        .iter()
        .flat_map(|(subset, _)| {
            CODES.iter().filter_map(|code| {
                resolve_teletext_national_conversion(original(*subset, *code)).ok()
            })
        })
        .collect()
}
#[test]
fn all_169_primary_operands_preserve_exact_identity_and_evidence() {
    let mut counts = [0; 3];
    for (subset, words) in EXPECTED {
        for (code, expected) in CODES.into_iter().zip(words) {
            let identity = original(subset, code);
            let result = resolve_teletext_national_conversion(identity);
            match expected {
                0 => {
                    assert_eq!(
                        result,
                        Err(TeletextNationalConversionError::UnsupportedIdentity { subset, code })
                    );
                    counts[1] += 1;
                }
                0xffff => {
                    assert_eq!(
                        result,
                        Err(TeletextNationalConversionError::UnprovenRecipe { subset, code })
                    );
                    counts[2] += 1;
                }
                _ => {
                    let result = result.unwrap();
                    assert_eq!(result.original(), identity);
                    if subset == TeletextNationalSubset::English && code == 0x7c {
                        assert_eq!(result.designation().get(), 0x55);
                        assert_eq!(
                            result.designation().pair(),
                            TeletextCharsetPair::HebrewArabic
                        );
                    } else {
                        assert_eq!(result.designation().get(), 0);
                        assert_eq!(result.designation().pair(), TeletextCharsetPair::LatinLatin);
                    }
                    assert_eq!(
                        operand_word(result.operation()),
                        expected,
                        "{subset:?}/{code:02x}"
                    );
                    counts[0] += 1;
                }
            }
        }
    }
    assert_eq!(counts, [165, 2, 2]);
}
#[test]
fn all_supported_results_follow_real_charset_and_intent_execution() {
    let conversions = conversions();
    let mut commands = Vec::new();
    for (index, conversion) in conversions.iter().enumerate() {
        let row = u8::try_from(index / 39 + 1).unwrap();
        let column = u8::try_from(index % 39).unwrap();
        commands.push(set(row, column));
        commands.push(at(
            column,
            TeletextColumnOperation::DesignateCharset {
                designation: conversion.designation(),
            },
        ));
        commands.push(at(column, conversion.operation()));
    }
    let service = service(commands, vec![]);
    let base = base();
    let trace = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 512,
            max_invocations: 0,
        },
    )
    .unwrap();
    let layers = build_teletext_glyph_layers(&base, &trace, 165).unwrap();
    let charset = resolve_teletext_charset_execution(&base, &trace, &layers, 512).unwrap();
    let intent = resolve_teletext_glyph_intent(&charset, 1157).unwrap();
    let enhanced: Vec<_> = intent
        .records()
        .iter()
        .filter(|record| record.context().candidate().operation().is_some())
        .collect();
    assert_eq!(enhanced.len(), 165);
    for (record, conversion) in enhanced.into_iter().zip(conversions) {
        assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
        assert_eq!(record.glyph(), Some(conversion.result()));
        assert_eq!(
            record.context().candidate().operation(),
            Some(conversion.operation())
        );
        assert!(record.reason().is_none());
    }
}
#[test]
fn malformed_codes_and_non_latin_national_repertoires_are_rejected() {
    for (subset, _) in EXPECTED {
        for code in 0..=u8::MAX {
            if !CODES.contains(&code) {
                let glyph = TeletextG0Glyph {
                    repertoire: TeletextG0Repertoire::Latin,
                    code,
                    national_subset: Some(subset),
                };
                assert_eq!(
                    resolve_teletext_national_conversion(TeletextGlyphIdentity::G0 { glyph }),
                    Err(TeletextNationalConversionError::InvalidNationalIdentity { glyph })
                );
            }
        }
        for repertoire in [
            TeletextG0Repertoire::Cyrillic1,
            TeletextG0Repertoire::Cyrillic2,
            TeletextG0Repertoire::Cyrillic3,
            TeletextG0Repertoire::Greek,
            TeletextG0Repertoire::Arabic,
            TeletextG0Repertoire::Hebrew,
        ] {
            let glyph = TeletextG0Glyph {
                repertoire,
                code: 0x23,
                national_subset: Some(subset),
            };
            assert_eq!(
                resolve_teletext_national_conversion(TeletextGlyphIdentity::G0 { glyph }),
                Err(TeletextNationalConversionError::InvalidNationalIdentity { glyph })
            );
        }
    }
}
#[test]
fn non_national_roles_do_not_acquire_a_conversion() {
    let identities = [
        TeletextGlyphIdentity::Blank,
        TeletextGlyphIdentity::AtSymbol,
        TeletextGlyphIdentity::SpacingControl { code: 0x1a },
        TeletextGlyphIdentity::G0 {
            glyph: TeletextG0Glyph {
                repertoire: TeletextG0Repertoire::Latin,
                code: 0x23,
                national_subset: None,
            },
        },
        TeletextGlyphIdentity::G2 {
            glyph: TeletextG2Glyph {
                repertoire: TeletextG2Repertoire::Latin,
                code: 0x23,
            },
        },
        TeletextGlyphIdentity::Composite {
            base: TeletextG0Glyph {
                repertoire: TeletextG0Repertoire::Latin,
                code: b'I',
                national_subset: None,
            },
            mark: TeletextG2Glyph {
                repertoire: TeletextG2Repertoire::Latin,
                code: 0x43,
            },
        },
    ];
    for identity in identities {
        assert_eq!(
            resolve_teletext_national_conversion(identity),
            Err(TeletextNationalConversionError::NonNationalIdentity { identity })
        );
    }
}
#[test]
fn proven_primary_mark_and_stroke_distinctions_are_literal() {
    let romanian =
        resolve_teletext_national_conversion(original(TeletextNationalSubset::Romanian, 0x5e))
            .unwrap();
    assert_eq!(
        romanian.result(),
        TeletextGlyphIdentity::Composite {
            base: TeletextG0Glyph {
                repertoire: TeletextG0Repertoire::Latin,
                code: b'I',
                national_subset: None
            },
            mark: TeletextG2Glyph {
                repertoire: TeletextG2Repertoire::Latin,
                code: 0x43
            },
        }
    );
    let latvian = TeletextNationalSubset::LatvianLithuanian;
    assert_eq!(
        operand_word(
            resolve_teletext_national_conversion(original(latvian, 0x5c))
                .unwrap()
                .operation()
        ),
        0x1b65
    );
    assert_eq!(
        operand_word(
            resolve_teletext_national_conversion(original(latvian, 0x7b))
                .unwrap()
                .operation()
        ),
        0x1e61
    );
    assert_eq!(
        operand_word(
            resolve_teletext_national_conversion(original(TeletextNationalSubset::Polish, 0x5d))
                .unwrap()
                .operation()
        ),
        0x0f68
    );
    assert_eq!(
        operand_word(
            resolve_teletext_national_conversion(original(TeletextNationalSubset::Polish, 0x7d))
                .unwrap()
                .operation()
        ),
        0x0f78
    );
}

fn alias_base(primary: u8) -> TeletextBaseDisplay {
    let template = base();
    let mut cells: Box<[[TeletextBaseDisplayCell; 40]; 25]> = template
        .cells()
        .to_vec()
        .into_boxed_slice()
        .try_into()
        .unwrap();
    for (column, code) in CODES.into_iter().enumerate() {
        cells[3][column] = TeletextBaseDisplayCell::new(
            TeletextEnhancementPosition::new(3, u8::try_from(column).unwrap()).unwrap(),
            code,
            TeletextBaseDisplayCellKind::Glyph,
            None,
            template.cells()[3][column].attributes(),
        )
        .unwrap();
    }
    let primary = TeletextPageCharsetDesignation::new(primary).unwrap();
    let presentation = TeletextLevel25PresentationDefaults {
        primary,
        secondary: None,
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
    };
    TeletextBaseDisplay::new(
        owner(),
        primary,
        TeletextNationalOption::new(0).unwrap(),
        Some(presentation),
        cells,
    )
    .unwrap()
}

#[test]
fn all_29_page_aliases_retain_exact_national_identity_before_conversion() {
    let aliases: [&[u8]; 13] = [
        &[0x00, 0x10, 0x40],
        &[0x01, 0x09, 0x11, 0x21],
        &[0x02, 0x0a, 0x12],
        &[0x03, 0x0b, 0x13],
        &[0x04, 0x0c, 0x14, 0x44],
        &[0x05, 0x15],
        &[0x06, 0x0e, 0x26],
        &[0x08],
        &[0x16, 0x36],
        &[0x1d],
        &[0x1f],
        &[0x22],
        &[0x23],
    ];
    let service = service(vec![], vec![]);
    let trace = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 0,
            max_invocations: 0,
        },
    )
    .unwrap();
    let mut alias_count = 0;
    for ((subset, _), aliases) in EXPECTED.into_iter().zip(aliases) {
        for &primary in aliases {
            let base = alias_base(primary);
            let layers = build_teletext_glyph_layers(&base, &trace, 0).unwrap();
            let charset = resolve_teletext_charset_execution(&base, &trace, &layers, 0).unwrap();
            let intent = resolve_teletext_glyph_intent(&charset, 992).unwrap();
            for (column, code) in CODES.into_iter().enumerate() {
                let record = intent
                    .records()
                    .iter()
                    .find(|record| {
                        record.context().candidate().base().is_some_and(|cell| {
                            cell.position().row() == 3
                                && usize::from(cell.position().column()) == column
                        })
                    })
                    .unwrap();
                assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
                assert_eq!(record.glyph(), Some(original(subset, code)));
                let converted = resolve_teletext_national_conversion(record.glyph().unwrap());
                assert_eq!(
                    converted,
                    resolve_teletext_national_conversion(original(subset, code))
                );
            }
            alias_count += 1;
        }
    }
    assert_eq!(alias_count, 29);
}

fn single_conversion_intent(designation: u8) -> TeletextExecutedGlyphIntent {
    let conversion =
        resolve_teletext_national_conversion(original(TeletextNationalSubset::English, 0x7c))
            .unwrap();
    let service = service(
        vec![
            set(1, 8),
            at(
                8,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(designation).unwrap(),
                },
            ),
            at(8, conversion.operation()),
        ],
        vec![],
    );
    let base = base();
    let trace = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 3,
            max_invocations: 0,
        },
    )
    .unwrap();
    let layers = build_teletext_glyph_layers(&base, &trace, 1).unwrap();
    let charset = resolve_teletext_charset_execution(&base, &trace, &layers, 3).unwrap();
    let intent = resolve_teletext_glyph_intent(&charset, 993).unwrap();
    *intent
        .records()
        .iter()
        .find(|record| record.context().candidate().operation().is_some())
        .unwrap()
}

#[test]
fn english_double_vertical_requires_explicit_hebrew_context_not_latin_alias() {
    // Primary Table36 English7/C and Table46 Hebrew7/C identify the double line.
    let identity = original(TeletextNationalSubset::English, 0x7c);
    let conversion = resolve_teletext_national_conversion(identity).unwrap();
    assert_eq!(conversion.original(), identity);
    assert_eq!(conversion.designation().get(), 0x55);
    assert_eq!(
        conversion.operation(),
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(0x7c).unwrap(),
        }
    );
    assert_eq!(
        conversion.result(),
        TeletextGlyphIdentity::G0 {
            glyph: TeletextG0Glyph {
                repertoire: TeletextG0Repertoire::Hebrew,
                code: 0x7c,
                national_subset: None
            },
        }
    );
    let actual = single_conversion_intent(0x55);
    assert_eq!(actual.disposition(), TeletextGlyphDisposition::Assigned);
    assert_eq!(actual.glyph(), Some(conversion.result()));
    let wrong = single_conversion_intent(0);
    assert_eq!(
        wrong.glyph(),
        Some(TeletextGlyphIdentity::G0 {
            glyph: TeletextG0Glyph {
                repertoire: TeletextG0Repertoire::Latin,
                code: 0x7c,
                national_subset: None
            },
        })
    );
    assert_ne!(wrong.glyph(), actual.glyph());
}

#[test]
fn generated_hebrew_designation_cannot_bypass_full_display_pair_limit() {
    let greek = at(
        8,
        TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(0x37).unwrap(),
        },
    );
    let original_commands = vec![
        set(1, 8),
        greek,
        at(
            9,
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'A').unwrap(),
            },
        ),
    ];
    let service = service(original_commands.clone(), vec![]);
    let original_program = TeletextEnhancementProgram::new(original_commands.clone()).unwrap();
    let budget = TeletextEnhancementCollectionBudget {
        max_commands: 6,
        max_drcs_references: 0,
    };
    let collected = service
        .objects()
        .collect_display_references(
            owner(),
            &original_program,
            TeletextCharsetPair::LatinLatin,
            budget,
        )
        .unwrap();
    assert_eq!(collected.charset_pairs().len(), 2);
    let conversion =
        resolve_teletext_national_conversion(original(TeletextNationalSubset::English, 0x7c))
            .unwrap();
    let mut actual_commands = original_commands;
    actual_commands.extend([
        set(1, 10),
        at(
            10,
            TeletextColumnOperation::DesignateCharset {
                designation: conversion.designation(),
            },
        ),
        at(10, conversion.operation()),
    ]);
    let actual = TeletextEnhancementProgram::new(actual_commands).unwrap();
    assert_eq!(
        service.objects().collect_display_references(
            owner(),
            &actual,
            TeletextCharsetPair::LatinLatin,
            budget
        ),
        Err(TeletextEnhancementCollectionError {
            display: owner(),
            source_location: TeletextEnhancementSource {
                object: None,
                command: 4
            },
            reason: TeletextEnhancementCollectionFailure::CharsetPairs {
                pair: TeletextCharsetPair::HebrewArabic,
            },
        })
    );
    assert_eq!(
        service.effective_program(owner()).unwrap().1.commands(),
        original_program.commands()
    );
}

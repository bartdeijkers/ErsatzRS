//! Fixed AB1/AB2 scalar qualification, distinct from native recipes/font aliases.
use super::*;

const ACCENTS: &str = "ˋˊˆ˜ˉ˘˙¨ˌ˚¸_˝˛ˇ";
// Table36 identities, corroborated by the pinned editor's explicit keymap.
// Polish5B is the rendered strokedZ, not its dottedZ keyboard substitution.
const NATIONAL_CASES: [(u8, char, u8); 8] = [
    (0x1f, '¤', 0x24),
    (0x1f, 'Î', 0x5e),
    (0x23, 'ȩ', 0x5c),
    (0x08, 'Ƶ', 0x5b),
    (0x1d, 'Đ', 0x5d),
    (0x1d, 'đ', 0x7d),
    (0x1f, 'Ă', 0x5d),
    (0x1f, 'ă', 0x7d),
];

pub(super) fn text(primary: u8, value: &str) -> TeletextDeclaredService {
    let mut source = draft(primary);
    source.base.pages[0].rows = rows(value);
    declare(source)
}

pub(super) fn check_preview(source: &TeletextDeclaredService) {
    let composition = compose_teletext_main_program(
        source.semantic(),
        owner(0),
        TeletextMainCompositionBudget {
            max_input_commands: 64,
            max_output_commands: 64,
            collection: TeletextEnhancementCollectionBudget {
                max_commands: 128,
                max_drcs_references: 0,
            },
        },
    )
    .unwrap();
    let raster = render_teletext_composition(
        &composition,
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextRasterBudget {
            max_pixels: 120_000,
            max_diagnostics: 1400,
        },
    )
    .unwrap();
    assert!(raster.diagnostics().is_empty());
}

#[test]
fn qualified_national_aliases_keep_native_fallback_and_need_no_enhancements() {
    let mut wrong = Vec::new();
    for (primary, scalar, code) in NATIONAL_CASES {
        let source = text(primary, &scalar.to_string());
        let service = source.semantic();
        let layout =
            match layout::compile_for_service_composition(service, &service.base().pages[0]) {
                Ok(layout) => layout,
                Err(error) => {
                    wrong.push(format!("{primary:02x}/{scalar}: {error:?}"));
                    continue;
                }
            };
        if layout.rows[1][8] != code || !layout.base_glyphs().is_empty() {
            wrong.push(format!(
                "{primary:02x}/{scalar}: byte{:02x},{} enhancements",
                layout.rows[1][8],
                layout.base_glyphs().len()
            ));
            continue;
        }
        check_preview(&source);
        let encoded = encode_teletext_authored_service(source.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &source);
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(encoded.magazine_units().is_empty());
        assert!(matches!(
            encoded.pages()[0],
            EncodedTeletextAuthoredPage::NativeDisplay(_)
        ));
        let row = encoded.pages()[0]
            .data_units()
            .iter()
            .find(|unit| packet(unit) == 1)
            .unwrap();
        assert_eq!(body(row)[8] & 0x7f, code);
        assert!(!encoded.pages()[0]
            .data_units()
            .iter()
            .any(|unit| packet(unit) == 26));
    }
    assert!(
        wrong.is_empty(),
        "qualified native aliases lost their Level1 cell: {wrong:?}"
    );
}

#[test]
fn assigned_spacing_accents_keep_context_and_literal_blank_base_fallback() {
    let mut rejected = Vec::new();
    let mut cases = Vec::new();
    for primary in [0, 0x20, 0x24, 0x25, 0x37] {
        let source = text(primary, ACCENTS);
        check_preview(&source);
        let service = source.semantic();
        let layout =
            match layout::compile_for_service_composition(service, &service.base().pages[0]) {
                Ok(layout) => layout,
                Err(error) => {
                    rejected.push((primary, format!("{error:?}")));
                    continue;
                }
            };
        assert_eq!(&layout.rows[1][8..23], &[b' '; 15]);
        assert_eq!(layout.base_glyphs().len(), 15);
        let encoded = encode_teletext_authored_service(source.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &source);
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(encoded.magazine_units().is_empty());
        let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
            panic!("spacing diacritics need their explicit X26 glyphs");
        };
        let mut units = encoded.pages()[0].data_units().to_vec();
        units.push(super::super::super::packets::terminator(2));
        let points: Vec<_> = ACCENTS
            .chars()
            .enumerate()
            .map(|(offset, scalar)| {
                json!({
                    "row": 1, "column": 8 + offset, "unicode": u32::from(scalar), "fallback": 32,
                })
            })
            .collect();
        cases.push(
            json!({"name": "assigned_spacing_accents", "primary": primary,
            "page": 0x200, "subcode": 0, "points": points,
            "units": units.iter().map(|unit| unit.to_vec()).collect::<Vec<_>>() }),
        );
        let composition = page.normalized().original();
        for (offset, scalar) in ACCENTS.chars().enumerate() {
            let column = offset + 8;
            let wire_column = u8::try_from(column).unwrap();
            let expected = if primary == 0 && scalar == '_' {
                TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(0x5f).unwrap(),
                    diacritic: TeletextDiacriticIndex::new(0).unwrap(),
                }
            } else {
                TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(0x41 + u8::try_from(offset).unwrap()).unwrap(),
                }
            };
            assert_eq!(composition.base_replacement(1, wire_column), Some(expected));
            assert_eq!(
                composition
                    .base_replacement_charset(1, wire_column)
                    .unwrap()
                    .get(),
                primary
            );
            assert_eq!(
                composition.base_display().cells()[1][column]
                    .authored()
                    .unwrap()
                    .content(),
                TeletextBaseAuthoredContent::Text { character: scalar }
            );
        }
    }
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_UNICODE_ELIGIBILITY_FIXTURE"])
    {
        for (primary, scalar, _) in NATIONAL_CASES {
            let encoded = encode_teletext_authored_service(
                text(primary, &scalar.to_string()),
                &[],
                &limits(),
            )
            .unwrap();
            let mut units = encoded.pages()[0].data_units().to_vec();
            units.push(super::super::super::packets::terminator(2));
            cases.push(json!({"name": "qualified_national", "primary": primary,
                "page": 0x200, "subcode": 0, "points": [{"row": 1, "column": 8,
                    "unicode": u32::from(scalar), "fallback": u32::from(scalar)}],
                "units": units.iter().map(|unit| unit.to_vec()).collect::<Vec<_>>() }));
        }
        std::fs::write(path, serde_json::to_vec(&json!({"cases": cases})).unwrap()).unwrap();
    }
    assert!(
        rejected.is_empty(),
        "assigned spacing accents were rejected: {rejected:?}"
    );
}

#[test]
fn spacing_accent_inverse_keeps_combining_input_and_unqualified_slots_distinct() {
    use crate::teletext::charset;
    for pair in [
        TeletextCharsetPair::LatinLatin,
        TeletextCharsetPair::Cyrillic1Cyrillic,
        TeletextCharsetPair::Cyrillic2Cyrillic,
        TeletextCharsetPair::Cyrillic3Cyrillic,
        TeletextCharsetPair::GreekGreek,
    ] {
        for (offset, scalar) in ACCENTS.chars().enumerate() {
            assert_eq!(
                charset::decode_base_text_spelling(
                    0x0f,
                    0x41 + u8::try_from(offset).unwrap(),
                    pair
                ),
                Some(scalar)
            );
        }
        assert_eq!(charset::decode_base_text_spelling(0x0f, 0x40, pair), None);
        if pair != TeletextCharsetPair::LatinLatin {
            assert_eq!(
                charset::decode_base_text_spelling(0x0f, 0x58, pair),
                Some('α')
            );
        }
    }
    assert_eq!(
        charset::decode_base_text_spelling(0x0f, 0x7f, TeletextCharsetPair::GreekGreek),
        Some('■')
    );
    for pair in [
        TeletextCharsetPair::Cyrillic1Cyrillic,
        TeletextCharsetPair::Cyrillic2Cyrillic,
        TeletextCharsetPair::Cyrillic3Cyrillic,
    ] {
        assert_eq!(
            charset::decode_base_text_spelling(0x0f, 0x7f, pair),
            Some('z')
        );
    }
    for primary in [0, 0x20, 0x24, 0x25, 0x37] {
        let source = text(primary, "\u{0300}");
        assert!(encode_teletext_authored_service(source.clone(), &[], &limits()).is_err());
        assert_eq!(
            source.semantic().base().pages[0].rows[0].runs[0].content,
            TeletextRunContent::Text {
                text: "\u{0300}".into()
            }
        );
    }
    assert!(charset::encode_native_g2('ˋ', TeletextCharsetPair::ArabicArabic).is_none());
}

#[test]
fn national_aliases_preserve_diacritics_eth_and_legacy_lira_distinctions() {
    for (primary, scalar, fallback, mode, code) in [
        (0x08, 'Ż', b'Z', 0x17, b'Z'),
        (0x1d, 'Ð', b' ', 0x0f, 0x62),
        (0x1d, 'ð', b' ', 0x0f, 0x73),
        (0x1f, 'Ǎ', b'A', 0x1f, b'A'),
        (0x1f, 'ǎ', b'a', 0x1f, b'a'),
    ] {
        let source = text(primary, &scalar.to_string());
        let service = source.semantic();
        let layout =
            layout::compile_for_service_composition(service, &service.base().pages[0]).unwrap();
        assert_eq!(layout.rows[1][8], fallback);
        assert_eq!(layout.base_glyphs().len(), 1);
        let expected = if mode == 0x0f {
            TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(code).unwrap(),
            }
        } else {
            TeletextColumnOperation::G0Diacritic {
                code: TeletextGlyphCode::new(code).unwrap(),
                diacritic: TeletextDiacriticIndex::new(mode & 15).unwrap(),
            }
        };
        assert_eq!(
            TeletextColumnOperation::from(&layout.base_glyphs()[0]),
            expected
        );
        let encoded = encode_teletext_authored_service(source.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &source);
    }
    let source = text(0x16, "₺");
    let error = encode_teletext_authored_service(source.clone(), &[], &limits()).unwrap_err();
    assert!(matches!(&error.reason,Failure::Layout(error)
        if matches!(error.as_ref(),TeletextEncodingError::Character {address,row:1,column:8,codepoint}
            if *address==owner(0) && *codepoint==0x20ba)));
    assert_eq!(
        source.semantic().base().pages[0].rows[0].runs[0].content,
        TeletextRunContent::Text { text: "₺".into() }
    );
    let source = text(0x16, "TL");
    let service = source.semantic();
    let layout =
        layout::compile_for_service_composition(service, &service.base().pages[0]).unwrap();
    assert_eq!(&layout.rows[1][8..10], b"TL");
    assert!(layout.base_glyphs().is_empty());
}

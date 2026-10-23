//! Explicit Arabic presentation forms retain the supplied physical cell order.
use super::*;

fn original(primary: u8, text: &str) -> TeletextDeclaredService {
    let mut input = draft(primary);
    input.base.pages[0].rows = vec![TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: text.into() },
        }],
    }];
    declare(input)
}

#[test]
fn native_arabic_presentation_form_headline_preserves_authored_order() {
    for primary in [0x47, 0x57] {
        // Explicit cells, including the Table44 lam-alef ligature at7/E.
        // The author supplies their order; no shaping or bidi is requested.
        let original = original(primary, "\u{fe8d}\u{fedf}\u{fed3}\u{fe8f}\u{fefb} 200");
        let encoded = encode_teletext_authored_service(original.clone(), &[], &limits())
            .expect("explicit Arabic forms must encode in their declared native context");
        assert_eq!(encoded.declared(), &original);
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(original.resource_inventory().is_empty());
        assert!(matches!(
            encoded.pages()[0],
            EncodedTeletextAuthoredPage::NativeDisplay(_)
        ));
        let row = encoded.pages()[0]
            .data_units()
            .iter()
            .find(|unit| packet(unit) == 1)
            .unwrap();
        assert_eq!(
            &body(row).map(|code| code & 0x7f)[8..17],
            &[0x47, 0x64, 0x61, 0x43, 0x7e, 0x20, 0x32, 0x30, 0x30]
        );
    }
}

fn points(supplementary: bool) -> Vec<(char, u8)> {
    // EN300706 Tables44/45 printed glyphs, identified through Unicode Arabic
    // Presentation Forms-A/B names. These are explicit forms, not base-letter
    // aliases or a mapping copied from a decoder's private-use font slots.
    let columns = if supplementary {
        [
            concat!(
                "\u{fec9}\u{fe81}\u{fe83}\u{fe85}\u{fe87}\u{fe8b}\u{fe89}",
                "\u{fb7c}\u{fb7d}\u{fb7a}\u{fb58}\u{fb59}\u{fb56}\u{fb8a}\u{fb94}"
            ),
            concat!(
                "٠١٢٣٤٥٦٧٨٩",
                "\u{fece}\u{fecd}\u{fefc}\u{feec}\u{feea}\u{fee9}"
            ),
            "àABCDEFGHIJKLMNO",
            "PQRSTUVWXYZëêùî\u{feca}",
            "éabcdefghijklmno",
            "pqrstuvwxyzâôûç",
        ]
    } else {
        [
            " !\"£$%\u{fe73}\u{fef1})(*+،-./",
            "0123456789:؛>=<؟",
            concat!(
                "\u{fe94}\u{fe80}\u{fe92}\u{fe8f}",
                "\u{fe98}\u{fe95}\u{fe8e}\u{fe8d}",
                "\u{fe91}\u{fe93}\u{fe97}\u{fe9b}",
                "\u{fe9f}\u{fea3}\u{fea7}\u{fea9}"
            ),
            concat!(
                "\u{feab}\u{fead}\u{feaf}\u{feb3}",
                "\u{feb7}\u{febb}\u{febf}\u{fec3}",
                "\u{fec7}\u{fecb}\u{fecf}\u{fe9c}",
                "\u{fea0}\u{fea4}\u{fea8}#"
            ),
            concat!(
                "ـ\u{fed3}\u{fed7}\u{fedb}",
                "\u{fedf}\u{fee3}\u{fee7}\u{feeb}",
                "\u{feed}\u{fef0}\u{fef3}\u{fe99}",
                "\u{fe9d}\u{fea1}\u{fea5}\u{fef4}"
            ),
            concat!(
                "\u{feef}\u{fecc}\u{fed0}\u{fed4}",
                "\u{fed1}\u{fed8}\u{fed5}\u{fed9}",
                "\u{fee0}\u{fedd}\u{fee4}\u{fee1}",
                "\u{fee8}\u{fee5}\u{fefb}"
            ),
        ]
    };
    columns
        .into_iter()
        .enumerate()
        .flat_map(|(column, text)| {
            text.chars().enumerate().map(move |(offset, character)| {
                let start = if supplementary && column == 0 {
                    0x21
                } else {
                    0x20 + column * 16
                };
                (character, u8::try_from(start + offset).unwrap())
            })
        })
        .collect()
}

fn matrix_original(primary: u8, points: &[(char, u8)]) -> TeletextDeclaredService {
    let mut input = draft(primary);
    input.base.pages[0].rows = points
        .chunks(36)
        .enumerate()
        .map(|(index, chunk)| TeletextRow {
            row: u8::try_from(index + 1).unwrap(),
            runs: vec![TeletextRun {
                column: 1,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text {
                    text: chunk.iter().map(|(character, _)| character).collect(),
                },
            }],
        })
        .collect();
    declare(input)
}

fn coded_reference(
    primary: u8,
    points: &[(char, u8)],
    supplementary: bool,
) -> TeletextDeclaredService {
    let mut input = draft(primary);
    input.base.pages[0].rows.clear();
    if supplementary {
        input.pages[0].local_program = Some(
            TeletextEnhancementProgram::new(
                points
                    .chunks(36)
                    .enumerate()
                    .flat_map(|(index, chunk)| {
                        let row = u8::try_from(index + 1).unwrap();
                        std::iter::once(TeletextEnhancementCommand::SetPosition {
                            position: TeletextEnhancementPosition::new(row, 1).unwrap(),
                        })
                        .chain(chunk.iter().enumerate().map(|(offset, (_, code))| {
                            TeletextEnhancementCommand::AtColumn {
                                column: u8::try_from(offset + 1).unwrap(),
                                operation: TeletextColumnOperation::G2 {
                                    code: TeletextGlyphCode::new(*code).unwrap(),
                                },
                            }
                        }))
                        .collect::<Vec<_>>()
                    })
                    .collect(),
            )
            .unwrap(),
        );
        return declare(input);
    }
    declare(input)
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(0),
                points
                    .chunks(36)
                    .enumerate()
                    .map(|(index, chunk)| {
                        let mut cells = [b' '; 40];
                        for (offset, (_, code)) in chunk.iter().enumerate() {
                            cells[offset + 1] = *code;
                        }
                        TeletextPhysicalBaseRow::new(u8::try_from(index + 1).unwrap(), cells)
                            .unwrap()
                    })
                    .collect(),
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap()
}

fn preview(original: &TeletextDeclaredService) -> TeletextMainComposition {
    let composition = compose_teletext_main_program(
        original.semantic(),
        owner(0),
        TeletextMainCompositionBudget {
            max_input_commands: 512,
            max_output_commands: 512,
            collection: TeletextEnhancementCollectionBudget {
                max_commands: 512,
                max_drcs_references: 0,
            },
        },
    )
    .unwrap();
    assert!(raster(&composition).diagnostics().is_empty());
    composition
}

fn raster(composition: &TeletextMainComposition) -> TeletextRaster {
    render_teletext_composition(
        composition,
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextRasterBudget {
            max_pixels: 120_000,
            max_diagnostics: 1400,
        },
    )
    .unwrap()
}

fn check_cell(
    composition: &TeletextMainComposition,
    primary: u8,
    supplementary: bool,
    index: usize,
    point: (char, u8),
) -> Value {
    let (character, code) = point;
    let row = u8::try_from(index / 36 + 1).unwrap();
    let column = u8::try_from(index % 36 + 1).unwrap();
    let cell = composition.base_display().cells()[usize::from(row)][usize::from(column)];
    let fallback = if supplementary { b' ' } else { code };
    assert_eq!(cell.code(), fallback);
    assert!(matches!(cell.authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text {character: actual} if actual == character));
    if supplementary {
        assert!(
            matches!(composition.base_replacement_identity(row, column).unwrap().unwrap(),
            TeletextGlyphIdentity::G2 {glyph}
            if glyph.repertoire() == TeletextG2Repertoire::Arabic && glyph.code() == code)
        );
        assert_eq!(
            composition
                .base_replacement_charset(row, column)
                .unwrap()
                .get(),
            primary
        );
    } else if code != 0x20 {
        let candidate = composition
            .glyph_layers()
            .candidates()
            .iter()
            .position(|candidate| {
                candidate.base().is_some_and(|base| {
                    base.position() == TeletextEnhancementPosition::new(row, column).unwrap()
                })
            })
            .unwrap();
        let record = composition.glyph_intent().records()[candidate];
        assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
        assert_eq!(record.reason(), None);
        assert!(
            matches!(record.glyph(), Some(TeletextGlyphIdentity::G0 {glyph})
            if glyph.repertoire() == TeletextG0Repertoire::Arabic
            && glyph.code() == code && glyph.national_subset().is_none())
        );
    }
    json!({"row":row,"column":column,"code":code,
        "unicode":u32::from(character),"fallback":fallback})
}

fn matrix(primary: u8, supplementary: bool) -> Value {
    let expected = points(supplementary);
    let first = if supplementary { 0x21 } else { 0x20 };
    assert_eq!(expected.len(), if supplementary { 94 } else { 95 });
    assert_eq!(
        expected.iter().map(|(_, code)| *code).collect::<Vec<_>>(),
        (first..=0x7e).collect::<Vec<_>>()
    );
    let original = matrix_original(primary, &expected);
    let reference = coded_reference(primary, &expected, supplementary);
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), &original);
    assert!(original.resource_inventory().is_empty());
    assert!(encoded.compiler_drcs_resources().is_empty());
    assert!(encoded.magazine_units().is_empty());
    let composition = preview(&original);
    assert_eq!(raster(&composition), raster(&preview(&reference)));
    if supplementary {
        assert!(matches!(
            encoded.pages()[0],
            EncodedTeletextAuthoredPage::CompiledDisplay(_)
        ));
        assert!(encoded.pages()[0]
            .data_units()
            .iter()
            .any(|unit| packet(unit) == 26));
    } else {
        assert!(matches!(
            encoded.pages()[0],
            EncodedTeletextAuthoredPage::NativeDisplay(_)
        ));
        let coded = encode_teletext_authored_service(reference, &[], &limits()).unwrap();
        assert_eq!(
            encoded.pages()[0].data_units(),
            coded.pages()[0].data_units()
        );
    }
    let positions = expected
        .iter()
        .enumerate()
        .map(|(index, point)| check_cell(&composition, primary, supplementary, index, *point))
        .collect::<Vec<_>>();
    for (index, chunk) in expected.chunks(36).enumerate() {
        let row = encoded.pages()[0]
            .data_units()
            .iter()
            .find(|unit| usize::from(packet(unit)) == index + 1)
            .unwrap();
        let fallback = chunk
            .iter()
            .map(|(_, code)| if supplementary { b' ' } else { *code })
            .collect::<Vec<_>>();
        assert_eq!(
            &body(row).map(|code| code & 0x7f)[1..=chunk.len()],
            fallback
        );
    }
    let mut units = encoded.pages()[0].data_units().to_vec();
    units.push(super::super::super::super::packets::terminator(2));
    json!({"name":if supplementary {"arabic_g2_text"} else {"arabic_g0_text"},
        "primary":primary,"page":0x200,"subcode":0,"points":positions,
        "units":units.iter().map(|unit| unit.to_vec()).collect::<Vec<_>>()})
}

#[test]
fn native_arabic_full_tables_preserve_originals_wire_pixels_and_namespace() {
    let cases = [0x47, 0x57]
        .into_iter()
        .flat_map(|primary| [matrix(primary, false), matrix(primary, true)])
        .collect::<Vec<_>>();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_ARABIC_TEXT_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&json!({"cases":cases})).unwrap()).unwrap();
    }
}

#[test]
fn native_arabic_unknown_and_whole_seen_forms_fail_at_original_cell() {
    for primary in [0x47, 0x57] {
        // Whole isolated/final Seen-family letters require their tail cell.
        // They must not collapse to a one-cell initial/medial body alias.
        for character in concat!(
            "ا\u{301}\u{651}\u{e620}😀",
            "\u{feb1}\u{feb2}\u{feb5}\u{feb6}",
            "\u{feb9}\u{feba}\u{febd}\u{febe}"
        )
        .chars()
        {
            let original = original(primary, &format!("\u{fe8d}{character}"));
            let before = original.clone();
            let Err(failure) = encode_teletext_authored_service(original.clone(), &[], &limits())
            else {
                panic!("unexpected Arabic alias {primary:02X}/{character:?}");
            };
            assert!(
                matches!(&failure.reason,Failure::Layout(error)
                if matches!(error.as_ref(), TeletextEncodingError::Character {address,row:1,column:9,codepoint}
                    if *address == owner(0) && *codepoint == u32::from(character))),
                "{primary:02X}/{character:?}: {failure:?}"
            );
            assert_eq!(original, before);
        }
    }
}

#[test]
fn native_arabic_requires_matching_explicit_context_and_retains_row_bounds() {
    for primary in [0x47, 0x57] {
        for character in ['\u{fe8d}', '\u{fe81}'] {
            let original = original(primary, &character.to_string());
            let page = &original.semantic().base().pages[0];
            let mut mismatch = page.clone();
            mismatch.national_option = TeletextNationalOption::new(0).unwrap();
            let latin = declare(draft(0));
            assert!(matches!(
                layout::compile_for_service_composition(original.semantic(), &mismatch),
                Err(TeletextEncodingError::Character {address,row:1,column:8,codepoint})
                if address == owner(0) && codepoint == u32::from(character)
            ));
            // Region-zero header7 remains reserved without a matching explicit
            // presentation; its established address-level rejection comes first.
            for failure in [
                layout::compile_for_service_composition(latin.semantic(), page),
                layout::compile(page),
            ] {
                assert!(matches!(failure,
                    Err(TeletextEncodingError::NationalOption {address})
                    if address == owner(0)));
            }
            let mut overflow = page.clone();
            overflow.rows[0].runs[0].column = 39;
            overflow.rows[0].runs[0].content = TeletextRunContent::Text {
                text: format!("{character}{character}"),
            };
            assert!(
                layout::compile_for_service_composition(original.semantic(), &overflow).is_err()
            );
            assert_eq!(page.rows[0].runs[0].column, 8);
        }
    }
}

#[test]
fn native_arabic_mixed_g0_g2_and_at_keep_exact_context_and_source_order() {
    for primary in [0x47, 0x57] {
        let text = "\u{fe8d}\u{fe81}@\u{fefb}";
        let original = original(primary, text);
        let composition = preview(&original);
        assert_eq!(composition.base_display().cells()[1][8].code(), 0x47);
        assert_eq!(composition.base_display().cells()[1][11].code(), 0x7e);
        for (column, identity) in [(9, Some(0x22)), (10, None)] {
            assert_eq!(composition.base_display().cells()[1][column].code(), b' ');
            let replacement = composition
                .base_replacement_identity(1, u8::try_from(column).unwrap())
                .unwrap()
                .unwrap();
            if let Some(code) = identity {
                assert!(matches!(replacement, TeletextGlyphIdentity::G2 {glyph}
                    if glyph.repertoire() == TeletextG2Repertoire::Arabic && glyph.code() == code));
            } else {
                assert_eq!(replacement, TeletextGlyphIdentity::AtSymbol);
            }
            assert_eq!(
                composition
                    .base_replacement_charset(1, u8::try_from(column).unwrap())
                    .unwrap()
                    .get(),
                primary
            );
        }
        let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &original);
        assert_eq!(
            original.semantic().base().pages[0].rows[0].runs[0].content,
            TeletextRunContent::Text { text: text.into() }
        );
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(original.resource_inventory().is_empty());
    }
}

#[test]
fn native_arabic_spelling_rejects_changed_meaning_foreign_charset_and_unmarked_cell() {
    for primary in [0x47, 0x57] {
        for (code, charset, operation, fallback) in [
            (0x23, primary, 0x0f, b' '), // Wrong letter for the retained scalar.
            (0x22, 0x37, 0x0f, b' '),    // Foreign repertoire.
            (0x7f, primary, 0x0f, b' '), // Unmarked Arabic G2/7F.
            (0x22, primary, 0x10, b' '), // G0 quote is not the G2 letter.
            (0x47, primary, 0x11, b' '), // No Arabic diacritic composition.
        ] {
            let original = original(primary, "\u{fe81}");
            let spelling = TeletextBaseTextSpelling::new(
                owner(0),
                TeletextEnhancementPosition::new(1, 8).unwrap(),
                '\u{fe81}',
                TeletextGlyphCode::new(fallback).unwrap(),
                TeletextEnhancementCharsetDesignation::new(charset).unwrap(),
                if operation == 0x0f {
                    TeletextColumnOperation::G2 {
                        code: TeletextGlyphCode::new(code).unwrap(),
                    }
                } else {
                    TeletextColumnOperation::G0Diacritic {
                        code: TeletextGlyphCode::new(code).unwrap(),
                        diacritic: TeletextDiacriticIndex::new(operation & 15).unwrap(),
                    }
                },
            )
            .unwrap();
            let hinted = original
                .clone()
                .with_base_text_spellings(vec![spelling], semantic_limits())
                .unwrap();
            let before = hinted.clone();
            let Err(error) = encode_teletext_authored_service(hinted.clone(), &[], &limits())
            else {
                panic!("invalid Arabic spelling {primary:02X}/{charset:02X}/{operation:02X}/{code:02X}");
            };
            assert!(
                format!("{error:?}").contains("BaseTextSpelling"),
                "{error:?}"
            );
            assert_eq!(hinted, before);
            assert_eq!(
                original.semantic().base().pages[0].rows[0].runs[0].content,
                TeletextRunContent::Text {
                    text: "\u{fe81}".into()
                }
            );
        }
    }
}

#[test]
fn native_arabic_explicit_spelling_retains_printable_fallback_and_exact_g2_meaning() {
    for primary in [0x47, 0x57] {
        // Typed native spelling deliberately retains its raw printable fallback.
        // Only ordinary generated G2 spelling defaults to SPACE; editor authority
        // for replacing a protected physical cell is a separate existing gate.
        let spelling = TeletextBaseTextSpelling::new(
            owner(0),
            TeletextEnhancementPosition::new(1, 8).unwrap(),
            '\u{fe81}',
            TeletextGlyphCode::new(b'X').unwrap(),
            TeletextEnhancementCharsetDesignation::new(primary).unwrap(),
            TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x22).unwrap(),
            },
        )
        .unwrap();
        let original = original(primary, "\u{fe81}")
            .with_base_text_spellings(vec![spelling], semantic_limits())
            .unwrap();
        let composition = preview(&original);
        let cell = composition.base_display().cells()[1][8];
        assert_eq!(cell.code(), b'X');
        assert_eq!(
            cell.authored().unwrap().content(),
            TeletextBaseAuthoredContent::Text {
                character: '\u{fe81}'
            }
        );
        assert!(
            matches!(composition.base_replacement_identity(1, 8).unwrap().unwrap(),
            TeletextGlyphIdentity::G2 {glyph}
            if glyph.repertoire() == TeletextG2Repertoire::Arabic && glyph.code() == 0x22)
        );
        assert_eq!(
            composition.base_replacement_charset(1, 8).unwrap().get(),
            primary
        );
        let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &original);
        let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
            panic!("explicit Arabic G2 spelling retains its enhancement")
        };
        assert_eq!(
            page.normalized().original().base_replacement(1, 8),
            Some(spelling.operation())
        );
        let row = encoded.pages()[0]
            .data_units()
            .iter()
            .find(|unit| packet(unit) == 1)
            .unwrap();
        assert_eq!(body(row)[8] & 0x7f, b'X');
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(original.resource_inventory().is_empty());
    }
}

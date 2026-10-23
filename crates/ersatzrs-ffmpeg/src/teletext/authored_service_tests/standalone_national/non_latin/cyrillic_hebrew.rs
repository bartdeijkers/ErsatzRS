//! Native Unicode stays in the declared G0 namespace and supplied cell order.
use super::*;

fn headline(primary: u8, text: &str, expected: &[u8]) {
    let mut input = draft(primary);
    input.base.pages[0].rows = vec![TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: text.into() },
        }],
    }];
    let original = declare(input);
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits())
        .expect("literal native Unicode must encode in its assigned G0 context");
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
        &body(row).map(|code| code & 0x7f)[8..8 + expected.len()],
        expected
    );
}

#[test]
fn native_cyrillic1_headline() {
    headline(
        0x20,
        "Београд 200",
        &[
            0x42, 0x65, 0x6f, 0x67, 0x72, 0x61, 0x64, 0x20, 0x32, 0x30, 0x30,
        ],
    );
}

#[test]
fn native_cyrillic2_headline() {
    headline(
        0x24,
        "Москва 200",
        &[0x4d, 0x6f, 0x73, 0x6b, 0x77, 0x61, 0x20, 0x32, 0x30, 0x30],
    );
}

#[test]
fn native_cyrillic3_headline() {
    headline(
        0x25,
        "Київ 200",
        &[0x4b, 0x69, 0x26, 0x77, 0x20, 0x32, 0x30, 0x30],
    );
}

#[test]
fn native_hebrew_headline() {
    // Text is already in authored cell order; the engine must not reorder RTL.
    headline(
        0x55,
        "שלום 200",
        &[0x79, 0x6c, 0x65, 0x6d, 0x20, 0x32, 0x30, 0x30],
    );
}

fn points(primary: u8) -> Vec<(char, u8)> {
    // Tables38–40,46 read by column. No aliases derived from decoder tables.
    let columns = match primary {
        0x20 => [
            " !\"#$%&'()*+,-./",
            "0123456789:;<=>?",
            "ЧАБЦДЕФГХИЈКЛМНО",
            "ПЌРСТУВЃЉЊЗЋЖЂШЏ",
            "чабцдефгхијклмно",
            "пќрстувѓљњзћжђш",
        ],
        0x24 => [
            " !\"#$%ы'()*+,-./",
            "0123456789:;<=>?",
            "ЮАБЦДЕФГХИЙКЛМНО",
            "ПЯРСТУЖВЬЪЗШЭЩЧЫ",
            "юабцдефгхийклмно",
            "пярстужвьъзшэщч",
        ],
        0x25 => [
            " !\"#$%ї'()*+,-./",
            "0123456789:;<=>?",
            "ЮАБЦДЕФГХИЙКЛМНО",
            "ПЯРСТУЖВЬІЗШЄЩЧЇ",
            "юабцдефгхийклмно",
            "пярстужвьізшєщч",
        ],
        0x55 => [
            " !\"£$%&'()*+,-./",
            "0123456789:;<=>?",
            "@ABCDEFGHIJKLMNO",
            "PQRSTUVWXYZ←½→↑#",
            "אבגדהוזחטיךכלםמן",
            "נסעףפץצקרשת₪‖¾÷",
        ],
        _ => panic!("qualified context"),
    };
    columns
        .into_iter()
        .enumerate()
        .flat_map(|(column, text)| {
            text.chars()
                .enumerate()
                .map(move |(row, ch)| (ch, u8::try_from(0x20 + column * 16 + row).unwrap()))
        })
        .collect()
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

fn native_case(primary: u8, repertoire: TeletextG0Repertoire) -> Value {
    let points = points(primary);
    assert_eq!(points.len(), 95);
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
                    text: chunk.iter().map(|(ch, _)| ch).collect(),
                },
            }],
        })
        .collect();
    let original = declare(input);
    let mut coded = draft(primary);
    coded.base.pages[0].rows.clear();
    let coded = declare(coded)
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
        .unwrap();
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    let reference = encode_teletext_authored_service(coded.clone(), &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), &original);
    assert!(original.resource_inventory().is_empty());
    assert!(encoded.compiler_drcs_resources().is_empty());
    assert!(encoded.magazine_units().is_empty());
    assert!(matches!(
        encoded.pages()[0],
        EncodedTeletextAuthoredPage::NativeDisplay(_)
    ));
    assert_eq!(
        encoded.pages()[0].data_units(),
        reference.pages()[0].data_units()
    );
    let composition = greek_unicode::preview(&original);
    let reference_composition = greek_unicode::preview(&coded);
    assert_eq!(raster(&composition), raster(&reference_composition));
    let layout = layout::compile_for_service_composition(
        original.semantic(),
        &original.semantic().base().pages[0],
    )
    .unwrap();
    assert!(layout.base_glyphs().is_empty());
    let mut positions = Vec::new();
    for (index, (ch, code)) in points.iter().enumerate() {
        let row = u8::try_from(index / 36 + 1).unwrap();
        let column = u8::try_from(index % 36 + 1).unwrap();
        let cell = composition.base_display().cells()[usize::from(row)][usize::from(column)];
        assert_eq!(cell.code(), *code);
        assert!(
            matches!(cell.authored().unwrap().content(),TeletextBaseAuthoredContent::Text {character} if character==*ch)
        );
        if *code != 0x20 {
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
            assert!(
                matches!(composition.glyph_intent().records()[candidate].glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
                if glyph.repertoire()==repertoire && glyph.code()==*code && glyph.national_subset().is_none())
            );
        }
        positions.push(json!({"row":row,"column":column,"code":code,"unicode":u32::from(*ch)}));
    }
    let mut units = encoded.pages()[0].data_units().to_vec();
    units.push(super::super::super::super::packets::terminator(2));
    json!({"primary":primary,"page":0x200,"subcode":0,"points":positions,
        "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()})
}

#[test]
fn native_cyrillic_hebrew_repertoires_preserve_originals_wire_pixels_and_namespace() {
    let cases = [
        (0x20, TeletextG0Repertoire::Cyrillic1),
        (0x24, TeletextG0Repertoire::Cyrillic2),
        (0x25, TeletextG0Repertoire::Cyrillic3),
        (0x55, TeletextG0Repertoire::Hebrew),
    ]
    .into_iter()
    .map(|(primary, repertoire)| native_case(primary, repertoire))
    .collect::<Vec<_>>();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_CYRILLIC_HEBREW_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&json!({"cases":cases})).unwrap()).unwrap();
    }
}

#[test]
fn native_cyrillic_hebrew_reject_wrong_namespace_and_unknown_aliases_at_exact_cell() {
    for (primary, first, unknowns) in [
        (0x20, 'А', "AЍйїא😀́"),
        (0x24, 'А', "AЍїא&😀́"),
        (0x25, 'А', "AЍыא&😀́"),
        (0x55, 'א', "аαï_😀́"),
    ] {
        for character in unknowns.chars() {
            let mut input = draft(primary);
            input.base.pages[0].rows = vec![TeletextRow {
                row: 1,
                runs: vec![TeletextRun {
                    column: 8,
                    style: TeletextStyle::default(),
                    content: TeletextRunContent::Text {
                        text: format!("{first}{character}"),
                    },
                }],
            }];
            let original = declare(input);
            let before = original.clone();
            let failure =
                encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap_err();
            assert!(
                matches!(&failure.reason,Failure::Layout(error)
                if matches!(error.as_ref(),TeletextEncodingError::Character { address,row:1,column:9,codepoint }
                    if *address==owner(0) && *codepoint==u32::from(character))),
                "{primary:02X}/{character:?}: {failure:?}"
            );
            assert_eq!(original, before);
        }
    }
}

#[test]
fn native_cyrillic_hebrew_require_matching_explicit_context_and_keep_row_bounds() {
    for (primary, character) in [(0x20, 'Б'), (0x24, 'Б'), (0x25, 'Б'), (0x55, 'א')] {
        let mut input = draft(primary);
        input.base.pages[0].rows = vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text {
                    text: character.to_string(),
                },
            }],
        }];
        let original = declare(input);
        let page = &original.semantic().base().pages[0];
        let mut mismatch = page.clone();
        mismatch.national_option = TeletextNationalOption::new((primary + 1) & 7).unwrap();
        assert!(matches!(
            layout::compile_for_service_composition(original.semantic(), &mismatch),
            Err(TeletextEncodingError::Character {
                row: 1,
                column: 8,
                ..
            })
        ));
        let latin_context = declare(draft(0));
        assert!(matches!(
            layout::compile_for_service_composition(latin_context.semantic(), page),
            Err(TeletextEncodingError::Character {
                row: 1,
                column: 8,
                ..
            })
        ));
        let mut overflow = page.clone();
        overflow.rows[0].runs[0].column = 39;
        overflow.rows[0].runs[0].content = TeletextRunContent::Text {
            text: format!("{character}{character}"),
        };
        assert!(layout::compile_for_service_composition(original.semantic(), &overflow).is_err());
        assert_eq!(page.rows[0].runs[0].column, 8);
    }
}

#[test]
fn native_cyrillic_hebrew_base_and_local_x26_keep_separate_native_provenance() {
    for (primary, character, base, repertoire) in [
        (0x20, 'А', 0x41, TeletextG0Repertoire::Cyrillic1),
        (0x24, 'А', 0x41, TeletextG0Repertoire::Cyrillic2),
        (0x25, 'А', 0x41, TeletextG0Repertoire::Cyrillic3),
        (0x55, 'א', 0x60, TeletextG0Repertoire::Hebrew),
    ] {
        let mut input = draft(primary);
        input.base.pages[0].rows = vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text {
                    text: character.to_string(),
                },
            }],
        }];
        input.pages[0].local_program = Some(
            TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::G0Diacritic {
                        code: TeletextGlyphCode::new(0x42).unwrap(),
                        diacritic: TeletextDiacriticIndex::new(0).unwrap(),
                    },
                },
            ])
            .unwrap(),
        );
        let original = declare(input);
        let composition = greek_unicode::preview(&original);
        assert_eq!(composition.base_display().cells()[1][8].code(), base);
        assert!(
            matches!(composition.base_display().cells()[1][8].authored().unwrap().content(),
            TeletextBaseAuthoredContent::Text {character:actual} if actual==character)
        );
        for code in [base, 0x42] {
            assert!(composition.glyph_intent().records().iter().any(
                |record| matches!(record.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
                if glyph.repertoire()==repertoire && glyph.code()==code)
            ));
        }
        let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &original);
        assert!(encoded.compiler_drcs_resources().is_empty());
        let row = encoded.pages()[0]
            .data_units()
            .iter()
            .find(|unit| packet(unit) == 1)
            .unwrap();
        assert_eq!(body(row)[8] & 0x7f, base);
        assert!(encoded.pages()[0]
            .data_units()
            .iter()
            .any(|unit| packet(unit) == 26));
    }
}

//! Supplementary Unicode must retain the primary G2 rather than Latin aliases.
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

fn encode(primary: u8, text: &str) {
    let original = original(primary, text);
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits())
        .expect("qualified native supplementary text must encode without optional banks");
    assert_eq!(encoded.declared(), &original);
    assert!(encoded.compiler_drcs_resources().is_empty());
    assert!(original.resource_inventory().is_empty());
    let _ = greek_unicode::preview(&original);
}

#[test]
fn native_g2_cyrillic1_text() {
    encode(0x20, "Б£ŁDБ");
}
#[test]
fn native_g2_cyrillic2_text() {
    encode(0x24, "Б£ŁDБ");
}
#[test]
fn native_g2_cyrillic3_text() {
    encode(0x25, "Б£ŁDБ");
}
#[test]
fn native_g2_greek_text() {
    encode(0x37, "Α£aΊΑ");
}

#[test]
fn generated_base_text_edits_bind_exact_original_cells_and_recompile_supported_contexts() {
    for (primary, before_character, after_character, code) in [
        (0x20, '£', '¥', 0x25_u8),
        (0x24, '£', '¥', 0x25),
        (0x25, '£', '¥', 0x25),
        (0x37, '£', 'e', 0x24),
        (0x47, 'A', 'B', 0x42),
        (0x57, 'A', 'B', 0x42),
        (0x01, '£', '¥', 0x25),
        (0x00, 'é', 'Á', 0x41),
    ] {
        let original = original(primary, &before_character.to_string());
        let owner = original.semantic().base().pages[0].address;
        let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
        let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
            panic!("generated text needs checked X26")
        };
        let position = page
            .normalized()
            .encoded()
            .command_positions()
            .iter()
            .find(|position| {
                position.original_base_glyph()
                    == Some(TeletextEnhancementPosition::new(1, 8).unwrap())
            })
            .unwrap();
        assert!(position.original_local_command().is_none());
        assert!(position.original_object_source().is_none());
        let before = project_encoded_teletext_display(page.page(), encoded.unit_count())
            .unwrap()
            .records
            .into_iter()
            .find_map(|record| match record {
                TeletextEncodedDisplayRecord::Triplets {
                    row: 26,
                    designation,
                    words,
                } if usize::from(designation) == position.semantic_word() / 13 => {
                    Some(words[position.semantic_word() % 13])
                }
                _ => None,
            })
            .unwrap();
        let after = (before & 0x7ff) | (u32::from(code) << 11);
        let edit =
            resolve_teletext_base_text_edit(&encoded, owner, position, before, after).unwrap();
        assert_eq!(edit.expected(), before_character);
        assert_eq!(edit.replacement(), after_character);
        let derived = original
            .clone()
            .with_base_text_edits(&[edit], semantic_limits())
            .unwrap();
        let result = encode_teletext_authored_service(derived, &[], &limits()).unwrap();
        assert!(result.compiler_drcs_resources().is_empty());
        assert_eq!(
            result.declared().semantic().base().pages[0].rows[0].runs[0].content,
            TeletextRunContent::Text {
                text: after_character.to_string()
            }
        );
        assert_eq!(
            original.semantic().base().pages[0].rows[0].runs[0].content,
            TeletextRunContent::Text {
                text: before_character.to_string()
            }
        );
        if matches!(primary, 0x20 | 0x24 | 0x25) {
            check_lossless_generated_text(&original, &encoded, position, (before, after), primary);
        }
        for (old_word, new_word) in [
            (before ^ 1, after),
            (before, after ^ 1),
            (before, (after & !(31 << 6)) | (1 << 6)),
            (before, (after & 0x7ff) | (0x80 << 11)),
        ] {
            assert!(
                resolve_teletext_base_text_edit(&encoded, owner, position, old_word, new_word)
                    .is_none()
            );
        }
    }
}

fn check_lossless_generated_text(
    original: &TeletextDeclaredService,
    encoded: &EncodedTeletextAuthoredService,
    position: &TeletextCompiledCommandPosition,
    words: (u32, u32),
    primary: u8,
) {
    let (before, after) = words;
    let owner = original.semantic().base().pages[0].address;
    // '$' is a canonical native G0 byte. Import must not silently
    // normalize an uploaded G2 spelling and change its physical rows.
    let noncanonical = (before & 0x7ff) | (0x24 << 11);
    assert!(
        resolve_teletext_base_text_edit(encoded, owner, position, before, noncanonical).is_none()
    );
    let lossless =
        resolve_teletext_lossless_base_text_edit(encoded, owner, position, before, noncanonical)
            .unwrap();
    assert_eq!((lossless.expected(), lossless.replacement()), ('£', '$'));
    let native = lossless.native_spelling().unwrap();
    assert_eq!(native.charset().get(), primary);
    assert_eq!(native.fallback().get(), b' ');
    assert_eq!(
        native.operation(),
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(0x24).unwrap()
        }
    );
    let derived = original
        .clone()
        .with_base_text_edits(&[lossless], semantic_limits())
        .unwrap();
    let result = encode_teletext_authored_service(derived, &[], &limits()).unwrap();
    let EncodedTeletextAuthoredPage::CompiledDisplay(next) = &result.pages()[0] else {
        panic!("retainedG2 remains compiled")
    };
    assert_eq!(
        next.normalized().original().base_replacement(1, 8),
        Some(native.operation())
    );
    assert_eq!(
        next.normalized().original().base_display().cells()[1][8]
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: '$' }
    );
    let following =
        resolve_teletext_lossless_base_text_edit(&result, owner, position, noncanonical, after)
            .unwrap();
    assert_eq!((following.expected(), following.replacement()), ('$', '¥'));
    assert!(following.native_spelling().is_none());
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

fn points(primary: u8) -> Vec<(char, u8)> {
    // Table41/43 column order, including only independently qualified aliases.
    // The retained matrix excludes AB2-qualified column4/58/Greek7F; its
    // original wire remains a reuse control. Greek34 is
    // multiplication;3F is Latin lowercase x, not an alias for that symbol.
    let groups = if primary == 0x37 {
        [
            (0x21, "ab£ehi§\0‘“k←↑→↓"),
            (0x30, "°±²³×mnp÷’”t¼½¾x"),
            (0x51, "¹®©™♪₠‰\0ΊΎΏ⅛⅜⅝⅞"),
            (0x60, "CDFGJLQRSUVWYZΆΉ"),
            (0x70, "cdfgjlqrsuvwyzΈ"),
        ]
    } else {
        [
            (0x21, "¡¢£\0¥\0§\0‘“«←↑→↓"),
            (0x30, "°±²³×µ¶·÷’”»¼½¾¿"),
            (0x50, "—¹®©™♪₠‰\0Łłß⅛⅜⅝⅞"),
            (0x60, "DEFGIJKLNQRSUVWZ"),
            (0x70, "defgijklnqrsuvwz"),
        ]
    };
    groups
        .into_iter()
        .flat_map(|(start, chars)| {
            chars
                .chars()
                .enumerate()
                .filter(|(_, ch)| *ch != '\0')
                .map(move |(i, ch)| (ch, start + u8::try_from(i).unwrap()))
        })
        .collect()
}

fn wire(original: &TeletextDeclaredService, primary: u8, name: &str, points: &[Value]) -> Value {
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), original);
    assert!(original.resource_inventory().is_empty());
    assert!(encoded.compiler_drcs_resources().is_empty());
    let mut units = encoded.pages()[0].data_units().to_vec();
    units.push(super::super::super::super::packets::terminator(2));
    json!({"name":name,"primary":primary,"page":0x200,"subcode":0,"points":points,
        "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()})
}

fn matrix(primary: u8) -> Value {
    let expected = points(primary);
    assert_eq!(expected.len(), 75);
    let repertoire = if primary == 0x37 {
        TeletextG2Repertoire::Greek
    } else {
        TeletextG2Repertoire::Cyrillic
    };
    let mut input = draft(primary);
    input.base.pages[0].rows = expected
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
    let composition = preview(&original);
    let mut reference = draft(primary);
    reference.base.pages[0].rows.clear();
    reference.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(
            expected
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
    assert_eq!(raster(&composition), raster(&preview(&declare(reference))));
    let mut positions = Vec::new();
    for (index, (ch, code)) in expected.iter().enumerate() {
        let row = u8::try_from(index / 36 + 1).unwrap();
        let column = u8::try_from(index % 36 + 1).unwrap();
        let cell = composition.base_display().cells()[usize::from(row)][usize::from(column)];
        assert_eq!(cell.code(), b' ');
        assert!(
            matches!(cell.authored().unwrap().content(),TeletextBaseAuthoredContent::Text {character} if character==*ch)
        );
        assert!(
            matches!(composition.base_replacement_identity(row,column).unwrap().unwrap(),TeletextGlyphIdentity::G2 {glyph}
            if glyph.repertoire()==repertoire && glyph.code()==*code)
        );
        assert_eq!(
            composition
                .base_replacement_charset(row, column)
                .unwrap()
                .get(),
            primary
        );
        positions.push(
            json!({"row":row,"column":column,"unicode":u32::from(*ch),"fallback":0x20,"code":code}),
        );
    }
    wire(&original, primary, "native_g2_matrix", &positions)
}

fn restored_local(primary: u8) -> Value {
    let greek = primary == 0x37;
    let text = if greek { "Α£aΊΑ" } else { "Б£ŁDБ" };
    let mut input = draft(primary);
    input.base = original(primary, text).semantic().base().clone();
    let designation =
        TeletextEnhancementCharsetDesignation::new(if greek { 0x24 } else { 0x37 }).unwrap();
    input.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::DesignateCharset { designation },
            },
            TeletextEnhancementCommand::AtColumn {
                column: 12,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(if greek { 0x42 } else { 0x43 }).unwrap(),
                },
            },
        ])
        .unwrap(),
    );
    let original = declare(input);
    let composition = preview(&original);
    // Generated glyphs carry their own context, then restore the exact Local
    // designation. Both original Local values/order and Base priority survive.
    assert!(composition.provenance().iter().any(|source| matches!(
        source,
        TeletextMainCompositionSource::BaseCell {
            kind: TeletextBaseCellCommandKind::GlyphCharset,
            ..
        }
    )));
    let native = if greek { 'Α' } else { 'Б' };
    let after = if greek { 'Б' } else { 'Γ' };
    let positions:Vec<_>=text.chars().enumerate().map(|(offset,ch)|json!({
        "row":1,"column":8+offset,"unicode":u32::from(ch),"fallback":u32::from(if offset==0 || offset==4 {native} else {' '}),
        "level25":u32::from(if offset==4 {after} else {ch}),
    })).collect();
    wire(&original, primary, "native_g2_restore_local", &positions)
}

#[test]
fn native_g2_full_qualified_matrix_and_local_restore_keep_fallback_and_original_context() {
    let cases = [0x20, 0x24, 0x25, 0x37]
        .into_iter()
        .flat_map(|primary| [matrix(primary), restored_local(primary)])
        .collect::<Vec<_>>();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_NATIVE_G2_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&json!({"cases":cases})).unwrap()).unwrap();
    }
}

#[test]
fn native_g2_unknown_aliases_remain_located_errors_without_latin_substitution() {
    for primary in [0x20, 0x24, 0x25, 0x37] {
        let first = if primary == 0x37 { 'Α' } else { 'Б' };
        for character in ['Æ', '¤', 'ɑ', '\u{301}', 'א', '😀'] {
            let original = original(primary, &format!("{first}{character}"));
            let before = original.clone();
            let failure =
                encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap_err();
            assert!(matches!(&failure.reason,Failure::Layout(error)
                if matches!(error.as_ref(),TeletextEncodingError::Character {address,row:1,column:9,codepoint}
                    if *address==owner(0) && *codepoint==u32::from(character))));
            assert_eq!(original, before);
        }
    }
}

#[test]
fn native_g2_keeps_existing_compiler_budget_and_physical_overlay_refusals() {
    for primary in [0x20, 0x24, 0x25, 0x37] {
        let text = if primary == 0x37 {
            "Α£aΊΑ"
        } else {
            "Б£ŁDБ"
        };
        let original = original(primary, text);
        let before = original.clone();
        let mut bounded = limits();
        bounded.normalization.composition.max_input_commands = 1;
        let error = encode_teletext_authored_service(original.clone(), &[], &bounded).unwrap_err();
        assert_eq!(error.display, Some(owner(0)));
        assert!(matches!(error.reason, Failure::Compiled(_)));
        assert_eq!(original, before);
        let mut cells = [b' '; 40];
        cells[8] = if primary == 0x37 { 0x41 } else { 0x42 };
        cells[12] = cells[8];
        cells[9] = b'X'; // A raw edit may not replace the protected fallback cell.
        let edited = original
            .with_physical_base_overlays(
                vec![TeletextPhysicalBaseOverlay::new(
                    owner(0),
                    vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
                )
                .unwrap()],
                semantic_limits(),
            )
            .unwrap();
        let failure = encode_teletext_authored_service(edited, &[], &limits()).unwrap_err();
        assert!(matches!(&failure.reason,Failure::Layout(error)
            if matches!(error.as_ref(),TeletextEncodingError::PhysicalBaseGeneratedGlyph {address,row:1,column:9}
                if *address==owner(0))));
    }
}

#[path = "native_g2/spelling.rs"]
mod spelling;

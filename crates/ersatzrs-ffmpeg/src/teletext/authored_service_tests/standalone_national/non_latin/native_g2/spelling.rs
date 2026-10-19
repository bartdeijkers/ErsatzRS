//! Lossless noncanonical spelling is validated in the complete existing factory.
use super::*;
fn native_operation(
    primary: u8,
    scalar: char,
    code: u8,
    charset: u8,
    mode: u8,
) -> TeletextDeclaredService {
    original(primary, &scalar.to_string())
        .with_base_text_spellings(
            vec![TeletextBaseTextSpelling::new(
                owner(0),
                TeletextEnhancementPosition::new(1, 8).unwrap(),
                scalar,
                TeletextGlyphCode::new(b' ').unwrap(),
                TeletextEnhancementCharsetDesignation::new(charset).unwrap(),
                if mode == 0x0f {
                    TeletextColumnOperation::G2 {
                        code: TeletextGlyphCode::new(code).unwrap(),
                    }
                } else {
                    TeletextColumnOperation::G0Diacritic {
                        code: TeletextGlyphCode::new(code).unwrap(),
                        diacritic: TeletextDiacriticIndex::new(mode & 15).unwrap(),
                    }
                },
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap()
}
fn native(primary: u8, scalar: char, code: u8, charset: u8) -> TeletextDeclaredService {
    native_operation(primary, scalar, code, charset, 0x0f)
}
fn local_reference(primary: u8, operation: TeletextColumnOperation) -> TeletextDeclaredService {
    let mut input = draft(primary);
    input.base.pages[0].rows = original(primary, " ").semantic().base().pages[0]
        .rows
        .clone();
    input
        .pages
        .iter_mut()
        .find(|p| p.address == owner(0))
        .unwrap()
        .local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 0).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation,
            },
        ])
        .unwrap(),
    );
    declare(input)
}
#[test]
fn native_base_spelling_retains_exact_g2_words_fallback_text_and_original_role() {
    for (primary, scalar, code, mode) in [
        (0x20, '$', 0x24, 0x0f),
        (0x24, '$', 0x24, 0x0f),
        (0x25, '$', 0x24, 0x0f),
        (0x01, '$', 0x24, 0x0f),
        (0x01, '¤', 0x24, 0x10),
        (0x01, '¦', 0x7c, 0x10),
        (0x01, '@', 0x2a, 0x10),
        (0x01, 'Á', 0x41, 0x12),
    ] {
        let charset = if primary == 0x01 { 0 } else { primary };
        let input = native_operation(primary, scalar, code, charset, mode);
        let encoded = encode_teletext_authored_service(input.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &input);
        assert!(encoded.compiler_drcs_resources().is_empty());
        let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
            panic!("native spelling needs generated X26")
        };
        let original = page.normalized().original();
        let cell = &original.base_display().cells()[1][8];
        assert_eq!(
            cell.authored().unwrap().content(),
            TeletextBaseAuthoredContent::Text { character: scalar }
        );
        assert_eq!(
            original.base_replacement(1, 8),
            Some(if mode == 0x0f {
                TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(code).unwrap(),
                }
            } else {
                TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(code).unwrap(),
                    diacritic: TeletextDiacriticIndex::new(mode & 15).unwrap(),
                }
            })
        );
        assert_eq!(
            original.original_program_source(),
            &TeletextEffectiveProgramSource::BaseOnly
        );
        let positions = page.normalized().encoded().command_positions();
        let position = positions
            .iter()
            .find(|p| {
                p.original_base_glyph() == Some(TeletextEnhancementPosition::new(1, 8).unwrap())
            })
            .unwrap();
        assert!(position.original_local_command().is_none());
        assert!(position.source().object.is_none());
        let records = project_encoded_teletext_display(page.page(), encoded.unit_count())
            .unwrap()
            .records;
        let mut fallback = None;
        let mut glyph = None;
        for record in records {
            match record {
                TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => {
                    fallback = Some(cells[8]);
                }
                TeletextEncodedDisplayRecord::Triplets {
                    row: 26,
                    designation,
                    words,
                } if usize::from(designation) == position.semantic_word() / 13 => {
                    glyph = Some(words[position.semantic_word() % 13]);
                }
                _ => {}
            }
        }
        assert_eq!(fallback, Some(b' '));
        assert_eq!(
            glyph,
            Some(8 | (u32::from(mode) << 6) | (u32::from(code) << 11))
        );
        assert_eq!(
            original.base_replacement_charset(1, 8).unwrap().get(),
            charset
        );
        let reference = local_reference(
            primary,
            input
                .semantic()
                .base_text_spellings()
                .values()
                .next()
                .unwrap()
                .operation(),
        );
        assert_eq!(raster(&preview(&input)), raster(&preview(&reference)));
    }
}
#[test]
fn native_base_spelling_rejects_wrong_meaning_and_foreign_charset_without_normalization() {
    for (primary, scalar, code, charset) in [
        (0x20, '$', 0x23, 0x20),
        (0x20, '$', 0x24, 0x37),
        (0x37, 'e', 0x24, 0x20),
    ] {
        let error = encode_teletext_authored_service(
            native(primary, scalar, code, charset),
            &[],
            &limits(),
        )
        .unwrap_err();
        assert!(format!("{error:?}").contains("BaseTextSpelling"));
    }
    for (scalar, code) in [('$', 0x24), ('|', 0x7c), ('*', 0x2a), ('■', 0x7f)] {
        let error = encode_teletext_authored_service(
            native_operation(0x01, scalar, code, 0, 0x10),
            &[],
            &limits(),
        )
        .unwrap_err();
        assert!(format!("{error:?}").contains("BaseTextSpelling"));
    }
}

#[test]
#[ignore = "requires fresh ERSATZRS_TELETEXT_SPELLING_UNITS destination for independent decoder proof"]
fn native_base_spelling_emit_qualified_decoder_units() {
    use std::io::Write as _;
    let path = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_SPELLING_UNITS"])
        .expect("explicit output path");
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut cases = Vec::new();
    for (primary, scalar, code, mode) in [
        (0x20, '$', 0x24, 0x0f),
        (0x24, '$', 0x24, 0x0f),
        (0x25, '$', 0x24, 0x0f),
        (0x01, '$', 0x24, 0x0f),
        (0x01, '¤', 0x24, 0x10),
        (0x01, '¦', 0x7c, 0x10),
        (0x01, '@', 0x2a, 0x10),
        (0x01, 'Á', 0x41, 0x12),
    ] {
        let charset = if primary == 0x01 { 0 } else { primary };
        let encoded = encode_teletext_authored_service(
            native_operation(primary, scalar, code, charset, mode),
            &[],
            &limits(),
        )
        .unwrap();
        let mut units = Vec::new();
        for _ in 0..2 {
            units.extend(encoded.magazine_units().iter().map(|u| u.to_vec()));
            units.extend(
                encoded
                    .pages()
                    .iter()
                    .flat_map(|page| page.data_units().iter().map(|u| u.to_vec())),
            );
            units.push(crate::teletext::packets::terminator(2).to_vec());
        }
        cases
            .push(json!({"primary":primary,"scalar":scalar,"mode":mode,"code":code,"units":units}));
    }
    output
        .write_all(&serde_json::to_vec_pretty(&cases).unwrap())
        .unwrap();
}

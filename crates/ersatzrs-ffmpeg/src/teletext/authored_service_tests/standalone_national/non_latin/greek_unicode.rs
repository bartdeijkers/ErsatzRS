//! Literal Greek input is Unicode, never pre-encoded ASCII or Latin fallback.
use super::*;

#[test]
fn literal_greek_headline_encodes_at_native_g0_without_optional_banks() {
    let mut input = draft(0x37);
    input.base.pages[0].rows = rows("Αθήνα 200");
    let original = declare(input);
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits())
        .expect("literal Greek must encode in its explicitly assigned G0 context");
    assert_eq!(encoded.declared(), &original);
    assert!(encoded.compiler_drcs_resources().is_empty());
    assert!(original.resource_inventory().is_empty());
    let EncodedTeletextAuthoredPage::NativeDisplay(_) = &encoded.pages()[0] else {
        panic!("native Greek G0 requires no enhancement program")
    };
    let row = encoded.pages()[0]
        .data_units()
        .iter()
        .find(|unit| packet(unit) == 1)
        .unwrap();
    assert_eq!(
        &body(row).map(|code| code & 0x7f)[8..17],
        &[0x41, 0x68, 0x5e, 0x6d, 0x61, 0x20, 0x32, 0x30, 0x30]
    );
}

fn original_greek_points() -> Vec<(char, u8)> {
    // Literal Unicode intervals and independent Table42 column ordering. The
    // retained94-point fixture omits the separately qualified AB2 sign52/rectangle7F.
    let mut points: Vec<_> = (0x20..=0x3f)
        .map(|code| {
            (
                match code {
                    0x3c => '«',
                    0x3e => '»',
                    _ => char::from(code),
                },
                code,
            )
        })
        .collect();
    points.push(('ΐ', 0x40));
    points.extend(('Α'..='Ρ').zip(0x41..=0x51));
    points.extend(('Σ'..='Ω').zip(0x53..=0x59));
    points.extend(('Ϊ'..='Ϋ').zip(0x5a..=0x5b));
    points.extend(('ά'..='ί').zip(0x5c..=0x5f));
    points.push(('ΰ', 0x60));
    points.extend(('α'..='ω').zip(0x61..=0x79));
    points.extend(('ϊ'..='ώ').zip(0x7a..=0x7e));
    assert_eq!(points.len(), 94);
    points
}

pub(super) fn preview(original: &TeletextDeclaredService) -> TeletextMainComposition {
    let composition = compose_teletext_main_program(
        original.semantic(),
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
    composition
}

fn wire(original: &TeletextDeclaredService, name: &str) -> Value {
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), original);
    assert!(original.resource_inventory().is_empty());
    assert!(encoded.compiler_drcs_resources().is_empty());
    assert!(encoded.magazine_units().is_empty());
    let mut units = encoded.pages()[0].data_units().to_vec();
    units.push(super::super::super::super::packets::terminator(2));
    json!({"name":name,"primary":0x37,"page":0x200,"subcode":0,
        "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()})
}

fn assert_coded_equivalence(original: &TeletextDeclaredService, points: &[(char, u8)]) {
    let mut input = draft(0x37);
    input.base.pages[0].rows.clear();
    let mut coded: Vec<_> = points
        .chunks(36)
        .enumerate()
        .map(|(index, chunk)| {
            let mut cells = [b' '; 40];
            for (offset, (_, code)) in chunk.iter().enumerate() {
                cells[offset + 1] = *code;
            }
            TeletextPhysicalBaseRow::new(u8::try_from(index + 1).unwrap(), cells).unwrap()
        })
        .collect();
    let mut raw = [b' '; 40];
    raw[8] = 0x52;
    coded.push(TeletextPhysicalBaseRow::new(4, raw).unwrap());
    let reference = declare(input)
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(owner(0), coded).unwrap()],
            semantic_limits(),
        )
        .unwrap();
    let a = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    let b = encode_teletext_authored_service(reference, &[], &limits()).unwrap();
    assert_eq!(a.pages()[0].data_units(), b.pages()[0].data_units());
}

fn native_case() -> Value {
    let points = original_greek_points();
    let mut input = draft(0x37);
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
    let mut raw = [b' '; 40];
    raw[8] = 0x52;
    let original = declare(input)
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(0),
                vec![TeletextPhysicalBaseRow::new(4, raw).unwrap()],
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap();
    assert_coded_equivalence(&original, &points);
    let composition = preview(&original);
    let service = original.semantic();
    let layout =
        layout::compile_for_service_composition(service, &service.base().pages[0]).unwrap();
    assert!(layout.base_glyphs().is_empty());
    let mut positions = Vec::new();
    for (index, (ch, code)) in points.into_iter().enumerate() {
        let row = u8::try_from(index / 36 + 1).unwrap();
        let column = u8::try_from(index % 36 + 1).unwrap();
        let cell = composition.base_display().cells()[usize::from(row)][usize::from(column)];
        assert_eq!(cell.code(), code);
        assert!(
            matches!(cell.authored().unwrap().content(),TeletextBaseAuthoredContent::Text {character} if character==ch)
        );
        if code != 0x20 {
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
                if glyph.repertoire()==TeletextG0Repertoire::Greek && glyph.code()==code && glyph.national_subset().is_none())
            );
        }
        positions.push(json!({"row":row,"column":column,"code":code,"unicode":u32::from(ch)}));
    }
    assert_eq!(composition.base_display().cells()[4][8].code(), 0x52);
    assert!(composition.base_display().cells()[4][8]
        .authored()
        .is_none());
    let mut value = wire(&original, "native_greek94");
    value["points"] = json!(positions);
    value
}

fn enhanced_case() -> Value {
    let mut input = draft(0x37);
    input.base.pages[0].rows = rows("Αθήνα 200");
    input.base.pages[0].rows.push(TeletextRow {
        row: 2,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "Ζ".into() },
        }],
    });
    input.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(0x43).unwrap(),
                    diacritic: TeletextDiacriticIndex::new(0).unwrap(),
                },
            },
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(2, 8).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(0x23).unwrap(),
                },
            },
        ])
        .unwrap(),
    );
    let original = declare(input);
    let composition = preview(&original);
    assert_eq!(composition.base_display().cells()[1][8].code(), 0x41);
    assert_eq!(composition.base_display().cells()[2][8].code(), 0x46);
    wire(&original, "native_greek_with_local_x26")
}

#[test]
fn native_greek_scalar_matrix_and_local_x26_preserve_preview_originals_and_fallback() {
    let request = json!({"cases":[native_case(),enhanced_case()]});
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_GREEK_UNICODE_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    }
}

#[test]
fn greek_namespace_rejects_cross_script_unknown_aliases_and_decomposed_input_at_exact_cell() {
    // Ά is a qualified Greek G2 character, separately covered by native_g2.
    for character in ['A', 'Æ', '\u{384}', '\u{301}', 'א', '😀'] {
        let mut input = draft(0x37);
        input.base.pages[0].rows = rows(&format!("Α{character}"));
        let original = declare(input);
        let before = original.clone();
        let failure =
            encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap_err();
        let Failure::Layout(error) = &failure.reason else {
            panic!("located layout error: {failure:?}")
        };
        assert!(
            matches!(error.as_ref(), TeletextEncodingError::Character {address,row:1,column:9,codepoint}
            if *address==owner(0) && *codepoint==u32::from(character)),
            "{character:?}: {failure:?}"
        );
        assert_eq!(original, before);
    }
}

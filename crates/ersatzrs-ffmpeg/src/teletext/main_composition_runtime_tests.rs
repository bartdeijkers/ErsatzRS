//! Opt-in fixed-cell evidence for the production composition/lowering path.
//! This does not settle Active overlap, header context or full client acceptance.

use ersatzrs_domain::*;

use super::main_composition::{compose_teletext_main_program, TeletextMainCompositionBudget};
use super::packets::{data_unit, terminator};
use super::{
    allocate_teletext_resource_objects, compile_teletext_composed_main_program,
    compile_teletext_local_program_with_objects, encode_teletext_page, source_header,
    TeletextEncodingRegion,
};

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}

fn text(row: u8, column: u8, content: &str) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![TeletextRun {
            column,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text {
                text: content.into(),
            },
        }],
    }
}

fn cell(row: u8, column: u8, l15: [u8; 5], l25: [u8; 5]) -> serde_json::Value {
    // Fixed independent-decoder tuple: code, repertoire, diacritic, foreground,
    // background. G2 pound is code23/set7; Latin acute is code65/set0/mark2.
    serde_json::json!({"row":row,"column":column,"l15":l15,"l25":l25})
}

fn service(
    page: &TeletextPageDraft,
    main: Vec<TeletextEnhancementCommand>,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextSemanticService {
    let owner = page.address;
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: page.language.clone(),
                initial_page: owner.page,
                pages: vec![page.clone()],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner,
                presentation: None,
                compositional_links: None,
                local_program: Some(TeletextEnhancementProgram::new(main).unwrap()),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [None; 4],
                },
            }],
            magazines: vec![],
            objects: definitions,
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 16,
            max_objects: 4,
            max_structure_items: 256,
            max_base_content_bytes: 256,
            max_commands: 64,
            max_collected_commands: 128,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}

fn encoded_case(
    name: &str,
    rows: Vec<TeletextRow>,
    main: Vec<TeletextEnhancementCommand>,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    cells: &[serde_json::Value],
) -> serde_json::Value {
    encoded_case_with_context(name, rows, main, definitions, cells, false)
}

fn encoded_case_with_context(
    name: &str,
    rows: Vec<TeletextRow>,
    main: Vec<TeletextEnhancementCommand>,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    cells: &[serde_json::Value],
    compatibility: bool,
) -> serde_json::Value {
    let mut page = super::transport_tests::draft(200, 0, false);
    // In English, pound is a direct national character. German forces the
    // intended G2 replacement and gives unchanged ASCII repertoire 15.
    page.national_option = TeletextNationalOption::new(1).unwrap();
    page.rows = rows;
    let owner = page.address;
    let service = service(&page, main, definitions);
    let objects = service.objects();
    let allocations = allocate_teletext_resource_objects(objects, &[], 4).unwrap();
    let composition_budget = TeletextMainCompositionBudget {
        max_input_commands: 64,
        max_output_commands: 64,
        collection: TeletextEnhancementCollectionBudget {
            max_commands: 128,
            max_drcs_references: 0,
        },
    };
    let packing_budget = TeletextObjectCatalogBudget {
        max_objects: 4,
        max_commands: 128,
    };
    let local = if compatibility {
        compile_teletext_composed_main_program(
            &service,
            owner,
            &allocations,
            composition_budget,
            packing_budget,
        )
        .unwrap()
        .encoded()
        .clone()
    } else {
        let composed = compose_teletext_main_program(&service, owner, composition_budget).unwrap();
        compile_teletext_local_program_with_objects(
            owner,
            composed.program(),
            objects,
            &allocations,
            TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            },
            packing_budget,
        )
        .unwrap()
    };
    if matches!(name, "invocation_anchor" | "context_body_restore") {
        let positions = local.definition_positions();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].absolute_index(), 5);
        assert_eq!(positions[0].object_number(), 5);
        assert_eq!(positions[0].target(), objects.definitions()[0].identity);
        assert_eq!(positions[0].object_type(), TeletextObjectType::Passive);
    }
    let encoded = encode_teletext_page(&page, TeletextEncodingRegion::WesternLatin).unwrap();
    let original = encoded.data_units();
    let mut units = Vec::new();
    let mut inserted = false;
    for unit in original {
        let address = source_header::observe_dvb_teletext_header(unit).unwrap();
        if matches!(address, source_header::TeletextHeaderObservation::NotHeader(address)
            if address.packet_number == 26)
        {
            if !inserted {
                units.extend(
                    local
                        .packet_bodies()
                        .iter()
                        .map(|body| data_unit(2, 26, body, false)),
                );
                inserted = true;
            }
        } else {
            units.push(*unit);
        }
    }
    assert!(inserted);
    // Only the original base X26 bodies are replaced. Header, presentation and
    // fallback row bytes are retained; the ordinary FF page closes the cycle.
    units.push(terminator(2));
    serde_json::json!({"name":name,"page":0x200,"subcode":0,
        "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),"cells":cells})
}

fn glyph_cases() -> Vec<serde_json::Value> {
    let code = |value| TeletextGlyphCode::new(value).unwrap();
    let white = [b'Z', 15, 0, 7, 0];
    let pound = [0x23, 7, 0, 7, 0];
    let acute = [b'e', 0, 2, 7, 0];
    let derived = encoded_case(
        "derived_unicode",
        vec![text(1, 8, "£éZ")],
        vec![],
        vec![],
        &[
            cell(1, 8, pound, pound),
            cell(1, 9, acute, acute),
            cell(1, 10, white, white),
        ],
    );
    let yen = [0x25, 7, 0, 7, 0];
    let latin_a = [b'A', 0, 0, 7, 0];
    let compatible = encoded_case(
        "compatible_suppression",
        vec![text(1, 8, "£éZ")],
        vec![
            position(1, 8),
            at(8, TeletextColumnOperation::G2 { code: code(0x25) }),
            at(
                9,
                TeletextColumnOperation::G0Diacritic {
                    code: code(b'A'),
                    diacritic: TeletextDiacriticIndex::new(0).unwrap(),
                },
            ),
        ],
        vec![],
        &[
            cell(1, 8, yen, yen),
            cell(1, 9, latin_a, latin_a),
            cell(1, 10, white, white),
        ],
    );
    let layering = encoded_case(
        "enhanced_fallback_layering",
        vec![text(1, 8, "£éZ")],
        vec![
            position(1, 8),
            at(8, TeletextColumnOperation::G0 { code: code(b'A') }),
            at(
                9,
                TeletextColumnOperation::G1 {
                    mosaic: TeletextMosaic::new(1).unwrap(),
                },
            ),
        ],
        vec![],
        &[
            cell(1, 8, pound, latin_a),
            cell(1, 9, acute, [0x21, 24, 0, 7, 0]),
            cell(1, 10, white, white),
        ],
    );
    vec![derived, compatible, layering]
}

fn anchored_case() -> serde_json::Value {
    let code = |value| TeletextGlyphCode::new(value).unwrap();
    let white = [b'Z', 15, 0, 7, 0];
    let pound = [0x23, 7, 0, 7, 0];
    let owner = super::transport_tests::draft(200, 0, false).address;
    let identity = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    encoded_case(
        "invocation_anchor",
        vec![text(1, 8, "£Z"), text(2, 9, "TT")],
        vec![
            position(1, 9),
            TeletextEnhancementCommand::InvokeObject {
                target: identity,
                object_type: TeletextObjectType::Passive,
                offset: None,
            },
        ],
        vec![TeletextEnhancementObjectDefinition {
            identity,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![
                position(1, 0),
                at(
                    0,
                    TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(1).unwrap(),
                    },
                ),
                at(
                    0,
                    TeletextColumnOperation::Background {
                        colour: TeletextColourReference::new(4).unwrap(),
                    },
                ),
                at(0, TeletextColumnOperation::G0 { code: code(b'A') }),
            ])
            .unwrap(),
        }],
        &[
            cell(1, 8, pound, pound),
            cell(1, 9, white, white),
            cell(2, 9, [b'T', 15, 0, 7, 0], [b'A', 0, 0, 1, 4]),
            cell(2, 10, [b'T', 15, 0, 7, 0], [b'T', 15, 0, 7, 0]),
        ],
    )
}

fn cases() -> Vec<serde_json::Value> {
    let mut result = glyph_cases();
    result.push(anchored_case());
    result
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn composed_main_program_decodes_with_pinned_independent_teletextimager() {
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.0","cases":cases()}))
            .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_MAIN_COMPOSITION_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-main-composition-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder":"teletextimager","decoder_version":"0.2.0",
        "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317","cases_passed":4,"levels":["1.5","2.5"]})
    );
}

fn g1_alphabet_case() -> serde_json::Value {
    encoded_case_with_context(
        "g1_alphabet_mode_one",
        vec![text(1, 8, "é£Z"), text(2, 1, "B")],
        vec![
            position(1, 8),
            at(
                8,
                TeletextColumnOperation::G1Alpha {
                    code: TeletextG1AlphaCode::new(0x40).unwrap(),
                },
            ),
            at(
                9,
                TeletextColumnOperation::G1Alpha {
                    code: TeletextG1AlphaCode::new(0x5f).unwrap(),
                },
            ),
        ],
        vec![],
        &[
            cell(1, 8, [b'e', 0, 2, 7, 0], [0x40, 0, 0, 7, 0]),
            cell(1, 9, [0x23, 7, 0, 7, 0], [0x5f, 0, 0, 7, 0]),
            cell(1, 10, [b'Z', 15, 0, 7, 0], [b'Z', 15, 0, 7, 0]),
        ],
        true,
    )
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn pinned_imager_g1_alphabet_limitation_preserves_level15_fallback() {
    let mut case = g1_alphabet_case();
    // Preserve the correct conformance oracle. The pinned decoder routes all
    // G1 codes to its mosaic font; its alphabetic entries have no glyph metrics.
    // Observed set 24 below reproduces that failure, not G1 rendering support.
    let conformance = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":[case.clone()],
    }))
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_G1_ALPHA_CONFORMANCE_FIXTURE",
    ]) {
        std::fs::write(path, &conformance).unwrap();
    }
    case["cells"][0]["l25"][1] = serde_json::json!(24);
    case["cells"][1]["l25"][1] = serde_json::json!(24);
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":[case],
    }))
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_G1_ALPHA_IMAGER_DIAGNOSTIC_FIXTURE",
    ]) {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-main-composition-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder":"teletextimager","decoder_version":"0.2.0",
        "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317","cases_passed":1,"levels":["1.5","2.5"]})
    );
    println!(
        "Pinned G1 alphabet decoder limitation reproduced; Level 1.5 fallback retained: {report}"
    );
}

// Intentional Linux x64 exception: the frozen public ctypes ABI oracle is
// calibrated only for that native layout, as in the existing object proof.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi decoder"]
async fn g1_alphabet_decodes_without_national_substitutions_with_libzvbi() {
    let mut case = g1_alphabet_case();
    case["cells"] = serde_json::json!([
        {"row":1,"column":8,"unicode":64,"foreground":7,"background":0},
        {"row":1,"column":9,"unicode":95,"foreground":7,"background":0},
        {"row":1,"column":10,"unicode":90,"foreground":7,"background":0},
    ]);
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.42", "cases":[case],
    }))
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_G1_ALPHA_LIBZVBI_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-object-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder_version":"0.2.42","cases_passed":1})
    );
}

// Intentional Linux x64 exception: the sibling's frozen public ctypes ABI is
// calibrated for that layout. This is a fixed-cell decoder proof, not a client
// rendering/navigation proof or acceptance of complete non-Latin authoring.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi decoder"]
async fn original_non_latin_g0_assignments_decode_with_libzvbi() {
    // Literal identities from EN 300 706 tables 39, 42 and 46, independent of
    // the production lookup and decoder mapping: Cyrillic A, Greek Alpha, Aleph.
    let samples = [
        ("original_cyrillic_a", 0x24, 0x41, 0x0410),
        ("original_greek_alpha", 0x37, 0x41, 0x0391),
        ("original_hebrew_aleph", 0x55, 0x60, 0x05d0),
    ];
    let cases: Vec<_> = samples
        .into_iter()
        .map(|(name, designation, code, unicode)| {
            let mut case = encoded_case_with_context(
                name,
                vec![text(1, 8, "T"), text(2, 1, "B")],
                vec![
                    position(1, 8),
                    at(
                        8,
                        TeletextColumnOperation::DesignateCharset {
                            designation: TeletextEnhancementCharsetDesignation::new(designation)
                                .unwrap(),
                        },
                    ),
                    at(
                        8,
                        TeletextColumnOperation::G0 {
                            code: TeletextGlyphCode::new(code).unwrap(),
                        },
                    ),
                ],
                vec![],
                &[],
                true,
            );
            case["cells"] = serde_json::json!([
                {"row":1,"column":8,"unicode":unicode,"foreground":7,"background":0},
            ]);
            case
        })
        .collect();
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.42", "cases":cases,
    }))
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_G0_ASSIGNMENTS_LIBZVBI_FIXTURE",
    ]) {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-object-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder_version":"0.2.42","cases_passed":3})
    );
}

fn contextual_cases() -> Vec<serde_json::Value> {
    let pound = [0x23, 7, 0, 7, 0];
    let white = [b'Z', 15, 0, 7, 0];
    let owner = super::transport_tests::draft(200, 0, false).address;
    let identity = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    let body = encoded_case_with_context(
        "context_body_restore",
        vec![text(1, 8, "Z£"), text(2, 8, "TT")],
        vec![
            position(1, 8),
            TeletextEnhancementCommand::InvokeObject {
                target: identity,
                object_type: TeletextObjectType::Passive,
                offset: None,
            },
        ],
        vec![TeletextEnhancementObjectDefinition {
            identity,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![
                position(1, 0),
                at(
                    0,
                    TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(1).unwrap(),
                    },
                ),
                at(
                    0,
                    TeletextColumnOperation::Background {
                        colour: TeletextColourReference::new(4).unwrap(),
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
        }],
        &[
            cell(1, 8, white, white),
            cell(1, 9, pound, pound),
            cell(2, 8, [b'T', 15, 0, 7, 0], [b'A', 0, 0, 1, 4]),
            cell(2, 9, [b'T', 15, 0, 7, 0], [b'T', 15, 0, 7, 0]),
        ],
        true,
    );
    let header = encoded_case_with_context(
        "context_initial_header",
        vec![text(0, 8, "£Z")],
        vec![TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::new(1).unwrap(),
        }],
        vec![],
        &[cell(0, 8, pound, pound), cell(0, 9, white, white)],
        true,
    );
    vec![body, header]
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn contextual_main_program_decodes_with_pinned_independent_teletextimager() {
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":contextual_cases()
    }))
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_CONTEXT_MAIN_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-main-composition-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({
            "decoder":"teletextimager", "decoder_version":"0.2.0",
            "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317",
            "cases_passed":2, "levels":["1.5","2.5"]
        })
    );
}

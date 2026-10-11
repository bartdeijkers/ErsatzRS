//! Opt-in fixed-cell evidence for the production composition/lowering path.
//! This does not settle Active overlap, header context or full client acceptance.

use ersatzrs_domain::*;

use super::main_composition::{compose_teletext_main_program, TeletextMainCompositionBudget};
use super::packets::{data_unit, terminator};
use super::{
    allocate_teletext_resource_objects, compile_teletext_local_program_with_objects,
    encode_teletext_page, source_header, TeletextEncodingRegion,
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
    let mut page = super::transport_tests::draft(200, 0, false);
    // In English, pound is a direct national character. German forces the
    // intended G2 replacement and gives unchanged ASCII repertoire 15.
    page.national_option = TeletextNationalOption::new(1).unwrap();
    page.rows = rows;
    let owner = page.address;
    let service = service(&page, main, definitions);
    let composed = compose_teletext_main_program(
        &service,
        owner,
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
    let objects = service.objects();
    let allocations = allocate_teletext_resource_objects(objects, &[], 4).unwrap();
    let local = compile_teletext_local_program_with_objects(
        owner,
        composed.program(),
        objects,
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 128,
        },
    )
    .unwrap();
    if name == "invocation_anchor" {
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

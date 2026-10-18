//! Opt-in fixed-cell evidence for the production composition/lowering path.
//! The Unicode/Active oracle uses the bounded normalization candidate; other
//! fixtures retain their explicit encoder paths. Full client acceptance is separate.

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

pub(super) fn encoded_case(
    name: &str,
    rows: Vec<TeletextRow>,
    main: Vec<TeletextEnhancementCommand>,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    cells: &[serde_json::Value],
) -> serde_json::Value {
    encoded_case_with_context(name, rows, main, definitions, cells, false)
}

#[derive(Clone, Copy)]
enum MainEncoding {
    Local,
    Composed,
    Normalized,
}

fn encoded_case_with_context(
    name: &str,
    rows: Vec<TeletextRow>,
    main: Vec<TeletextEnhancementCommand>,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    cells: &[serde_json::Value],
    compatibility: bool,
) -> serde_json::Value {
    let encoding = if compatibility {
        MainEncoding::Composed
    } else {
        MainEncoding::Local
    };
    encoded_case_with_encoding(name, rows, main, definitions, cells, encoding)
}

fn encoded_case_with_encoding(
    name: &str,
    rows: Vec<TeletextRow>,
    main: Vec<TeletextEnhancementCommand>,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    cells: &[serde_json::Value],
    encoding: MainEncoding,
) -> serde_json::Value {
    let mut page = super::transport_tests::draft(200, 0, false);
    // In English, pound is a direct national character. German forces the
    // intended G2 replacement and gives unchanged ASCII repertoire 15.
    page.national_option =
        TeletextNationalOption::new(u8::from(name != "english_national_enlarged_overlap")).unwrap();
    page.rows = rows;
    let owner = page.address;
    let service = service(&page, main, definitions);
    let objects = service.objects();
    let local = encode_fixture_main(name, &service, owner, encoding);
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

fn encode_fixture_main(
    name: &str,
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    encoding: MainEncoding,
) -> super::EncodedTeletextLocalObjectProgram {
    let composition_budget = TeletextMainCompositionBudget {
        max_input_commands: 64,
        max_output_commands: 64,
        collection: TeletextEnhancementCollectionBudget {
            max_commands: 128,
            max_drcs_references: 0,
        },
    };
    let packing_budget = TeletextObjectCatalogBudget {
        max_objects: if matches!(
            name,
            "national_base_enlarged_overlap" | "english_national_enlarged_overlap"
        ) {
            8
        } else {
            4
        },
        max_commands: 128,
    };
    if matches!(encoding, MainEncoding::Normalized) {
        return super::compile_teletext_normalized_main_program(
            service,
            owner,
            normalization_budget(composition_budget, packing_budget),
        )
        .unwrap()
        .encoded()
        .clone();
    }
    let objects = service.objects();
    let allocations = allocate_teletext_resource_objects(objects, &[], 4).unwrap();
    if matches!(encoding, MainEncoding::Composed) {
        let compiled = compile_teletext_composed_main_program(
            service,
            owner,
            &allocations,
            composition_budget,
            packing_budget,
        )
        .unwrap();
        if matches!(
            name,
            "compiled_local_colour_tie" | "compiled_adaptive_colour_tie"
        ) {
            assert_compiled_colour_tie(&compiled, name);
        }
        compiled.encoded().clone()
    } else {
        let composed = compose_teletext_main_program(service, owner, composition_budget).unwrap();
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
    }
}

fn normalization_budget(
    composition: TeletextMainCompositionBudget,
    packing: TeletextObjectCatalogBudget,
) -> super::TeletextNormalizedMainProgramBudget {
    use super::{
        TeletextAugmentedGlyphComparisonBudget, TeletextAugmentedInvocationBudget,
        TeletextAugmentedOriginProgramBudget, TeletextBaseCompatibilityRealizationBudget,
        TeletextNormalizedMainProgramBudget, TeletextOriginCarrierBudget,
        TeletextOriginCarrierPlacementBudget, TeletextPhysicalComparisonBudget,
    };
    TeletextNormalizedMainProgramBudget {
        composition,
        packing,
        replay: composition.collection,
        max_resource_objects: packing.max_objects,
        realization: TeletextBaseCompatibilityRealizationBudget {
            max_commands: 128,
            max_candidates: 2048,
            max_origins: 4096,
            max_seeds: 16,
            max_edges: 16,
            max_updates: 100_000,
        },
        placement: TeletextOriginCarrierPlacementBudget {
            max_commands: 128,
            max_origins: 4096,
            max_edges: 16,
            catalog: packing,
            spatial: TeletextSpatialTraceBudget {
                max_events: 128,
                max_invocations: 16,
            },
            max_work: 100_000,
        },
        assembly: TeletextAugmentedOriginProgramBudget {
            max_input_commands: 128,
            max_output_commands: 128,
            max_carriers: 4,
            max_identity_checks: 1024,
            max_work: 100_000,
            recipe: TeletextOriginCarrierBudget {
                max_candidates: 2048,
                max_origins: 4096,
                catalog: packing,
                max_id_checks: 1024,
                max_output_commands: 6,
            },
        },
        ancestry: TeletextAugmentedInvocationBudget {
            max_original_invocations: 16,
            max_actual_invocations: 16,
            max_carriers: 4,
            max_work: 100_000,
        },
        glyph_comparison: TeletextAugmentedGlyphComparisonBudget {
            max_records: 10_000,
            max_cells: 1000,
            max_diagnostics: 64,
            max_work: 1_000_000,
        },
        display_comparison: TeletextPhysicalComparisonBudget {
            max_records: 10_000,
            max_cells: 1000,
            max_diagnostics: 64,
            max_updates: 1_000_000,
        },
    }
}

fn assert_compiled_colour_tie(compiled: &super::EncodedTeletextComposedMainProgram, name: &str) {
    let column = if name == "compiled_local_colour_tie" {
        18
    } else {
        5
    };
    let composition = compiled.composition();
    let display = composition.display_resolution();
    let selected = display.cell(1, column).unwrap().attributes().foreground();
    assert_eq!(
        selected.value(),
        Some(TeletextColourReference::new(5).unwrap())
    );
    assert_eq!(selected.pending(), None);
    let pixel = composition
        .colour_composition()
        .pixel(
            display,
            1,
            column,
            true,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
        )
        .unwrap();
    let TeletextColourPixel::Rgb(rgb) = pixel else {
        panic!("known magenta foreground");
    };
    assert_eq!(rgb.get(), [15, 0, 15]);
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

fn compiled_colour_tie_cases() -> Vec<serde_json::Value> {
    let foreground = |column, colour| {
        at(
            column,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(colour).unwrap(),
            },
        )
    };
    let glyph = |column| {
        at(
            column,
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(b'A').unwrap(),
            },
        )
    };
    let black_screen = TeletextEnhancementCommand::SetScreenColour {
        colour: TeletextColourReference::BLACK,
    };
    let mut main = vec![black_screen; 11];
    main.extend([
        position(1, 18),
        foreground(18, 4),
        foreground(18, 5),
        glyph(18),
    ]);
    let local = encoded_case_with_context(
        "compiled_local_colour_tie",
        vec![text(1, 18, "T"), text(2, 1, "B")],
        main,
        vec![],
        &[cell(1, 18, [b'T', 15, 0, 7, 0], [b'A', 0, 0, 5, 0])],
        true,
    );
    let owner = super::transport_tests::draft(200, 0, false).address;
    let identity = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    let adaptive = encoded_case_with_context(
        "compiled_adaptive_colour_tie",
        vec![text(1, 5, "T"), text(2, 1, "B")],
        vec![
            position(1, 0),
            TeletextEnhancementCommand::InvokeObject {
                target: identity,
                object_type: TeletextObjectType::Adaptive,
                offset: None,
            },
        ],
        vec![TeletextEnhancementObjectDefinition {
            identity,
            object_type: TeletextObjectType::Adaptive,
            program: TeletextEnhancementProgram::new(vec![
                foreground(5, 4),
                foreground(5, 5),
                glyph(5),
            ])
            .unwrap(),
        }],
        &[cell(1, 5, [b'T', 15, 0, 7, 0], [b'A', 0, 0, 5, 0])],
        true,
    );
    vec![local, adaptive]
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn compiled_same_emitter_colour_ties_decode_with_pinned_imager() {
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":compiled_colour_tie_cases(),
    }))
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_COMPILED_COLOUR_TIE_IMAGER_FIXTURE",
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
        "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317","cases_passed":2,"levels":["1.5","2.5"]})
    );
}

// Intentional Linux x64 calibration of the frozen public decoder ABI. This
// checks a formatted main cell; it is not an interactive-client acceptance gate.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi decoder"]
async fn compiled_local_colour_tie_decodes_with_libzvbi() {
    let mut case = compiled_colour_tie_cases().remove(0);
    case["cells"] = serde_json::json!([
        {"row":1,"column":18,"unicode":65,"foreground":5,"background":0},
    ]);
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.42", "cases":[case],
    }))
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_COMPILED_COLOUR_TIE_LIBZVBI_FIXTURE",
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
        serde_json::json!({"decoder_version":"0.2.42","cases_passed":1})
    );
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

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn base_unicode_active_override_decodes_with_pinned_imager() {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let identity = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    let case = encoded_case_with_encoding(
        "base_unicode_active_override",
        vec![text(1, 8, "£Z"), text(2, 1, "B")],
        vec![
            position(1, 8),
            TeletextEnhancementCommand::InvokeObject {
                target: identity,
                object_type: TeletextObjectType::Active,
                offset: None,
            },
        ],
        vec![TeletextEnhancementObjectDefinition {
            identity,
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(vec![at(
                0,
                TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            )])
            .unwrap(),
        }],
        &[
            cell(1, 8, [0x23, 7, 0, 7, 0], [b'A', 0, 0, 7, 0]),
            cell(1, 9, [b'Z', 15, 0, 7, 0], [b'Z', 15, 0, 7, 0]),
        ],
        MainEncoding::Normalized,
    );
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":[case],
    }))
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_UNICODE_ACTIVE_IMAGER_FIXTURE",
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
}

fn national_overlap_case(english: bool) -> serde_json::Value {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let active = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    let adaptive = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xfffe));
    let attributes = |column, double_width, double_height| {
        at(
            column,
            TeletextColumnOperation::Attributes {
                double_width,
                double_height,
                boxed: false,
                conceal: false,
                invert: false,
                underline_or_separated_mosaics: false,
            },
        )
    };
    let invoke = |target, object_type| TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: None,
    };
    let glyph = |column, value| {
        at(
            column,
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(value).unwrap(),
            },
        )
    };
    encoded_case_with_encoding(
        if english {
            "english_national_enlarged_overlap"
        } else {
            "national_base_enlarged_overlap"
        },
        if english {
            vec![text(1, 7, "‖"), text(2, 8, "é"), text(3, 1, "B")]
        } else {
            vec![text(1, 7, "§"), text(2, 8, "£"), text(3, 1, "B")]
        },
        vec![
            position(1, 7),
            TeletextEnhancementCommand::InvokeObject {
                target: active,
                object_type: TeletextObjectType::Active,
                offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
            },
            invoke(adaptive, TeletextObjectType::Adaptive),
        ],
        vec![
            TeletextEnhancementObjectDefinition {
                identity: active,
                object_type: TeletextObjectType::Active,
                program: TeletextEnhancementProgram::new(vec![
                    attributes(0, true, true),
                    glyph(0, b'A'),
                    attributes(2, false, false),
                ])
                .unwrap(),
            },
            TeletextEnhancementObjectDefinition {
                identity: adaptive,
                object_type: TeletextObjectType::Adaptive,
                program: TeletextEnhancementProgram::new(vec![
                    attributes(0, true, false),
                    attributes(2, false, false),
                    glyph(2, b'X'),
                ])
                .unwrap(),
            },
        ],
        &if english {
            [
                cell(1, 7, [0x7c, 12, 0, 7, 0], [0x7c, 6, 0, 7, 0]),
                cell(1, 8, [b' ', 12, 0, 7, 0], [0x7c, 6, 0, 7, 0]),
                cell(1, 9, [b' ', 12, 0, 7, 0], [b'X', 0, 0, 7, 0]),
                cell(2, 8, [b'e', 0, 2, 7, 0], [b'A', 0, 0, 7, 0]),
                cell(2, 9, [b' ', 12, 0, 7, 0], [b'A', 0, 0, 7, 0]),
                cell(3, 1, [b'B', 12, 0, 7, 0], [b'B', 12, 0, 7, 0]),
            ]
        } else {
            [
                cell(1, 7, [0x40, 15, 0, 7, 0], [0x27, 7, 0, 7, 0]),
                cell(1, 8, [b' ', 15, 0, 7, 0], [0x27, 7, 0, 7, 0]),
                cell(1, 9, [b' ', 15, 0, 7, 0], [b'X', 0, 0, 7, 0]),
                cell(2, 8, [0x23, 7, 0, 7, 0], [b'A', 0, 0, 7, 0]),
                cell(2, 9, [b' ', 15, 0, 7, 0], [b'A', 0, 0, 7, 0]),
                cell(3, 1, [b'B', 15, 0, 7, 0], [b'B', 15, 0, 7, 0]),
            ]
        },
        MainEncoding::Normalized,
    )
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn national_base_enlarged_overlap_decodes_with_pinned_imager() {
    assert_national_overlap_decoding(false).await;
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn english_national_overlap_records_pinned_imager_private_charset_gap() {
    assert_national_overlap_decoding(true).await;
}

async fn assert_national_overlap_decoding(english: bool) {
    let case = national_overlap_case(english);
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.0", "cases":[case]}))
            .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[if english {
        "ERSATZRS_TELETEXT_TEST_ENGLISH_NATIONAL_OVERLAP_IMAGER_FIXTURE"
    } else {
        "ERSATZRS_TELETEXT_TEST_NATIONAL_OVERLAP_IMAGER_FIXTURE"
    }]) {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(if english {
        "../../scripts/e2e/teletext-national-carrier-calibration.py"
    } else {
        "../../scripts/e2e/teletext-main-composition-decoder.py"
    });
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    if english {
        assert_eq!(
            report,
            serde_json::json!({
                "decoder": "teletextimager", "decoder_version": "0.2.0",
                "decoder_commit": "2b0125b778c45ffdfa2df0b196870c093361f317",
                "conformance": false,
                "known_failure": "english_national_enlarged_overlap Level 2.5 cell 1,7: actual [124, 0, 0, 7, 0], expected [124, 6, 0, 7, 0]",
                "level_1_5_selected_cells_passed": 6,
            })
        );
    } else {
        assert_eq!(
            report,
            serde_json::json!({"decoder":"teletextimager","decoder_version":"0.2.0",
            "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317",
            "cases_passed":1,"levels":["1.5","2.5"]})
        );
    }
}

fn cases() -> Vec<serde_json::Value> {
    let mut result = glyph_cases();
    result.push(anchored_case());
    result
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn redundant_initial_object_position_decodes_without_wire_or_colour_changes() {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let identity = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    let mut cases = Vec::new();
    for object_type in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        let build = |name: &str, explicit: bool| {
            let mut commands = Vec::new();
            let mut main = Vec::new();
            if explicit {
                commands.push(position(0, 0));
                commands.push(position(0, 0));
                main.push(position(0, 0));
            }
            commands.push(at(
                0,
                TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            ));
            main.extend([
                position(1, 8),
                TeletextEnhancementCommand::InvokeObject {
                    target: identity,
                    object_type,
                    offset: None,
                },
            ]);
            encoded_case(
                name,
                vec![text(1, 8, "ZZ")],
                main,
                vec![TeletextEnhancementObjectDefinition {
                    identity,
                    object_type,
                    program: TeletextEnhancementProgram::new(commands).unwrap(),
                }],
                &[
                    cell(1, 8, [b'Z', 15, 0, 7, 0], [b'A', 0, 0, 7, 0]),
                    cell(1, 9, [b'Z', 15, 0, 7, 0], [b'Z', 15, 0, 7, 0]),
                ],
            )
        };
        let implicit = build(&format!("implicit_{object_type:?}"), false);
        let explicit = build(&format!("explicit_{object_type:?}"), true);
        // Both authored nops disappear without changing pointers, header/rows,
        // object colours or any complete production DVB data-unit bytes.
        assert_eq!(implicit["units"], explicit["units"]);
        cases.extend([implicit, explicit]);
    }
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":cases,
    }))
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_ROW_ZERO_IMAGER_FIXTURE"])
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
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output).unwrap(),
        serde_json::json!({"decoder":"teletextimager","decoder_version":"0.2.0",
        "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317","cases_passed":6,"levels":["1.5","2.5"]})
    );
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
    let mut cases = vec![body, header];
    for column in [8, 39] {
        cases.push(encoded_case_with_context(
            &format!("context_forward_header_{column}"),
            vec![text(0, column, "£"), text(1, 8, "Z")],
            vec![
                position(0, column),
                position(0, column),
                TeletextEnhancementCommand::SetScreenColour {
                    colour: TeletextColourReference::new(1).unwrap(),
                },
            ],
            vec![],
            &[cell(0, column, pound, pound), cell(1, 8, white, white)],
            true,
        ));
    }
    cases.extend(contextual_header_objects());
    cases
}

fn contextual_header_objects() -> Vec<serde_json::Value> {
    let mut cases = Vec::new();
    for (column, foregrounds) in [
        (8, &[][..]),
        (39, &[][..]),
        (9, &[1, 2][..]),
        (9, &[2, 1][..]),
    ] {
        let owner = super::transport_tests::draft(200, 0, false).address;
        let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
        let mut main = vec![TeletextEnhancementCommand::SetRowColour {
            row: 0,
            colour: TeletextColourReference::new(3).unwrap(),
            following_rows: false,
        }];
        main.extend(foregrounds.iter().map(|&value| {
            at(
                8,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(value).unwrap(),
                },
            )
        }));
        main.extend([
            position(0, column),
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Passive,
                offset: None,
            },
        ]);
        let rows = vec![
            text(0, column, if foregrounds.is_empty() { "Z" } else { "ZZ" }),
            text(1, 8, "B"),
        ];
        let mut cells = vec![
            cell(0, column, [b'Z', 15, 0, 7, 0], [b'A', 0, 0, 2, 0]),
            cell(1, 8, [b'B', 15, 0, 7, 0], [b'B', 15, 0, 7, 0]),
        ];
        if let Some(&value) = foregrounds.last() {
            cells.push(cell(0, 10, [b'Z', 15, 0, 7, 0], [b'Z', 15, 0, value, 0]));
        }
        cases.push(encoded_case_with_encoding(
            &if foregrounds.is_empty() {
                format!("context_header_object_{column}")
            } else {
                format!("context_header_c2_{}{}", foregrounds[0], foregrounds[1])
            },
            rows,
            main,
            vec![TeletextEnhancementObjectDefinition {
                identity: target,
                object_type: TeletextObjectType::Passive,
                program: TeletextEnhancementProgram::new(vec![
                    at(
                        0,
                        TeletextColumnOperation::Foreground {
                            colour: TeletextColourReference::new(2).unwrap(),
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
            &cells,
            MainEncoding::Normalized,
        ));
    }
    cases
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn local_active_header_prefix_decodes_with_pinned_independent_teletextimager() {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let active = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65534));
    let passive = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    let foreground = |column, value| {
        at(
            column,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(value).unwrap(),
            },
        )
    };
    let glyph = |column, code| {
        at(
            column,
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(code).unwrap(),
            },
        )
    };
    let mut cases = Vec::new();
    for (first, last) in [(3, 4), (4, 3)] {
        cases.push(encoded_case_with_encoding(
            &format!("local_active_header_prefix_{first}_{last}"),
            vec![text(0, 9, "ZZ"), text(1, 8, "B")],
            vec![
                TeletextEnhancementCommand::InvokeObject {
                    target: active,
                    object_type: TeletextObjectType::Active,
                    offset: None,
                },
                position(0, 9),
                TeletextEnhancementCommand::InvokeObject {
                    target: passive,
                    object_type: TeletextObjectType::Passive,
                    offset: None,
                },
            ],
            vec![
                TeletextEnhancementObjectDefinition {
                    identity: active,
                    object_type: TeletextObjectType::Active,
                    program: TeletextEnhancementProgram::new(vec![
                        foreground(8, first),
                        foreground(8, last),
                        glyph(10, b'B'),
                    ])
                    .unwrap(),
                },
                TeletextEnhancementObjectDefinition {
                    identity: passive,
                    object_type: TeletextObjectType::Passive,
                    program: TeletextEnhancementProgram::new(vec![
                        foreground(0, 2),
                        glyph(0, b'A'),
                    ])
                    .unwrap(),
                },
            ],
            &[
                cell(0, 9, [b'Z', 15, 0, 7, 0], [b'A', 0, 0, 2, 0]),
                cell(0, 10, [b'Z', 15, 0, 7, 0], [b'B', 0, 0, last, 0]),
                cell(1, 8, [b'B', 15, 0, 7, 0], [b'B', 15, 0, 7, 0]),
            ],
            MainEncoding::Normalized,
        ));
    }
    cases.extend(initial_header_active_cases());
    cases.push(body_return_header_case());
    cases.push(nested_header_case(
        "nested_adaptive_header_position",
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ));
    for (name, child_type) in [
        (
            "nested_active_passive_header_position",
            TeletextObjectType::Passive,
        ),
        (
            "nested_active_adaptive_header_position",
            TeletextObjectType::Adaptive,
        ),
    ] {
        cases.push(nested_header_case(
            name,
            TeletextObjectType::Active,
            child_type,
        ));
    }
    assert_contextual_decoding(cases, "ERSATZRS_TELETEXT_TEST_LOCAL_ACTIVE_HEADER_FIXTURE").await;
}

fn nested_header_case(
    name: &str,
    outer_type: TeletextObjectType,
    child_type: TeletextObjectType,
) -> serde_json::Value {
    assert!(outer_type.may_invoke(child_type));
    let owner = super::transport_tests::draft(200, 0, false).address;
    let outer = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65534));
    let child = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    let mut rows = vec![text(0, 8, "Z")];
    let mut cells = vec![cell(0, 8, [b'Z', 15, 0, 7, 0], [b'A', 0, 0, 2, 0])];
    if outer_type == TeletextObjectType::Active {
        for column in [9, 20, 39] {
            rows[0].runs.push(TeletextRun {
                column,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "Z".into() },
            });
            cells.push(cell(0, column, [b'Z', 15, 0, 7, 0], [b'Z', 15, 0, 7, 0]));
        }
        rows.push(text(1, 8, "Z"));
        cells.push(cell(1, 8, [b'Z', 15, 0, 7, 0], [b'Z', 15, 0, 7, 0]));
    }
    encoded_case_with_encoding(
        name,
        rows,
        vec![TeletextEnhancementCommand::InvokeObject {
            target: outer,
            object_type: outer_type,
            offset: None,
        }],
        vec![
            TeletextEnhancementObjectDefinition {
                identity: outer,
                object_type: outer_type,
                program: TeletextEnhancementProgram::new(vec![
                    position(0, 8),
                    TeletextEnhancementCommand::InvokeObject {
                        target: child,
                        object_type: child_type,
                        offset: None,
                    },
                ])
                .unwrap(),
            },
            TeletextEnhancementObjectDefinition {
                identity: child,
                object_type: child_type,
                program: TeletextEnhancementProgram::new(vec![
                    at(
                        0,
                        TeletextColumnOperation::Foreground {
                            colour: TeletextColourReference::new(2).unwrap(),
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
            },
        ],
        &cells,
        MainEncoding::Normalized,
    )
}

fn body_return_header_case() -> serde_json::Value {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let active = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65534));
    let passive = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    let foreground = |column, value| {
        at(
            column,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(value).unwrap(),
            },
        )
    };
    let glyph = |column, code| {
        at(
            column,
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(code).unwrap(),
            },
        )
    };
    encoded_case_with_encoding(
        "local_active_body_return_header",
        vec![text(0, 8, "Z"), text(1, 8, "Z")],
        vec![
            TeletextEnhancementCommand::InvokeObject {
                target: active,
                object_type: TeletextObjectType::Active,
                offset: None,
            },
            position(0, 8),
            TeletextEnhancementCommand::InvokeObject {
                target: passive,
                object_type: TeletextObjectType::Passive,
                offset: None,
            },
        ],
        vec![
            TeletextEnhancementObjectDefinition {
                identity: active,
                object_type: TeletextObjectType::Active,
                program: TeletextEnhancementProgram::new(vec![
                    position(1, 8),
                    foreground(8, 3),
                    glyph(8, b'B'),
                ])
                .unwrap(),
            },
            TeletextEnhancementObjectDefinition {
                identity: passive,
                object_type: TeletextObjectType::Passive,
                program: TeletextEnhancementProgram::new(vec![foreground(0, 2), glyph(0, b'A')])
                    .unwrap(),
            },
        ],
        &[
            cell(0, 8, [b'Z', 15, 0, 7, 0], [b'A', 0, 0, 2, 0]),
            cell(1, 8, [b'Z', 15, 0, 7, 0], [b'B', 0, 0, 3, 0]),
        ],
        MainEncoding::Normalized,
    )
}

fn initial_header_active_cases() -> Vec<serde_json::Value> {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let active = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65534));
    let passive = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    let foreground = |column, value| {
        at(
            column,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(value).unwrap(),
            },
        )
    };
    let glyph = |column, code| {
        at(
            column,
            TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(code).unwrap(),
            },
        )
    };
    [(3, 4), (4, 3)]
        .into_iter()
        .map(|(first, last)| {
            encoded_case_with_encoding(
                &format!("initial_header_active_{first}_{last}"),
                vec![text(0, 8, "ZZZ")],
                vec![
                    position(0, 8),
                    TeletextEnhancementCommand::InvokeObject {
                        target: active,
                        object_type: TeletextObjectType::Active,
                        offset: None,
                    },
                    position(0, 9),
                    TeletextEnhancementCommand::InvokeObject {
                        target: passive,
                        object_type: TeletextObjectType::Passive,
                        offset: None,
                    },
                ],
                vec![
                    TeletextEnhancementObjectDefinition {
                        identity: active,
                        object_type: TeletextObjectType::Active,
                        program: TeletextEnhancementProgram::new(vec![
                            foreground(0, first),
                            foreground(0, last),
                            glyph(2, b'B'),
                        ])
                        .unwrap(),
                    },
                    TeletextEnhancementObjectDefinition {
                        identity: passive,
                        object_type: TeletextObjectType::Passive,
                        program: TeletextEnhancementProgram::new(vec![
                            foreground(0, 2),
                            glyph(0, b'A'),
                        ])
                        .unwrap(),
                    },
                ],
                &[
                    cell(0, 8, [b'Z', 15, 0, 7, 0], [b'Z', 15, 0, last, 0]),
                    cell(0, 9, [b'Z', 15, 0, 7, 0], [b'A', 0, 0, 2, 0]),
                    cell(0, 10, [b'Z', 15, 0, 7, 0], [b'B', 0, 0, last, 0]),
                ],
                MainEncoding::Normalized,
            )
        })
        .collect()
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn contextual_main_program_decodes_with_pinned_independent_teletextimager() {
    let cases = contextual_cases();
    assert_eq!(cases.len(), 8);
    assert_contextual_decoding(cases, "ERSATZRS_TELETEXT_TEST_CONTEXT_MAIN_FIXTURE").await;
}

async fn assert_contextual_decoding(cases: Vec<serde_json::Value>, fixture_env: &str) {
    let count = cases.len();
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":cases
    }))
    .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[fixture_env]) {
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
            "cases_passed":count, "levels":["1.5","2.5"]
        })
    );
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn relative_object_positions_decode_with_pinned_independent_teletextimager() {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    let mut cases = Vec::new();
    for object_type in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        cases.push(encoded_case_with_encoding(
            &format!("relative_object_{object_type:?}"),
            vec![text(1, 8, "B       Z")],
            vec![
                position(1, 8),
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    offset: None,
                },
            ],
            vec![TeletextEnhancementObjectDefinition {
                identity: target,
                object_type,
                program: TeletextEnhancementProgram::new(vec![
                    position(0, 8),
                    at(
                        8,
                        TeletextColumnOperation::Foreground {
                            colour: TeletextColourReference::new(2).unwrap(),
                        },
                    ),
                    at(
                        8,
                        TeletextColumnOperation::G0 {
                            code: TeletextGlyphCode::new(b'A').unwrap(),
                        },
                    ),
                ])
                .unwrap(),
            }],
            &[
                cell(1, 8, [b'B', 15, 0, 7, 0], [b'B', 15, 0, 7, 0]),
                cell(1, 16, [b'Z', 15, 0, 7, 0], [b'A', 0, 0, 2, 0]),
            ],
            MainEncoding::Normalized,
        ));
    }
    assert_eq!(cases.len(), 3);
    assert_contextual_decoding(cases, "ERSATZRS_TELETEXT_TEST_RELATIVE_OBJECT_FIXTURE").await;
}

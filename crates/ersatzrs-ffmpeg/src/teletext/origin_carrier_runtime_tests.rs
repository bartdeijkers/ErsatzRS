//! One derived-origin wire oracle, not automatic normalization/client acceptance.

use ersatzrs_domain::*;

use super::*;

fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}

fn row(row: u8, column: u8, value: &str) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![TeletextRun {
            column,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: value.into() },
        }],
    }
}

fn service(page: &TeletextPageDraft) -> TeletextSemanticService {
    let owner = page.address;
    let active = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
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
                local_program: Some(
                    TeletextEnhancementProgram::new(vec![
                        TeletextEnhancementCommand::SetPosition {
                            position: TeletextEnhancementPosition::new(1, 8).unwrap(),
                        },
                        TeletextEnhancementCommand::InvokeObject {
                            target: active,
                            object_type: TeletextObjectType::Active,
                            offset: None,
                        },
                    ])
                    .unwrap(),
                ),
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
            objects: vec![TeletextEnhancementObjectDefinition {
                identity: active,
                object_type: TeletextObjectType::Active,
                program: TeletextEnhancementProgram::new(vec![at(
                    0,
                    TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'A').unwrap(),
                    },
                )])
                .unwrap(),
            }],
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

fn compiled(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
) -> EncodedTeletextPhysicalDisplayProgram {
    let active = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    let collection = TeletextEnhancementCollectionBudget {
        max_commands: 128,
        max_drcs_references: 0,
    };
    let original = compose_teletext_main_program(
        service,
        owner,
        TeletextMainCompositionBudget {
            max_input_commands: 64,
            max_output_commands: 64,
            collection,
        },
    )
    .unwrap();
    let origin = original
        .display_resolution()
        .cell(1, 8)
        .unwrap()
        .origin()
        .unwrap();
    let packing = TeletextObjectCatalogBudget {
        max_objects: 4,
        max_commands: 128,
    };
    let gap = derive_gap(&original, service.objects(), packing, origin);
    let carrier = build_teletext_origin_carrier(
        &original,
        origin,
        service.objects(),
        TeletextOriginCarrierBudget {
            max_candidates: 2048,
            max_origins: 4096,
            catalog: packing,
            max_id_checks: 65536,
            max_output_commands: 16,
        },
    )
    .unwrap();
    assert_eq!(carrier.original_origin_index(), origin);
    assert_eq!(
        carrier.definition().object_type,
        TeletextObjectType::Passive
    );
    let mut definitions = service.objects().definitions().to_vec();
    definitions.push(carrier.definition().clone());
    let catalog = TeletextEnhancementObjectCatalog::new(definitions, packing).unwrap();
    let mut commands = original.program().commands().to_vec();
    // Retain every original command and use the producer-derived unchanged gap.
    // Actual replay below proves the literal anchor. This one-component oracle
    // does not settle full augmented correspondence or arbitrary later paints.
    assert!(commands.iter().any(|command| matches!(command,
        TeletextEnhancementCommand::InvokeObject { target, object_type: TeletextObjectType::Active, offset: None } if *target == active)));
    commands.insert(
        gap.after_command().map_or(0, |command| command + 1),
        TeletextEnhancementCommand::InvokeObject {
            target: carrier.definition().identity,
            object_type: TeletextObjectType::Passive,
            offset: None,
        },
    );
    let program = TeletextEnhancementProgram::new(commands).unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 4).unwrap();
    let actual = compile_teletext_physical_display_program(
        service,
        &original,
        &program,
        &catalog,
        &allocations,
        collection,
        packing,
    )
    .unwrap();
    let invocation = actual
        .replay()
        .spatial_trace()
        .invocations()
        .iter()
        .find(|invocation| invocation.target() == carrier.definition().identity)
        .unwrap();
    assert_eq!(
        (
            invocation.caller_position().row(),
            invocation.caller_position().column()
        ),
        (1, 8)
    );
    assert_eq!(
        (invocation.origin().row(), invocation.origin().column()),
        (1, 8)
    );
    assert_selected_model_matches(&original, actual.replay());
    actual
}

fn derive_gap(
    original: &TeletextMainComposition,
    catalog: &TeletextEnhancementObjectCatalog,
    packing: TeletextObjectCatalogBudget,
    origin: usize,
) -> TeletextOriginCarrierGap {
    let plan = plan_teletext_base_compatibility_realization(
        original,
        TeletextBaseCompatibilityRealizationBudget {
            max_commands: 128,
            max_candidates: 2048,
            max_origins: 4096,
            max_seeds: 16,
            max_edges: 16,
            max_updates: 100_000,
        },
    )
    .unwrap();
    let placement = plan_teletext_origin_carrier_placement(
        &plan,
        catalog,
        TeletextOriginCarrierPlacementBudget {
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
    )
    .unwrap();
    assert_eq!(placement.components().len(), 1);
    let component = &placement.components()[0];
    assert!(component.pending().is_none());
    assert_eq!(component.origin_indices(), &[origin]);
    let gap = component.gap().unwrap();
    assert_eq!(gap.caller_position(), (1, 8));
    assert_eq!(component.calls().len(), 1);
    let call = component.calls()[0];
    assert_eq!(call.original_origin_index(), origin);
    assert_eq!(call.raw_position(), (1, 8));
    assert_eq!((call.offset().row(), call.offset().column()), (0, 0));
    gap
}

fn assert_selected_model_matches(
    original: &TeletextMainComposition,
    physical: &TeletextPhysicalDisplayReplay,
) {
    let comparison = compare_teletext_physical_display(
        original,
        physical,
        TeletextPhysicalComparisonBudget {
            max_records: 100_000,
            max_cells: 1800,
            max_diagnostics: 32_000,
            max_updates: 1_000_000,
        },
    )
    .unwrap();
    assert_eq!(comparison.compared_cells(), 1000);
    assert!(
        comparison.diagnostics().is_empty(),
        "{:?}",
        comparison.diagnostics()
    );
    // This selected-model observation is additional to the independent literal
    // wire cells; it is not a general glyph-lineage or readiness certificate.
}

fn case() -> serde_json::Value {
    let mut page = transport_tests::draft(200, 0, false);
    page.national_option = TeletextNationalOption::new(1).unwrap();
    page.rows = vec![row(1, 8, "£Z"), row(2, 1, "B")];
    let service = service(&page);
    let actual = compiled(&service, page.address);
    let fallback = encode_teletext_page(&page, TeletextEncodingRegion::WesternLatin).unwrap();
    let mut units = Vec::new();
    let mut replaced = false;
    for unit in fallback.data_units() {
        if matches!(source_header::observe_dvb_teletext_header(unit).unwrap(),
            source_header::TeletextHeaderObservation::NotHeader(address) if address.packet_number == 26)
        {
            if !replaced {
                units.extend(
                    actual
                        .encoded()
                        .packet_bodies()
                        .iter()
                        .map(|body| packets::data_unit(2, 26, body, false)),
                );
                replaced = true;
            }
        } else {
            units.push(*unit);
        }
    }
    assert!(replaced);
    units.push(packets::terminator(2));
    serde_json::json!({
        "name":"origin_carrier_active_override", "page":0x200, "subcode":0,
        "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "cells":[
            {"row":1,"column":8,"l15":[0x23,7,0,7,0],"l25":[b'A',0,0,7,0]},
            {"row":1,"column":9,"l15":[b'Z',15,0,7,0],"l25":[b'Z',15,0,7,0]},
            {"row":2,"column":1,"l15":[b'B',15,0,7,0],"l25":[b'B',15,0,7,0]},
        ],
    })
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn original_active_origin_carrier_decodes_without_changing_level15_fallback() {
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.0","cases":[case()]}))
            .unwrap();
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
            "cases_passed":1, "levels":["1.5","2.5"],
        })
    );
}

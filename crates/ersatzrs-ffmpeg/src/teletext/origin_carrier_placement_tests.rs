//! Literal unchanged-anchor/offset oracles, not augmented wire admission.
use super::*;
use crate::teletext::{
    compose_teletext_main_program, plan_teletext_base_compatibility_realization,
    TeletextBaseCompatibilityRealizationBudget, TeletextMainComposition,
    TeletextMainCompositionBudget,
};
use ersatzrs_domain::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn target(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn invoke(id: u16, object_type: TeletextObjectType) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(id),
        object_type,
        offset: None,
    }
}
fn glyph() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    }
}
fn size(size: TeletextDisplaySize) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::Attributes {
            double_width: matches!(
                size,
                TeletextDisplaySize::DoubleWidth | TeletextDisplaySize::DoubleSize
            ),
            double_height: matches!(
                size,
                TeletextDisplaySize::DoubleHeight | TeletextDisplaySize::DoubleSize
            ),
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    }
}
fn definition(
    id: u16,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: target(id),
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}
fn service(
    base_rows: &[u8],
    main: Vec<TeletextEnhancementCommand>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    panels: Option<TeletextSidePanels>,
) -> TeletextSemanticService {
    let mut page = crate::teletext::transport_tests::draft(200, 0x1234, false);
    page.national_option = TeletextNationalOption::new(1).unwrap();
    page.rows = base_rows
        .iter()
        .map(|&row| TeletextRow {
            row,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "£".into() },
            }],
        })
        .collect();
    let presentation = panels.map(|panels| {
        let mut defaults =
            crate::teletext::presentation::western_latin_v1_presentation(page.national_option)
                .unwrap();
        defaults.panels = panels;
        defaults
    });
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: page.language.clone(),
                initial_page: owner().page,
                pages: vec![page],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation,
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
            objects,
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 8,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 2048,
            max_collected_commands: 4096,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}
fn compose(service: &TeletextSemanticService) -> TeletextMainComposition {
    compose_teletext_main_program(
        service,
        owner(),
        TeletextMainCompositionBudget {
            max_input_commands: 1024,
            max_output_commands: 1024,
            collection: TeletextEnhancementCollectionBudget {
                max_commands: 2048,
                max_drcs_references: 0,
            },
        },
    )
    .unwrap()
}
fn closure_budget() -> TeletextBaseCompatibilityRealizationBudget {
    TeletextBaseCompatibilityRealizationBudget {
        max_commands: 2048,
        max_candidates: 2048,
        max_origins: 4096,
        max_seeds: 40,
        max_edges: 4096,
        max_updates: 20000,
    }
}
fn budget() -> TeletextOriginCarrierPlacementBudget {
    TeletextOriginCarrierPlacementBudget {
        max_commands: 1024,
        max_origins: 4096,
        max_edges: 4096,
        catalog: TeletextObjectCatalogBudget {
            max_objects: 8,
            max_commands: 2048,
        },
        spatial: TeletextSpatialTraceBudget {
            max_events: 2048,
            max_invocations: 128,
        },
        max_work: 50000,
    }
}
fn basic() -> TeletextSemanticService {
    service(
        &[1],
        vec![position(1, 8), invoke(0xffff, TeletextObjectType::Active)],
        vec![definition(
            0xffff,
            TeletextObjectType::Active,
            vec![glyph()],
        )],
        None,
    )
}
fn call_positions(
    component: &TeletextOriginCarrierPlacementComponent,
) -> Vec<((u8, u8), (u8, u8))> {
    component
        .calls()
        .iter()
        .map(|call| {
            (
                call.raw_position(),
                (call.offset().row(), call.offset().column()),
            )
        })
        .collect()
}

#[test]
fn latest_actual_main_gap_retains_exact_borrow_and_origin_lineage() {
    let service = basic();
    let before = service.clone();
    let composition = compose(&service);
    let main = composition.program().clone();
    let plan =
        plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&plan, service.objects(), budget()).unwrap();
    assert!(std::ptr::eq(
        std::ptr::from_ref(placement.plan()),
        std::ptr::from_ref(&plan)
    ));
    assert!(std::ptr::eq(placement.catalog(), service.objects()));
    let component = &placement.components()[0];
    assert_eq!(placement.components().len(), 1);
    assert_eq!(component.pending(), None);
    let gap = component.gap().unwrap();
    assert_eq!(gap.after_command(), Some(main.commands().len() - 1));
    assert_eq!(gap.caller_position(), (1, 8));
    assert_eq!(call_positions(component), vec![((1, 8), (0, 0))]);
    assert_eq!(component.origin_indices(), &[plan.seeds()[0].origin]);
    assert_eq!(
        plan.origins()[0].candidate.source(),
        Some(TeletextEnhancementSource {
            object: Some(target(0xffff)),
            command: 0
        })
    );
    assert_eq!(plan.origins()[0].candidate.invocation().unwrap().get(), 0);
    assert_eq!(composition.program(), &main);
    assert_eq!(service, before);
}
#[test]
fn derived_base_ap_and_child_events_never_substitute_original_main_anchors() {
    let service = service(
        &[1],
        vec![
            position(1, 8),
            invoke(0xffff, TeletextObjectType::Active),
            position(3, 20),
        ],
        vec![definition(
            0xffff,
            TeletextObjectType::Active,
            vec![glyph(), position(2, 10)],
        )],
        None,
    );
    let composition = compose(&service);
    let plan =
        plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&plan, service.objects(), budget()).unwrap();
    let gap = placement.components()[0].gap().unwrap();
    assert_eq!(gap.caller_position(), (1, 8));
    let index = gap.after_command().unwrap();
    // The later row move flushes the Base glyph after the original call. German
    // primary alias1 already selects Latin/Latin, so no selector/restore is
    // needed: the literal G2 pound is the latest viable unchanged main gap.
    assert_eq!(
        composition.program().commands()[index],
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x23).unwrap(),
            },
        }
    );
    assert!(index > 1);
    assert_eq!(
        composition.provenance()[index],
        crate::teletext::TeletextMainCompositionSource::BaseCell {
            position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            kind: crate::teletext::TeletextBaseCellCommandKind::Glyph,
        }
    );
    assert!(composition
        .spatial_trace()
        .events()
        .iter()
        .any(|event| event.invocation().is_some()
            && event.absolute_after().row() == 3
            && event.absolute_after().column() == 18));
}
#[test]
fn enlarged_partial_component_uses_dag_order_and_upper_left_anchor_not_seed() {
    let service = service(
        &[2],
        vec![
            position(1, 7),
            invoke(2, TeletextObjectType::Passive),
            position(1, 8),
            invoke(0xffff, TeletextObjectType::Active),
        ],
        vec![
            definition(
                2,
                TeletextObjectType::Passive,
                vec![size(TeletextDisplaySize::DoubleWidth), glyph()],
            ),
            definition(
                0xffff,
                TeletextObjectType::Active,
                vec![size(TeletextDisplaySize::DoubleSize), glyph()],
            ),
        ],
        None,
    );
    let composition = compose(&service);
    let plan =
        plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&plan, service.objects(), budget()).unwrap();
    assert_eq!(plan.seeds()[0].visual_position, (2, 8));
    assert_eq!(plan.fragments().len(), 5);
    let component = &placement.components()[0];
    assert_eq!(component.origin_indices().len(), 2);
    assert_eq!(component.pending(), None);
    assert_eq!(component.gap().unwrap().caller_position(), (1, 7));
    let ordered = component.origin_indices();
    assert_eq!(
        composition.glyph_layers().candidates()
            [composition.display_resolution().origins()[ordered[0]].candidate_index()]
        .priority(),
        TeletextGlyphPriority::Active
    );
    assert_eq!(
        composition.glyph_layers().candidates()
            [composition.display_resolution().origins()[ordered[1]].candidate_index()]
        .priority(),
        TeletextGlyphPriority::Passive
    );
    assert_eq!(
        call_positions(component),
        vec![((1, 8), (0, 1)), ((1, 7), (0, 0))]
    );
    assert_eq!(plan.precedence()[0].before, ordered[0]);
    assert_eq!(plan.precedence()[0].after, ordered[1]);
}
#[test]
fn double_height_bottom_seed_places_the_original_top_origin() {
    let service = service(
        &[2],
        vec![position(1, 8), invoke(0xffff, TeletextObjectType::Active)],
        vec![definition(
            0xffff,
            TeletextObjectType::Active,
            vec![size(TeletextDisplaySize::DoubleHeight), glyph()],
        )],
        None,
    );
    let composition = compose(&service);
    let plan =
        plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&plan, service.objects(), budget()).unwrap();
    assert_eq!(plan.seeds()[0].base_position, (2, 8));
    assert_eq!(
        call_positions(&placement.components()[0]),
        vec![((1, 8), (0, 0))]
    );
    assert_eq!(
        placement.components()[0].gap().unwrap().caller_position(),
        (1, 8)
    );
}
#[test]
fn selected_panel_status_inverse_keeps_raw_main_coordinates() {
    for (status, visual) in [
        (TeletextPanelStatus::Level25And35, 12),
        (TeletextPanelStatus::Level35Only, 8),
    ] {
        let service = service(
            &[1],
            vec![position(1, 8), invoke(0xffff, TeletextObjectType::Active)],
            vec![definition(
                0xffff,
                TeletextObjectType::Active,
                vec![glyph()],
            )],
            Some(TeletextSidePanels {
                left: true,
                right: true,
                status,
                width: TeletextPanelWidth::new(4).unwrap(),
            }),
        );
        let composition = compose(&service);
        let plan =
            plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
        let placement =
            plan_teletext_origin_carrier_placement(&plan, service.objects(), budget()).unwrap();
        assert_eq!(plan.seeds()[0].visual_position, (1, visual));
        assert_eq!(
            call_positions(&placement.components()[0]),
            vec![((1, 8), (0, 0))]
        );
    }
}
#[test]
fn repeated_full_identity_keeps_distinct_components_and_original_instances() {
    let service = service(
        &[1, 2],
        vec![
            position(1, 8),
            invoke(0xffff, TeletextObjectType::Active),
            position(2, 8),
            invoke(0xffff, TeletextObjectType::Active),
        ],
        vec![definition(
            0xffff,
            TeletextObjectType::Active,
            vec![glyph()],
        )],
        None,
    );
    let composition = compose(&service);
    let plan =
        plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&plan, service.objects(), budget()).unwrap();
    assert_eq!(placement.components().len(), 2);
    let calls: Vec<_> = placement
        .components()
        .iter()
        .map(|component| {
            (
                component.gap().unwrap().caller_position(),
                component.calls()[0].raw_position(),
            )
        })
        .collect();
    assert_eq!(calls, vec![((1, 8), (1, 8)), ((2, 8), (2, 8))]);
    assert_eq!(
        plan.origins()
            .iter()
            .map(|o| o.candidate.invocation().unwrap().get())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_ne!(
        placement.components()[0].origin_indices(),
        placement.components()[1].origin_indices()
    );
}
#[test]
fn no_seed_preserves_unknown_active_and_global_pending_without_claiming_ready() {
    let service = service(
        &[1],
        vec![position(1, 8), invoke(0xffff, TeletextObjectType::Active)],
        vec![definition(
            0xffff,
            TeletextObjectType::Active,
            vec![
                size(TeletextDisplaySize::Normal),
                size(TeletextDisplaySize::DoubleWidth),
                glyph(),
            ],
        )],
        None,
    );
    let composition = compose(&service);
    let plan =
        plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&plan, service.objects(), budget()).unwrap();
    assert!(placement.components().is_empty());
    assert!(!placement.plan().original_pending().is_empty());
    assert_eq!(
        placement.plan().original_pending(),
        composition.display_resolution().pending()
    );
    assert!(placement
        .plan()
        .original_pending()
        .iter()
        .any(|pending| pending.reason() == TeletextDisplayPendingReason::UnknownSize));
}
#[test]
fn raw_limits_precede_traversal_and_work_and_storage_are_checked() {
    let service = basic();
    let composition = compose(&service);
    let plan =
        plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
    let before = composition.program().clone();
    for (kind, count) in [
        ("main commands", before.commands().len()),
        ("origins", plan.origins().len()),
        ("catalog objects", service.objects().definitions().len()),
    ] {
        let mut limits = budget();
        limits.max_work = 0;
        match kind {
            "main commands" => limits.max_commands = count - 1,
            "origins" => limits.max_origins = count - 1,
            _ => limits.catalog.max_objects = count - 1,
        }
        let error =
            plan_teletext_origin_carrier_placement(&plan, service.objects(), limits).unwrap_err();
        assert!(
            matches!(error.reason,Failure::InputBudget{kind:actual,count:actual_count,maximum}if actual==kind&&actual_count==count&&maximum==count-1)
        );
    }
    let mut limits = budget();
    limits.max_work = 0;
    assert!(matches!(
        plan_teletext_origin_carrier_placement(&plan, service.objects(), limits)
            .unwrap_err()
            .reason,
        Failure::WorkBudget { maximum: 0 }
    ));
    let mut limits = budget();
    limits.spatial.max_events = usize::MAX;
    // Two catalogue definitions force the checked possible lookup-work product to overflow.
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![
            definition(0xffff, TeletextObjectType::Active, vec![glyph()]),
            definition(3, TeletextObjectType::Passive, vec![glyph()]),
        ],
        limits.catalog,
    )
    .unwrap();
    assert!(matches!(
        plan_teletext_origin_carrier_placement(&plan, &catalog, limits)
            .unwrap_err()
            .reason,
        Failure::AllocationSize
    ));
    let exact = TeletextOriginCarrierPlacementBudget {
        max_commands: before.commands().len(),
        max_origins: plan.origins().len(),
        max_edges: plan.precedence().len(),
        catalog: TeletextObjectCatalogBudget {
            max_objects: 1,
            max_commands: 1,
        },
        ..budget()
    };
    assert_eq!(
        plan_teletext_origin_carrier_placement(&plan, service.objects(), exact)
            .unwrap()
            .components()
            .len(),
        1
    );
    assert_eq!(composition.program(), &before);
}
#[test]
fn fresh_actual_trace_rejects_missing_catalogue_and_expanded_event_cutoff() {
    let service = basic();
    let composition = compose(&service);
    let plan =
        plan_teletext_base_compatibility_realization(&composition, closure_budget()).unwrap();
    let empty = TeletextEnhancementObjectCatalog::new(vec![], budget().catalog).unwrap();
    let error = plan_teletext_origin_carrier_placement(&plan, &empty, budget()).unwrap_err();
    assert!(matches!(error.reason, Failure::Spatial(_)));
    assert_eq!(error.location.unwrap().object, None);
    let mut limits = budget();
    limits.spatial.max_events = 0;
    assert!(matches!(
        plan_teletext_origin_carrier_placement(&plan, service.objects(), limits)
            .unwrap_err()
            .reason,
        Failure::Spatial(_)
    ));
}
#[test]
fn raw_panel_inverse_and_componentwise_offset_boundaries_are_literal() {
    assert_eq!(inverse_visual(4, 12, 1, 0), Some((1, 68)));
    assert_eq!(inverse_visual(4, 12, 1, 3), Some((1, 71)));
    assert_eq!(inverse_visual(4, 12, 1, 4), Some((1, 0)));
    assert_eq!(inverse_visual(4, 12, 1, 43), Some((1, 39)));
    assert_eq!(inverse_visual(4, 12, 1, 44), Some((1, 40)));
    assert_eq!(inverse_visual(4, 12, 1, 55), Some((1, 51)));
    assert_eq!(inverse_visual(4, 12, 0, 0), None);
    assert_eq!(inverse_visual(4, 12, 0, 44), None);
    assert_eq!(inverse_visual(4, 12, 0, 11), None);
    assert_eq!(inverse_visual(4, 12, 0, 12), Some((0, 8)));
    assert_eq!(inverse_visual(4, 12, 1, 56), None);
    assert_eq!(
        offset((0, 0), (23, 71)).map(|o| (o.row(), o.column())),
        Some((23, 71))
    );
    assert_eq!(offset((0, 0), (24, 71)), None);
    assert_eq!(
        offset((1, 0), (24, 71)).map(|o| (o.row(), o.column())),
        Some((23, 71))
    );
    assert_eq!(offset((1, 9), (2, 8)), None);
    assert_eq!(offset((3, 0), (2, 8)), None);
}

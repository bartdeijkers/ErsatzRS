//! Final ownership oracles, not synthetic-carrier wire or receiver acceptance.

use super::*;
use crate::teletext::{compose_teletext_main_program, TeletextMainCompositionBudget};
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
fn at(operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation,
    }
}
fn glyph() -> TeletextEnhancementCommand {
    at(TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(b'A').unwrap(),
    })
}
fn size(wide: bool, high: bool) -> TeletextEnhancementCommand {
    at(TeletextColumnOperation::Attributes {
        double_width: wide,
        double_height: high,
        boxed: false,
        conceal: false,
        invert: false,
        underline_or_separated_mosaics: false,
    })
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
    base_row: u8,
    main: Vec<TeletextEnhancementCommand>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    panels: Option<TeletextSidePanels>,
) -> TeletextSemanticService {
    let mut page = crate::teletext::transport_tests::draft(200, 0x1234, false);
    // German makes pound a real generated G2 event, unlike the English subset.
    page.national_option = TeletextNationalOption::new(1).unwrap();
    page.rows = vec![TeletextRow {
        row: base_row,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "£".into() },
        }],
    }];
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
fn budget() -> TeletextBaseCompatibilityRealizationBudget {
    TeletextBaseCompatibilityRealizationBudget {
        max_commands: 1024,
        max_candidates: 2048,
        max_origins: 4096,
        max_seeds: 40,
        max_edges: 4096,
        max_updates: 20_000,
    }
}
fn active(commands: Vec<TeletextEnhancementCommand>, base_row: u8) -> TeletextMainComposition {
    compose(&service(
        base_row,
        vec![position(1, 8), invoke(0xffff, TeletextObjectType::Active)],
        vec![definition(0xffff, TeletextObjectType::Active, commands)],
        None,
    ))
}
fn partial() -> TeletextMainComposition {
    compose(&service(
        1,
        vec![
            position(1, 8),
            invoke(0xffff, TeletextObjectType::Active),
            position(1, 9),
            invoke(2, TeletextObjectType::Passive),
        ],
        vec![
            definition(
                0xffff,
                TeletextObjectType::Active,
                vec![size(true, true), glyph()],
            ),
            definition(2, TeletextObjectType::Passive, vec![glyph()]),
        ],
        None,
    ))
}

#[test]
fn actual_base_collision_retains_exact_active_source_and_borrowed_context() {
    let service = service(
        1,
        vec![position(1, 8), invoke(0xffff, TeletextObjectType::Active)],
        vec![definition(
            0xffff,
            TeletextObjectType::Active,
            vec![glyph()],
        )],
        None,
    );
    let before = service.clone();
    let composition = compose(&service);
    let plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
    assert!(std::ptr::eq(
        std::ptr::from_ref(plan.composition()),
        std::ptr::from_ref(&composition)
    ));
    assert_eq!(plan.seeds().len(), 1);
    let seed = plan.seeds()[0];
    assert_eq!((seed.base_position, seed.visual_position), ((1, 8), (1, 8)));
    assert!(matches!(
        composition.provenance()[seed.composed_command],
        TeletextMainCompositionSource::BaseCell {
            kind: TeletextBaseCellCommandKind::Glyph,
            ..
        }
    ));
    let origin = plan.origins()[0];
    assert_eq!(origin.origin_index, seed.origin);
    assert_eq!(origin.candidate.priority(), TeletextGlyphPriority::Active);
    assert_eq!(
        origin.candidate.source(),
        Some(TeletextEnhancementSource {
            object: Some(target(0xffff)),
            command: 0
        })
    );
    assert_eq!(
        origin.candidate.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(origin.candidate.invocation().unwrap().get(), 0);
    assert_eq!(
        origin.rectangle,
        Some(TeletextBaseCompatibilityRectangle {
            row: 1,
            column: 8,
            height: 1,
            width: 1
        })
    );
    assert_eq!(
        plan.fragments(),
        &[TeletextBaseCompatibilityFragment {
            visual_position: (1, 8),
            fragment: TeletextDisplayFragment::Normal,
            origin: Some(seed.origin)
        }]
    );
    assert!(plan.precedence().is_empty());
    assert_eq!(service, before);
}

#[test]
fn double_height_bottom_seed_closes_over_top_and_bottom() {
    let composition = active(vec![size(false, true), glyph()], 2);
    let plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
    assert_eq!(plan.seeds()[0].visual_position, (2, 8));
    assert_eq!(
        plan.origins()[0].rectangle,
        Some(TeletextBaseCompatibilityRectangle {
            row: 1,
            column: 8,
            height: 2,
            width: 1
        })
    );
    assert_eq!(
        plan.fragments()
            .iter()
            .map(|cell| (cell.visual_position, cell.fragment))
            .collect::<Vec<_>>(),
        vec![
            ((1, 8), TeletextDisplayFragment::DoubleHeightTop),
            ((2, 8), TeletextDisplayFragment::DoubleHeightBottom)
        ]
    );
}

#[test]
fn higher_partial_owner_is_added_with_whole_rectangle_precedence() {
    let composition = partial();
    let plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
    assert_eq!(plan.origins().len(), 2);
    let lower = plan
        .origins()
        .iter()
        .find(|origin| origin.candidate.priority() == TeletextGlyphPriority::Active)
        .unwrap();
    let upper = plan
        .origins()
        .iter()
        .find(|origin| origin.candidate.priority() == TeletextGlyphPriority::Passive)
        .unwrap();
    assert_eq!(lower.rectangle.unwrap().height, 2);
    assert_eq!(lower.rectangle.unwrap().width, 2);
    assert_eq!(
        upper.candidate.source(),
        Some(TeletextEnhancementSource {
            object: Some(target(2)),
            command: 0
        })
    );
    assert_eq!(
        plan.precedence(),
        &[TeletextBaseCompatibilityPrecedence {
            before: lower.origin_index,
            after: upper.origin_index,
            visual_position: (1, 9)
        }]
    );
    assert_eq!(plan.fragments().len(), 4);
    let right = plan
        .fragments()
        .iter()
        .find(|cell| cell.visual_position == (1, 9))
        .unwrap();
    assert_eq!(
        (right.fragment, right.origin),
        (TeletextDisplayFragment::Normal, Some(upper.origin_index))
    );
    assert_eq!(
        composition
            .display_resolution()
            .cell(2, 9)
            .unwrap()
            .origin(),
        Some(lower.origin_index)
    );
}

#[test]
fn selected_panel_status_maps_raw_base_seed_to_actual_visual_column() {
    for (status, expected) in [
        (TeletextPanelStatus::Level25And35, 12),
        (TeletextPanelStatus::Level35Only, 8),
    ] {
        let panels = TeletextSidePanels {
            left: true,
            right: true,
            status,
            width: TeletextPanelWidth::new(4).unwrap(),
        };
        let composition = compose(&service(
            1,
            vec![position(1, 8), invoke(0xffff, TeletextObjectType::Active)],
            vec![definition(
                0xffff,
                TeletextObjectType::Active,
                vec![glyph()],
            )],
            Some(panels),
        ));
        let plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
        assert_eq!(plan.seeds()[0].base_position, (1, 8));
        assert_eq!(plan.seeds()[0].visual_position, (1, expected));
        assert_eq!(plan.fragments()[0].visual_position, (1, expected));
    }
}

#[test]
fn no_seed_keeps_unknown_active_and_all_original_pending() {
    let composition = active(vec![size(false, false), size(true, false), glyph()], 1);
    assert!(composition
        .display_resolution()
        .pending()
        .iter()
        .any(|pending| pending.reason() == TeletextDisplayPendingReason::UnknownSize));
    let plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
    assert!(plan.seeds().is_empty());
    assert!(plan.origins().is_empty());
    assert_eq!(
        plan.original_pending(),
        composition.display_resolution().pending()
    );
    assert!(!plan.original_pending().is_empty());
    assert!(plan
        .original_pending()
        .iter()
        .any(|pending| pending.invocation().is_some_and(|id| id.get() == 0)));
    assert!(std::ptr::eq(
        std::ptr::from_ref(plan.composition()),
        std::ptr::from_ref(&composition)
    ));
}

#[test]
fn retained_base_word_without_active_owner_has_no_collision_seed() {
    let composition = compose(&service(1, vec![], vec![], None));
    assert!(composition.provenance().iter().any(|source| matches!(
        source,
        TeletextMainCompositionSource::BaseCell {
            kind: TeletextBaseCellCommandKind::Glyph,
            ..
        }
    )));
    let plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
    assert!(plan.seeds().is_empty());
    assert!(plan.origins().is_empty());
    assert!(plan.fragments().is_empty());
    assert_eq!(
        plan.original_pending(),
        composition.display_resolution().pending()
    );
}

#[test]
fn count_fences_precede_work_and_closure_cutoffs_retain_source() {
    let composition = partial();
    let before = composition.program().clone();
    for (kind, count) in [
        (
            "commands",
            composition
                .program()
                .commands()
                .len()
                .max(composition.spatial_trace().events().len()),
        ),
        ("candidates", composition.glyph_layers().candidates().len()),
        ("origins", composition.display_resolution().origins().len()),
    ] {
        let mut limits = budget();
        limits.max_updates = 0;
        match kind {
            "commands" => limits.max_commands = count - 1,
            "candidates" => limits.max_candidates = count - 1,
            _ => limits.max_origins = count - 1,
        }
        let error = plan_teletext_base_compatibility_realization(&composition, limits).unwrap_err();
        assert_eq!(
            error.reason,
            TeletextBaseCompatibilityRealizationFailure::InputBudget {
                kind,
                count,
                maximum: count - 1
            }
        );
    }
    let mut limits = budget();
    limits.max_updates = 0;
    assert_eq!(
        plan_teletext_base_compatibility_realization(&composition, limits)
            .unwrap_err()
            .reason,
        TeletextBaseCompatibilityRealizationFailure::WorkBudget { maximum: 0 }
    );
    for seed_limit in [true, false] {
        let mut limits = budget();
        if seed_limit {
            limits.max_seeds = 0;
        } else {
            limits.max_edges = 0;
        }
        let error = plan_teletext_base_compatibility_realization(&composition, limits).unwrap_err();
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: Some(target(0xffff)),
                command: 1
            })
        );
        assert_eq!(error.invocation.unwrap().get(), 0);
        assert_eq!(
            error.reason,
            if seed_limit {
                TeletextBaseCompatibilityRealizationFailure::SeedBudget { maximum: 0 }
            } else {
                TeletextBaseCompatibilityRealizationFailure::EdgeBudget { maximum: 0 }
            }
        );
    }
    let plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
    let exact = TeletextBaseCompatibilityRealizationBudget {
        max_seeds: plan.seeds().len(),
        max_edges: plan.precedence().len(),
        ..budget()
    };
    assert_eq!(
        plan_teletext_base_compatibility_realization(&composition, exact)
            .unwrap()
            .fragments(),
        plan.fragments()
    );
    assert_eq!(composition.program(), &before);
}

#[test]
fn cycle_graph_keeps_cycle_and_downstream_blocked_origins_pending() {
    // Private graph calibration, not a claim this graph is publicly authored.
    let composition = partial();
    let mut plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
    let a = plan.origins()[0].origin_index;
    let b = plan.origins()[1].origin_index;
    let c = composition
        .display_resolution()
        .origins()
        .iter()
        .position(|origin| {
            origin.candidate_index() != plan.origins()[0].original.candidate_index()
                && origin.candidate_index() != plan.origins()[1].original.candidate_index()
        })
        .unwrap();
    plan.precedence.extend([
        TeletextBaseCompatibilityPrecedence {
            before: b,
            after: a,
            visual_position: (1, 8),
        },
        TeletextBaseCompatibilityPrecedence {
            before: b,
            after: c,
            visual_position: (2, 8),
        },
    ]);
    let original = *composition.display_resolution().origins().get(c).unwrap();
    plan.origins.push(TeletextBaseCompatibilityOrigin {
        origin_index: c,
        original,
        candidate: composition.glyph_layers().candidates()[original.candidate_index()],
        rectangle: rectangle(original),
    });
    let mut included = vec![false; composition.display_resolution().origins().len()];
    for index in [a, b, c] {
        included[index] = true;
    }
    let mut planner = Planner {
        composition: &composition,
        budget: budget(),
        remaining: budget().max_updates,
    };
    planner.cycles(&mut plan, &included).unwrap();
    let blocked: Vec<_> = plan
        .pending()
        .iter()
        .filter(|pending| {
            pending.reason == TeletextBaseCompatibilityPendingReason::CyclicPrecedence
        })
        .map(|pending| pending.origin)
        .collect();
    for index in [a, b, c] {
        assert!(blocked.contains(&index));
    }
}

#[test]
fn closure_extends_to_higher_owner_origin_outside_initial_rectangle() {
    let composition = compose(&service(
        2,
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
                vec![size(true, false), glyph()],
            ),
            definition(
                0xffff,
                TeletextObjectType::Active,
                vec![size(true, true), glyph()],
            ),
        ],
        None,
    ));
    let plan = plan_teletext_base_compatibility_realization(&composition, budget()).unwrap();
    assert_eq!(plan.seeds()[0].visual_position, (2, 8));
    assert_eq!(plan.origins().len(), 2);
    assert_eq!(plan.fragments().len(), 5);
    let active = plan
        .origins()
        .iter()
        .find(|origin| origin.candidate.priority() == TeletextGlyphPriority::Active)
        .unwrap();
    let passive = plan
        .origins()
        .iter()
        .find(|origin| origin.candidate.priority() == TeletextGlyphPriority::Passive)
        .unwrap();
    assert_eq!(
        active.rectangle,
        Some(TeletextBaseCompatibilityRectangle {
            row: 1,
            column: 8,
            height: 2,
            width: 2
        })
    );
    assert_eq!(
        passive.rectangle,
        Some(TeletextBaseCompatibilityRectangle {
            row: 1,
            column: 7,
            height: 1,
            width: 2
        })
    );
    assert_eq!(
        plan.precedence(),
        &[TeletextBaseCompatibilityPrecedence {
            before: active.origin_index,
            after: passive.origin_index,
            visual_position: (1, 8)
        }]
    );
    assert!(plan
        .fragments()
        .iter()
        .any(|cell| cell.visual_position == (1, 7)
            && cell.origin == Some(passive.origin_index)
            && cell.fragment == TeletextDisplayFragment::DoubleWidthLeft));
    assert_eq!(
        plan.fragments()
            .iter()
            .filter(|cell| cell.origin == Some(active.origin_index))
            .count(),
        3
    );
}

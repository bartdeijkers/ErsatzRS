//! Private wrapper source authority and shared admission, without publication claims.
use super::*;
use crate::teletext::{
    compose_teletext_main_program, plan_teletext_base_compatibility_realization,
    plan_teletext_origin_carrier_placement, TeletextBaseCompatibilityRealizationBudget,
    TeletextMainCompositionBudget, TeletextOriginCarrierPlacementBudget,
};
use ersatzrs_domain::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(124).unwrap(),
        subcode: TeletextSubcode::new(32).unwrap(),
    }
}
fn target(page: u8, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, page).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}
fn service() -> TeletextSemanticService {
    let language = TeletextLanguage::new("eng".to_owned()).unwrap();
    let body = TeletextEnhancementProgram::new(
        (0..7)
            .map(|column| TeletextEnhancementCommand::AtColumn {
                column,
                operation: TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(1).unwrap(),
                },
            })
            .collect(),
    )
    .unwrap();
    let definition = |identity| TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Passive,
        program: body.clone(),
    };
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: language.clone(),
                initial_page: owner().page,
                pages: vec![TeletextPageDraft {
                    address: owner(),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(0).unwrap(),
                    rows: vec![],
                    navigation: vec![],
                }],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation: None,
                compositional_links: None,
                local_program: None,
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: Some(target(0xa1, 3).resource().unwrap().page()),
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [
                        None,
                        None,
                        Some(TeletextDefaultObjectSelection {
                            target: target(0xa0, 1),
                            object_type: TeletextObjectType::Passive,
                        }),
                        Some(TeletextDefaultObjectSelection {
                            target: target(0xa0, 2),
                            object_type: TeletextObjectType::Passive,
                        }),
                    ],
                },
            }],
            magazines: vec![],
            objects: vec![
                definition(target(0xa0, 1)),
                definition(target(0xa0, 2)),
                definition(target(0xa1, 3)),
            ],
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 16,
            max_structure_items: 2048,
            max_base_content_bytes: 1024,
            max_commands: 1024,
            max_collected_commands: 1024,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}
fn budget() -> TeletextAugmentedOriginProgramBudget {
    TeletextAugmentedOriginProgramBudget {
        max_input_commands: 1024,
        max_output_commands: 1024,
        max_carriers: 8,
        max_identity_checks: 1000,
        max_work: 100_000,
        recipe: TeletextOriginCarrierBudget {
            max_candidates: 1024,
            max_origins: 1024,
            catalog: TeletextObjectCatalogBudget {
                max_objects: 16,
                max_commands: 1024,
            },
            max_id_checks: 1000,
            max_output_commands: 6,
        },
    }
}
fn composition(service: &TeletextSemanticService) -> super::super::TeletextMainComposition {
    compose_teletext_main_program(
        service,
        owner(),
        TeletextMainCompositionBudget {
            max_input_commands: 1024,
            max_output_commands: 1024,
            collection: TeletextEnhancementCollectionBudget {
                max_commands: 1024,
                max_drcs_references: 0,
            },
        },
    )
    .unwrap()
}
fn plan(
    composition: &super::super::TeletextMainComposition,
) -> super::super::TeletextBaseCompatibilityRealizationPlan<'_> {
    plan_teletext_base_compatibility_realization(
        composition,
        TeletextBaseCompatibilityRealizationBudget {
            max_commands: 1024,
            max_candidates: 1024,
            max_origins: 1024,
            max_seeds: 64,
            max_edges: 1024,
            max_updates: 10_000,
        },
    )
    .unwrap()
}
fn placement<'a>(
    plan: &'a super::super::TeletextBaseCompatibilityRealizationPlan<'a>,
    service: &'a TeletextSemanticService,
) -> TeletextOriginCarrierPlacement<'a> {
    plan_teletext_origin_carrier_placement(
        plan,
        service.objects(),
        TeletextOriginCarrierPlacementBudget {
            max_commands: 1024,
            max_origins: 1024,
            max_edges: 1024,
            catalog: budget().recipe.catalog,
            spatial: TeletextSpatialTraceBudget {
                max_events: 1024,
                max_invocations: 64,
            },
            max_work: 10_000,
        },
    )
    .unwrap()
}
fn assert_replacement_refusals(
    wrapper: &ForeignDefault,
    ordinal: usize,
    original: TeletextEnhancementCommand,
) {
    if let TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        ..
    } = original
    {
        assert_eq!(
            wrapper.replace(
                ordinal,
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    offset: Some(TeletextObjectOffset::new(0, 1).unwrap())
                }
            ),
            None
        );
        assert_eq!(
            wrapper.replace(
                ordinal,
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type: TeletextObjectType::Active,
                    offset: None
                }
            ),
            None
        );
    }
}

#[test]
fn foreign_default_wrappers_map_exact_roots_but_never_authored_editor_sources() {
    let service = service();
    let before = service.clone();
    let composition = composition(&service);
    let plan = plan(&composition);
    let placement = placement(&plan, &service);
    let candidate =
        build_augmented_with_foreign_defaults_and_work_usage(&placement, &service, budget())
            .0
            .unwrap();
    assert_eq!(candidate.foreign_default_count(), 2);
    let roots = composition.spatial_trace().invocations();
    assert_eq!(roots.len(), 2);
    for (index, root) in roots.iter().enumerate() {
        let wrapper = &candidate.foreign_defaults[index];
        let physical = wrapper.identity();
        assert_ne!(physical, root.target());
        assert_eq!(
            candidate.foreign_default_target(physical, root.id()),
            Some(root.target())
        );
        assert_eq!(
            candidate.foreign_default_target(physical, roots[1 - index].id()),
            None
        );
        assert_eq!(
            candidate.foreign_default_target(root.target(), root.id()),
            None
        );
        let actual = candidate.catalog().definition(physical).unwrap();
        let original = service.objects().definition(root.target()).unwrap();
        assert_eq!(actual.object_type, original.object_type);
        assert_eq!(actual.program, original.program);
        assert!(actual.program.commands().len() > budget().recipe.max_output_commands);
        for command in 0..actual.program.commands().len() {
            let source = TeletextEnhancementSource {
                object: Some(physical),
                command,
            };
            assert_eq!(
                candidate.emitter_source(source),
                Some(TeletextAugmentedOriginSource::RetainedObject {
                    identity: root.target(),
                    command
                })
            );
            assert_eq!(candidate.original_object_source(source), None);
        }
        let outside = TeletextEnhancementSource {
            object: Some(physical),
            command: actual.program.commands().len(),
        };
        assert_eq!(candidate.emitter_source(outside), None);
        let mut changed = false;
        for (ordinal, provenance) in composition.provenance().iter().enumerate() {
            let original = composition.program().commands()[ordinal];
            if *provenance
                == (TeletextMainCompositionSource::DefaultSlot {
                    slot: if index == 0 { 2 } else { 3 },
                })
            {
                assert_eq!(
                    wrapper.replace(ordinal, original),
                    Some(candidate.program().commands()[ordinal])
                );
                assert_replacement_refusals(wrapper, ordinal, original);
                changed = true;
            } else {
                assert_eq!(wrapper.replace(ordinal, original), None);
            }
        }
        assert!(changed);
    }
    assert_eq!(service, before);
    let strict = build_augmented_with_service_and_work_usage(&placement, &service, budget())
        .0
        .unwrap();
    assert_eq!(strict.foreign_default_count(), 0);
    assert_eq!(strict.catalog(), service.objects());
}
#[test]
fn foreign_default_identity_and_combined_carrier_limits_share_existing_usage_on_failure() {
    let service = service();
    let composition = composition(&service);
    let plan = plan(&composition);
    let placement = placement(&plan, &service);
    let (candidate, used) =
        build_augmented_with_foreign_defaults_and_work_usage(&placement, &service, budget());
    assert!(candidate.is_ok());
    assert!(used.identity_checks > 0);
    let mut limited = budget();
    limited.max_identity_checks = used.identity_checks - 1;
    let (result, failed) =
        build_augmented_with_foreign_defaults_and_work_usage(&placement, &service, limited);
    assert!(
        matches!(result,Err(error) if matches!(error.reason,TeletextAugmentedOriginProgramFailure::InputBudget {kind:"identity checks",..}))
    );
    assert!(failed.identity_checks > 0 && failed.identity_checks <= limited.max_identity_checks);
    assert!(failed.work > 0);
    limited = budget();
    limited.max_carriers = 1;
    let (result, failed) =
        build_augmented_with_foreign_defaults_and_work_usage(&placement, &service, limited);
    assert!(
        matches!(result,Err(error) if matches!(error.reason,TeletextAugmentedOriginProgramFailure::InputBudget {kind:"carriers",..}))
    );
    assert_eq!(failed.identity_checks, 0);
}

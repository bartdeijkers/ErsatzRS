//! Candidate assembly/lineage oracles, not normalization or receiver acceptance.
use super::*;
use crate::teletext::{
    build_teletext_augmented_origin_program, plan_teletext_base_compatibility_realization,
    plan_teletext_origin_carrier_placement, TeletextAugmentedOriginProgramBudget,
    TeletextAugmentedOriginProgramFailure as Failure, TeletextAugmentedOriginSource as Source,
    TeletextBaseCompatibilityRealizationBudget, TeletextOriginCarrierBudget,
    TeletextOriginCarrierCommandRole as Role, TeletextOriginCarrierFailure,
    TeletextOriginCarrierPlacementBudget,
};
fn limits() -> TeletextAugmentedOriginProgramBudget {
    TeletextAugmentedOriginProgramBudget {
        max_input_commands: 1024,
        max_output_commands: 2048,
        max_carriers: 32,
        max_identity_checks: 100_000,
        max_work: 100_000,
        recipe: TeletextOriginCarrierBudget {
            max_candidates: 4096,
            max_origins: 8192,
            catalog: TeletextObjectCatalogBudget {
                max_objects: 64,
                max_commands: 8192,
            },
            max_id_checks: 65600,
            max_output_commands: 6,
        },
    }
}
fn closure_limits() -> TeletextBaseCompatibilityRealizationBudget {
    TeletextBaseCompatibilityRealizationBudget {
        max_commands: 2048,
        max_candidates: 4096,
        max_origins: 8192,
        max_seeds: 64,
        max_edges: 4096,
        max_updates: 50000,
    }
}
fn placement_limits() -> TeletextOriginCarrierPlacementBudget {
    TeletextOriginCarrierPlacementBudget {
        max_commands: 2048,
        max_origins: 8192,
        max_edges: 4096,
        catalog: limits().recipe.catalog,
        spatial: TeletextSpatialTraceBudget {
            max_events: 4096,
            max_invocations: 256,
        },
        max_work: 100_000,
    }
}
fn active_fixture() -> Fixture {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "£")],
        Some(vec![
            position(1, 8),
            invoke(local(0xffff), TeletextObjectType::Active),
            position(3, 20),
        ]),
    );
    fixture.objects.push(definition(
        local(0xffff),
        TeletextObjectType::Active,
        vec![g0(0)],
    ));
    fixture
}
fn attributes(size: TeletextDisplaySize) -> TeletextEnhancementCommand {
    at(
        0,
        TeletextColumnOperation::Attributes {
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
    )
}
fn repeated_fixture() -> Fixture {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "£"), text(2, 8, "£")],
        Some(vec![
            position(1, 8),
            invoke(local(0xffff), TeletextObjectType::Active),
            position(2, 8),
            invoke(local(0xffff), TeletextObjectType::Active),
        ]),
    );
    fixture.objects.push(definition(
        local(0xffff),
        TeletextObjectType::Active,
        vec![g0(0)],
    ));
    fixture
}
fn retained_commands(
    candidate: &crate::teletext::TeletextAugmentedOriginProgram<'_>,
) -> Vec<TeletextEnhancementCommand> {
    candidate
        .program()
        .commands()
        .iter()
        .zip(candidate.main_sources())
        .filter_map(|(&command, source)| {
            matches!(source, Source::RetainedMain { .. }).then_some(command)
        })
        .collect()
}
fn assert_carrier_lineage(
    candidate: &crate::teletext::TeletextAugmentedOriginProgram<'_>,
    expected_origin: usize,
) {
    let carrier = &candidate.carriers()[0];
    assert_eq!(carrier.identity(), local(0));
    assert_eq!(
        carrier.original_source(),
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        })
    );
    assert_eq!(
        carrier.original_root_source(),
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(carrier.original_invocation().unwrap().get(), 0);
    assert_eq!(carrier.original_origin_index(), expected_origin);
    assert_eq!(
        carrier.command_roles(),
        &[
            Role::Foreground,
            Role::Background,
            Role::Flash,
            Role::Display,
            Role::Charset,
            Role::Glyph
        ]
    );
    for (command, &role) in carrier.command_roles().iter().enumerate() {
        assert_eq!(
            candidate.emitter_source(TeletextEnhancementSource {
                object: Some(carrier.identity()),
                command
            }),
            Some(Source::CarrierBody { carrier: 0, role })
        );
    }
    assert_eq!(
        candidate.emitter_source(TeletextEnhancementSource {
            object: Some(carrier.identity()),
            command: 6
        }),
        None
    );
}
#[test]
fn exact_shifted_main_and_object_emitters_preserve_original_values_and_base_provenance() {
    let service = active_fixture().build();
    let composition = compose(&service);
    let before = (service.clone(), composition.clone());
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    let candidate = build_teletext_augmented_origin_program(&placement, limits()).unwrap();
    assert!(std::ptr::eq(
        std::ptr::from_ref(candidate.placement()),
        std::ptr::from_ref(&placement)
    ));
    assert_eq!(
        retained_commands(&candidate),
        composition.program().commands()
    );
    assert_eq!(
        &candidate.catalog().definitions()[..1],
        service.objects().definitions()
    );
    assert_eq!(
        candidate.main_sources().len(),
        candidate.program().commands().len()
    );
    let mut shifted = false;
    let mut generated_base = false;
    for (ordinal, source) in candidate.main_sources().iter().enumerate() {
        assert_eq!(
            candidate.emitter_source(TeletextEnhancementSource {
                object: None,
                command: ordinal
            }),
            Some(*source)
        );
        if let Source::RetainedMain {
            composed_command,
            provenance,
        } = *source
        {
            assert_eq!(
                candidate.program().commands()[ordinal],
                composition.program().commands()[composed_command]
            );
            assert_eq!(provenance, composition.provenance()[composed_command]);
            shifted |= ordinal > composed_command;
            generated_base |= matches!(
                provenance,
                TeletextMainCompositionSource::BaseCell {
                    kind: TeletextBaseCellCommandKind::Glyph,
                    ..
                }
            );
        }
    }
    assert!(shifted && generated_base);
    assert_eq!(
        candidate.emitter_source(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        }),
        Some(Source::RetainedObject {
            identity: local(0xffff),
            command: 0
        })
    );
    assert_eq!(
        candidate.emitter_source(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 1
        }),
        None
    );
    assert_eq!(
        candidate.emitter_source(TeletextEnhancementSource {
            object: None,
            command: candidate.main_sources().len()
        }),
        None
    );
    assert_carrier_lineage(&candidate, closure.seeds()[0].origin);
    assert_eq!((service, composition), before);
}
#[test]
fn repeated_origins_receive_unique_lowest_free_ids_and_exact_two_pass_identity_budget() {
    let service = repeated_fixture().build();
    let composition = compose(&service);
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    // Count pass: two times (one catalogue entry + probe0). Emit: same first,
    // then two entries + probes0/1. Aggregate 4+2+4=10, not per-recipe reset.
    let mut exact = limits();
    exact.max_identity_checks = 10;
    exact.recipe.catalog.max_objects = 3;
    exact.recipe.catalog.max_commands = 13;
    let candidate = build_teletext_augmented_origin_program(&placement, exact).unwrap();
    assert_eq!(
        candidate
            .carriers()
            .iter()
            .map(crate::teletext::TeletextAugmentedOriginCarrier::identity)
            .collect::<Vec<_>>(),
        vec![local(0), local(1)]
    );
    assert_eq!(
        candidate
            .carriers()
            .iter()
            .map(|c| c.original_invocation().unwrap().get())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_ne!(
        candidate.carriers()[0].original_origin_index(),
        candidate.carriers()[1].original_origin_index()
    );
    assert_eq!(
        candidate.catalog().definitions()[0],
        service.objects().definitions()[0]
    );
    exact.max_identity_checks = 9;
    let error = build_teletext_augmented_origin_program(&placement, exact).unwrap_err();
    let Failure::Carrier(error) = error.reason else {
        panic!("aggregate ID cutoff lost recipe diagnostics");
    };
    assert!(matches!(
        error.reason,
        TeletextOriginCarrierFailure::InputBudget {
            kind: "identity checks",
            count: 4,
            maximum: 3
        }
    ));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        })
    );
    assert_eq!(error.invocation.unwrap().get(), 1);
}
#[test]
fn partial_enlarged_component_calls_are_contiguous_in_dag_order_with_exact_offsets() {
    let mut fixture = Fixture::new(
        vec![text(2, 8, "£")],
        Some(vec![
            position(1, 7),
            invoke(local(2), TeletextObjectType::Passive),
            position(1, 8),
            invoke(local(0xffff), TeletextObjectType::Active),
        ]),
    );
    fixture.objects = vec![
        definition(
            local(2),
            TeletextObjectType::Passive,
            vec![attributes(TeletextDisplaySize::DoubleWidth), g0(0)],
        ),
        definition(
            local(0xffff),
            TeletextObjectType::Active,
            vec![attributes(TeletextDisplaySize::DoubleSize), g0(0)],
        ),
    ];
    let service = fixture.build();
    let composition = compose(&service);
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    let candidate = build_teletext_augmented_origin_program(&placement, limits()).unwrap();
    assert_eq!(
        candidate
            .carriers()
            .iter()
            .map(|c| c.original_source().unwrap().object.unwrap())
            .collect::<Vec<_>>(),
        vec![local(0xffff), local(2)]
    );
    let indices: Vec<_> = candidate
        .main_sources()
        .iter()
        .enumerate()
        .filter_map(|(index, source)| matches!(source, Source::CarrierCall { .. }).then_some(index))
        .collect();
    assert_eq!(indices.len(), 2);
    assert_eq!(indices[1], indices[0] + 1);
    for (index, (target, row, column)) in indices
        .into_iter()
        .zip([(local(0), 0, 1), (local(1), 0, 0)])
    {
        assert_eq!(
            candidate.program().commands()[index],
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Passive,
                offset: Some(TeletextObjectOffset::new(row, column).unwrap())
            }
        );
    }
    assert_eq!(
        &candidate.catalog().definitions()[..2],
        service.objects().definitions()
    );
}
#[test]
fn independent_components_sharing_gap_have_stable_component_order() {
    let mut fixture = Fixture::new(
        vec![text(2, 8, "£ £")],
        Some(vec![
            position(1, 0),
            TeletextEnhancementCommand::InvokeObject {
                target: local(0xffff),
                object_type: TeletextObjectType::Active,
                offset: Some(TeletextObjectOffset::new(0, 8).unwrap()),
            },
            TeletextEnhancementCommand::InvokeObject {
                target: local(0xfffe),
                object_type: TeletextObjectType::Active,
                offset: Some(TeletextObjectOffset::new(0, 10).unwrap()),
            },
        ]),
    );
    fixture.objects = vec![
        definition(
            local(0xffff),
            TeletextObjectType::Active,
            vec![attributes(TeletextDisplaySize::DoubleHeight), g0(0)],
        ),
        definition(
            local(0xfffe),
            TeletextObjectType::Active,
            vec![attributes(TeletextDisplaySize::DoubleHeight), g0(0)],
        ),
    ];
    let service = fixture.build();
    let composition = compose(&service);
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    assert_eq!(placement.components().len(), 2);
    assert_eq!(
        placement.components()[0].gap(),
        placement.components()[1].gap()
    );
    assert_eq!(
        placement.components()[0].gap().unwrap().caller_position(),
        (1, 0)
    );
    let candidate = build_teletext_augmented_origin_program(&placement, limits()).unwrap();
    let calls: Vec<_> = candidate
        .main_sources()
        .iter()
        .enumerate()
        .filter_map(|(index, source)| match source {
            Source::CarrierCall { carrier } => Some((index, *carrier)),
            _ => None,
        })
        .collect();
    assert_eq!(calls[1], (calls[0].0 + 1, 1));
    assert_eq!(calls[0].1, 0);
    assert_eq!(
        candidate
            .carriers()
            .iter()
            .map(crate::teletext::TeletextAugmentedOriginCarrier::component_index)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert_eq!(
        candidate
            .carriers()
            .iter()
            .map(|c| c.original_source().unwrap().object.unwrap())
            .collect::<Vec<_>>(),
        vec![local(0xffff), local(0xfffe)]
    );
}
#[test]
fn zero_seed_retains_original_pending_and_exact_program_catalogue_without_recipe_authority() {
    let mut fixture = active_fixture();
    fixture.objects[0].program = program(vec![
        attributes(TeletextDisplaySize::Normal),
        attributes(TeletextDisplaySize::DoubleWidth),
        g0(0),
    ]);
    let service = fixture.build();
    let composition = compose(&service);
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    assert!(placement.components().is_empty());
    assert!(!placement.plan().original_pending().is_empty());
    let mut exact = limits();
    exact.max_carriers = 0;
    exact.max_identity_checks = 0;
    let candidate = build_teletext_augmented_origin_program(&placement, exact).unwrap();
    assert!(candidate.carriers().is_empty());
    assert_eq!(candidate.program(), composition.program());
    assert_eq!(candidate.catalog(), service.objects());
    assert_eq!(
        candidate.placement().plan().original_pending(),
        composition.display_resolution().pending()
    );
    assert!(candidate
        .main_sources()
        .iter()
        .all(|source| matches!(source, Source::RetainedMain { .. })));
}
#[test]
fn pending_placement_and_unknown_recipe_fail_with_original_origin_source_and_no_mutation() {
    for uncertain_glyph in [true, false] {
        let mut fixture = active_fixture();
        fixture.objects[0].program = if uncertain_glyph {
            program(vec![
                charset(0, 0x47),
                at(0, TeletextColumnOperation::G0 { code: code(0x7e) }),
            ])
        } else {
            program(vec![
                at(
                    0,
                    TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(4).unwrap(),
                    },
                ),
                at(
                    0,
                    TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(13).unwrap(),
                    },
                ),
                g0(0),
            ])
        };
        let service = fixture.build();
        let composition = compose(&service);
        let before = (service.clone(), composition.clone());
        let closure =
            plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
        let placement =
            plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
                .unwrap();
        let error = build_teletext_augmented_origin_program(&placement, limits()).unwrap_err();
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: Some(local(0xffff)),
                command: if uncertain_glyph { 1 } else { 2 }
            })
        );
        assert_eq!(error.invocation.unwrap().get(), 0);
        if uncertain_glyph {
            assert!(matches!(
                error.reason,
                Failure::Placement { component: 0, .. }
            ));
        } else {
            let Failure::Carrier(error) = error.reason else {
                panic!("unknown attribute replaced by a fresh writer");
            };
            assert_eq!(
                error.reason,
                TeletextOriginCarrierFailure::Attribute {
                    field: "foreground"
                }
            );
        }
        assert_eq!((service, composition), before);
    }
}
#[test]
fn input_output_catalogue_and_aggregate_work_limits_preserve_input() {
    let service = repeated_fixture().build();
    let composition = compose(&service);
    let before = (service.clone(), composition.clone());
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    for kind in [
        "input commands",
        "output commands",
        "carriers",
        "catalog objects",
        "catalog commands",
        "work",
    ] {
        let mut bound = limits();
        match kind {
            "input commands" => {
                bound.max_input_commands = composition.program().commands().len() - 1;
            }
            "output commands" => {
                bound.max_output_commands = composition.program().commands().len() + 1;
            }
            "carriers" => bound.max_carriers = 1,
            "catalog objects" => bound.recipe.catalog.max_objects = 2,
            "catalog commands" => bound.recipe.catalog.max_commands = 12,
            _ => bound.max_work = 0,
        }
        let error = build_teletext_augmented_origin_program(&placement, bound).unwrap_err();
        if kind == "work" {
            assert!(matches!(error.reason, Failure::WorkBudget { maximum: 0 }));
        } else {
            assert!(matches!(error.reason,Failure::InputBudget{kind:actual,..}if actual==kind));
        }
    }
    let mut exact = limits();
    exact.max_input_commands = composition.program().commands().len();
    exact.max_output_commands = exact.max_input_commands + 2;
    exact.max_carriers = 2;
    exact.recipe.catalog.max_objects = 3;
    exact.recipe.catalog.max_commands = 13;
    assert_eq!(
        build_teletext_augmented_origin_program(&placement, exact)
            .unwrap()
            .program()
            .commands()
            .len(),
        exact.max_output_commands
    );
    assert_eq!((service, composition), before);
}
#[test]
fn sparse_default_root_and_resource_body_owner_remain_original_not_generated_authority() {
    let selected = public(0xffff);
    let mut fixture = Fixture::new(vec![text(1, 8, "£")], None);
    fixture.objects = vec![definition(
        selected,
        TeletextObjectType::Active,
        vec![position(1, 8), g0(8)],
    )];
    fixture.defaults[3] = Some(TeletextDefaultObjectSelection {
        target: selected,
        object_type: TeletextObjectType::Active,
    });
    let service = fixture.build();
    let composition = compose(&service);
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    let candidate = build_teletext_augmented_origin_program(&placement, limits()).unwrap();
    assert_eq!(
        candidate.carriers()[0].original_source(),
        Some(TeletextEnhancementSource {
            object: Some(selected),
            command: 1
        })
    );
    assert_eq!(
        candidate.carriers()[0].original_root_source(),
        Some(TeletextSpatialRootSource::DefaultSlot { slot: 3 })
    );
    assert!(candidate.main_sources().iter().any(|source| matches!(
        source,
        Source::RetainedMain {
            provenance: TeletextMainCompositionSource::DefaultSlot { slot: 3 },
            ..
        }
    )));
    assert_eq!(
        candidate.emitter_source(TeletextEnhancementSource {
            object: Some(selected),
            command: 1
        }),
        Some(Source::RetainedObject {
            identity: selected,
            command: 1
        })
    );
    assert_eq!(
        candidate.catalog().definitions()[0],
        service.objects().definitions()[0]
    );
    // No identity aliasing between public IDffff and the new exact-owner Local ID0.
    assert_eq!(candidate.carriers()[0].identity(), local(0));
    assert_eq!(
        candidate.emitter_source(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        }),
        None
    );
}

#[test]
fn exact_five_command_charset_independent_recipe_fits_without_conservative_six_charge() {
    let mut fixture = active_fixture();
    fixture.objects[0].program = program(vec![at(
        0,
        TeletextColumnOperation::G0Diacritic {
            code: code(0x2a),
            diacritic: TeletextDiacriticIndex::new(0).unwrap(),
        },
    )]);
    let service = fixture.build();
    let composition = compose(&service);
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    let mut exact = limits();
    exact.recipe.max_output_commands = 5;
    exact.recipe.catalog.max_commands = 6; // original one + actual five, not six.
    let candidate = build_teletext_augmented_origin_program(&placement, exact).unwrap();
    assert_eq!(
        candidate.carriers()[0].command_roles(),
        &[
            Role::Foreground,
            Role::Background,
            Role::Flash,
            Role::Display,
            Role::Glyph
        ]
    );
    let definition = candidate
        .catalog()
        .definition(candidate.carriers()[0].identity())
        .unwrap();
    assert_eq!(definition.program.commands().len(), 5);
    assert_eq!(
        definition.program.commands()[4],
        at(
            0,
            TeletextColumnOperation::G0Diacritic {
                code: code(0x2a),
                diacritic: TeletextDiacriticIndex::new(0).unwrap(),
            }
        )
    );
}

#[test]
fn later_uncertain_component_member_blames_its_own_source_and_instance() {
    let mut fixture = Fixture::new(
        vec![text(2, 8, "£")],
        Some(vec![
            position(1, 8),
            invoke(local(0xffff), TeletextObjectType::Active),
            position(1, 9),
            invoke(local(2), TeletextObjectType::Passive),
        ]),
    );
    fixture.objects = vec![
        definition(
            local(0xffff),
            TeletextObjectType::Active,
            vec![attributes(TeletextDisplaySize::DoubleSize), g0(0)],
        ),
        definition(
            local(2),
            TeletextObjectType::Passive,
            vec![
                charset(0, 0x47),
                at(0, TeletextColumnOperation::G0 { code: code(0x7e) }),
            ],
        ),
    ];
    let service = fixture.build();
    let composition = compose(&service);
    let before = (service.clone(), composition.clone());
    let closure =
        plan_teletext_base_compatibility_realization(&composition, closure_limits()).unwrap();
    assert_eq!(closure.seeds()[0].visual_position, (2, 8));
    let blocked = closure
        .origins()
        .iter()
        .find(|origin| {
            origin
                .candidate
                .source()
                .is_some_and(|source| source.object == Some(local(2)))
        })
        .unwrap()
        .origin_index;
    let placement =
        plan_teletext_origin_carrier_placement(&closure, service.objects(), placement_limits())
            .unwrap();
    let component = &placement.components()[0];
    assert_eq!(placement.components().len(), 1);
    assert_ne!(component.origin_indices()[0], blocked);
    assert_eq!(
        component.pending(),
        Some(
            crate::teletext::TeletextOriginCarrierPlacementPending::OriginalPlan {
                origin: blocked
            }
        )
    );
    let error = build_teletext_augmented_origin_program(&placement, limits()).unwrap_err();
    assert_eq!(error.origin, Some(blocked));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(local(2)),
            command: 1
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 3 })
    );
    assert_eq!(error.invocation.unwrap().get(), 1);
    assert!(
        matches!(error.reason, Failure::Placement { component: 0, pending: crate::teletext::TeletextOriginCarrierPlacementPending::OriginalPlan { origin } } if origin == blocked)
    );
    assert_eq!((service, composition), before);
}

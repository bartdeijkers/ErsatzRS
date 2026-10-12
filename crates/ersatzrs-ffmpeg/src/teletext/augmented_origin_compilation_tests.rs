//! Paired replay ancestry; these are not whole-display or receiver acceptance.
use super::*;
use crate::teletext::augmented_origin_compilation::compile_augmented_with_context_and_usage;
use crate::teletext::compile_context::TeletextCompileContext;
use crate::teletext::{
    allocate_teletext_resource_objects, build_teletext_augmented_origin_program,
    compile_teletext_augmented_origin_program, plan_teletext_base_compatibility_realization,
    plan_teletext_origin_carrier_placement, EncodedTeletextAugmentedOriginProgram,
    TeletextAugmentedInvocationBudget, TeletextAugmentedInvocationSource as Mapping,
    TeletextAugmentedOriginCompilationFailure as Failure, TeletextAugmentedOriginProgram,
    TeletextAugmentedOriginProgramBudget, TeletextBaseCompatibilityRealizationBudget,
    TeletextOriginCarrierBudget, TeletextOriginCarrierPlacementBudget,
};
pub(super) fn ancestry() -> TeletextAugmentedInvocationBudget {
    TeletextAugmentedInvocationBudget {
        max_original_invocations: 256,
        max_actual_invocations: 512,
        max_carriers: 64,
        max_work: 100_000,
    }
}
pub(super) fn objects() -> TeletextObjectCatalogBudget {
    TeletextObjectCatalogBudget {
        max_objects: 64,
        max_commands: 8192,
    }
}
pub(super) fn with_candidate(
    fixture: Fixture,
    changed: Option<Vec<TeletextEnhancementObjectDefinition>>,
    inspect: impl FnOnce(
        &TeletextSemanticService,
        &TeletextMainComposition,
        &TeletextAugmentedOriginProgram<'_>,
    ),
) {
    let service = fixture.build();
    let composition = compose(&service);
    let catalog = changed
        .map(|definitions| TeletextEnhancementObjectCatalog::new(definitions, objects()).unwrap());
    let closure = plan_teletext_base_compatibility_realization(
        &composition,
        TeletextBaseCompatibilityRealizationBudget {
            max_commands: 4096,
            max_candidates: 4096,
            max_origins: 8192,
            max_seeds: 64,
            max_edges: 4096,
            max_updates: 50_000,
        },
    )
    .unwrap();
    let placement = plan_teletext_origin_carrier_placement(
        &closure,
        catalog.as_ref().unwrap_or(service.objects()),
        TeletextOriginCarrierPlacementBudget {
            max_commands: 4096,
            max_origins: 8192,
            max_edges: 4096,
            catalog: objects(),
            spatial: TeletextSpatialTraceBudget {
                max_events: 8192,
                max_invocations: 512,
            },
            max_work: 100_000,
        },
    )
    .unwrap();
    let augmented = build_teletext_augmented_origin_program(
        &placement,
        TeletextAugmentedOriginProgramBudget {
            max_input_commands: 4096,
            max_output_commands: 4096,
            max_carriers: 64,
            max_identity_checks: 100_000,
            max_work: 100_000,
            recipe: TeletextOriginCarrierBudget {
                max_candidates: 4096,
                max_origins: 8192,
                catalog: objects(),
                max_id_checks: 65600,
                max_output_commands: 6,
            },
        },
    )
    .unwrap();
    inspect(&service, &composition, &augmented);
}
pub(super) fn compiled<'a>(
    service: &TeletextSemanticService,
    augmented: &'a TeletextAugmentedOriginProgram<'a>,
    limits: TeletextAugmentedInvocationBudget,
) -> Result<
    EncodedTeletextAugmentedOriginProgram<'a>,
    crate::teletext::TeletextAugmentedOriginCompilationError,
> {
    let allocations = allocate_teletext_resource_objects(augmented.catalog(), &[], 64).unwrap();
    compile_teletext_augmented_origin_program(
        service,
        augmented,
        &allocations,
        TeletextEnhancementCollectionBudget {
            max_commands: 8192,
            max_drcs_references: 0,
        },
        objects(),
        limits,
    )
}
pub(super) fn active() -> Fixture {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "£")],
        Some(vec![
            position(1, 8),
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
#[test]
fn paired_factory_maps_original_active_and_generated_passive_separately() {
    with_candidate(active(), None, |service, original, augmented| {
        let saved = (
            service.clone(),
            original.clone(),
            augmented.program().clone(),
            augmented.catalog().clone(),
        );
        let result = compiled(service, augmented, ancestry()).unwrap();
        assert!(std::ptr::eq(result.augmented(), augmented));
        let old = original.spatial_trace().invocations()[0];
        let calls = result.physical().replay().spatial_trace().invocations();
        assert_eq!(calls.len(), 2);
        assert_eq!(result.actual_for_original(old.id()).unwrap().get(), 0);
        assert_eq!(result.actual_for_carrier(0).unwrap().get(), 1);
        assert_eq!(
            result.invocations()[0].source(),
            Mapping::Retained { original: old.id() }
        );
        assert_eq!(
            result.invocations()[1].source(),
            Mapping::Carrier { carrier: 0 }
        );
        assert_eq!(calls[0].target(), local(0xffff));
        assert_eq!(calls[0].offset(), None);
        assert_eq!(calls[1].target(), local(0));
        assert_eq!(
            calls[1].offset(),
            Some(TeletextObjectOffset::new(0, 0).unwrap())
        );
        assert_eq!(
            (calls[1].origin().row(), calls[1].origin().column()),
            (1, 8)
        );
        assert_eq!(
            augmented.carriers()[0].original_invocation(),
            Some(old.id())
        );
        assert_eq!(result.actual_for_carrier(1), None);
        assert_eq!(
            (
                service.clone(),
                original.clone(),
                augmented.program().clone(),
                augmented.catalog().clone()
            ),
            saved
        );
        assert_eq!(result.into_physical().replay().owner(), owner());
    });
}
#[test]
fn repeated_nested_calls_map_by_parent_and_callsite_not_numeric_shift() {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "£"), text(2, 8, "£")],
        Some(vec![
            position(1, 8),
            invoke(local(0xffff), TeletextObjectType::Active),
            position(2, 8),
            invoke(local(0xffff), TeletextObjectType::Active),
        ]),
    );
    let nested = |target, object_type| TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: Some(TeletextObjectOffset::new(0, 2).unwrap()),
    };
    fixture.objects = vec![
        definition(
            local(0xffff),
            TeletextObjectType::Active,
            vec![g0(0), nested(local(0x100), TeletextObjectType::Adaptive)],
        ),
        definition(
            local(0x100),
            TeletextObjectType::Adaptive,
            vec![g0(0), nested(local(2), TeletextObjectType::Passive)],
        ),
        definition(local(2), TeletextObjectType::Passive, vec![g0(0)]),
    ];
    with_candidate(fixture, None, |service, original, augmented| {
        let result = compiled(service, augmented, ancestry()).unwrap();
        let old = original.spatial_trace().invocations();
        let actual = result.physical().replay().spatial_trace().invocations();
        assert_eq!(old.len(), 6);
        assert_eq!(actual.len(), 8);
        let mapped: Vec<_> = old
            .iter()
            .map(|call| result.actual_for_original(call.id()).unwrap().get())
            .collect();
        assert_eq!(mapped, vec![0, 1, 2, 4, 5, 6]);
        assert_eq!(result.actual_for_carrier(0).unwrap().get(), 3);
        assert_eq!(result.actual_for_carrier(1).unwrap().get(), 7);
        for (index, &call) in old.iter().enumerate() {
            let value = actual[mapped[index]];
            assert_eq!(value.target(), call.target());
            assert_eq!(value.origin(), call.origin());
            assert_eq!(value.caller_position(), call.caller_position());
            assert_eq!(value.offset(), call.offset());
            assert_eq!(
                value.parent(),
                call.parent()
                    .map(|parent| result.actual_for_original(parent).unwrap())
            );
        }
        assert_eq!(actual[1].source(), actual[5].source());
        assert_ne!(actual[1].parent(), actual[5].parent());
    });
}
#[test]
fn sparse_default_root_maps_slot_not_dense_or_actual_main_ordinal() {
    let mut fixture = Fixture::new(vec![text(1, 8, "£")], None);
    fixture.objects.push(definition(
        public(0xffff),
        TeletextObjectType::Active,
        vec![position(1, 8), g0(8)],
    ));
    fixture.defaults[3] = Some(TeletextDefaultObjectSelection {
        target: public(0xffff),
        object_type: TeletextObjectType::Active,
    });
    with_candidate(fixture, None, |service, original, augmented| {
        let result = compiled(service, augmented, ancestry()).unwrap();
        let old = original.spatial_trace().invocations()[0];
        assert_eq!(
            old.source(),
            TeletextEnhancementSource {
                object: None,
                command: 0
            }
        );
        assert_eq!(
            old.root_source(),
            TeletextSpatialRootSource::DefaultSlot { slot: 3 }
        );
        let actual_id = result.actual_for_original(old.id()).unwrap();
        let actual = result.physical().replay().spatial_trace().invocations()[actual_id.get()];
        assert_eq!(actual.target(), public(0xffff));
        assert_eq!(actual.offset(), None);
        assert_eq!(
            actual.root_source(),
            TeletextSpatialRootSource::Local {
                command: actual.source().command
            }
        );
        assert_eq!(actual.origin(), old.origin());
        assert_eq!(
            augmented.carriers()[0].original_root_source(),
            Some(TeletextSpatialRootSource::DefaultSlot { slot: 3 })
        );
    });
}
#[test]
fn explicit_zero_offset_and_absent_offset_remain_distinct_retained_facts() {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "B")],
        Some(vec![
            position(1, 8),
            invoke(local(0xffff), TeletextObjectType::Passive),
            position(2, 8),
            TeletextEnhancementCommand::InvokeObject {
                target: local(0xffff),
                object_type: TeletextObjectType::Passive,
                offset: Some(TeletextObjectOffset::new(0, 0).unwrap()),
            },
        ]),
    );
    fixture.objects.push(definition(
        local(0xffff),
        TeletextObjectType::Passive,
        vec![g0(0)],
    ));
    with_candidate(fixture, None, |service, original, augmented| {
        assert!(augmented.carriers().is_empty());
        let result = compiled(service, augmented, ancestry()).unwrap();
        for call in original.spatial_trace().invocations() {
            let id = result.actual_for_original(call.id()).unwrap();
            assert_eq!(
                result.physical().replay().spatial_trace().invocations()[id.get()].offset(),
                call.offset()
            );
        }
        let actual = result.physical().replay().spatial_trace().invocations();
        assert_eq!(actual[0].offset(), None);
        assert_eq!(
            actual[1].offset(),
            Some(TeletextObjectOffset::new(0, 0).unwrap())
        );
    });
}
#[test]
fn empty_local_and_absent_programs_have_empty_maps_without_readiness_claim() {
    for main in [None, Some(vec![])] {
        with_candidate(
            Fixture::new(vec![text(1, 8, "B")], main),
            None,
            |service, original, augmented| {
                let result = compiled(
                    service,
                    augmented,
                    TeletextAugmentedInvocationBudget {
                        max_original_invocations: 0,
                        max_actual_invocations: 0,
                        max_carriers: 0,
                        max_work: 0,
                    },
                )
                .unwrap();
                assert!(result.invocations().is_empty());
                assert!(result
                    .augmented()
                    .placement()
                    .plan()
                    .composition()
                    .spatial_trace()
                    .invocations()
                    .is_empty());
                assert_eq!(
                    result.augmented().placement().plan().composition(),
                    original
                );
            },
        );
    }
}
#[test]
fn no_seed_keeps_original_pending_distinct_from_actual_packed_projection() {
    let display = |double_width| {
        at(
            8,
            TeletextColumnOperation::Attributes {
                double_height: false,
                double_width,
                boxed: false,
                conceal: false,
                invert: false,
                underline_or_separated_mosaics: false,
            },
        )
    };
    let fixture = Fixture::new(
        vec![text(1, 8, "B")],
        Some(vec![position(1, 8), display(false), display(true), g0(8)]),
    );
    with_candidate(fixture, None, |service, original, augmented| {
        assert!(augmented.carriers().is_empty());
        assert!(!original.display_resolution().pending().is_empty());
        let pending = original.display_resolution().pending().to_vec();
        let result = compiled(service, augmented, ancestry()).unwrap();
        assert!(result.invocations().is_empty());
        assert_eq!(
            result
                .augmented()
                .placement()
                .plan()
                .composition()
                .display_resolution()
                .pending(),
            pending
        );
        // Actual packed C2 is separately resolved; it does not rewrite the target.
        assert_eq!(
            result
                .physical()
                .replay()
                .spatial_trace()
                .invocations()
                .len(),
            0
        );
    });
}
#[test]
fn altered_nested_geometry_with_same_ids_and_ordinals_is_rejected() {
    let mut fixture = active();
    fixture.objects[0].program = program(vec![
        g0(0),
        position(1, 0),
        invoke(local(0x100), TeletextObjectType::Adaptive),
    ]);
    fixture.objects.push(definition(
        local(0x100),
        TeletextObjectType::Adaptive,
        vec![g0(0)],
    ));
    let mut changed = fixture.objects.clone();
    changed[0].program = program(vec![
        g0(0),
        position(2, 0),
        invoke(local(0x100), TeletextObjectType::Adaptive),
    ]);
    with_candidate(fixture, Some(changed), |service, original, augmented| {
        let retained = (
            service.clone(),
            original.clone(),
            augmented.program().clone(),
            augmented.catalog().clone(),
        );
        let ordinary = compiled(service, augmented, ancestry()).unwrap_err();
        let (result, admitted) = compiled_with_usage(service, augmented, ancestry());
        let error = result.unwrap_err();
        assert!(admitted > 0);
        assert_eq!(error.to_string(), ordinary.to_string());
        assert!(matches!(error.reason, Failure::Correspondence));
        assert_eq!(error.actual.unwrap().get(), 1);
        assert_eq!(error.original.unwrap().get(), 1);
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: Some(local(0xffff)),
                command: 2
            })
        );
        assert_eq!(
            error.root_source,
            Some(TeletextSpatialRootSource::Local { command: 1 })
        );
        assert_eq!(
            (
                service.clone(),
                original.clone(),
                augmented.program().clone(),
                augmented.catalog().clone()
            ),
            retained
        );
    });
}
#[test]
fn raw_invocation_carrier_and_work_budgets_guard_pairing_without_mutation() {
    with_candidate(active(), None, |service, original, augmented| {
        let retained = (
            service.clone(),
            original.clone(),
            augmented.program().clone(),
            augmented.catalog().clone(),
        );
        for (kind, count, maximum, budget) in [
            (
                "original invocations",
                1,
                0,
                TeletextAugmentedInvocationBudget {
                    max_original_invocations: 0,
                    ..ancestry()
                },
            ),
            (
                "carriers",
                1,
                0,
                TeletextAugmentedInvocationBudget {
                    max_carriers: 0,
                    ..ancestry()
                },
            ),
            (
                "actual invocations",
                2,
                1,
                TeletextAugmentedInvocationBudget {
                    max_actual_invocations: 1,
                    ..ancestry()
                },
            ),
        ] {
            let error = compiled(service, augmented, budget).unwrap_err();
            assert!(
                matches!(error.reason, Failure::InputBudget {kind:k,count:c,maximum:m} if (k,c,m)==(kind,count,maximum))
            );
        }
        let error = compiled(
            service,
            augmented,
            TeletextAugmentedInvocationBudget {
                max_work: 0,
                ..ancestry()
            },
        )
        .unwrap_err();
        assert!(matches!(error.reason, Failure::WorkBudget { maximum: 0 }));
        let exact = TeletextAugmentedInvocationBudget {
            max_original_invocations: 1,
            max_actual_invocations: 2,
            max_carriers: 1,
            ..ancestry()
        };
        assert_eq!(
            compiled(service, augmented, exact)
                .unwrap()
                .invocations()
                .len(),
            2
        );
        assert_eq!(
            (
                service.clone(),
                original.clone(),
                augmented.program().clone(),
                augmented.catalog().clone()
            ),
            retained
        );
    });
}

fn compiled_with_usage<'a>(
    service: &TeletextSemanticService,
    augmented: &'a TeletextAugmentedOriginProgram<'a>,
    limits: TeletextAugmentedInvocationBudget,
) -> (
    Result<
        EncodedTeletextAugmentedOriginProgram<'a>,
        crate::teletext::TeletextAugmentedOriginCompilationError,
    >,
    usize,
) {
    let allocations = allocate_teletext_resource_objects(augmented.catalog(), &[], 64).unwrap();
    compile_augmented_with_context_and_usage(
        service,
        augmented,
        &allocations,
        TeletextEnhancementCollectionBudget {
            max_commands: 8192,
            max_drcs_references: 0,
        },
        objects(),
        limits,
        &mut TeletextCompileContext::new(None, None, None),
    )
}

#[test]
fn ancestry_usage_exact_credit_preserves_wire_maps_and_rejected_charge() {
    with_candidate(active(), None, |service, _, augmented| {
        let ordinary = compiled(service, augmented, ancestry()).unwrap();
        let (result, admitted) = compiled_with_usage(service, augmented, ancestry());
        let metered = result.unwrap();
        // Two actual calls: visit+lookup for each, one original comparison,
        // then one original and one carrier coverage visit.
        assert_eq!(admitted, 11);
        assert_eq!(metered.invocations(), ordinary.invocations());
        assert_eq!(
            metered.physical().encoded().packet_bodies(),
            ordinary.physical().encoded().packet_bodies()
        );
        assert_eq!(metered.physical().replay(), ordinary.physical().replay());
        let exact = TeletextAugmentedInvocationBudget {
            max_work: admitted,
            ..ancestry()
        };
        let (result, exact_used) = compiled_with_usage(service, augmented, exact);
        assert_eq!(result.unwrap().invocations(), ordinary.invocations());
        assert_eq!(exact_used, admitted);
        let short = TeletextAugmentedInvocationBudget {
            max_work: admitted - 1,
            ..ancestry()
        };
        let (result, failed_used) = compiled_with_usage(service, augmented, short);
        let error = result.unwrap_err();
        assert!(matches!(error.reason, Failure::WorkBudget { maximum: 10 }));
        assert_eq!(
            failed_used, 10,
            "rejected final coverage charge is not admitted"
        );
        assert_eq!(
            error.to_string(),
            compiled(service, augmented, short).unwrap_err().to_string()
        );
    });
}

#[test]
fn ancestry_usage_raw_preflight_minimum_and_physical_failure_admit_zero() {
    with_candidate(active(), None, |service, _, augmented| {
        for limits in [
            TeletextAugmentedInvocationBudget {
                max_original_invocations: 0,
                ..ancestry()
            },
            TeletextAugmentedInvocationBudget {
                max_carriers: 0,
                ..ancestry()
            },
            TeletextAugmentedInvocationBudget {
                max_actual_invocations: 0,
                ..ancestry()
            },
            TeletextAugmentedInvocationBudget {
                max_work: 0,
                ..ancestry()
            },
        ] {
            let (result, admitted) = compiled_with_usage(service, augmented, limits);
            let error = result.unwrap_err();
            assert_eq!(admitted, 0);
            assert_eq!(
                error.to_string(),
                compiled(service, augmented, limits)
                    .unwrap_err()
                    .to_string()
            );
        }
        let allocations = allocate_teletext_resource_objects(augmented.catalog(), &[], 64).unwrap();
        let (result, admitted) = compile_augmented_with_context_and_usage(
            service,
            augmented,
            &allocations,
            TeletextEnhancementCollectionBudget {
                max_commands: 0,
                max_drcs_references: 0,
            },
            objects(),
            ancestry(),
            &mut TeletextCompileContext::new(None, None, None),
        );
        assert!(matches!(result.unwrap_err().reason, Failure::Physical(_)));
        assert_eq!(admitted, 0);
    });
}

#[test]
fn empty_ancestry_usage_is_zero_under_zero_credit() {
    with_candidate(
        Fixture::new(vec![], Some(vec![])),
        None,
        |service, _, augmented| {
            let (result, admitted) = compiled_with_usage(
                service,
                augmented,
                TeletextAugmentedInvocationBudget {
                    max_work: 0,
                    ..ancestry()
                },
            );
            assert!(result.unwrap().invocations().is_empty());
            assert_eq!(admitted, 0);
        },
    );
}

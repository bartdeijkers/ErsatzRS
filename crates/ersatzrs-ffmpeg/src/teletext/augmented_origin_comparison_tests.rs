//! Typed provenance/identity oracles, independent of selected font-mask equality.
use super::augmented_origin_compilation_tests::{
    active, ancestry, compiled, objects, with_candidate,
};
use super::*;
use crate::teletext::augmented_origin_comparison::{
    compare_teletext_augmented_origin_glyphs, compare_teletext_augmented_origin_glyphs_with_usage,
    TeletextAugmentedGlyphComparisonBudget, TeletextAugmentedGlyphComparisonFailure as Failure,
    TeletextAugmentedGlyphFinding as Finding,
};
use crate::teletext::{
    allocate_teletext_resource_objects, build_teletext_augmented_origin_program,
    compile_teletext_augmented_origin_program, plan_teletext_base_compatibility_realization,
    plan_teletext_origin_carrier_placement, EncodedTeletextAugmentedOriginProgram,
    TeletextAugmentedOriginProgramBudget, TeletextBaseCompatibilityRealizationBudget,
    TeletextOriginCarrierBudget, TeletextOriginCarrierPlacementBudget,
};
fn limits() -> TeletextAugmentedGlyphComparisonBudget {
    TeletextAugmentedGlyphComparisonBudget {
        max_records: 65536,
        max_cells: 1800,
        max_diagnostics: 4096,
        max_work: 1_000_000,
    }
}

#[test]
fn glyph_usage_exact_boundary_and_denied_charge_preserve_models_and_errors() {
    with_candidate(active(), None, |service, original, augmented| {
        let compiled = compiled(service, augmented, ancestry()).unwrap();
        let retained = (
            service.clone(),
            original.clone(),
            compiled.physical().clone(),
        );
        let expected = compare_teletext_augmented_origin_glyphs(&compiled, limits()).unwrap();
        let (result, admitted) =
            compare_teletext_augmented_origin_glyphs_with_usage(&compiled, limits());
        let actual = result.unwrap();
        assert!(admitted > 1000);
        assert_eq!(actual.compared_cells(), expected.compared_cells());
        assert_eq!(actual.diagnostics(), expected.diagnostics());
        assert_eq!(actual.original_pending(), expected.original_pending());
        assert_eq!(actual.actual_pending(), expected.actual_pending());
        assert!(std::ptr::eq(actual.compiled(), expected.compiled()));
        let mut exact = limits();
        exact.max_work = admitted;
        let (result, repeated) =
            compare_teletext_augmented_origin_glyphs_with_usage(&compiled, exact);
        let result = result.unwrap();
        assert_eq!(repeated, admitted);
        assert_eq!(result.compared_cells(), expected.compared_cells());
        assert_eq!(result.diagnostics(), expected.diagnostics());
        exact.max_work -= 1;
        let (result, partial) =
            compare_teletext_augmented_origin_glyphs_with_usage(&compiled, exact);
        let failure = result.unwrap_err();
        assert_eq!(
            failure,
            compare_teletext_augmented_origin_glyphs(&compiled, exact).unwrap_err()
        );
        let Failure::Budget {
            kind,
            count,
            maximum,
        } = failure.reason
        else {
            panic!("one fewer admitted-work credit must reject comparison");
        };
        assert_eq!(kind, "work");
        assert_eq!(maximum, exact.max_work);
        assert!(count > maximum);
        assert!(partial > 0 && partial <= maximum && partial < count);
        assert!(failure.position.is_some());
        assert_eq!(service, &retained.0);
        assert_eq!(original, &retained.1);
        assert_eq!(compiled.physical(), &retained.2);
    });
}

#[test]
fn glyph_usage_preflight_rejections_report_zero_and_original_errors() {
    with_candidate(active(), None, |service, _, augmented| {
        let compiled = compiled(service, augmented, ancestry()).unwrap();
        for field in ["records", "cells", "work"] {
            let mut budget = limits();
            match field {
                "records" => budget.max_records = 0,
                "cells" => budget.max_cells = 999,
                _ => budget.max_work = 0,
            }
            let (result, admitted) =
                compare_teletext_augmented_origin_glyphs_with_usage(&compiled, budget);
            let error = result.unwrap_err();
            assert_eq!(admitted, 0);
            assert_eq!(
                error,
                compare_teletext_augmented_origin_glyphs(&compiled, budget).unwrap_err()
            );
            assert!(matches!(error.reason, Failure::Budget {kind,..} if kind == field));
            assert_eq!(error.position, None);
        }
    });
}
fn designate(value: u8) -> TeletextEnhancementCommand {
    at(
        0,
        TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(value).unwrap(),
        },
    )
}
#[test]
fn unicode_replacements_and_active_carrier_match_exact_typed_origin() {
    for value in ["£", "é", "@"] {
        with_candidate(
            Fixture::new(vec![text(1, 8, value)], Some(vec![])),
            None,
            |service, original, augmented| {
                let result = compiled(service, augmented, ancestry()).unwrap();
                let comparison =
                    compare_teletext_augmented_origin_glyphs(&result, limits()).unwrap();
                assert_eq!(comparison.compared_cells(), 1000);
                assert!(
                    comparison.diagnostics().is_empty(),
                    "{:?}",
                    comparison.diagnostics()
                );
                assert!(std::ptr::eq(
                    comparison.compiled(),
                    std::ptr::from_ref(&result)
                ));
                assert_eq!(
                    comparison.original_pending(),
                    original.display_resolution().pending()
                );
            },
        );
    }
    with_candidate(active(), None, |service, original, augmented| {
        let result = compiled(service, augmented, ancestry()).unwrap();
        assert_eq!(augmented.carriers().len(), 1);
        let comparison = compare_teletext_augmented_origin_glyphs(&result, limits()).unwrap();
        assert!(
            comparison.diagnostics().is_empty(),
            "{:?}",
            comparison.diagnostics()
        );
        let old = original
            .display_resolution()
            .cell(1, 8)
            .unwrap()
            .origin()
            .unwrap();
        assert_eq!(augmented.carriers()[0].original_origin_index(), old);
    });
}
#[test]
fn retained_national_base_keeps_literal_identity_without_a_carrier() {
    for text_value in ["§", "$", "#", "Ä"] {
        with_candidate(
            Fixture::new(vec![text(1, 8, text_value)], Some(vec![])),
            None,
            |service, original, augmented| {
                assert!(original.base_replacement(1, 8).is_none());
                assert!(augmented.carriers().is_empty());
                let result = compiled(service, augmented, ancestry()).unwrap();
                let comparison =
                    compare_teletext_augmented_origin_glyphs(&result, limits()).unwrap();
                assert_eq!(comparison.compared_cells(), 1000);
                assert!(
                    comparison.diagnostics().is_empty(),
                    "{:?}",
                    comparison.diagnostics()
                );
                for (display, intent) in [
                    (original.display_resolution(), original.glyph_intent()),
                    (
                        result.physical().replay().display_resolution(),
                        result.physical().replay().glyph_intent(),
                    ),
                ] {
                    let origin = display.cell(1, 8).unwrap().origin().unwrap();
                    let record = intent.records()[display.origins()[origin].candidate_index()];
                    assert!(
                        matches!(record.glyph(), Some(TeletextGlyphIdentity::G0 { glyph })
                        if glyph.national_subset() == Some(TeletextNationalSubset::German))
                    );
                    assert!(record.context().candidate().base().is_some());
                }
            },
        );
    }
}

#[test]
fn same_mask_latin_and_cyrillic_is_a_typed_identity_failure() {
    let target = local(0xffff);
    let mut fixture = Fixture::new(
        vec![],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Active,
        vec![designate(0), g0(0)],
    ));
    let changed = vec![definition(
        target,
        TeletextObjectType::Active,
        vec![designate(0x20), g0(0)],
    )];
    with_candidate(fixture, Some(changed), |service, original, augmented| {
        let result = compiled(service, augmented, ancestry()).unwrap();
        let comparison = compare_teletext_augmented_origin_glyphs(&result, limits()).unwrap();
        let (measured, admitted) =
            compare_teletext_augmented_origin_glyphs_with_usage(&result, limits());
        let measured = measured.unwrap();
        assert!(admitted > 0);
        assert_eq!(measured.diagnostics(), comparison.diagnostics());
        assert_eq!(measured.compared_cells(), comparison.compared_cells());
        let finding = comparison
            .diagnostics()
            .iter()
            .find(|d| matches!(d.finding, Finding::Identity { .. }))
            .unwrap();
        assert_eq!(finding.position, (1, 8));
        assert_eq!(
            finding.original.location,
            Some(TeletextEnhancementSource {
                object: Some(target),
                command: 1
            })
        );
        assert_eq!(finding.actual.location, finding.original.location);
        let (
            Some(TeletextGlyphIdentity::G0 { glyph: a }),
            Some(TeletextGlyphIdentity::G0 { glyph: b }),
        ) = (match finding.finding {
            Finding::Identity { original, actual } => (original, actual),
            _ => unreachable!(),
        })
        else {
            panic!("expected exact G0 identities")
        };
        assert_eq!(a.repertoire(), TeletextG0Repertoire::Latin);
        assert_eq!(b.repertoire(), TeletextG0Repertoire::Cyrillic1);
        assert_eq!((a.code(), b.code()), (0x41, 0x41));
        assert_eq!(
            resolve_teletext_glyph_mask(TeletextGlyphIdentity::G0 { glyph: a }, None, false)
                .unwrap(),
            resolve_teletext_glyph_mask(TeletextGlyphIdentity::G0 { glyph: b }, None, false)
                .unwrap()
        );
        assert_eq!(original.spatial_trace().invocations().len(), 1);
    });
}
#[test]
fn altered_current_g2_is_not_authorized_by_identical_raw_operation() {
    let target = local(0xffff);
    let g2 = at(
        0,
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(0x61).unwrap(),
        },
    );
    let mut fixture = Fixture::new(
        vec![],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Active,
        vec![designate(0), g2],
    ));
    let changed = vec![definition(
        target,
        TeletextObjectType::Active,
        vec![designate(0x37), g2],
    )];
    with_candidate(fixture, Some(changed), |service, _, augmented| {
        let result = compiled(service, augmented, ancestry()).unwrap();
        let comparison = compare_teletext_augmented_origin_glyphs(&result, limits()).unwrap();
        assert!(comparison.diagnostics().iter().any(|d|matches!(d.finding,Finding::Identity {original:Some(TeletextGlyphIdentity::G2 {glyph:a}),actual:Some(TeletextGlyphIdentity::G2 {glyph:b})} if a.repertoire()==TeletextG2Repertoire::Latin && b.repertoire()==TeletextG2Repertoire::Greek && a.code()==b.code())));
    });
}
#[test]
fn repeated_nested_instances_and_sparse_defaults_use_paired_namespaces() {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "£"), text(2, 8, "é")],
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
        let mapped: Vec<_> = original
            .spatial_trace()
            .invocations()
            .iter()
            .map(|i| result.actual_for_original(i.id()).unwrap().get())
            .collect();
        assert_eq!(mapped, vec![0, 1, 2, 4, 5, 6]);
        let comparison = compare_teletext_augmented_origin_glyphs(&result, limits()).unwrap();
        assert!(
            comparison.diagnostics().is_empty(),
            "{:?}",
            comparison.diagnostics()
        );
    });
    let mut defaults = Fixture::new(vec![text(1, 8, "£")], None);
    defaults.objects.push(definition(
        public(0xffff),
        TeletextObjectType::Active,
        vec![position(1, 8), g0(8)],
    ));
    defaults.defaults[3] = Some(TeletextDefaultObjectSelection {
        target: public(0xffff),
        object_type: TeletextObjectType::Active,
    });
    with_candidate(defaults, None, |service, original, augmented| {
        let result = compiled(service, augmented, ancestry()).unwrap();
        assert_eq!(
            original.spatial_trace().invocations()[0].root_source(),
            TeletextSpatialRootSource::DefaultSlot { slot: 3 }
        );
        let comparison = compare_teletext_augmented_origin_glyphs(&result, limits()).unwrap();
        assert!(
            comparison.diagnostics().is_empty(),
            "{:?}",
            comparison.diagnostics()
        );
    });
}
#[test]
fn exact_budget_failures_do_not_mutate_the_private_assembly() {
    with_candidate(active(), None, |service, original, augmented| {
        let result = compiled(service, augmented, ancestry()).unwrap();
        let saved = (
            original.clone(),
            augmented.program().clone(),
            augmented.catalog().clone(),
        );
        for kind in ["records", "cells", "work"] {
            let mut budget = limits();
            match kind {
                "records" => budget.max_records = 0,
                "cells" => budget.max_cells = 999,
                _ => budget.max_work = 0,
            }
            let error = compare_teletext_augmented_origin_glyphs(&result, budget).unwrap_err();
            assert!(matches!(error.reason,Failure::Budget {kind:k,..} if k==kind));
            assert_eq!(error.position, None);
        }
        assert_eq!(
            (
                original.clone(),
                augmented.program().clone(),
                augmented.catalog().clone()
            ),
            saved
        );
    });
}

fn inspect_drcs(inspect: impl FnOnce(&EncodedTeletextAugmentedOriginProgram<'_>)) {
    let mut fixture = drcs_fixture([1; 10]);
    fixture.rows = vec![];
    fixture.main = Some(vec![
        position(1, 8),
        invoke(local(0xffff), TeletextObjectType::Passive),
    ]);
    // Each independently packed definition needs an explicit non-header row
    // before its DRCS selector. Keep both bodies' glyph source at ordinal 1.
    let global_glyph = fixture.objects[0].program.commands()[1];
    fixture.objects[0] = definition(
        local(0xffff),
        TeletextObjectType::Passive,
        vec![position(1, 0), global_glyph],
    );
    let service = fixture.build();
    let original = compose_drcs(&service);
    let normal = TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Normal,
        subtable: TeletextDrcsSubtableIndex::new(15).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
    };
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![definition(
            local(0xffff),
            TeletextObjectType::Passive,
            vec![
                position(1, 0),
                at(0, TeletextColumnOperation::Drcs { reference: normal }),
            ],
        )],
        objects(),
    )
    .unwrap();
    let plan = plan_teletext_base_compatibility_realization(
        &original,
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
        &plan,
        &catalog,
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
    let allocations = allocate_teletext_resource_objects(augmented.catalog(), &[], 64).unwrap();
    let result = compile_teletext_augmented_origin_program(
        &service,
        &augmented,
        &allocations,
        TeletextEnhancementCollectionBudget {
            max_commands: 8192,
            max_drcs_references: 4,
        },
        objects(),
        ancestry(),
    )
    .unwrap();
    inspect(&result);
}
#[test]
fn identical_drcs_pixels_do_not_equate_scope_bank_slot_or_resource() {
    inspect_drcs(|result| {
        let original = result.augmented().placement().plan().composition();
        let actual = result.physical().replay();
        let old = &original.drcs_execution().records()[0];
        let new = &actual.drcs_execution().records()[0];
        assert_eq!(old.candidate().position(), (2, 8));
        assert_eq!(new.candidate().position(), (2, 8));
        assert_eq!(old.candidate().source(), new.candidate().source());
        assert_eq!(old.candidate().source().unwrap().command, 1);
        assert_eq!(old.bitmap(), new.bitmap());
        assert_ne!(old.reference(), new.reference());
        assert_ne!(old.resource(), new.resource());
        let comparison = compare_teletext_augmented_origin_glyphs(result, limits()).unwrap();
        assert!(comparison.diagnostics().iter().any(|d|d.position==(2,8) && matches!(d.finding,Finding::Identity {original:Some(TeletextGlyphIdentity::Drcs {reference:a}),actual:Some(TeletextGlyphIdentity::Drcs {reference:b})} if a.table!=b.table && a.subtable!=b.subtable && a.slot!=b.slot)));
    });
}
#[test]
fn located_diagnostic_budget_and_pending_histories_are_retained() {
    let target = local(0xffff);
    let mut fixture = Fixture::new(
        vec![],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Active,
        vec![designate(0), g0(0)],
    ));
    let changed = vec![definition(
        target,
        TeletextObjectType::Active,
        vec![designate(0x20), g0(0)],
    )];
    with_candidate(fixture, Some(changed), |service, _, augmented| {
        let result = compiled(service, augmented, ancestry()).unwrap();
        let mut budget = limits();
        budget.max_diagnostics = 0;
        let error = compare_teletext_augmented_origin_glyphs(&result, budget).unwrap_err();
        let (measured, admitted) =
            compare_teletext_augmented_origin_glyphs_with_usage(&result, budget);
        assert_eq!(measured.unwrap_err(), error);
        assert!(admitted > 0 && admitted <= budget.max_work);
        assert_eq!(error.position, Some((1, 8)));
        assert!(matches!(
            error.reason,
            Failure::Budget {
                kind: "diagnostics",
                count: 1,
                maximum: 0
            }
        ));
    });
    let foreground = |value| {
        at(
            8,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(value).unwrap(),
            },
        )
    };
    with_candidate(
        Fixture::new(
            vec![],
            Some(vec![position(1, 8), foreground(4), foreground(13), g0(8)]),
        ),
        None,
        |service, original, augmented| {
            let result = compiled(service, augmented, ancestry()).unwrap();
            let comparison = compare_teletext_augmented_origin_glyphs(&result, limits()).unwrap();
            assert!(!original.display_resolution().pending().is_empty());
            assert_eq!(
                comparison.original_pending(),
                original.display_resolution().pending()
            );
            assert_eq!(
                comparison.actual_pending(),
                result.physical().replay().display_resolution().pending()
            );
            // Typed glyph identity does not erase pending attribute evidence or assert full equivalence.
            assert!(comparison.original_pending().len() >= comparison.actual_pending().len());
        },
    );
}

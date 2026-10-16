//! Owned engine candidate checks, separate from native and whole-service acceptance.
use super::*;
use crate::teletext::{
    compile_teletext_normalized_main_program, encode_teletext_page,
    EncodedTeletextNormalizedMainProgram, TeletextAugmentedGlyphComparisonBudget,
    TeletextAugmentedInvocationBudget, TeletextAugmentedOriginProgramBudget,
    TeletextBaseCompatibilityRealizationBudget, TeletextEncodingRegion,
    TeletextNormalizedMainProgramBudget, TeletextNormalizedMainProgramFailure as Failure,
    TeletextOriginCarrierBudget, TeletextOriginCarrierPlacementBudget,
    TeletextPhysicalComparisonBudget, TeletextPhysicalComparisonFinding,
};
use std::time::{Duration, Instant};

#[test]
fn expired_normalization_deadline_rejects_before_allocation_without_mutating_service() {
    let service = active().build();
    let before = service.clone();
    let mut budget = limits();
    budget.max_resource_objects = 0;
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let error = crate::teletext::normalized_main_program::compile_normalized_with_resources_before(
        &service,
        owner(),
        &[],
        &budget,
        None,
        Some(expired),
    )
    .unwrap_err();
    assert_eq!(error.owner, owner());
    assert!(matches!(error.reason, Failure::TimedOut));
    assert_eq!(service, before);
}

#[test]
fn future_normalization_deadline_preserves_original_and_physical_candidate() {
    let service = active().build();
    let before = service.clone();
    let expected = compile_teletext_normalized_main_program(&service, owner(), limits()).unwrap();
    let deadline = Instant::now() + Duration::from_mins(1);
    let actual =
        crate::teletext::normalized_main_program::compile_normalized_with_resources_before(
            &service,
            owner(),
            &[],
            &limits(),
            None,
            Some(deadline),
        )
        .unwrap();
    assert_eq!(actual.original(), expected.original());
    assert_eq!(actual.physical(), expected.physical());
    assert_eq!(actual.encoded(), expected.encoded());
    assert_eq!(service, before);
}

#[test]
fn future_normalization_deadline_retains_existing_allocation_failure() {
    let service = active().build();
    let mut budget = limits();
    budget.max_resource_objects = 0;
    let deadline = Instant::now() + Duration::from_mins(1);
    let error = crate::teletext::normalized_main_program::compile_normalized_with_resources_before(
        &service,
        owner(),
        &[],
        &budget,
        None,
        Some(deadline),
    )
    .unwrap_err();
    assert_eq!(error.owner, owner());
    assert!(matches!(error.reason, Failure::Allocation(_)));
}

fn limits() -> TeletextNormalizedMainProgramBudget {
    let packing = TeletextObjectCatalogBudget {
        max_objects: 64,
        max_commands: 8192,
    };
    let replay = TeletextEnhancementCollectionBudget {
        max_commands: 8192,
        max_drcs_references: 0,
    };
    TeletextNormalizedMainProgramBudget {
        composition: budget(),
        realization: TeletextBaseCompatibilityRealizationBudget {
            max_commands: 4096,
            max_candidates: 4096,
            max_origins: 8192,
            max_seeds: 64,
            max_edges: 4096,
            max_updates: 100_000,
        },
        placement: TeletextOriginCarrierPlacementBudget {
            max_commands: 4096,
            max_origins: 8192,
            max_edges: 4096,
            catalog: packing,
            spatial: TeletextSpatialTraceBudget {
                max_events: 8192,
                max_invocations: 512,
            },
            max_work: 100_000,
        },
        assembly: TeletextAugmentedOriginProgramBudget {
            max_input_commands: 4096,
            max_output_commands: 4096,
            max_carriers: 64,
            max_identity_checks: 100_000,
            max_work: 100_000,
            recipe: TeletextOriginCarrierBudget {
                max_candidates: 4096,
                max_origins: 8192,
                catalog: packing,
                max_id_checks: 65600,
                max_output_commands: 6,
            },
        },
        packing,
        replay,
        ancestry: TeletextAugmentedInvocationBudget {
            max_original_invocations: 256,
            max_actual_invocations: 512,
            max_carriers: 64,
            max_work: 100_000,
        },
        glyph_comparison: TeletextAugmentedGlyphComparisonBudget {
            max_records: 100_000,
            max_cells: 1800,
            max_diagnostics: 4096,
            max_work: 1_000_000,
        },
        display_comparison: TeletextPhysicalComparisonBudget {
            max_records: 100_000,
            max_cells: 1800,
            max_diagnostics: 32_000,
            max_updates: 1_000_000,
        },
        max_resource_objects: 64,
    }
}
fn active() -> Fixture {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "£Z"), text(2, 1, "B")],
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
fn selected(display: &TeletextDisplayResolution, row: u8, column: u8) -> usize {
    display.origins()[display.cell(row, column).unwrap().origin().unwrap()].candidate_index()
}

fn display_attributes(column: u8, wide: bool, high: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_width: wide,
            double_height: high,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    )
}

fn enlarged_national_base() -> Fixture {
    let mut fixture = Fixture::new(
        vec![text(1, 7, "§"), text(2, 8, "£"), text(3, 1, "B")],
        Some(vec![
            position(1, 7),
            TeletextEnhancementCommand::InvokeObject {
                target: local(0xffff),
                object_type: TeletextObjectType::Active,
                offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
            },
            invoke(local(0xfffe), TeletextObjectType::Adaptive),
        ]),
    );
    fixture.objects = vec![
        definition(
            local(0xffff),
            TeletextObjectType::Active,
            vec![
                display_attributes(0, true, true),
                g0(0),
                display_attributes(2, false, false),
            ],
        ),
        definition(
            local(0xfffe),
            TeletextObjectType::Adaptive,
            vec![
                display_attributes(0, true, false),
                display_attributes(2, false, false),
                at(2, TeletextColumnOperation::G0 { code: code(b'X') }),
            ],
        ),
    ];
    fixture
}

fn assert_national_base_original(original: &TeletextMainComposition) -> usize {
    let display = original.display_resolution();
    let origin = display.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(display.cell(1, 8).unwrap().origin(), Some(origin));
    assert_eq!(
        display.origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    // Adaptive repaint retains the Base candidate but creates a distinct origin.
    assert!(origin >= original.glyph_layers().candidates().len());
    let candidate =
        original.glyph_layers().candidates()[display.origins()[origin].candidate_index()];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    let base = candidate.base().unwrap();
    assert_eq!((base.position().row(), base.position().column()), (1, 7));
    assert_eq!(base.code(), 0x40);
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.root_source(), None);
    assert_eq!(candidate.invocation(), None);
    assert!(matches!(
        original.glyph_intent().records()[display.origins()[origin].candidate_index()].glyph(),
        Some(TeletextGlyphIdentity::G0 { glyph })
            if glyph.repertoire() == TeletextG0Repertoire::Latin
                && glyph.code() == 0x40
                && glyph.national_subset() == Some(TeletextNationalSubset::German)
    ));
    assert_eq!(original.base_replacement(1, 7), None);
    assert!(!original.provenance().iter().any(|source| matches!(
        source,
        TeletextMainCompositionSource::BaseCell { position, kind: TeletextBaseCellCommandKind::Glyph }
            if (position.row(), position.column()) == (1, 7)
    )));
    origin
}

#[test]
fn enlarged_overlap_carries_german_national_base_without_changing_original_identity() {
    use crate::teletext::{
        compare_teletext_augmented_origin_glyphs, compare_teletext_physical_display,
        TeletextAugmentedOriginSource, TeletextOriginCarrierCommandRole,
    };
    super::augmented_origin_compilation_tests::with_candidate(
        enlarged_national_base(),
        None,
        |service, original, augmented| {
            let before = service.clone();
            let fallback = encode_teletext_page(
                &service.base().pages[0],
                TeletextEncodingRegion::WesternLatin,
            )
            .unwrap();
            let origin = assert_national_base_original(original);
            assert_eq!(augmented.carriers().len(), 3);
            let carrier_index = augmented
                .carriers()
                .iter()
                .position(|carrier| carrier.original_origin_index() == origin)
                .unwrap();
            let carrier = &augmented.carriers()[carrier_index];
            assert_eq!(carrier.original_source(), None);
            assert_eq!(carrier.original_root_source(), None);
            assert_eq!(carrier.original_invocation(), None);
            let body = &augmented
                .catalog()
                .definition(carrier.identity())
                .unwrap()
                .program;
            assert_eq!(
                body.commands()[4],
                at(
                    0,
                    TeletextColumnOperation::DesignateCharset {
                        designation: TeletextEnhancementCharsetDesignation::new(0).unwrap()
                    }
                )
            );
            assert_eq!(
                body.commands()[5],
                at(0, TeletextColumnOperation::G2 { code: code(0x27) })
            );
            let compiled = super::augmented_origin_compilation_tests::compiled(
                service,
                augmented,
                limits().ancestry,
            )
            .unwrap();
            let actual = compiled.physical().replay();
            let record =
                actual.glyph_intent().records()[selected(actual.display_resolution(), 1, 7)];
            assert!(
                matches!(record.glyph(), Some(TeletextGlyphIdentity::G2 { glyph })
                if glyph.repertoire() == TeletextG2Repertoire::Latin && glyph.code() == 0x27)
            );
            assert_eq!(
                augmented.emitter_source(record.context().candidate().source().unwrap()),
                Some(TeletextAugmentedOriginSource::CarrierBody {
                    carrier: carrier_index,
                    role: TeletextOriginCarrierCommandRole::Glyph
                })
            );
            let typed =
                compare_teletext_augmented_origin_glyphs(&compiled, limits().glyph_comparison)
                    .unwrap();
            assert_eq!(typed.compared_cells(), 1000);
            assert!(typed.diagnostics().is_empty(), "{:?}", typed.diagnostics());
            let display =
                compare_teletext_physical_display(original, actual, limits().display_comparison)
                    .unwrap();
            assert_eq!(display.compared_cells(), 1000);
            assert!(
                display.diagnostics().is_empty(),
                "{:?}",
                display.diagnostics()
            );
            let normalized =
                compile_teletext_normalized_main_program(service, owner(), limits()).unwrap();
            assert_national_base_original(normalized.original());
            assert_eq!(
                normalized.original().base_display(),
                original.base_display()
            );
            assert_eq!(
                encode_teletext_page(
                    &service.base().pages[0],
                    TeletextEncodingRegion::WesternLatin
                )
                .unwrap(),
                fallback
            );
            assert_eq!(service, &before);
        },
    );
}
#[test]
fn enlarged_overlap_carries_english_double_vertical_in_hebrew_without_changing_base() {
    use crate::teletext::compare_teletext_physical_display;
    let mut fixture = enlarged_national_base();
    fixture.national = 0;
    fixture.rows = vec![text(1, 7, "‖"), text(2, 8, "é"), text(3, 1, "B")];
    let service = fixture.build();
    let before = service.clone();
    let fallback = encode_teletext_page(
        &service.base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    let normalized = compile_teletext_normalized_main_program(&service, owner(), limits()).unwrap();
    let original = normalized.original();
    let display = original.display_resolution();
    let origin = display.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(display.cell(1, 8).unwrap().origin(), Some(origin));
    assert!(origin >= original.glyph_layers().candidates().len());
    let candidate_index = display.origins()[origin].candidate_index();
    let candidate = original.glyph_layers().candidates()[candidate_index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), 0x7c);
    assert_eq!(candidate.source(), None);
    assert_eq!(candidate.invocation(), None);
    assert!(
        matches!(original.glyph_intent().records()[candidate_index].glyph(),
        Some(TeletextGlyphIdentity::G0 { glyph })
        if glyph.repertoire() == TeletextG0Repertoire::Latin
            && glyph.code() == 0x7c
            && glyph.national_subset() == Some(TeletextNationalSubset::English))
    );
    assert_eq!(original.base_replacement(1, 7), None);
    let actual = normalized.physical().replay();
    let record = actual.glyph_intent().records()[selected(actual.display_resolution(), 1, 7)];
    assert!(
        matches!(record.glyph(), Some(TeletextGlyphIdentity::G0 { glyph })
        if glyph.repertoire() == TeletextG0Repertoire::Hebrew
            && glyph.code() == 0x7c && glyph.national_subset().is_none())
    );
    let comparison =
        compare_teletext_physical_display(original, actual, limits().display_comparison).unwrap();
    assert_eq!(comparison.compared_cells(), 1000);
    assert!(
        comparison.diagnostics().is_empty(),
        "{:?}",
        comparison.diagnostics()
    );
    assert_eq!(
        normalized.original().base_display(),
        original.base_display()
    );
    assert_eq!(
        encode_teletext_page(
            &service.base().pages[0],
            TeletextEncodingRegion::WesternLatin
        )
        .unwrap(),
        fallback
    );
    assert_eq!(service, before);
}
#[test]
fn active_unicode_candidate_owns_original_and_actual_after_inputs_leave_scope() {
    let result: EncodedTeletextNormalizedMainProgram = {
        let service = active().build();
        let before = service.clone();
        let legacy = encode_teletext_page(
            &service.base().pages[0],
            TeletextEncodingRegion::WesternLatin,
        )
        .unwrap();
        let result = compile_teletext_normalized_main_program(&service, owner(), limits()).unwrap();
        assert_eq!(service, before);
        assert_eq!(
            encode_teletext_page(
                &service.base().pages[0],
                TeletextEncodingRegion::WesternLatin
            )
            .unwrap(),
            legacy
        );
        result
    };
    assert_eq!(result.owner(), owner());
    assert_eq!(
        result.original().base_display().cell(1, 8).unwrap().code(),
        b' '
    );
    assert!(matches!(
        result
            .original()
            .base_display()
            .cell(1, 8)
            .unwrap()
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: '£' }
    ));
    let old = selected(result.original().display_resolution(), 1, 8);
    let actual = selected(result.physical().replay().display_resolution(), 1, 8);
    let old_candidate = result.original().glyph_layers().candidates()[old];
    assert_eq!(old_candidate.priority(), TeletextGlyphPriority::Active);
    assert_eq!(
        old_candidate.source(),
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        })
    );
    assert_eq!(
        result.physical().replay().glyph_layers().candidates()[actual].priority(),
        TeletextGlyphPriority::Passive
    );
    for record in [
        result.original().glyph_intent().records()[old],
        result.physical().replay().glyph_intent().records()[actual],
    ] {
        assert!(
            matches!(record.glyph(), Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.code()==b'A' && glyph.repertoire()==TeletextG0Repertoire::Latin)
        );
    }
    assert_eq!(result.original().spatial_trace().invocations().len(), 1);
    assert_eq!(
        result
            .physical()
            .replay()
            .spatial_trace()
            .invocations()
            .len(),
        2
    );
    assert_eq!(
        result
            .physical()
            .replay()
            .display_resolution()
            .cell(1, 9)
            .unwrap()
            .attributes()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(7).unwrap())
    );
    assert!(!result.encoded().packet_bodies().is_empty());
    assert_eq!(result.into_physical().replay().owner(), owner());
}
#[test]
fn no_collision_keeps_unicode_fallback_once_and_original_c2_settlement() {
    let service = Fixture::new(
        vec![text(1, 8, "£"), text(2, 8, "é")],
        Some(vec![
            position(3, 8),
            at(
                8,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(4).unwrap(),
                },
            ),
            at(
                8,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(13).unwrap(),
                },
            ),
            g0(8),
        ]),
    )
    .build();
    let raw = compose(&service);
    assert!(raw
        .display_resolution()
        .cell(3, 8)
        .unwrap()
        .attributes()
        .foreground()
        .value()
        .is_none());
    let result = compile_teletext_normalized_main_program(&service, owner(), limits()).unwrap();
    assert_eq!(
        result
            .original()
            .display_resolution()
            .cell(3, 8)
            .unwrap()
            .attributes()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert!(result
        .physical()
        .replay()
        .spatial_trace()
        .invocations()
        .is_empty());
    let root: Vec<_> = result
        .physical()
        .replay()
        .spatial_trace()
        .events()
        .iter()
        .filter(|event| event.source().object.is_none())
        .map(|event| event.command())
        .collect();
    assert_eq!(root, result.original().program().commands());
    assert_eq!(
        result.original().base_display().cell(2, 8).unwrap().code(),
        b'e'
    );
}
#[test]
fn missing_exact_subcode_owner_returns_existing_composition_location() {
    let service = active().build();
    let before = service.clone();
    let other = TeletextPageAddress {
        subcode: TeletextSubcode::new(0x1235).unwrap(),
        ..owner()
    };
    let error = compile_teletext_normalized_main_program(&service, other, limits()).unwrap_err();
    assert_eq!(error.owner, other);
    let Failure::Original(error) = error.reason else {
        panic!("missing owner lost original phase");
    };
    assert!(
        matches!(*error, crate::teletext::TeletextComposedMainEncodingError::Composition(
        TeletextMainCompositionError {reason:TeletextMainCompositionFailure::Service(TeletextServiceError::MissingPage {address}),..}
    ) if address==other)
    );
    assert_eq!(service, before);
}
#[test]
fn no_seed_invalid_glyph_returns_owned_typed_source_diagnostic() {
    let service = Fixture::new(
        vec![text(1, 8, "B")],
        Some(vec![
            position(1, 8),
            at(
                8,
                TeletextColumnOperation::G3 {
                    code: code(0x6e),
                    compatibility: TeletextG3Compatibility::Level25,
                },
            ),
        ]),
    )
    .build();
    let before = service.clone();
    let error = compile_teletext_normalized_main_program(&service, owner(), limits()).unwrap_err();
    let Failure::GlyphMismatch(diagnostic) = error.reason else {
        panic!("invalid glyph escaped typed check");
    };
    assert_eq!(diagnostic.position, (1, 8));
    assert_eq!(
        diagnostic.original.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert!(matches!(
        diagnostic.finding,
        crate::teletext::TeletextAugmentedGlyphFinding::Pending { .. }
    ));
    assert_eq!(service, before);
}
#[test]
fn no_seed_unplaced_private_origin_returns_retained_full_model_uncertainty() {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "B")],
        Some(vec![
            position(1, 39),
            TeletextEnhancementCommand::InvokeObject {
                target: local(0xffff),
                object_type: TeletextObjectType::Passive,
                offset: Some(TeletextObjectOffset::new(0, 71).unwrap()),
            },
        ]),
    );
    fixture.objects.push(definition(
        local(0xffff),
        TeletextObjectType::Passive,
        vec![g0(0)],
    ));
    let service = fixture.build();
    let before = service.clone();
    let error = compile_teletext_normalized_main_program(&service, owner(), limits()).unwrap_err();
    let Failure::DisplayMismatch(diagnostic) = error.reason else {
        panic!("outside origin lost global uncertainty check");
    };
    assert!(matches!(
        diagnostic.finding,
        TeletextPhysicalComparisonFinding::Pending { .. }
    ));
    assert_eq!(
        diagnostic.original.location,
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        })
    );
    assert_eq!(
        diagnostic.original.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(diagnostic.original.invocation.unwrap().get(), 0);
    assert_eq!(service, before);
}
#[test]
fn original_and_augmented_resource_counts_are_both_fenced() {
    let service = active().build();
    let before = service.clone();
    for (maximum, count) in [(0, 1), (1, 2)] {
        let budget = TeletextNormalizedMainProgramBudget {
            max_resource_objects: maximum,
            ..limits()
        };
        let error =
            compile_teletext_normalized_main_program(&service, owner(), budget).unwrap_err();
        let Failure::Allocation(error) = error.reason else {
            panic!("allocation boundary lost");
        };
        assert!(
            matches!(*error, crate::teletext::TeletextObjectAllocationError::ObjectBudget {count:c,maximum:m} if (c,m)==(count,maximum))
        );
    }
    assert_eq!(service, before);
}
#[test]
fn bounded_phase_failures_never_return_partial_wire_or_mutate_service() {
    type PhaseCutoff = (fn(&mut TeletextNormalizedMainProgramBudget), &'static str);
    let service = active().build();
    let before = service.clone();
    let phases: [PhaseCutoff; 7] = [
        (|b| b.composition.max_input_commands = 0, "original"),
        (|b| b.realization.max_seeds = 0, "realization"),
        (|b| b.placement.max_work = 0, "placement"),
        (|b| b.assembly.max_carriers = 0, "assembly"),
        (|b| b.ancestry.max_actual_invocations = 1, "compilation"),
        (|b| b.glyph_comparison.max_cells = 0, "glyph"),
        (|b| b.display_comparison.max_cells = 0, "display"),
    ];
    for (change, stage) in phases {
        let mut budget = limits();
        change(&mut budget);
        let error =
            compile_teletext_normalized_main_program(&service, owner(), budget).unwrap_err();
        assert_eq!(error.owner, owner());
        assert!(matches!(
            (error.reason, stage),
            (Failure::Original(_), "original")
                | (Failure::Realization(_), "realization")
                | (Failure::Placement(_), "placement")
                | (Failure::Assembly(_), "assembly")
                | (Failure::Compilation(_), "compilation")
                | (Failure::GlyphComparison(_), "glyph")
                | (Failure::DisplayComparison(_), "display")
        ));
    }
    assert_eq!(service, before);
}
#[test]
fn foreign_mot_default_is_not_cloned_or_exempted_from_selected_link_lowering() {
    let selected_page = TeletextRawDataPageNumber::new(2, 0xa1).unwrap();
    let selected = TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            selected_page,
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(0x100),
    )
    .unwrap();
    let mut fixture = Fixture::new(vec![text(1, 8, "B")], None);
    // Fixture selects the first resource page, while sparse MOT slot3 owns A0.
    fixture.objects = vec![
        definition(
            selected,
            TeletextObjectType::Passive,
            vec![position(1, 8), g0(8)],
        ),
        definition(
            public(0xffff),
            TeletextObjectType::Passive,
            vec![position(1, 8), g0(8)],
        ),
    ];
    fixture.defaults[3] = Some(TeletextDefaultObjectSelection {
        target: public(0xffff),
        object_type: TeletextObjectType::Passive,
    });
    let service = fixture.build();
    let before = service.clone();
    let error = compile_teletext_normalized_main_program(&service, owner(), limits()).unwrap_err();
    let Failure::Original(error) = error.reason else {
        panic!("default owner bypassed original lowering");
    };
    assert!(
        matches!(*error, crate::teletext::TeletextComposedMainEncodingError::Packing(
        crate::teletext::TeletextComposedMainPackingError {
            location:Some(TeletextMainCompositionSource::DefaultSlot {slot:3}),
            reason:crate::teletext::TeletextLocalObjectProgramEncodingFailure::Lowering(
                crate::teletext::TeletextLocalProgramEncodingFailure::ObjectSelection {target,selected:Some(page)}),..
        }) if target==public(0xffff) && page==selected_page)
    );
    assert_eq!(service, before);
}

#[path = "normalized_main_program_tests/polish_carrier.rs"]
mod polish_carrier;

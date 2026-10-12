//! Whole-page diagnostic observations, not source lineage or receiver acceptance.

use crate::teletext::physical_comparison::{
    compare_teletext_physical_display, compare_teletext_physical_display_with_usage,
    TeletextPhysicalComparison, TeletextPhysicalComparisonBudget,
    TeletextPhysicalComparisonFailure, TeletextPhysicalComparisonField as Field,
    TeletextPhysicalComparisonFinding as Finding, TeletextPhysicalComparisonPendingKind,
    TeletextPhysicalComparisonSide as Side,
};
use crate::teletext::{
    compose_teletext_main_program, replay_teletext_physical_display, TeletextMainComposition,
    TeletextMainCompositionBudget, TeletextPhysicalDisplayReplay,
};
use ersatzrs_domain::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn set(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn foreground(column: u8, value: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    )
}
fn glyph(column: u8, value: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(value).unwrap(),
        },
    )
}
fn flags(column: u8, dh: bool, dw: bool, conceal: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_height: dh,
            double_width: dw,
            boxed: false,
            conceal,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    )
}
fn text(value: &str) -> Vec<TeletextRow> {
    vec![TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: value.into() },
        }],
    }]
}
fn target() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(0xffff))
}
fn invoke() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(),
        object_type: TeletextObjectType::Active,
        offset: None,
    }
}
fn service(
    rows: Vec<TeletextRow>,
    main: Vec<TeletextEnhancementCommand>,
    body: Option<Vec<TeletextEnhancementCommand>>,
    presentation: Option<TeletextLevel25PresentationDefaults>,
) -> TeletextSemanticService {
    let mut page = crate::teletext::transport_tests::draft(200, 0x1234, false);
    page.rows = rows;
    page.national_option = TeletextNationalOption::new(1).unwrap();
    let language = page.language.clone();
    let objects = body
        .into_iter()
        .map(|commands| TeletextEnhancementObjectDefinition {
            identity: target(),
            object_type: TeletextObjectType::Active,
            program: program(commands),
        })
        .collect();
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language,
                initial_page: owner().page,
                pages: vec![page],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation,
                compositional_links: None,
                local_program: Some(program(main)),
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
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 8192,
            max_commands: 8192,
            max_collected_commands: 8192,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}
fn collection() -> TeletextEnhancementCollectionBudget {
    TeletextEnhancementCollectionBudget {
        max_commands: 8192,
        max_drcs_references: 0,
    }
}
fn compose(service: &TeletextSemanticService) -> TeletextMainComposition {
    compose_teletext_main_program(
        service,
        owner(),
        TeletextMainCompositionBudget {
            max_input_commands: 8192,
            max_output_commands: 8192,
            collection: collection(),
        },
    )
    .unwrap()
}
fn replay(
    service: &TeletextSemanticService,
    original: &TeletextMainComposition,
    actual: Vec<TeletextEnhancementCommand>,
) -> TeletextPhysicalDisplayReplay {
    replay_teletext_physical_display(
        service,
        original,
        &program(actual),
        service.objects(),
        collection(),
    )
    .unwrap()
}
fn budget() -> TeletextPhysicalComparisonBudget {
    TeletextPhysicalComparisonBudget {
        max_records: 65536,
        max_cells: 1800,
        max_diagnostics: 65536,
        max_updates: 1_000_000,
    }
}

#[test]
fn physical_usage_exact_boundary_and_failed_reservation_preserve_models_and_errors() {
    let service = service(text("é"), vec![], None, None);
    let original = compose(&service);
    let physical = replay(&service, &original, original.program().commands().to_vec());
    let retained = (service.clone(), original.clone(), physical.clone());
    let expected = compare_teletext_physical_display(&original, &physical, budget()).unwrap();
    let (result, admitted) =
        compare_teletext_physical_display_with_usage(&original, &physical, budget());
    let actual = result.unwrap();
    assert!(admitted > 1000);
    assert_eq!(actual.compared_cells(), expected.compared_cells());
    assert_eq!(actual.diagnostics(), expected.diagnostics());
    assert_eq!(actual.original_pending(), expected.original_pending());
    assert_eq!(actual.physical_pending(), expected.physical_pending());
    assert!(std::ptr::eq(actual.original(), expected.original()));
    assert!(std::ptr::eq(actual.physical(), expected.physical()));
    let mut exact = budget();
    exact.max_updates = admitted;
    let (result, repeated) =
        compare_teletext_physical_display_with_usage(&original, &physical, exact);
    let result = result.unwrap();
    assert_eq!(repeated, admitted);
    assert_eq!(result.compared_cells(), expected.compared_cells());
    assert_eq!(result.diagnostics(), expected.diagnostics());
    exact.max_updates -= 1;
    let (result, partial) =
        compare_teletext_physical_display_with_usage(&original, &physical, exact);
    let failure = result.unwrap_err();
    assert_eq!(
        failure,
        compare_teletext_physical_display(&original, &physical, exact).unwrap_err()
    );
    assert!(
        matches!(failure.reason, TeletextPhysicalComparisonFailure::Budget {
        kind:"updates", maximum,..} if maximum == exact.max_updates)
    );
    assert!(partial > 0 && partial <= exact.max_updates);
    assert!(failure.position.is_some());
    assert_eq!((service, original, physical), retained);
}

#[test]
fn physical_usage_preflight_rejections_report_zero_and_original_errors() {
    let service = service(text("é"), vec![], None, None);
    let original = compose(&service);
    let physical = replay(&service, &original, vec![]);
    for field in ["records", "cells", "updates"] {
        let mut limits = budget();
        match field {
            "records" => limits.max_records = 0,
            "cells" => limits.max_cells = 999,
            _ => limits.max_updates = 0,
        }
        let (result, admitted) =
            compare_teletext_physical_display_with_usage(&original, &physical, limits);
        let error = result.unwrap_err();
        assert_eq!(admitted, 0);
        assert_eq!(
            error,
            compare_teletext_physical_display(&original, &physical, limits).unwrap_err()
        );
        assert!(
            matches!(error.reason, TeletextPhysicalComparisonFailure::Budget {kind,..} if kind == field)
        );
        assert_eq!(error.position, None);
    }
}
fn difference(result: &TeletextPhysicalComparison<'_>, position: (u8, u8), field: Field) -> bool {
    result.diagnostics().iter().any(|diagnostic| {
        diagnostic.position == Some(position) && diagnostic.finding == Finding::Difference(field)
    })
}

#[test]
fn dropping_and_restoring_authored_unicode_uses_sided_masks() {
    for value in ["£", "é"] {
        let service = service(text(value), vec![], None, None);
        let original = compose(&service);
        let dropped = replay(&service, &original, vec![]);
        let comparison = compare_teletext_physical_display(&original, &dropped, budget()).unwrap();
        let (measured, admitted) =
            compare_teletext_physical_display_with_usage(&original, &dropped, budget());
        let measured = measured.unwrap();
        assert!(admitted > 0);
        assert_eq!(measured.compared_cells(), comparison.compared_cells());
        assert_eq!(measured.diagnostics(), comparison.diagnostics());
        assert_eq!(comparison.compared_cells(), 1000);
        assert!(difference(&comparison, (1, 8), Field::GlyphMask));
        let restored = replay(&service, &original, original.program().commands().to_vec());
        let comparison = compare_teletext_physical_display(&original, &restored, budget()).unwrap();
        assert!(
            comparison.diagnostics().is_empty(),
            "{:?}",
            comparison.diagnostics()
        );
        assert_eq!(comparison.original().owner(), comparison.physical().owner());
    }
}

#[test]
fn original_active_glyph_and_generated_local_replacement_are_compared() {
    let service = service(
        text("é"),
        vec![set(1, 8), invoke()],
        Some(vec![glyph(0, b'A')]),
        None,
    );
    let original = compose(&service);
    let actual = replay(&service, &original, original.program().commands().to_vec());
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    let diagnostic = comparison
        .diagnostics()
        .iter()
        .find(|d| d.position == Some((1, 8)) && d.finding == Finding::Difference(Field::GlyphMask))
        .unwrap();
    assert_eq!(diagnostic.original.location.unwrap().object, Some(target()));
    assert_eq!(diagnostic.original.invocation.unwrap().get(), 0);
    assert_eq!(diagnostic.physical.location.unwrap().object, None);
    assert_eq!(diagnostic.original.raw_position, Some((1, 8)));
    assert_eq!(diagnostic.physical.raw_position, Some((1, 8)));
}

#[test]
fn invisible_conceal_flash_rate_and_rgb_alias_references_remain_observable() {
    let original_main = vec![set(1, 8), foreground(8, 0), glyph(8, b'A')];
    let mut presentation = crate::teletext::presentation::western_latin_v1_presentation(
        TeletextNationalOption::new(1).unwrap(),
    )
    .unwrap();
    presentation.palette.clut2[0] = TeletextRgb::new([0, 0, 0]).unwrap();
    let service = service(vec![], original_main, None, Some(presentation));
    let original = compose(&service);
    let actual = replay(
        &service,
        &original,
        vec![
            set(1, 8),
            foreground(8, 16),
            flags(8, false, false, true),
            at(
                8,
                TeletextColumnOperation::Flash {
                    mode: TeletextFlashMode::Steady,
                    rate: TeletextFlashRate::Fast2HzPhase3,
                },
            ),
            glyph(8, b'A'),
        ],
    );
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    assert!(difference(&comparison, (1, 8), Field::ForegroundReference));
    assert!(difference(&comparison, (1, 8), Field::Conceal));
    assert!(difference(&comparison, (1, 8), Field::Flash));
    assert!(!difference(&comparison, (1, 8), Field::GlyphMask));
    assert!(!difference(&comparison, (1, 8), Field::ColourResponse));
}

#[test]
fn effective_c3_parts_and_unaffected_far_page_cells_are_not_seed_limited() {
    let original_main = vec![
        set(1, 8),
        flags(8, true, true, false),
        glyph(8, b'A'),
        set(24, 30),
        glyph(30, b'B'),
    ];
    let service = service(vec![], original_main, None, None);
    let original = compose(&service);
    let actual = replay(
        &service,
        &original,
        vec![
            set(1, 8),
            flags(8, true, false, false),
            glyph(8, b'A'),
            set(24, 30),
            glyph(30, b'C'),
        ],
    );
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    assert!(difference(&comparison, (1, 8), Field::EffectiveSize));
    assert!(difference(&comparison, (1, 9), Field::Fragment));
    assert!(difference(&comparison, (2, 9), Field::Fragment));
    assert!(difference(&comparison, (24, 30), Field::GlyphMask));
    assert_eq!(comparison.compared_cells(), 1000);
}

#[test]
fn original_and_physical_pending_survive_even_without_replacement_seeds() {
    let service = service(
        vec![],
        vec![
            set(1, 8),
            foreground(8, 4),
            foreground(8, 13),
            glyph(8, b'A'),
        ],
        None,
        None,
    );
    let original = compose(&service);
    let actual = replay(&service, &original, original.program().commands().to_vec());
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    assert!(!comparison.original_pending().is_empty());
    assert!(!comparison.physical_pending().is_empty());
    for side in [Side::Original, Side::Physical] {
        assert!(comparison
            .diagnostics()
            .iter()
            .any(|d| matches!(d.finding,Finding::Pending {side:actual,..} if actual==side)));
    }
}

#[test]
fn superseded_display_history_does_not_make_known_selected_cells_pending() {
    let main = vec![
        set(1, 7),
        flags(7, false, true, false),
        glyph(7, b'A'),
        foreground(8, 4),
        foreground(8, 13),
        foreground(9, 2),
        glyph(9, b'B'),
    ];
    let service = service(vec![], main, None, None);
    let original = compose(&service);
    let actual = replay(&service, &original, original.program().commands().to_vec());
    assert!(!original.display_resolution().pending().is_empty());
    assert_eq!(
        original
            .display_resolution()
            .cell(1, 9)
            .unwrap()
            .attributes()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(2).unwrap())
    );
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    assert!(!comparison.original_pending().is_empty());
    assert!(!comparison
        .diagnostics()
        .iter()
        .any(|d| d.position == Some((1, 9))
            && matches!(
                d.finding,
                Finding::Pending {
                    kind: TeletextPhysicalComparisonPendingKind::SelectedAttributes,
                    ..
                } | Finding::Colour { .. }
            )));
}

#[test]
fn same_shape_across_repertoires_has_no_lineage_or_identity_assertion() {
    let service = service(vec![], vec![set(1, 8), glyph(8, b'A')], None, None);
    let original = compose(&service);
    let actual = replay(
        &service,
        &original,
        vec![
            set(1, 8),
            at(
                8,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(0x20).unwrap(),
                },
            ),
            glyph(8, 0x41),
        ],
    );
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    let first=original.glyph_intent().records().iter().find_map(|record|matches!(record.glyph(),Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.repertoire()==TeletextG0Repertoire::Latin && glyph.code()==0x41).then_some(record.glyph())).unwrap();
    let second=actual.glyph_intent().records().iter().find_map(|record|matches!(record.glyph(),Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.repertoire()==TeletextG0Repertoire::Cyrillic1 && glyph.code()==0x41).then_some(record.glyph())).unwrap();
    assert_ne!(first, second);
    // The selected atlas gives both identities the same A shape. No findings is
    // still a selected-model observation, never an authored identity assertion.
    assert!(!difference(&comparison, (1, 8), Field::GlyphMask));
    assert!(
        comparison.diagnostics().is_empty(),
        "{:?}",
        comparison.diagnostics()
    );
    assert_eq!(comparison.compared_cells(), 1000);
    assert_eq!(
        comparison.original().glyph_intent(),
        original.glyph_intent()
    );
    assert_eq!(comparison.physical().glyph_intent(), actual.glyph_intent());
}

#[test]
fn bounded_failures_return_no_partial_comparison_and_preserve_inputs() {
    let service = service(text("é"), vec![], None, None);
    let original = compose(&service);
    let actual = replay(&service, &original, vec![]);
    let retained = (service.clone(), original.clone(), actual.clone());
    for (kind, limit) in [
        ("records", 0),
        ("cells", 999),
        ("updates", 0),
        ("diagnostics", 0),
    ] {
        let mut limits = budget();
        match kind {
            "records" => limits.max_records = limit,
            "cells" => limits.max_cells = limit,
            "updates" => limits.max_updates = limit,
            _ => limits.max_diagnostics = limit,
        }
        let error = compare_teletext_physical_display(&original, &actual, limits).unwrap_err();
        let (measured, admitted) =
            compare_teletext_physical_display_with_usage(&original, &actual, limits);
        assert_eq!(measured.unwrap_err(), error);
        if kind == "diagnostics" {
            assert!(admitted > 0 && admitted <= limits.max_updates);
        } else {
            assert_eq!(admitted, 0);
        }
        assert!(
            matches!(error.reason,TeletextPhysicalComparisonFailure::Budget {kind:actual,..} if actual==kind)
        );
        assert_eq!(error.owner, owner());
        if kind == "diagnostics" {
            assert_eq!(error.position, Some((1, 8)));
        } else {
            assert_eq!(error.position, None);
        }
    }
    assert_eq!((service, original, actual), retained);
}

#[test]
fn invert_boxing_and_undefined_g3_d5_are_not_laundered_by_black_pixels() {
    let g3 = at(
        8,
        TeletextColumnOperation::G3 {
            code: TeletextGlyphCode::new(0x20).unwrap(),
            compatibility: TeletextG3Compatibility::Level25,
        },
    );
    let service = service(vec![], vec![set(1, 8), foreground(8, 0), g3], None, None);
    let original = compose(&service);
    let actual = replay(
        &service,
        &original,
        vec![
            set(1, 8),
            foreground(8, 0),
            at(
                8,
                TeletextColumnOperation::Attributes {
                    double_height: false,
                    double_width: false,
                    boxed: true,
                    conceal: false,
                    invert: true,
                    underline_or_separated_mosaics: true,
                },
            ),
            g3,
        ],
    );
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    assert!(difference(&comparison, (1, 8), Field::Invert));
    assert!(difference(&comparison, (1, 8), Field::Boxed));
    assert!(difference(&comparison, (1, 8), Field::GlyphEffects));
    assert!(!difference(&comparison, (1, 8), Field::GlyphMask));
    assert!(!difference(&comparison, (1, 8), Field::ColourResponse));
}

#[test]
fn unused_palette_entries_are_compared_without_claiming_generation_pairing() {
    let original_service = service(vec![], vec![], None, None);
    let original = compose(&original_service);
    let mut presentation = crate::teletext::presentation::western_latin_v1_presentation(
        TeletextNationalOption::new(1).unwrap(),
    )
    .unwrap();
    presentation.palette.clut3[7] = TeletextRgb::new([1, 2, 3]).unwrap();
    let actual_service = service(vec![], vec![], None, Some(presentation));
    let actual_original = compose(&actual_service);
    let actual = replay(&actual_service, &actual_original, vec![]);
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    assert!(comparison.diagnostics().iter().any(|d| d.position.is_none()
        && d.finding == Finding::Difference(Field::Palette { reference: 31 })));
    assert_eq!(comparison.original().owner(), comparison.physical().owner());
}

#[test]
fn base_spacing_and_enhanced_d5_with_same_separation_have_equal_glyph_effects() {
    let rows = vec![TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Mosaics {
                cells: vec![TeletextMosaic::new(63).unwrap()],
                separated: true,
            },
        }],
    }];
    let service = service(rows, vec![], None, None);
    let original = compose(&service);
    let actual = replay(
        &service,
        &original,
        vec![
            set(1, 8),
            at(
                8,
                TeletextColumnOperation::Attributes {
                    double_height: false,
                    double_width: false,
                    boxed: false,
                    conceal: false,
                    invert: false,
                    underline_or_separated_mosaics: true,
                },
            ),
            at(
                8,
                TeletextColumnOperation::G1 {
                    mosaic: TeletextMosaic::new(63).unwrap(),
                },
            ),
        ],
    );
    let comparison = compare_teletext_physical_display(&original, &actual, budget()).unwrap();
    // Distinct spacing/nonspacing mechanisms produce the same effective mosaic.
    assert!(!difference(&comparison, (1, 8), Field::GlyphEffects));
    assert!(!difference(&comparison, (1, 8), Field::GlyphMask));
}

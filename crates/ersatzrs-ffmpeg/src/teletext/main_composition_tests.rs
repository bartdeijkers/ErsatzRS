//! Semantic merge proofs; these do not stand in for final packet/render proof.

use ersatzrs_domain::*;

use super::main_composition::*;

#[path = "raster_tests.rs"]
mod raster_tests;

// Intentional opt-in Linux x64 public decoder-ABI calibration; all other native
// platforms retain portable raster tests and separate platform/client gates.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[path = "raster_runtime_tests.rs"]
mod raster_runtime_tests;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn pos(row: u8, column: u8) -> TeletextEnhancementPosition {
    TeletextEnhancementPosition::new(row, column).unwrap()
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: pos(row, column),
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn charset(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(code).unwrap(),
        },
    )
}
fn code(value: u8) -> TeletextGlyphCode {
    TeletextGlyphCode::new(value).unwrap()
}
fn g2(column: u8) -> TeletextEnhancementCommand {
    at(column, TeletextColumnOperation::G2 { code: code(0x54) })
}
fn g0(column: u8) -> TeletextEnhancementCommand {
    at(column, TeletextColumnOperation::G0 { code: code(b'A') })
}
fn accent(column: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0Diacritic {
            code: code(b'e'),
            diacritic: TeletextDiacriticIndex::new(2).unwrap(),
        },
    )
}
fn text(row: u8, column: u8, text: &str) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![TeletextRun {
            column,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: text.into() },
        }],
    }
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}
fn public(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}
fn invoke(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: None,
    }
}
fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: program(commands),
    }
}
fn budget() -> TeletextMainCompositionBudget {
    TeletextMainCompositionBudget {
        max_input_commands: 4096,
        max_output_commands: 4096,
        collection: TeletextEnhancementCollectionBudget {
            max_commands: 8192,
            max_drcs_references: 0,
        },
    }
}

#[derive(Clone)]
struct Fixture {
    rows: Vec<TeletextRow>,
    main: Option<Vec<TeletextEnhancementCommand>>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    defaults: [Option<TeletextDefaultObjectSelection>; 4],
    primary: Option<u8>,
    presentation: Option<TeletextLevel25PresentationDefaults>,
    drcs_links: TeletextDrcsPageLinks,
    drcs: Vec<TeletextDrcsSubtable>,
}
impl Fixture {
    fn new(rows: Vec<TeletextRow>, main: Option<Vec<TeletextEnhancementCommand>>) -> Self {
        Self {
            rows,
            main,
            objects: vec![],
            defaults: [None; 4],
            primary: None,
            presentation: None,
            drcs_links: TeletextDrcsPageLinks::unlinked(),
            drcs: vec![],
        }
    }
    fn build(self) -> TeletextSemanticService {
        let language = TeletextLanguage::new("nld".into()).unwrap();
        let presentation = self.presentation.or_else(|| {
            self.primary.map(|primary| {
                let mut result = super::presentation::western_latin_v1_presentation(
                    TeletextNationalOption::new(1).unwrap(),
                )
                .unwrap();
                result.primary = TeletextPageCharsetDesignation::new(primary).unwrap();
                result
            })
        });
        let public = self.objects.iter().find_map(|object| {
            object
                .identity
                .resource()
                .map(TeletextResourceIdentity::page)
        });
        TeletextSemanticService::new(
            TeletextResolvedServiceDraft {
                base: TeletextPageSetDraft {
                    language: language.clone(),
                    initial_page: owner().page,
                    pages: vec![TeletextPageDraft {
                        address: owner(),
                        kind: TeletextPageKind::FullPage,
                        language,
                        national_option: TeletextNationalOption::new(1).unwrap(),
                        rows: self.rows,
                        navigation: vec![],
                    }],
                },
                pages: vec![TeletextResolvedPageEnhancementDraft {
                    address: owner(),
                    presentation,
                    compositional_links: None,
                    local_program: self.main.map(program),
                    resources: TeletextResolvedPageResourceSelection {
                        objects: TeletextResolvedObjectPageLinks {
                            public,
                            global: None,
                        },
                        drcs: self.drcs_links,
                        defaults: self.defaults,
                    },
                }],
                magazines: vec![],
                objects: self.objects,
                drcs: self.drcs,
            },
            TeletextServiceBudget {
                max_records: 64,
                max_objects: 32,
                max_structure_items: 4096,
                max_base_content_bytes: 8192,
                max_commands: 8192,
                max_collected_commands: 16384,
                max_collected_drcs_references: 8192,
            },
        )
        .unwrap()
    }
}
fn compose(service: &TeletextSemanticService) -> TeletextMainComposition {
    compose_teletext_main_program(service, owner(), budget()).unwrap()
}

#[test]
fn same_service_display_resolution_defers_covered_writers_without_rewriting_observations() {
    let size = |column, double_width| {
        at(
            column,
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
    let service = Fixture::new(
        vec![text(1, 8, "ABCD")],
        Some(vec![
            position(1, 8),
            original_foreground(8, 1),
            size(8, true),
            original_foreground(9, 4),
            original_foreground(10, 2),
            size(10, false),
        ]),
    )
    .build();
    let retained = service.clone();
    let plan = compose(&service);
    let display = plan.display_resolution();
    assert_eq!(display.owner(), plan.owner());
    assert_eq!(display.width(), 40);
    let originals = plan.glyph_layers().candidates().len();
    assert!(display.origins().len() >= originals);
    for (index, origin) in display.origins()[..originals].iter().enumerate() {
        assert_eq!(origin.candidate_index(), index);
    }
    for column in [8, 9] {
        let cell = display.cell(1, column).unwrap();
        assert_eq!(cell.attributes().foreground().value().unwrap().get(), 1);
        assert_eq!(cell.origin(), display.cell(1, 8).unwrap().origin());
    }
    assert_eq!(
        display.cell(1, 9).unwrap().fragment(),
        TeletextDisplayFragment::DoubleWidthRight
    );
    assert_eq!(
        display
            .cell(1, 10)
            .unwrap()
            .attributes()
            .foreground()
            .value()
            .unwrap()
            .get(),
        2
    );
    let blue = display
        .writers()
        .iter()
        .find(|writer| {
            matches!(writer.source(),
                TeletextAttributeSource::Enhancement { location, .. } if location.command == 3
            )
        })
        .unwrap();
    assert_eq!(blue.placed_position(), (1, 9));
    assert_eq!(blue.requested_start(), (1, 9));
    assert_eq!(blue.effective_start(), (1, 10));
    // The old pre-geometry projection is diagnostic and remains unchanged.
    assert_eq!(
        plan.attribute_execution()
            .cell(1, 9)
            .unwrap()
            .foreground()
            .value()
            .unwrap()
            .get(),
        4
    );
    assert_eq!(service, retained);
}

#[test]
fn same_service_display_resolution_keeps_suppressed_background_and_glyph_baseline_distinct() {
    let style = TeletextStyle {
        foreground: TeletextColour::Red,
        background: TeletextColour::Blue,
        height: TeletextHeight::Double,
        ..TeletextStyle::default()
    };
    let upper = TeletextRow {
        row: 1,
        runs: vec![
            TeletextRun {
                column: 8,
                style,
                content: TeletextRunContent::Text { text: "A".into() },
            },
            TeletextRun {
                column: 14,
                style: TeletextStyle {
                    height: TeletextHeight::Normal,
                    ..style
                },
                content: TeletextRunContent::Text { text: "C".into() },
            },
        ],
    };
    // Authored fallback leaves the spacing-suppressed row empty. Original
    // ignored coded controls are tested independently by the domain fixtures.
    let service = Fixture::new(vec![upper], Some(vec![position(2, 14), g0(14)])).build();
    let plan = compose(&service);
    let display = plan.display_resolution();
    assert_eq!(
        display.cell(2, 8).unwrap().fragment(),
        TeletextDisplayFragment::DoubleHeightBottom
    );
    assert_eq!(
        display
            .cell(2, 8)
            .unwrap()
            .attributes()
            .foreground()
            .value()
            .unwrap()
            .get(),
        1
    );
    let glyph = display.cell(2, 14).unwrap();
    assert_eq!(glyph.attributes().foreground().value().unwrap().get(), 7);
    assert_eq!(glyph.attributes().background().value().unwrap().get(), 0);
    let background = display.cell(2, 15).unwrap();
    assert_eq!(
        background.fragment(),
        TeletextDisplayFragment::BackgroundOnly
    );
    assert_eq!(
        background.attributes().background().value().unwrap().get(),
        4
    );
    let candidate = plan.glyph_layers().candidates()
        [display.origins()[glyph.origin().unwrap()].candidate_index()];
    assert_eq!(candidate.source().unwrap().command, 1);
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Local);
    assert_eq!(
        plan.attribute_execution()
            .cell(2, 15)
            .unwrap()
            .background()
            .value()
            .unwrap()
            .get(),
        0
    );
}

fn originals(plan: &TeletextMainComposition) -> Vec<TeletextEnhancementCommand> {
    plan.program()
        .commands()
        .iter()
        .zip(plan.provenance())
        .filter_map(|(command, location)| {
            matches!(
                location,
                TeletextMainCompositionSource::OriginalPage { .. }
                    | TeletextMainCompositionSource::DefaultSlot { .. }
            )
            .then_some(*command)
        })
        .collect()
}

#[test]
fn interleaving_preserves_attributes_calls_offsets_and_original_command_values() {
    let target = local(0xffff);
    let call = TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: Some(TeletextObjectOffset::new(2, 7).unwrap()),
    };
    let foreground = at(
        0,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(3).unwrap(),
        },
    );
    let authored = vec![
        invoke(target, TeletextObjectType::Passive),
        position(1, 0),
        foreground,
        call,
        g0(0),
        position(2, 2),
        g0(2),
    ];
    let mut fixture = Fixture::new(
        vec![text(0, 8, "é"), text(1, 0, "éé"), text(2, 2, "é")],
        Some(authored.clone()),
    );
    fixture
        .objects
        .push(definition(target, TeletextObjectType::Passive, vec![]));
    let service = fixture.build();
    let plan = compose(&service);
    assert_eq!(originals(&plan), authored);
    assert_eq!(
        plan.program().commands(),
        &[
            authored[0],
            accent(8),
            position(1, 0),
            foreground,
            call,
            accent(0),
            g0(0),
            accent(1),
            position(2, 2),
            accent(2),
            g0(2),
        ]
    );
    assert_eq!(plan.owner(), owner());
    assert_eq!(plan.program().commands().len(), plan.provenance().len());
    assert_eq!(
        plan.source_location(plan.collected().object_references()[1].source_location),
        Some(TeletextMainCompositionSource::OriginalPage { command: 3 })
    );
}

#[test]
fn compatible_replacements_suppress_only_derived_cell_and_keep_enhanced_same_cell() {
    for replacement in [
        g2(0),
        accent(0),
        at(
            0,
            TeletextColumnOperation::G3 {
                code: code(0x20),
                compatibility: TeletextG3Compatibility::Level15,
            },
        ),
    ] {
        let original = vec![position(1, 0), g0(0), replacement];
        let service = Fixture::new(vec![text(1, 0, "é")], Some(original.clone())).build();
        let plan = compose(&service);
        assert_eq!(plan.program().commands(), original);
        assert!(plan
            .provenance()
            .iter()
            .all(|source| matches!(source, TeletextMainCompositionSource::OriginalPage { .. })));
    }
    let service = Fixture::new(vec![text(1, 0, "é")], Some(vec![position(1, 0), g0(0)])).build();
    assert_eq!(
        compose(&service).program().commands(),
        &[position(1, 0), accent(0), g0(0)]
    );
}

#[test]
fn duplicate_compatible_glyph_is_located_without_mutating_service() {
    let service = Fixture::new(
        vec![text(1, 0, "é")],
        Some(vec![position(1, 0), accent(0), g2(0)]),
    )
    .build();
    let before = service.clone();
    let failure = compose_teletext_main_program(&service, owner(), budget()).unwrap_err();
    assert_eq!(service, before);
    assert_eq!(
        failure.location,
        Some(TeletextMainCompositionSource::OriginalPage { command: 2 })
    );
    assert_eq!(
        failure.reason,
        TeletextMainCompositionFailure::DuplicateCompatibleGlyph {
            position: pos(1, 0),
            first: TeletextMainCompositionSource::OriginalPage { command: 1 }
        }
    );
}

#[test]
fn latin_injection_restores_exact_alias_and_resets_only_on_real_row_change() {
    let row_colour = TeletextEnhancementCommand::SetRowColour {
        row: 1,
        colour: TeletextColourReference::new(2).unwrap(),
        following_rows: false,
    };
    let authored = vec![
        position(1, 0),
        charset(0, 0x57),
        row_colour,
        position(1, 1),
        g0(1),
        position(2, 0),
        g0(1),
    ];
    let service = Fixture::new(
        vec![text(1, 1, "é"), text(2, 1, "é")],
        Some(authored.clone()),
    )
    .build();
    let plan = compose(&service);
    assert_eq!(originals(&plan), authored);
    assert_eq!(
        plan.program().commands(),
        &[
            position(1, 0),
            charset(0, 0x57),
            row_colour,
            position(1, 1),
            charset(1, 0),
            accent(1),
            charset(1, 0x57),
            g0(1),
            position(2, 0),
            accent(1),
            g0(1),
        ]
    );
    assert_eq!(plan.collected().charset_pairs().len(), 2);
}

#[test]
fn latin_alias_does_not_inject_redundant_charset_or_use_conservative_output_budget() {
    let main = vec![position(1, 0), charset(0, 0x1f), g0(1)];
    let service = Fixture::new(vec![text(1, 1, "é")], Some(main)).build();
    let mut bounds = budget();
    bounds.max_output_commands = 4;
    let plan = compose_teletext_main_program(&service, owner(), bounds).unwrap();
    assert_eq!(
        plan.program().commands(),
        &[position(1, 0), charset(0, 0x1f), accent(1), g0(1)]
    );
    bounds.max_output_commands = 3;
    let error = compose_teletext_main_program(&service, owner(), bounds).unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextMainCompositionSource::OriginalPage { command: 2 })
    );
    assert!(matches!(
        error.reason,
        TeletextMainCompositionFailure::OutputBudget { maximum: 3 }
    ));
}

#[test]
fn added_latin_pair_is_collected_and_third_pair_failure_maps_to_base_cell() {
    let mut fixture = Fixture::new(
        vec![text(1, 1, "é")],
        Some(vec![position(1, 0), charset(0, 0x37), g0(1)]),
    );
    fixture.primary = Some(0x20);
    let service = fixture.build();
    let error = compose_teletext_main_program(&service, owner(), budget()).unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextMainCompositionSource::BaseCell {
            position: pos(1, 1),
            kind: TeletextBaseCellCommandKind::LatinCharset
        })
    );
    assert_eq!(
        error.reason,
        TeletextMainCompositionFailure::Collection(
            TeletextEnhancementCollectionFailure::CharsetPairs {
                pair: TeletextCharsetPair::LatinLatin
            }
        )
    );
}

#[test]
fn sparse_defaults_keep_original_owner_source_and_none_offset_while_empty_local_suppresses() {
    let first = public(0xffff);
    let second = public(0x8123);
    let mut fixture = Fixture::new(vec![text(1, 0, "é")], None);
    fixture.objects = vec![
        definition(first, TeletextObjectType::Active, vec![]),
        definition(second, TeletextObjectType::Passive, vec![]),
    ];
    fixture.defaults[2] = Some(TeletextDefaultObjectSelection {
        target: first,
        object_type: TeletextObjectType::Active,
    });
    fixture.defaults[3] = Some(TeletextDefaultObjectSelection {
        target: second,
        object_type: TeletextObjectType::Passive,
    });
    let mut empty_fixture = fixture.clone();
    empty_fixture.main = Some(vec![]);
    let service = fixture.build();
    let plan = compose(&service);
    assert_eq!(
        plan.original_program_source(),
        &TeletextEffectiveProgramSource::Defaults { slots: vec![2, 3] }
    );
    assert_eq!(
        &plan.program().commands()[..2],
        &[
            invoke(first, TeletextObjectType::Active),
            invoke(second, TeletextObjectType::Passive)
        ]
    );
    assert_eq!(
        &plan.provenance()[..2],
        &[
            TeletextMainCompositionSource::DefaultSlot { slot: 2 },
            TeletextMainCompositionSource::DefaultSlot { slot: 3 }
        ]
    );
    let empty = empty_fixture.build();
    let plan = compose(&empty);
    assert_eq!(
        plan.original_program_source(),
        &TeletextEffectiveProgramSource::Local
    );
    assert_eq!(plan.program().commands(), &[position(1, 0), accent(0)]);
    assert!(plan.collected().object_references().is_empty());
    let base = Fixture::new(vec![], None).build();
    assert_eq!(
        compose(&base).original_program_source(),
        &TeletextEffectiveProgramSource::BaseOnly
    );
}

#[test]
fn original_nonblack_header_colours_and_row_zero_positions_remain_semantic() {
    let screen = TeletextEnhancementCommand::SetScreenColour {
        colour: TeletextColourReference::new(5).unwrap(),
    };
    let row = TeletextEnhancementCommand::SetRowColour {
        row: 0,
        colour: TeletextColourReference::new(3).unwrap(),
        following_rows: false,
    };
    let main = vec![screen, row, position(0, 8), g0(8)];
    let service = Fixture::new(vec![text(0, 8, "é")], Some(main.clone())).build();
    let plan = compose(&service);
    assert_eq!(
        plan.program().commands(),
        &[screen, row, position(0, 8), accent(8), g0(8)]
    );
    assert_eq!(originals(&plan), main);
    assert_eq!(
        plan.colour_context().row_colour(0),
        Some(TeletextColourReference::new(3).unwrap())
    );
    assert_eq!(
        plan.colour_context().screen_colour(),
        TeletextColourReference::new(5).unwrap()
    );
}

#[test]
fn input_and_expanded_work_budgets_fail_with_exact_original_locations() {
    let target = local(7);
    let main = vec![
        position(1, 0),
        invoke(target, TeletextObjectType::Passive),
        invoke(target, TeletextObjectType::Passive),
    ];
    let mut fixture = Fixture::new(vec![text(1, 0, "é")], Some(main));
    fixture
        .objects
        .push(definition(target, TeletextObjectType::Passive, vec![g0(0)]));
    let service = fixture.build();
    let mut bounds = budget();
    bounds.max_input_commands = 3;
    assert_eq!(
        compose_teletext_main_program(&service, owner(), bounds)
            .unwrap_err()
            .reason,
        TeletextMainCompositionFailure::InputBudget { maximum: 3 }
    );
    bounds.max_input_commands = 4;
    bounds.collection.max_commands = 5;
    let error = compose_teletext_main_program(&service, owner(), bounds).unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextMainCompositionSource::BaseCell {
            position: pos(1, 0),
            kind: TeletextBaseCellCommandKind::Glyph
        })
    );
    assert!(matches!(
        error.reason,
        TeletextMainCompositionFailure::Collection(
            TeletextEnhancementCollectionFailure::CommandBudget { maximum: 5 }
        )
    ));
    bounds.collection.max_commands = 6;
    let plan = compose_teletext_main_program(&service, owner(), bounds).unwrap();
    assert_eq!(plan.collected().command_count(), 6);
    assert_eq!(
        plan.source_location(TeletextEnhancementSource {
            object: Some(target),
            command: 0
        }),
        Some(TeletextMainCompositionSource::Object {
            identity: target,
            command: 0
        })
    );
    assert_eq!(
        plan.source_location(TeletextEnhancementSource {
            object: None,
            command: 999
        }),
        None
    );
}

#[test]
fn unknown_exact_owner_and_backwards_positions_are_explicit_failures() {
    let service = Fixture::new(vec![], Some(vec![])).build();
    let mut foreign = owner();
    foreign.subcode = TeletextSubcode::new(1).unwrap();
    let error = compose_teletext_main_program(&service, foreign, budget()).unwrap_err();
    assert_eq!(
        error.reason,
        TeletextMainCompositionFailure::Service(TeletextServiceError::MissingPage {
            address: foreign
        })
    );
    // The supplied semantic program has checked coordinates but no sorted AP
    // promise. Composition reports its exact original command instead of sorting.
    let service = Fixture::new(vec![], Some(vec![position(1, 3), g0(2)])).build();
    let error = compose_teletext_main_program(&service, owner(), budget()).unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextMainCompositionSource::OriginalPage { command: 1 })
    );
    assert_eq!(
        error.reason,
        TeletextMainCompositionFailure::ActivePositionOrder {
            previous: (1, 3),
            requested: (1, 2)
        }
    );
}

#[test]
fn composition_sizes_surviving_cells_instead_of_legacy_unsuppressed_wire_words() {
    let mut rows = Vec::new();
    let mut main = Vec::new();
    for row in 1..=6 {
        let count = if row == 6 { 8 } else { 40 };
        rows.push(text(row, 0, &"é".repeat(count)));
        main.push(position(row, 0));
        for column in 0..count {
            main.push(accent(u8::try_from(column).unwrap()));
        }
    }
    let service = Fixture::new(rows, Some(main.clone())).build();
    assert_eq!(
        super::layout::compile_for_composition(&service.base().pages[0])
            .unwrap()
            .base_glyphs()
            .len(),
        208
    );
    assert!(matches!(
        super::layout::compile(&service.base().pages[0]),
        Err(super::TeletextEncodingError::Enhancements { .. })
    ));
    let plan = compose(&service);
    assert_eq!(plan.program().commands(), main);
    assert_eq!(plan.program().commands().len(), 214); // Semantic plan has no wire cap.
    let allocations = super::allocate_teletext_resource_objects(service.objects(), &[], 0).unwrap();
    let physical = super::compile_teletext_local_program_with_objects(
        owner(),
        plan.program(),
        service.objects(),
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        TeletextObjectCatalogBudget {
            max_objects: 0,
            max_commands: 214,
        },
    )
    .unwrap_err();
    assert_eq!(
        physical.reason,
        super::TeletextLocalObjectProgramEncodingFailure::Capacity {
            count: 209,
            maximum: 208,
        }
    );
    assert_eq!(physical.owner, owner());
    let location = physical.location.unwrap();
    assert_eq!(
        plan.source_location(location),
        Some(TeletextMainCompositionSource::OriginalPage {
            command: location.command
        })
    );
}

#[test]
fn generated_compatible_glyph_precedes_authored_level25_mosaic() {
    let mosaic = at(
        0,
        TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(0x15).unwrap(),
        },
    );
    let service = Fixture::new(vec![text(1, 0, "é")], Some(vec![position(1, 0), mosaic])).build();
    let plan = compose(&service);
    assert_eq!(
        plan.program().commands(),
        &[position(1, 0), accent(0), mosaic]
    );
    assert_eq!(
        plan.provenance()[1],
        TeletextMainCompositionSource::BaseCell {
            position: pos(1, 0),
            kind: TeletextBaseCellCommandKind::Glyph
        }
    );
}

#[test]
fn retained_spatial_trace_uses_original_commands_before_derived_base_insertion() {
    let target = local(42);
    let original = vec![
        position(1, 8),
        invoke(target, TeletextObjectType::Passive),
        g0(9),
    ];
    let mut fixture = Fixture::new(vec![text(1, 9, "£")], Some(original.clone()));
    fixture.objects = vec![definition(
        target,
        TeletextObjectType::Passive,
        vec![position(0, 1), g0(1)],
    )];
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let spatial = plan.spatial_trace();
    assert_eq!(spatial.owner(), owner());
    assert_eq!(
        spatial.original_program_source(),
        &TeletextEffectiveProgramSource::Local
    );
    let root_commands: Vec<_> = spatial
        .events()
        .iter()
        .filter(|event| event.invocation().is_none())
        .map(|event| event.command())
        .collect();
    assert_eq!(root_commands, original);
    assert_eq!(spatial.events().len(), 5);
    assert_eq!(spatial.invocations().len(), 1);
    let instance = &spatial.invocations()[0];
    assert_eq!(instance.target(), target);
    assert_eq!(
        (instance.origin().row(), instance.origin().column()),
        (1, 8)
    );
    let body = spatial
        .events()
        .iter()
        .find(|event| event.source().object == Some(target) && event.source().command == 1)
        .unwrap();
    assert_eq!(
        (body.absolute_after().row(), body.absolute_after().column()),
        (1, 9)
    );
    assert_eq!(
        body.root_source(),
        TeletextSpatialRootSource::Local { command: 1 }
    );
    assert!(plan.provenance().iter().any(|source| matches!(
        source,
        TeletextMainCompositionSource::BaseCell {
            kind: TeletextBaseCellCommandKind::Glyph,
            ..
        }
    )));
    assert_eq!(
        plan.base_display()
            .cell(1, 9)
            .unwrap()
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: '£' }
    );
    assert_eq!(service, before);
}

#[test]
fn spatial_rejection_keeps_nested_original_source_and_invocation_identity() {
    let target = local(43);
    let mut fixture = Fixture::new(
        vec![],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Adaptive),
        ]),
    );
    fixture.objects = vec![definition(
        target,
        TeletextObjectType::Adaptive,
        vec![position(0, 2), g0(1)],
    )];
    let service = fixture.build();
    let before = service.clone();
    let error = compose_teletext_main_program(&service, owner(), budget()).unwrap_err();
    assert_eq!(error.owner, owner());
    assert_eq!(
        error.location,
        Some(TeletextMainCompositionSource::Object {
            identity: target,
            command: 1,
        })
    );
    let TeletextMainCompositionFailure::SpatialTrace(spatial) = error.reason else {
        panic!("expected original spatial rejection");
    };
    assert_eq!(
        spatial.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 1,
        })
    );
    assert_eq!(
        spatial.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(spatial.invocation.unwrap().get(), 0);
    assert_eq!(
        spatial.reason,
        TeletextSpatialTraceFailure::ActivePositionOrder {
            previous: (0, 2),
            requested: (0, 1),
        }
    );
    assert_eq!(service, before);
}

#[test]
fn original_active_glyph_outranks_base_unicode_without_promoting_derived_words() {
    let target = local(44);
    let mut fixture = Fixture::new(
        vec![text(1, 9, "é")],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture.objects = vec![definition(target, TeletextObjectType::Active, vec![g0(1)])];
    let service = fixture.build();
    let before = service.clone();
    let plan = compose_teletext_main_program(&service, owner(), budget()).unwrap();
    let layers = plan.glyph_layers();
    assert_eq!(layers.owner(), owner());
    let contenders = layers.candidates_at(1, 9);
    assert_eq!(contenders.len(), 2);
    assert_eq!(contenders[0].priority(), TeletextGlyphPriority::Base);
    let base = contenders[0].base().unwrap();
    assert_eq!(base.code(), b'e');
    assert_eq!(
        base.authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(base, *plan.base_display().cell(1, 9).unwrap());
    let active = layers.highest_priority_candidate(1, 9).unwrap();
    assert_eq!(active.priority(), TeletextGlyphPriority::Active);
    assert_eq!(
        active.operation(),
        Some(TeletextColumnOperation::G0 { code: code(b'A') })
    );
    assert_eq!(
        active.source(),
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 0,
        })
    );
    assert_eq!(active.invocation().unwrap().get(), 0);
    assert_eq!(
        active.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert!(plan.provenance().iter().any(|source| matches!(
        source,
        TeletextMainCompositionSource::BaseCell {
            kind: TeletextBaseCellCommandKind::Glyph,
            ..
        }
    )));
    assert_eq!(
        layers
            .candidates()
            .iter()
            .filter(|candidate| candidate.operation().is_some())
            .count(),
        1
    );
    // This is the semantic intent oracle. Charset/attributes/footprints and the
    // two physical receiver views are separate required acceptance evidence.
    assert_eq!(service, before);
}

#[test]
fn original_local_candidate_outranks_active_while_all_base_intent_survives() {
    let target = local(45);
    let mut fixture = Fixture::new(
        vec![text(1, 9, "é")],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
            g2(9),
        ]),
    );
    fixture.objects = vec![definition(target, TeletextObjectType::Active, vec![g0(1)])];
    let service = fixture.build();
    let before = service.clone();
    let plan = compose_teletext_main_program(&service, owner(), budget()).unwrap();
    let layers = plan.glyph_layers();
    let contenders = layers.candidates_at(1, 9);
    assert_eq!(
        contenders
            .iter()
            .map(TeletextGlyphCandidate::priority)
            .collect::<Vec<_>>(),
        vec![
            TeletextGlyphPriority::Base,
            TeletextGlyphPriority::Active,
            TeletextGlyphPriority::Local,
        ]
    );
    let base = contenders[0].base().unwrap();
    assert_eq!(
        base.authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    let local = layers.highest_priority_candidate(1, 9).unwrap();
    assert_eq!(
        local.operation(),
        Some(TeletextColumnOperation::G2 { code: code(0x54) })
    );
    assert_eq!(local.invocation(), None);
    assert_eq!(
        local.source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 2,
        })
    );
    assert_eq!(
        local.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 2 })
    );
    assert_eq!(service, before);
}

fn coloured_run(column: u8, colour: TeletextColour, value: &str) -> TeletextRun {
    TeletextRun {
        column,
        style: TeletextStyle {
            foreground: colour,
            ..TeletextStyle::default()
        },
        content: TeletextRunContent::Text { text: value.into() },
    }
}

fn original_foreground(column: u8, colour: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(colour).unwrap(),
        },
    )
}

#[test]
fn actual_layout_preserves_distinct_active_adaptive_and_passive_colour_inheritance() {
    let expected = [
        (TeletextObjectType::Active, [4, 4, 13, 13, 1, 3, 3]),
        (TeletextObjectType::Adaptive, [4, 4, 13, 13, 13, 3, 2]),
        (TeletextObjectType::Passive, [4, 7, 13, 4, 13, 3, 2]),
    ];
    for (object_type, colours) in expected {
        let target = local(46);
        let mut fixture = Fixture::new(
            vec![TeletextRow {
                row: 1,
                runs: vec![
                    coloured_run(1, TeletextColour::Blue, "THE FAST DOG AND"),
                    coloured_run(18, TeletextColour::Red, "CAT"),
                    coloured_run(22, TeletextColour::Green, "RAN IN"),
                ],
            }],
            Some(vec![position(1, 0), invoke(target, object_type)]),
        );
        fixture.objects = vec![definition(
            target,
            object_type,
            vec![
                at(5, TeletextColumnOperation::G0 { code: code(b'L') }),
                at(6, TeletextColumnOperation::G0 { code: code(b'A') }),
                at(7, TeletextColumnOperation::G0 { code: code(b'Z') }),
                at(8, TeletextColumnOperation::G0 { code: code(b'Y') }),
                original_foreground(10, 13),
                at(10, TeletextColumnOperation::G0 { code: code(b'C') }),
                at(11, TeletextColumnOperation::G0 { code: code(b'O') }),
                at(12, TeletextColumnOperation::G0 { code: code(b'W') }),
                at(18, TeletextColumnOperation::G0 { code: code(b'F') }),
                at(19, TeletextColumnOperation::G0 { code: code(b'O') }),
                at(20, TeletextColumnOperation::G0 { code: code(b'X') }),
                original_foreground(23, 3),
                at(23, TeletextColumnOperation::G0 { code: code(b'S') }),
                at(24, TeletextColumnOperation::G0 { code: code(b'A') }),
                at(25, TeletextColumnOperation::G0 { code: code(b'T') }),
            ],
        )];
        let service = fixture.build();
        let before = service.clone();
        let plan = compose(&service);
        let base = plan.base_display();
        assert_eq!(base.cell(1, 0).unwrap().code(), 0x04);
        assert_eq!(base.cell(1, 17).unwrap().code(), 0x01);
        assert_eq!(base.cell(1, 21).unwrap().code(), 0x02);
        let attributes = plan.attribute_execution();
        assert_eq!(attributes.owner(), owner());
        for (column, colour) in [3, 5, 10, 14, 18, 23, 27].into_iter().zip(colours) {
            assert_eq!(
                attributes.cell(1, column).unwrap().foreground().value(),
                Some(TeletextColourReference::new(colour).unwrap()),
                "{object_type:?} at column {column}"
            );
        }
        assert_eq!(service, before);
    }
}

#[test]
fn original_attribute_only_scope_keeps_base_spacing_and_unicode_character_rank_separate() {
    let target = local(47);
    let mut fixture = Fixture::new(
        vec![TeletextRow {
            row: 1,
            runs: vec![coloured_run(18, TeletextColour::Red, "é")],
        }],
        Some(vec![
            position(1, 0),
            original_foreground(10, 13),
            position(1, 18),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture.objects = vec![definition(target, TeletextObjectType::Active, vec![g0(0)])];
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let attributes = plan.attribute_execution();
    assert_eq!(
        attributes.cell(1, 14).unwrap().foreground().value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert_eq!(plan.base_display().cell(1, 17).unwrap().code(), 0x01);
    assert_eq!(
        attributes.cell(1, 18).unwrap().foreground().value(),
        Some(TeletextColourReference::new(1).unwrap())
    );
    let contenders = plan.glyph_layers().candidates_at(1, 18);
    assert_eq!(contenders.len(), 2);
    assert_eq!(
        contenders[0].base().unwrap().authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(contenders[1].priority(), TeletextGlyphPriority::Active);
    assert_eq!(service, before);
}

#[test]
fn original_row_colour_initializes_independent_panel_defaults_without_main_attribute_spill() {
    let target = local(48);
    let mut fixture = Fixture::new(
        vec![],
        Some(vec![
            TeletextEnhancementCommand::SetRowColour {
                row: 1,
                colour: TeletextColourReference::new(20).unwrap(),
                following_rows: true,
            },
            invoke(target, TeletextObjectType::Passive),
            original_foreground(10, 9),
        ]),
    );
    let mut presentation =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    presentation.remapping = TeletextColourRemapping::FgClut2BgClut3;
    presentation.black_background_substitution = false;
    fixture.presentation = Some(presentation);
    fixture.objects = vec![definition(target, TeletextObjectType::Passive, vec![g0(1)])];
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let attributes = plan.attribute_execution();
    assert_eq!(
        plan.base_display()
            .cell(1, 5)
            .unwrap()
            .attributes()
            .foreground()
            .get(),
        23
    );
    assert_eq!(
        plan.base_display()
            .cell(1, 5)
            .unwrap()
            .attributes()
            .background()
            .get(),
        24
    );
    assert_eq!(
        attributes.cell(1, 1).unwrap().foreground().value(),
        Some(TeletextColourReference::WHITE)
    );
    assert_eq!(
        attributes.cell(1, 1).unwrap().background().value(),
        Some(TeletextColourReference::BLACK)
    );
    assert_eq!(
        attributes.cell(1, 39).unwrap().foreground().value(),
        Some(TeletextColourReference::new(9).unwrap())
    );
    for (row, column) in [(1, 40), (1, 55), (1, 56), (1, 71), (24, 71)] {
        let cell = attributes.cell(row, column).unwrap();
        assert_eq!(
            cell.foreground().value(),
            Some(TeletextColourReference::new(7).unwrap())
        );
        assert_eq!(
            cell.background().value(),
            Some(TeletextColourReference::new(20).unwrap())
        );
    }
    assert!(attributes.cell(0, 7).is_none());
    assert!(attributes.cell(0, 40).is_none());
    assert!(attributes.cell(0, 71).is_none());
    assert_eq!(service, before);
}

#[test]
fn same_service_retains_unsettled_transmission_ties_instead_of_promoting_event_order() {
    let service = Fixture::new(
        vec![text(1, 18, "é")],
        Some(vec![
            position(1, 18),
            original_foreground(18, 4),
            original_foreground(18, 13),
        ]),
    )
    .build();
    let before = service.clone();
    let plan = compose(&service);
    let attributes = plan.attribute_execution();
    let foreground = attributes.cell(1, 18).unwrap().foreground();
    assert_eq!(foreground.value(), None);
    assert_eq!(foreground.source(), None);
    let pending = attributes.pending()[foreground.pending().unwrap()];
    assert_eq!(
        pending.reason(),
        TeletextAttributePendingReason::TransmissionOrder
    );
    assert_eq!(
        pending.location(),
        TeletextEnhancementSource {
            object: None,
            command: 2
        }
    );
    assert_eq!(
        pending.root_source(),
        TeletextSpatialRootSource::Local { command: 2 }
    );
    assert_eq!(pending.invocation(), None);
    assert_eq!(
        plan.glyph_layers().candidates_at(1, 18)[0]
            .base()
            .unwrap()
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(service, before);
}

#[test]
fn new_background_keeps_raw_base_foreground_when_nonspacing_colour_changes_display_foreground() {
    let service = Fixture::new(
        vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle {
                    foreground: TeletextColour::Red,
                    background: TeletextColour::Blue,
                    boxed: true,
                    flash: true,
                    conceal: true,
                    ..TeletextStyle::default()
                },
                content: TeletextRunContent::Text { text: "A".into() },
            }],
        }],
        Some(vec![position(1, 2), original_foreground(2, 11)]),
    )
    .build();
    let before = service.clone();
    let plan = compose(&service);
    assert_eq!(plan.base_display().cell(1, 1).unwrap().code(), 0x04);
    assert_eq!(plan.base_display().cell(1, 2).unwrap().code(), 0x1d);
    assert_eq!(plan.base_display().cell(1, 3).unwrap().code(), 0x01);
    let attributes = plan.attribute_execution();
    for column in [2, 3] {
        let cell = attributes.cell(1, column).unwrap();
        assert_eq!(
            cell.foreground().value(),
            Some(TeletextColourReference::new(11).unwrap())
        );
        assert_eq!(
            cell.background().value(),
            Some(TeletextColourReference::new(4).unwrap())
        );
    }
    let final_cell = attributes.cell(1, 8).unwrap();
    assert_eq!(
        final_cell.foreground().value(),
        Some(TeletextColourReference::new(1).unwrap())
    );
    assert_eq!(
        final_cell.background().value(),
        Some(TeletextColourReference::new(4).unwrap())
    );
    assert_eq!(final_cell.boxed().value(), Some(true));
    assert_eq!(final_cell.conceal().value(), Some(true));
    assert_eq!(
        final_cell.flash().value().unwrap().mode(),
        TeletextFlashMode::Normal
    );
    assert!(matches!(
        final_cell.background().source(),
        Some(TeletextAttributeSource::Base {
            source: TeletextBaseAttributeSource::Spacing {
                position: origin,
                control: TeletextBaseSpacingControl::NewBackground,
                timing: TeletextSpacingTiming::SetAt,
            },
            ..
        }) if origin == pos(1, 2)
    ));
    assert_eq!(service, before);
}

#[test]
fn enhanced_g1_alphabet_preserves_unicode_base_replacement_before_original_mode_one() {
    let alpha = at(
        8,
        TeletextColumnOperation::G1Alpha {
            code: TeletextG1AlphaCode::new(0x40).unwrap(),
        },
    );
    let service = Fixture::new(vec![text(1, 8, "é")], Some(vec![position(1, 8), alpha])).build();
    let before = service.clone();
    let plan = compose(&service);
    assert_eq!(
        plan.program().commands(),
        &[position(1, 8), accent(8), alpha]
    );
    assert_eq!(originals(&plan), &[position(1, 8), alpha]);
    let candidates = plan.glyph_layers().candidates_at(1, 8);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].priority(), TeletextGlyphPriority::Base);
    assert_eq!(
        candidates[0].base().unwrap().authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(candidates[1].priority(), TeletextGlyphPriority::Local);
    assert_eq!(
        candidates[1].operation(),
        Some(TeletextColumnOperation::G1Alpha {
            code: TeletextG1AlphaCode::new(0x40).unwrap(),
        })
    );
    assert_eq!(
        candidates[1].source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(service, before);
}

#[test]
fn original_charset_context_does_not_promote_generated_base_designations() {
    let alpha = at(
        8,
        TeletextColumnOperation::G1Alpha {
            code: TeletextG1AlphaCode::new(0x40).unwrap(),
        },
    );
    let mut fixture = Fixture::new(vec![text(1, 8, "é")], Some(vec![position(1, 8), alpha]));
    fixture.primary = Some(0x37);
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let execution = plan.charset_execution();
    let primary = TeletextPageCharsetDesignation::new(0x37).unwrap();
    assert_eq!(execution.owner(), owner());
    assert_eq!(execution.primary(), primary);
    assert_eq!(execution.secondary(), None);
    assert_eq!(
        execution.national_option(),
        plan.base_display().national_option()
    );
    assert!(execution.declarations().is_empty());
    assert!(execution.pending().is_empty());
    assert_eq!(
        execution.records().len(),
        plan.glyph_layers().candidates().len()
    );
    for (record, candidate) in execution
        .records()
        .iter()
        .zip(plan.glyph_layers().candidates())
    {
        assert_eq!(record.candidate(), candidate);
    }
    let contenders: Vec<_> = execution
        .records()
        .iter()
        .filter(|record| record.candidate().position() == (1, 8))
        .collect();
    assert_eq!(contenders.len(), 2);
    assert_eq!(
        contenders[0].candidate().priority(),
        TeletextGlyphPriority::Base
    );
    assert_eq!(
        contenders[1].candidate().operation(),
        Some(TeletextColumnOperation::G1Alpha {
            code: TeletextG1AlphaCode::new(0x40).unwrap(),
        })
    );
    assert!(contenders[0].national_substitution());
    assert!(!contenders[1].national_substitution());
    for record in contenders {
        assert_eq!(
            record.selection().unwrap().value(),
            Some(TeletextCharsetSelection::PagePrimary(primary))
        );
    }
    assert!(plan.program().commands().contains(&charset(8, 0)));
    assert_eq!(originals(&plan), &[position(1, 8), alpha]);
    assert_eq!(service, before);
}

#[test]
fn same_service_charset_execution_uses_spatial_shared_state_for_active_glyphs() {
    let target = local(90);
    let mut fixture = Fixture::new(
        vec![text(1, 20, "Z")],
        Some(vec![
            position(1, 5),
            charset(5, 0x37),
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
            charset(10, 0x1f),
            g2(21),
        ]),
    );
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Active,
        vec![position(0, 12), g0(12)],
    ));
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let execution = plan.charset_execution();
    let enhanced: Vec<_> = execution
        .records()
        .iter()
        .filter(|record| {
            matches!(record.candidate().position(), (1, 20 | 21))
                && record.candidate().operation().is_some()
        })
        .collect();
    assert_eq!(enhanced.len(), 2);
    for record in enhanced {
        assert_eq!(
            record.selection().unwrap().value(),
            Some(TeletextCharsetSelection::Enhancement(
                TeletextEnhancementCharsetDesignation::new(0x1f).unwrap(),
            ))
        );
    }
    let active = execution
        .records()
        .iter()
        .find(|record| {
            record.candidate().position() == (1, 20)
                && record.candidate().priority() == TeletextGlyphPriority::Active
        })
        .unwrap();
    assert_eq!(
        active.candidate().source(),
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 1
        })
    );
    let writer = &execution.declarations()[active.selection().unwrap().declaration().unwrap()];
    assert_eq!(
        writer.source(),
        TeletextEnhancementSource {
            object: None,
            command: 4
        }
    );
    assert!(active.candidate().invocation().is_some());
    assert_eq!(service, before);
}

#[test]
fn glyph_intent_retains_exact_charset_candidates_without_promoting_generated_words() {
    let alpha = at(
        8,
        TeletextColumnOperation::G1Alpha {
            code: TeletextG1AlphaCode::new(0x40).unwrap(),
        },
    );
    let mut fixture = Fixture::new(vec![text(1, 8, "é")], Some(vec![position(1, 8), alpha]));
    fixture.primary = Some(0x37);
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let intent = plan.glyph_intent();
    assert_eq!(intent.owner(), owner());
    assert_eq!(
        intent.records().len(),
        plan.charset_execution().records().len()
    );
    for (record, context) in intent
        .records()
        .iter()
        .zip(plan.charset_execution().records())
    {
        assert_eq!(record.context(), context);
    }
    let contenders: Vec<_> = intent
        .records()
        .iter()
        .filter(|record| record.context().candidate().position() == (1, 8))
        .collect();
    assert_eq!(contenders.len(), 2);
    assert_eq!(
        contenders[0]
            .context()
            .candidate()
            .base()
            .unwrap()
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    for record in &contenders {
        assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
        let Some(TeletextGlyphIdentity::G0 { glyph }) = record.glyph() else {
            panic!("original Greek G0 identity must remain distinct from generated Latin words");
        };
        assert_eq!(glyph.repertoire(), TeletextG0Repertoire::Greek);
        assert_eq!(glyph.national_subset(), None);
    }
    assert!(plan.program().commands().contains(&charset(8, 0)));
    assert!(plan.charset_execution().declarations().is_empty());
    assert_eq!(service, before);
}

#[test]
fn glyph_intent_preserves_forbidden_and_unmarked_positions_without_mutating_programs() {
    let commands = vec![
        position(1, 1),
        at(1, TeletextColumnOperation::G2 { code: code(0x59) }),
        at(2, TeletextColumnOperation::G2 { code: code(0x40) }),
        at(
            3,
            TeletextColumnOperation::G3 {
                code: code(0x6e),
                compatibility: TeletextG3Compatibility::Level25,
            },
        ),
        at(
            4,
            TeletextColumnOperation::G0Diacritic {
                code: code(0x2a),
                diacritic: TeletextDiacriticIndex::new(0).unwrap(),
            },
        ),
    ];
    let service = Fixture::new(vec![], Some(commands.clone())).build();
    let before = service.clone();
    let plan = compose(&service);
    let records: Vec<_> = plan
        .glyph_intent()
        .records()
        .iter()
        .filter(|record| record.context().candidate().operation().is_some())
        .collect();
    assert_eq!(records.len(), 4);
    assert_eq!(
        records[0].disposition(),
        TeletextGlyphDisposition::ShouldNotTransmit
    );
    assert_eq!(
        records[0].reason(),
        Some(TeletextGlyphIntentReason::ProhibitedTablePosition)
    );
    assert!(
        matches!(records[0].glyph(), Some(TeletextGlyphIdentity::G2 { glyph }) if glyph.code() == 0x59)
    );
    assert_eq!(records[1].disposition(), TeletextGlyphDisposition::Pending);
    assert_eq!(
        records[1].reason(),
        Some(TeletextGlyphIntentReason::UnmarkedTableCell)
    );
    assert_eq!(
        records[2].disposition(),
        TeletextGlyphDisposition::ShouldNotTransmit
    );
    assert!(
        matches!(records[2].glyph(), Some(TeletextGlyphIdentity::G3 { code }) if code.get() == 0x6e)
    );
    assert_eq!(records[3].disposition(), TeletextGlyphDisposition::Assigned);
    assert_eq!(records[3].glyph(), Some(TeletextGlyphIdentity::AtSymbol));
    assert_eq!(originals(&plan), commands);
    assert_eq!(service, before);
}

#[test]
fn actual_base_layout_uses_full_national_alias_instead_of_low_three_bits() {
    let mut fixture = Fixture::new(vec![text(1, 1, "#")], None);
    fixture.primary = Some(0x08); // Table 32 Polish, although low three bits equal English.
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let record = plan
        .glyph_intent()
        .records()
        .iter()
        .find(|record| record.context().candidate().position() == (1, 1))
        .unwrap();
    let Some(TeletextGlyphIdentity::G0 { glyph }) = record.glyph() else {
        panic!("coded Base G0");
    };
    assert_eq!(glyph.code(), 0x23);
    assert_eq!(glyph.repertoire(), TeletextG0Repertoire::Latin);
    assert_eq!(
        glyph.national_subset(),
        Some(TeletextNationalSubset::Polish)
    );
    assert_eq!(
        record
            .context()
            .candidate()
            .base()
            .unwrap()
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: '#' }
    );
    assert_eq!(service, before);
}

fn drcs_fixture(normal_bitmap: [u16; 10]) -> Fixture {
    let target = local(0xffff);
    let normal = TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Normal,
        subtable: TeletextDrcsSubtableIndex::new(15).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
    };
    let global = TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Global,
        subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(3).unwrap(),
    };
    let normal_page = TeletextRawDataPageNumber::new(2, 0xa0).unwrap();
    let global_page = TeletextRawDataPageNumber::new(2, 0xa1).unwrap();
    let mut fixture = Fixture::new(
        vec![text(1, 8, "T")],
        Some(vec![
            position(1, 8),
            at(8, TeletextColumnOperation::Drcs { reference: normal }),
            invoke(target, TeletextObjectType::Passive),
            invoke(target, TeletextObjectType::Passive),
        ]),
    );
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Passive,
        vec![
            position(0, 0),
            at(0, TeletextColumnOperation::Drcs { reference: global }),
        ],
    ));
    fixture.drcs_links = TeletextDrcsPageLinks::new(Some(normal_page), Some(global_page)).unwrap();
    for (kind, page, reference, bitmap) in [
        (
            TeletextResourceKind::Drcs,
            normal_page,
            normal,
            normal_bitmap,
        ),
        (TeletextResourceKind::Gdrcs, global_page, global, [1; 10]),
    ] {
        fixture.drcs.push(
            TeletextDrcsSubtable::new(
                TeletextResourceIdentity::new(
                    kind,
                    page,
                    TeletextResourceSubpageIndex::new(reference.subtable.get()).unwrap(),
                )
                .unwrap(),
                vec![TeletextDrcsGlyphDefinition {
                    slot: reference.slot,
                    bitmap: TeletextDrcsBitmap::new(bitmap).unwrap(),
                }],
            )
            .unwrap(),
        );
    }
    fixture
}

fn compose_drcs(service: &TeletextSemanticService) -> TeletextMainComposition {
    let mut limits = budget();
    limits.collection.max_drcs_references = 3;
    compose_teletext_main_program(service, owner(), limits).unwrap()
}

#[test]
fn same_service_drcs_execution_retains_losing_original_instances_and_exact_bitmaps() {
    let pixels = [0x800, 0x400, 0x200, 0x100, 0x80, 0x40, 0x20, 0x10, 0x8, 0x1];
    let service = drcs_fixture(pixels).build();
    let before = service.clone();
    let plan = compose_drcs(&service);
    let execution = plan.drcs_execution();
    assert_eq!(execution.owner(), owner());
    let records = execution.records();
    assert_eq!(records.len(), 3);
    for record in records {
        assert_eq!(
            record.candidate(),
            plan.glyph_intent().records()[record.candidate_index()]
                .context()
                .candidate()
        );
        assert_eq!(record.candidate().position(), (1, 8));
        assert!(
            matches!(plan.glyph_intent().records()[record.candidate_index()].glyph(),
            Some(TeletextGlyphIdentity::Drcs { reference }) if reference == record.reference())
        );
    }
    assert_eq!(records[0].resource().kind(), TeletextResourceKind::Drcs);
    assert_eq!(records[0].resource().logical_subpage().get(), 15);
    assert_eq!(records[0].bitmap().get(), pixels);
    assert_eq!(
        records[0].candidate().source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    for record in &records[1..] {
        assert_eq!(record.resource().kind(), TeletextResourceKind::Gdrcs);
        assert_eq!(record.bitmap().get(), [1; 10]);
        assert_eq!(
            record.candidate().source(),
            Some(TeletextEnhancementSource {
                object: Some(local(0xffff)),
                command: 1
            })
        );
    }
    assert_ne!(
        records[1].candidate().invocation(),
        records[2].candidate().invocation()
    );
    assert_eq!(service, before);
}

#[test]
fn drcs_execution_is_bound_to_current_immutable_service_even_when_ids_match() {
    let first = drcs_fixture([0x800; 10]).build();
    let second = drcs_fixture([0; 10]).build();
    let plan = compose_drcs(&first);
    let changed = compose_drcs(&second);
    let a = &plan.drcs_execution().records()[0];
    let b = &changed.drcs_execution().records()[0];
    assert_eq!(a.resource(), b.resource());
    assert_eq!(a.reference(), b.reference());
    assert_eq!(a.candidate(), b.candidate());
    assert_eq!(a.bitmap().get(), [0x800; 10]);
    assert_eq!(b.bitmap().get(), [0; 10]);
    assert_eq!(
        plan.drcs_execution().records()[0].bitmap().get(),
        [0x800; 10]
    );
    assert_ne!(plan, changed);
}

fn split_panel_fixture(main: Vec<TeletextEnhancementCommand>) -> Fixture {
    let mut fixture = Fixture::new(vec![text(1, 4, "é")], Some(main));
    let mut presentation =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    presentation.panels = TeletextSidePanels {
        left: true,
        right: true,
        status: TeletextPanelStatus::Level25And35,
        width: TeletextPanelWidth::new(4).unwrap(),
    };
    fixture.presentation = Some(presentation);
    fixture
}

#[test]
fn selected_panels_place_original_candidates_without_promoting_generated_base_words() {
    let target = local(0xffff);
    let mut fixture = split_panel_fixture(vec![
        position(1, 4),
        g0(4),
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 36).unwrap()),
        },
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 64).unwrap()),
        },
    ]);
    fixture
        .objects
        .push(definition(target, TeletextObjectType::Passive, vec![g0(0)]));
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let geometry = plan.display_geometry();
    assert_eq!(geometry.owner(), owner());
    assert_eq!(
        (
            geometry.left_width(),
            geometry.right_width(),
            geometry.total_width()
        ),
        (4, 12, 56)
    );
    assert_eq!(
        geometry.panels(),
        plan.base_display().presentation().map(|value| value.panels)
    );
    assert_eq!(
        geometry.records().len(),
        plan.glyph_layers().candidates().len()
    );
    for (index, record) in geometry.records().iter().enumerate() {
        assert_eq!(record.candidate_index(), index);
        assert_eq!(record.candidate(), &plan.glyph_layers().candidates()[index]);
    }
    for (raw, visual, area) in [
        (4, 8, TeletextRawDisplayArea::Main),
        (40, 44, TeletextRawDisplayArea::RightPanel),
        (68, 0, TeletextRawDisplayArea::LeftPanel),
    ] {
        let record = geometry
            .records()
            .iter()
            .find(|record| {
                record.candidate().position() == (1, raw)
                    && record.candidate().operation().is_some()
            })
            .unwrap();
        assert_eq!(record.area(), area);
        assert_eq!(
            record.placement(),
            TeletextCandidatePlacement::Placed {
                row: 1,
                column: visual
            }
        );
    }
    let contenders: Vec<_> = geometry
        .records()
        .iter()
        .filter(|record| record.candidate().position() == (1, 4))
        .collect();
    assert_eq!(contenders.len(), 2);
    assert_eq!(
        contenders[0]
            .candidate()
            .base()
            .unwrap()
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(
        contenders[1].candidate().source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(service, before);
}

#[test]
fn display_geometry_keeps_header_gap_and_wide_object_candidates_explicitly_unplaced() {
    let target = local(0xffff);
    let wide = local(0xfffe);
    let mut fixture = split_panel_fixture(vec![
        position(0, 39),
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 2).unwrap()),
        },
        position(1, 39),
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 13).unwrap()),
        },
        TeletextEnhancementCommand::InvokeObject {
            target: wide,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 45).unwrap()),
        },
    ]);
    fixture
        .objects
        .push(definition(target, TeletextObjectType::Passive, vec![g0(0)]));
    fixture.objects.push(definition(
        wide,
        TeletextObjectType::Passive,
        vec![position(0, 39), g0(39)],
    ));
    let service = fixture.build();
    let before = service.clone();
    let plan = compose(&service);
    let records = plan.display_geometry().records();
    for (position, reason) in [
        ((0, 41), TeletextCandidateNonplacement::HeaderPanel),
        ((1, 52), TeletextCandidateNonplacement::OutsideActiveWidth),
        ((1, 123), TeletextCandidateNonplacement::OutsideDisplay),
    ] {
        let record = records
            .iter()
            .find(|record| record.candidate().position() == position)
            .unwrap();
        assert_eq!(
            record.placement(),
            TeletextCandidatePlacement::Unplaced(reason)
        );
    }
    let object = records
        .iter()
        .find(|record| record.candidate().position() == (1, 123))
        .unwrap();
    assert_eq!(
        object.candidate().source(),
        Some(TeletextEnhancementSource {
            object: Some(wide),
            command: 1
        })
    );
    assert_eq!(object.candidate().invocation().unwrap().get(), 2);
    assert_eq!(
        object.candidate().root_source(),
        Some(TeletextSpatialRootSource::Local { command: 4 })
    );
    assert_eq!(service, before);
}

#[test]
fn original_object_cannot_write_across_panels_but_may_invoke_a_separate_panel_child() {
    let parent = local(30);
    let child = local(31);
    let mut fixture = split_panel_fixture(vec![
        position(1, 39),
        invoke(parent, TeletextObjectType::Active),
    ]);
    fixture.objects.push(definition(
        parent,
        TeletextObjectType::Active,
        vec![position(0, 0), g0(0), g0(1)],
    ));
    let invalid = fixture.clone().build();
    let before = invalid.clone();
    let rejected = compose_teletext_main_program(&invalid, owner(), budget()).unwrap_err();
    assert_eq!(
        rejected.location,
        Some(TeletextMainCompositionSource::Object {
            identity: parent,
            command: 2
        })
    );
    let TeletextMainCompositionFailure::DisplayGeometry(error) = rejected.reason else {
        panic!("located geometry error");
    };
    assert_eq!(error.position, Some((1, 40)));
    assert_eq!(error.invocation.unwrap().get(), 0);
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::CrossAreaWrite {
            previous: TeletextRawDisplayArea::Main,
            requested: TeletextRawDisplayArea::RightPanel,
        }
    );
    assert_eq!(invalid, before);
    fixture.objects[0].program = program(vec![
        position(0, 0),
        g0(0),
        TeletextEnhancementCommand::InvokeObject {
            target: child,
            object_type: TeletextObjectType::Passive,
            offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
        },
    ]);
    fixture.objects.push(definition(
        child,
        TeletextObjectType::Passive,
        vec![position(0, 0), g0(0)],
    ));
    let valid = fixture.build();
    let before = valid.clone();
    let plan = compose(&valid);
    for (source, raw, visual, invocation) in [(parent, 39, 43, 0), (child, 40, 44, 1)] {
        let record = plan
            .display_geometry()
            .records()
            .iter()
            .find(|record| {
                record.candidate().source()
                    == Some(TeletextEnhancementSource {
                        object: Some(source),
                        command: 1,
                    })
            })
            .unwrap();
        assert_eq!(record.candidate().position(), (1, raw));
        assert_eq!(record.candidate().invocation().unwrap().get(), invocation);
        assert_eq!(
            record.placement(),
            TeletextCandidatePlacement::Placed {
                row: 1,
                column: visual
            }
        );
    }
    assert_eq!(valid, before);
}

fn simultaneous_object_fixture(count: u8) -> Fixture {
    let types = [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ];
    let commands = (0..count)
        .map(|column| {
            let index = usize::from(column.min(14) / 5);
            TeletextEnhancementCommand::InvokeObject {
                target: local(u16::try_from(index).unwrap()),
                object_type: types[index],
                offset: Some(TeletextObjectOffset::new(1, column).unwrap()),
            }
        })
        .collect();
    let mut fixture = Fixture::new(vec![text(1, 8, "é")], Some(commands));
    for (index, object_type) in types.into_iter().enumerate() {
        fixture.objects.push(definition(
            local(u16::try_from(index).unwrap()),
            object_type,
            vec![g0(0)],
        ));
    }
    fixture
}

#[test]
fn original_geometry_counts_all_object_types_and_rejects_sixteenth_with_call_provenance() {
    let valid = simultaneous_object_fixture(15).build();
    let before = valid.clone();
    let plan = compose(&valid);
    let count = plan.display_geometry().object_open_count();
    assert_eq!(count.known_intervals(), 15);
    assert_eq!(count.pending_intervals(), 0);
    assert_eq!(count.maximum_known_open(), 15);
    assert_eq!(plan.spatial_trace().invocations().len(), 15);
    for index in 0..15 {
        assert!(plan.display_geometry().records().iter().any(|record| {
            record.candidate().position() == (1, index) && record.candidate().operation().is_some()
        }));
    }
    assert_eq!(valid, before);
    let invalid = simultaneous_object_fixture(16).build();
    let before = invalid.clone();
    let rejected = compose_teletext_main_program(&invalid, owner(), budget()).unwrap_err();
    assert_eq!(
        rejected.location,
        Some(TeletextMainCompositionSource::OriginalPage { command: 15 })
    );
    let TeletextMainCompositionFailure::DisplayGeometry(error) = rejected.reason else {
        panic!("located simultaneous-open rejection");
    };
    assert_eq!(error.position, Some((0, 0)));
    assert_eq!(error.invocation.unwrap().get(), 15);
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 15
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 15 })
    );
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::TooManyOpenObjects {
            count: 16,
            maximum: 15
        }
    );
    assert_eq!(invalid, before);
}

fn colour(column: u8, value: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    )
}
fn observation_fixture() -> Fixture {
    let first = local(0xffff);
    let second = local(0x0100);
    let adaptive = local(0);
    let mut fixture = Fixture::new(
        vec![text(1, 8, "é")],
        Some(vec![
            position(1, 7),
            colour(7, 2),
            position(1, 8),
            invoke(adaptive, TeletextObjectType::Adaptive),
            invoke(first, TeletextObjectType::Passive),
            invoke(second, TeletextObjectType::Passive),
        ]),
    );
    fixture.objects.push(definition(
        adaptive,
        TeletextObjectType::Adaptive,
        vec![colour(0, 3), g0(0)],
    ));
    fixture.objects.push(definition(
        first,
        TeletextObjectType::Passive,
        vec![
            colour(0, 1),
            at(
                0,
                TeletextColumnOperation::Attributes {
                    double_height: true,
                    double_width: true,
                    boxed: false,
                    conceal: false,
                    invert: false,
                    underline_or_separated_mosaics: false,
                },
            ),
            g0(0),
        ],
    ));
    fixture.objects.push(definition(
        second,
        TeletextObjectType::Passive,
        vec![colour(0, 4), g0(0)],
    ));
    fixture
}

#[test]
fn composition_retains_losing_private_attributes_and_shared_execution_stages() {
    let service = observation_fixture().build();
    let before = service.clone();
    let plan = compose(&service);
    let execution = plan.attribute_execution();
    assert_eq!(
        execution.records().len(),
        plan.glyph_layers().candidates().len()
    );
    for (index, record) in execution.records().iter().enumerate() {
        assert_eq!(record.candidate_index(), index);
        assert_eq!(record.candidate(), &plan.glyph_layers().candidates()[index]);
    }
    assert_eq!(
        execution
            .before_adaptive_cell(1, 8)
            .unwrap()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(2).unwrap())
    );
    assert_eq!(
        execution
            .before_passive_cell(1, 8)
            .unwrap()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(3).unwrap())
    );
    assert_eq!(
        execution.cell(1, 8).unwrap().foreground().value(),
        Some(TeletextColourReference::new(4).unwrap())
    );
    let first = execution
        .records()
        .iter()
        .find(|record| {
            record.candidate().source()
                == Some(TeletextEnhancementSource {
                    object: Some(local(0xffff)),
                    command: 2,
                })
        })
        .unwrap();
    let attributes = first.attributes().unwrap();
    assert_eq!(
        attributes.foreground().value(),
        Some(TeletextColourReference::new(1).unwrap())
    );
    assert_eq!(attributes.double_height().value(), Some(true));
    assert_eq!(attributes.double_width().value(), Some(true));
    assert_eq!(first.candidate().invocation().unwrap().get(), 1);
    assert_eq!(first.reason(), None);
    let base = execution
        .records()
        .iter()
        .find(|record| {
            record.candidate().position() == (1, 8) && record.candidate().base().is_some()
        })
        .unwrap();
    assert_eq!(
        base.attributes().unwrap().foreground().value(),
        Some(TeletextColourReference::new(3).unwrap())
    );
    assert_eq!(
        base.candidate()
            .base()
            .unwrap()
            .authored()
            .unwrap()
            .content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(service, before);
}

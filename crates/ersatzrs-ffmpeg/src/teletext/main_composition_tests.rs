//! Semantic merge proofs; these do not stand in for final packet/render proof.

use ersatzrs_domain::*;

use super::main_composition::*;

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
}
impl Fixture {
    fn new(rows: Vec<TeletextRow>, main: Option<Vec<TeletextEnhancementCommand>>) -> Self {
        Self {
            rows,
            main,
            objects: vec![],
            defaults: [None; 4],
            primary: None,
        }
    }
    fn build(self) -> TeletextSemanticService {
        let language = TeletextLanguage::new("nld".into()).unwrap();
        let presentation = self.primary.map(|primary| {
            let mut result = super::presentation::western_latin_v1_presentation(
                TeletextNationalOption::new(1).unwrap(),
            )
            .unwrap();
            result.primary = TeletextPageCharsetDesignation::new(primary).unwrap();
            result
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
                        drcs: TeletextDrcsPageLinks::unlinked(),
                        defaults: self.defaults,
                    },
                }],
                magazines: vec![],
                objects: self.objects,
                drcs: vec![],
            },
            TeletextServiceBudget {
                max_records: 64,
                max_objects: 32,
                max_structure_items: 4096,
                max_base_content_bytes: 8192,
                max_commands: 8192,
                max_collected_commands: 16384,
                max_collected_drcs_references: 0,
            },
        )
        .unwrap()
    }
}
fn compose(service: &TeletextSemanticService) -> TeletextMainComposition {
    compose_teletext_main_program(service, owner(), budget()).unwrap()
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

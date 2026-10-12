//! Actual supplied-program packing and bounded same-instance C.2 evidence.

use super::*;
use crate::teletext::compose_teletext_main_program;
use ersatzrs_domain::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn foreground(column: u8, value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    }
}
fn glyph(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    }
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}
fn public(page: u8, id: u16) -> TeletextObjectIdentity {
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
fn collection() -> TeletextEnhancementCollectionBudget {
    TeletextEnhancementCollectionBudget {
        max_commands: 8192,
        max_drcs_references: 0,
    }
}
fn packing() -> TeletextObjectCatalogBudget {
    TeletextObjectCatalogBudget {
        max_objects: 16,
        max_commands: 8192,
    }
}
fn service(
    main: Option<Vec<TeletextEnhancementCommand>>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    selected: Option<TeletextRawDataPageNumber>,
    defaults: [Option<TeletextDefaultObjectSelection>; 4],
    unicode: bool,
) -> TeletextSemanticService {
    let mut page = crate::teletext::transport_tests::draft(200, 0x1234, false);
    page.rows.clear();
    page.national_option = TeletextNationalOption::new(1).unwrap();
    if unicode {
        page.rows.push(TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "é".into() },
            }],
        });
    }
    let language = page.language.clone();
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language,
                initial_page: owner().page,
                pages: vec![page],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation: None,
                compositional_links: None,
                local_program: main.map(program),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: selected,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults,
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
            max_base_content_bytes: 1024,
            max_commands: 8192,
            max_collected_commands: 8192,
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
            max_input_commands: 8192,
            max_output_commands: 8192,
            collection: collection(),
        },
    )
    .unwrap()
}
fn compile(
    service: &TeletextSemanticService,
    composition: &TeletextMainComposition,
    actual: &TeletextEnhancementProgram,
) -> EncodedTeletextPhysicalDisplayProgram {
    let allocations =
        crate::teletext::allocate_teletext_resource_objects(service.objects(), &[], 16).unwrap();
    compile_teletext_physical_display_program(
        service,
        composition,
        actual,
        service.objects(),
        &allocations,
        collection(),
        packing(),
    )
    .unwrap()
}
fn assert_pink(result: &EncodedTeletextPhysicalDisplayProgram, row: u8, column: u8) {
    let replay = result.replay();
    let field = replay
        .attribute_execution()
        .cell(row, column)
        .unwrap()
        .foreground();
    assert_eq!(
        field.value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert_eq!(field.pending(), None);
    let field = replay
        .display_resolution()
        .cell(row, column)
        .unwrap()
        .attributes()
        .foreground();
    assert_eq!(
        field.value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert_eq!(field.pending(), None);
    let pixel = replay
        .colour_composition()
        .pixel(
            replay.display_resolution(),
            row,
            column,
            true,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
        )
        .unwrap();
    assert!(matches!(pixel, TeletextColourPixel::Rgb(rgb) if rgb.get() == [7, 0, 7]));
}

#[test]
fn actual_shifted_main_and_generated_base_sources_bind_to_final_words() {
    let service = service(
        Some(vec![
            position(1, 8),
            foreground(8, 4),
            foreground(8, 13),
            glyph(8),
        ]),
        vec![],
        None,
        [None; 4],
        true,
    );
    let original = compose(&service);
    assert!(original
        .attribute_execution()
        .cell(1, 8)
        .unwrap()
        .foreground()
        .pending()
        .is_some());
    assert!(original
        .provenance()
        .iter()
        .any(|source| matches!(source, TeletextMainCompositionSource::BaseCell { .. })));
    let mut commands = vec![TeletextEnhancementCommand::SetScreenColour {
        colour: TeletextColourReference::BLACK,
    }];
    commands.extend_from_slice(original.program().commands());
    let actual = program(commands);
    let retained = (service.clone(), original.clone(), actual.clone());
    let result = compile(&service, &original, &actual);
    assert_pink(&result, 1, 8);
    let replay = result.replay();
    let evidence = replay
        .attribute_execution()
        .transmission_evidence()
        .unwrap();
    assert!(evidence.matches(replay.spatial_trace()));
    assert_eq!(
        replay.spatial_trace().events().len(),
        actual.commands().len()
    );
    for (ordinal, command) in actual.commands().iter().enumerate() {
        assert_eq!(replay.spatial_trace().events()[ordinal].command(), *command);
        assert_eq!(
            replay.spatial_trace().events()[ordinal].source().command,
            ordinal
        );
    }
    let accent = result
        .encoded()
        .command_positions()
        .iter()
        .find(|position| {
            matches!(
                actual.commands()[position.source().command],
                TeletextEnhancementCommand::AtColumn {
                    operation: TeletextColumnOperation::G0Diacritic { .. },
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(accent.source().object, None);
    assert!(accent.source().command > 2); // actual inserted command, not original main ordinal
    assert_eq!((service, original, actual), retained);
}

#[test]
fn final_compatibility_restore_and_local_definition_words_settle_own_ties() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            position(1, 0),
            invoke(target, TeletextObjectType::Adaptive),
            TeletextEnhancementCommand::AtColumn {
                column: 9,
                operation: TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(0x23).unwrap(),
                },
            },
        ]),
        vec![definition(
            target,
            TeletextObjectType::Adaptive,
            vec![foreground(5, 4), foreground(5, 13), glyph(5)],
        )],
        None,
        [None; 4],
        false,
    );
    let original = compose(&service);
    let result = compile(&service, &original, original.program());
    assert_pink(&result, 1, 5);
    let compatible = result
        .encoded()
        .command_positions()
        .iter()
        .find(|position| {
            position.source()
                == TeletextEnhancementSource {
                    object: None,
                    command: 2,
                }
        })
        .unwrap();
    assert_eq!(compatible.first_word(), 2);
    assert_eq!(compatible.semantic_word(), 3); // row restoration precedes the semantic glyph
    let words: Vec<_> = result
        .encoded()
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(target))
        .collect();
    let marker = usize::from(result.encoded().definition_positions()[0].absolute_index());
    assert_eq!(words[0].semantic_word(), marker + 1);
    assert_eq!(words[1].semantic_word(), marker + 2);
    let source = result
        .replay()
        .attribute_execution()
        .cell(1, 5)
        .unwrap()
        .foreground()
        .source()
        .unwrap();
    assert!(
        matches!(source, TeletextAttributeSource::Enhancement { location: TeletextEnhancementSource { object: Some(identity), command: 1 }, invocation: Some(id), .. } if identity == target && id.get() == 0)
    );
}

#[test]
fn different_instances_and_unmapped_resource_definitions_do_not_gain_order() {
    let local_target = local(0xffff);
    let shared = service(
        Some(vec![
            position(1, 18),
            foreground(18, 4),
            invoke(local_target, TeletextObjectType::Active),
            glyph(18),
        ]),
        vec![definition(
            local_target,
            TeletextObjectType::Active,
            vec![foreground(0, 13)],
        )],
        None,
        [None; 4],
        false,
    );
    let original = compose(&shared);
    let result = compile(&shared, &original, original.program());
    let field = result
        .replay()
        .attribute_execution()
        .cell(1, 18)
        .unwrap()
        .foreground();
    assert_eq!(field.value(), None);
    let pending = &result.replay().attribute_execution().pending()[field.pending().unwrap()];
    assert_eq!(
        result
            .replay()
            .attribute_execution()
            .transmission_evidence()
            .unwrap()
            .compare(
                pending.first_source().unwrap(),
                pending.second_source().unwrap()
            ),
        None
    );

    let resource = public(0xa0, 0xffff);
    let selected = resource.resource().unwrap().page();
    let shared = service(
        Some(vec![
            position(1, 0),
            invoke(resource, TeletextObjectType::Adaptive),
        ]),
        vec![definition(
            resource,
            TeletextObjectType::Adaptive,
            vec![foreground(5, 4), foreground(5, 13), glyph(5)],
        )],
        Some(selected),
        [None; 4],
        false,
    );
    let original = compose(&shared);
    let result = compile(&shared, &original, original.program());
    assert!(result
        .encoded()
        .command_positions()
        .iter()
        .all(|position| position.source().object.is_none()));
    assert!(result
        .replay()
        .attribute_execution()
        .cell(1, 5)
        .unwrap()
        .foreground()
        .pending()
        .is_some());
    assert!(result
        .replay()
        .display_resolution()
        .cell(1, 5)
        .unwrap()
        .attributes()
        .foreground()
        .pending()
        .is_some());
}

#[test]
fn actual_packing_keeps_default_owner_selection_protection() {
    let target = public(0xa0, 0xffff);
    let other = public(0xa1, 0x0100);
    let mut defaults = [None; 4];
    defaults[2] = Some(TeletextDefaultObjectSelection {
        target,
        object_type: TeletextObjectType::Passive,
    });
    let service = service(
        None,
        vec![
            definition(target, TeletextObjectType::Passive, vec![]),
            definition(other, TeletextObjectType::Passive, vec![]),
        ],
        Some(other.resource().unwrap().page()),
        defaults,
        false,
    );
    let original = compose(&service);
    let allocations =
        crate::teletext::allocate_teletext_resource_objects(service.objects(), &[], 16).unwrap();
    let error = compile_teletext_physical_display_program(
        &service,
        &original,
        original.program(),
        service.objects(),
        &allocations,
        collection(),
        packing(),
    )
    .unwrap_err();
    assert!(
        matches!(error, TeletextPhysicalDisplayEncodingError::Packing(TeletextLocalObjectProgramEncodingError { location: Some(TeletextEnhancementSource { object: None, command: 0 }), reason: TeletextLocalObjectProgramEncodingFailure::Lowering(super::super::TeletextLocalProgramEncodingFailure::ObjectSelection { target: actual, selected: Some(page) }), .. }) if actual == target && page == other.resource().unwrap().page())
    );
}

#[test]
fn actual_sequence_exact_capacity_and_cutoff_leave_original_inputs_unchanged() {
    let service = service(Some(vec![]), vec![], None, [None; 4], false);
    let original = compose(&service);
    let allocations =
        crate::teletext::allocate_teletext_resource_objects(service.objects(), &[], 16).unwrap();
    let screen = TeletextEnhancementCommand::SetScreenColour {
        colour: TeletextColourReference::BLACK,
    };
    let fitting = program(vec![screen; 207]);
    let oversized = program(vec![screen; 208]);
    let retained = (
        service.clone(),
        original.clone(),
        fitting.clone(),
        oversized.clone(),
        allocations.clone(),
    );
    let result = compile_teletext_physical_display_program(
        &service,
        &original,
        &fitting,
        service.objects(),
        &allocations,
        collection(),
        packing(),
    )
    .unwrap();
    assert_eq!(result.encoded().packet_bodies().len(), 16);
    assert_eq!(result.encoded().main_triplet_count(), 207);
    let error = compile_teletext_physical_display_program(
        &service,
        &original,
        &oversized,
        service.objects(),
        &allocations,
        collection(),
        packing(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        TeletextPhysicalDisplayEncodingError::Packing(TeletextLocalObjectProgramEncodingError {
            reason: TeletextLocalObjectProgramEncodingFailure::Capacity {
                count: 209,
                maximum: 208
            },
            ..
        })
    ));
    let error = compile_teletext_physical_display_program(
        &service,
        &original,
        &fitting,
        service.objects(),
        &allocations,
        TeletextEnhancementCollectionBudget {
            max_commands: 206,
            max_drcs_references: 0,
        },
        packing(),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        TeletextPhysicalDisplayEncodingError::PhysicalReplay(TeletextPhysicalDisplayReplayError {
            location: None,
            reason: TeletextMainCompositionFailure::InputBudget { maximum: 206 },
            ..
        })
    ));
    assert_eq!(
        (service, original, fitting, oversized, allocations),
        retained
    );
}

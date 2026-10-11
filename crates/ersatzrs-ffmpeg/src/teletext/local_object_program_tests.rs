//! Independent Table 28 local selectors, markers and shared X26 packing oracles.

use ersatzrs_domain::*;

use super::*;

fn owner(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}

fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}

fn catalog(
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 256,
            max_commands: 2_000,
        },
    )
    .unwrap()
}

fn local(owner: TeletextPageAddress, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner, TeletextObjectId::new(id))
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

fn links() -> TeletextResolvedObjectPageLinks {
    TeletextResolvedObjectPageLinks {
        public: None,
        global: None,
    }
}

fn budget() -> TeletextObjectCatalogBudget {
    TeletextObjectCatalogBudget {
        max_objects: 256,
        max_commands: 2_000,
    }
}

fn compile(
    owner: TeletextPageAddress,
    main: &TeletextEnhancementProgram,
    objects: &TeletextEnhancementObjectCatalog,
) -> EncodedTeletextLocalObjectProgram {
    let allocations = allocate_teletext_resource_objects(objects, &[], 256).unwrap();
    compile_teletext_local_program_with_objects(
        owner,
        main,
        objects,
        &allocations,
        links(),
        budget(),
    )
    .unwrap()
}

fn invoke(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    offset: Option<TeletextObjectOffset>,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset,
    }
}

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn foreground() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 8,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(5).unwrap(),
        },
    }
}

fn drcs(column: u8, table: TeletextDrcsTableScope) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                table,
                subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
            },
        },
    }
}

// Literal Hamming8 table and independent Hamming24 parity/data positions.
fn words(bodies: &[[u8; 40]]) -> Vec<(u32, u32, u32)> {
    const HAMMING: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    bodies
        .iter()
        .enumerate()
        .flat_map(|(designation, body)| {
            assert_eq!(body[0], HAMMING[designation]);
            body[1..].chunks_exact(3).map(|bytes| {
                let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
                assert_eq!(raw.count_ones() % 2, 1);
                for parity in [1_u32, 2, 4, 8, 16] {
                    let ones: u32 = (1_u32..24)
                        .filter(|position| position & parity != 0)
                        .map(|position| (raw >> (position - 1)) & 1)
                        .sum();
                    assert_eq!(ones % 2, 1);
                }
                let value = [
                    3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
                ]
                .into_iter()
                .enumerate()
                .fold(0, |data, (bit, position)| {
                    data | (((raw >> (position - 1)) & 1) << bit)
                });
                (value & 63, (value >> 6) & 31, value >> 11)
            })
        })
        .collect()
}

#[test]
fn forward_references_point_to_markers_at_literal_packet_and_triplet_boundaries() {
    let display = owner(0x1234);
    let target = local(display, 65535);
    let objects = catalog(vec![definition(
        target,
        TeletextObjectType::Passive,
        vec![],
    )]);
    // Literal calibration points: packet boundary, selector highbit, final packet.
    for (marker, designation, slot, address, data) in [
        (2_usize, 0_u8, 2_u8, 40_u32, 2_u32),
        (12, 0, 12, 40, 12),
        (13, 1, 0, 40, 16),
        (103, 7, 12, 40, 124),
        (104, 8, 0, 41, 0),
        (195, 15, 0, 41, 112),
        (206, 15, 11, 41, 123),
    ] {
        let mut commands = vec![foreground(); marker - 2];
        commands.push(invoke(target, TeletextObjectType::Passive, None));
        let main = program(commands);
        let before = main.clone();
        let page = compile(display, &main, &objects);
        assert_eq!(main, before);
        assert_eq!(page.owner(), display);
        assert_eq!(page.main_triplet_count(), marker - 1);
        let positions = page.definition_positions();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].target(), target);
        assert_eq!(positions[0].object_type(), TeletextObjectType::Passive);
        assert_eq!(usize::from(positions[0].absolute_index()), marker);
        assert_eq!(positions[0].object_number(), (designation << 4) | slot);
        let actual = words(page.packet_bodies());
        assert_eq!(actual[marker - 2], (address, 19, data));
        assert_eq!(actual[marker - 1], (63, 31, 6));
        assert_eq!(actual[marker], (address, 23, data));
        assert!(actual[marker + 1..]
            .iter()
            .all(|triplet| *triplet == (63, 31, 5)));
        // Recover the physical location from independently decoded invocation.
        let selected_designation = ((actual[marker - 2].0 & 1) << 3) | (actual[marker - 2].2 >> 4);
        let selected_slot = actual[marker - 2].2 & 15;
        assert_eq!(selected_designation, u32::from(designation));
        assert_eq!(selected_slot, u32::from(slot));
        assert_eq!(
            usize::try_from(selected_designation * 13 + selected_slot).unwrap(),
            marker
        );
    }
}

#[test]
fn all_types_and_full_semantic_ids_coexist_without_definition_scope_confusion() {
    let display = owner(0x1234);
    let kinds = [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ];
    let objects = catalog(
        kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                definition(
                    local(display, 65533 + u16::try_from(index).unwrap()),
                    kind,
                    vec![],
                )
            })
            .collect(),
    );
    let main = program(
        objects
            .definitions()
            .iter()
            .map(|item| invoke(item.identity, item.object_type, None))
            .collect(),
    );
    let page = compile(display, &main, &objects);
    let actual = words(page.packet_bodies());
    assert_eq!(
        &actual[..8],
        &[
            (40, 17, 4),
            (40, 18, 5),
            (40, 19, 6),
            (63, 31, 6),
            (40, 21, 4),
            (40, 22, 5),
            (40, 23, 6),
            (63, 31, 5)
        ]
    );
    for (position, source) in page
        .definition_positions()
        .iter()
        .zip(objects.definitions())
    {
        assert_eq!(position.target(), source.identity);
    }
}

#[test]
fn nested_local_forward_calls_keep_definitions_separate_and_resolve_exact_types() {
    let display = owner(0x1234);
    let active = local(display, 65535);
    let adaptive = local(display, 7);
    let passive = local(display, 0);
    let objects = catalog(vec![
        definition(
            active,
            TeletextObjectType::Active,
            vec![invoke(adaptive, TeletextObjectType::Adaptive, None)],
        ),
        definition(
            adaptive,
            TeletextObjectType::Adaptive,
            vec![invoke(passive, TeletextObjectType::Passive, None)],
        ),
        definition(
            passive,
            TeletextObjectType::Passive,
            vec![TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(75).unwrap(),
                },
            }],
        ),
    ]);
    let page = compile(
        display,
        &program(vec![invoke(active, TeletextObjectType::Active, None)]),
        &objects,
    );
    let actual = words(page.packet_bodies());
    assert_eq!(
        &actual[..9],
        &[
            (40, 17, 2),
            (63, 31, 6),
            (40, 21, 2),
            (40, 18, 4),
            (40, 22, 4),
            (40, 19, 6),
            (40, 23, 6),
            (8, 9, 75),
            (63, 31, 5)
        ]
    );
    assert_eq!(
        page.definition_positions()
            .iter()
            .map(TeletextLocalObjectDefinitionPosition::absolute_index)
            .collect::<Vec<_>>(),
        [2, 4, 6]
    );
}

#[test]
fn wrong_subcode_missing_id_and_type_fail_at_original_main_command() {
    let display = owner(0x1234);
    let correct = local(display, 65535);
    let objects = catalog(vec![
        definition(correct, TeletextObjectType::Passive, vec![]),
        definition(
            local(owner(0x1235), 65535),
            TeletextObjectType::Passive,
            vec![],
        ),
    ]);
    let allocations = allocate_teletext_resource_objects(&objects, &[], 256).unwrap();
    for (target, kind) in [
        (local(owner(0x1235), 65535), TeletextObjectType::Passive),
        (local(display, 65534), TeletextObjectType::Passive),
        (correct, TeletextObjectType::Active),
    ] {
        let main = program(vec![foreground(), invoke(target, kind, None)]);
        let before = main.clone();
        let error = compile_teletext_local_program_with_objects(
            display,
            &main,
            &objects,
            &allocations,
            links(),
            budget(),
        )
        .unwrap_err();
        assert_eq!(main, before);
        assert_eq!(error.owner, display);
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: None,
                command: 1
            })
        );
        assert_eq!(
            error.reason,
            TeletextLocalObjectProgramEncodingFailure::Lowering(
                TeletextLocalProgramEncodingFailure::MissingObjectAllocation {
                    target,
                    object_type: kind
                }
            )
        );
    }
}

#[test]
fn definitions_reset_ap_and_normal_global_drcs_latches() {
    let display = owner(0x1234);
    let objects = catalog(vec![
        definition(
            local(display, 65535),
            TeletextObjectType::Active,
            vec![
                position(20, 38),
                drcs(38, TeletextDrcsTableScope::Global),
                drcs(38, TeletextDrcsTableScope::Normal),
            ],
        ),
        definition(
            local(display, 1),
            TeletextObjectType::Active,
            vec![
                position(1, 8),
                drcs(8, TeletextDrcsTableScope::Global),
                drcs(8, TeletextDrcsTableScope::Normal),
            ],
        ),
    ]);
    let before = objects.clone();
    let page = compile(display, &program(vec![]), &objects);
    assert_eq!(objects, before);
    assert_eq!(page.main_triplet_count(), 0);
    let actual = words(page.packet_bodies());
    assert_eq!(
        &actual[..14],
        &[
            (63, 31, 6),
            (40, 21, 1),
            (60, 4, 38),
            (40, 24, 16),
            (38, 13, 47),
            (40, 24, 80),
            (38, 13, 111),
            (40, 21, 7),
            (41, 4, 8),
            (40, 24, 16),
            (8, 13, 47),
            (40, 24, 80),
            (8, 13, 111),
            (63, 31, 5)
        ]
    );
}

#[test]
fn aggregate_capacity_includes_origin_modifiers_drcs_selectors_and_both_terminators() {
    let display = owner(0x1234);
    let target = local(display, 65535);
    let objects = catalog(vec![definition(
        target,
        TeletextObjectType::Active,
        vec![
            position(1, 8),
            drcs(8, TeletextDrcsTableScope::Global),
            drcs(8, TeletextDrcsTableScope::Normal),
        ],
    )]);
    let allocations = allocate_teletext_resource_objects(&objects, &[], 256).unwrap();
    for extra in [0, 1] {
        let mut commands = vec![foreground(); 198 + extra];
        commands.push(invoke(
            target,
            TeletextObjectType::Active,
            Some(TeletextObjectOffset::new(0, 0).unwrap()),
        ));
        let main = program(commands);
        let result = compile_teletext_local_program_with_objects(
            display,
            &main,
            &objects,
            &allocations,
            links(),
            budget(),
        );
        if extra == 0 {
            let page = result.unwrap();
            assert_eq!(page.packet_bodies().len(), 16);
            assert_eq!(page.main_triplet_count(), 200);
            assert_eq!(page.definition_positions()[0].absolute_index(), 201);
            let actual = words(page.packet_bodies());
            assert_eq!(actual.len(), 208);
            assert_eq!(actual[198], (40, 16, 0));
            assert_eq!(actual[199], (41, 17, 118));
            assert_eq!(actual[200], (63, 31, 6));
            assert_eq!(actual[201], (41, 21, 118));
            assert_eq!(actual[207], (63, 31, 5));
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.owner, display);
            assert_eq!(
                error.reason,
                TeletextLocalObjectProgramEncodingFailure::Capacity {
                    count: 209,
                    maximum: 208
                }
            );
        }
    }
}

#[test]
fn marker_in_last_slot_cannot_displace_mandatory_final_termination() {
    let display = owner(0x1234);
    let target = local(display, 65535);
    let objects = catalog(vec![definition(
        target,
        TeletextObjectType::Passive,
        vec![],
    )]);
    let allocations = allocate_teletext_resource_objects(&objects, &[], 256).unwrap();
    let mut commands = vec![foreground(); 205];
    commands.push(invoke(target, TeletextObjectType::Passive, None));
    let error = compile_teletext_local_program_with_objects(
        display,
        &program(commands),
        &objects,
        &allocations,
        links(),
        budget(),
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextLocalObjectProgramEncodingFailure::Capacity {
            count: 209,
            maximum: 208
        }
    );
}

#[test]
fn raw_object_and_exact_owned_command_budgets_precede_lowering() {
    let display = owner(0x1234);
    let local_target = local(display, 65535);
    let objects = catalog(vec![
        definition(local_target, TeletextObjectType::Active, vec![foreground()]),
        definition(
            local(owner(0x1235), 1),
            TeletextObjectType::Active,
            vec![foreground(); 500],
        ),
    ]);
    let allocations = allocate_teletext_resource_objects(&objects, &[], 256).unwrap();
    let main = program(vec![position(0, 8)]);
    let error = compile_teletext_local_program_with_objects(
        display,
        &main,
        &objects,
        &allocations,
        links(),
        TeletextObjectCatalogBudget {
            max_objects: 1,
            max_commands: 0,
        },
    )
    .unwrap_err();
    assert_eq!(error.location, None);
    assert_eq!(
        error.reason,
        TeletextLocalObjectProgramEncodingFailure::ObjectBudget {
            count: 2,
            maximum: 1
        }
    );
    let error = compile_teletext_local_program_with_objects(
        display,
        &main,
        &objects,
        &allocations,
        links(),
        TeletextObjectCatalogBudget {
            max_objects: 2,
            max_commands: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextLocalObjectProgramEncodingFailure::CommandBudget { maximum: 1 }
    );
    let error = compile_teletext_local_program_with_objects(
        display,
        &main,
        &objects,
        &allocations,
        links(),
        TeletextObjectCatalogBudget {
            max_objects: 2,
            max_commands: 2,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 0
        })
    );
    assert!(matches!(
        error.reason,
        TeletextLocalObjectProgramEncodingFailure::Lowering(
            TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering { .. }
        )
    ));
}

#[test]
fn forbidden_colours_in_non_active_definitions_retain_object_and_command() {
    let display = owner(0x1234);
    let target = local(display, 65535);
    for object_type in [TeletextObjectType::Adaptive, TeletextObjectType::Passive] {
        for command in [
            TeletextEnhancementCommand::SetScreenColour {
                colour: TeletextColourReference::new(5).unwrap(),
            },
            TeletextEnhancementCommand::SetRowColour {
                row: 0,
                colour: TeletextColourReference::new(5).unwrap(),
                following_rows: false,
            },
            TeletextEnhancementCommand::SetRowColour {
                row: 24,
                colour: TeletextColourReference::new(5).unwrap(),
                following_rows: true,
            },
        ] {
            let objects = catalog(vec![definition(
                target,
                object_type,
                vec![position(1, 8), command],
            )]);
            let allocations = allocate_teletext_resource_objects(&objects, &[], 256).unwrap();
            let error = compile_teletext_local_program_with_objects(
                display,
                &program(vec![]),
                &objects,
                &allocations,
                links(),
                budget(),
            )
            .unwrap_err();
            assert_eq!(error.owner, display);
            assert_eq!(
                error.location,
                Some(TeletextEnhancementSource {
                    object: Some(target),
                    command: 1
                })
            );
            assert_eq!(
                error.reason,
                TeletextLocalObjectProgramEncodingFailure::ForbiddenColour { object_type }
            );
        }
    }
}

#[test]
fn no_owned_definitions_preserve_original_bytes_and_empty_program_presence() {
    let display = owner(0x1234);
    let objects = catalog(vec![definition(
        local(owner(0x1235), 65535),
        TeletextObjectType::Passive,
        vec![],
    )]);
    for main in [
        program(vec![]),
        program(vec![
            position(1, 8),
            foreground(),
            drcs(8, TeletextDrcsTableScope::Global),
        ]),
    ] {
        let original = compile_teletext_local_program(display, &main, 2_000).unwrap();
        let page = compile(display, &main, &objects);
        assert_eq!(page.packet_bodies(), original.packet_bodies());
        assert_eq!(page.main_triplet_count(), original.payload_triplet_count());
        assert!(page.definition_positions().is_empty());
        assert!(!page.packet_bodies().is_empty());
        assert_eq!(*words(page.packet_bodies()).last().unwrap(), (63, 31, 7));
    }
}

#[test]
fn local_definition_resource_calls_preserve_scope_and_validate_display_links() {
    let display = owner(0x1234);
    let identity = TeletextResourceIdentity::new(
        TeletextResourceKind::Gpop,
        TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap();
    let child = TeletextObjectIdentity::global(identity, TeletextObjectId::new(65535)).unwrap();
    let caller = local(display, 65535);
    let objects = catalog(vec![
        definition(child, TeletextObjectType::Passive, vec![]),
        definition(
            caller,
            TeletextObjectType::Active,
            vec![
                invoke(
                    child,
                    TeletextObjectType::Passive,
                    Some(TeletextObjectOffset::new(23, 71).unwrap()),
                ),
                foreground(),
            ],
        ),
    ]);
    let allocations = allocate_teletext_resource_objects(&objects, &[], 256).unwrap();
    let main = program(vec![invoke(caller, TeletextObjectType::Active, None)]);
    let selected = TeletextResolvedObjectPageLinks {
        public: None,
        global: Some(identity.page()),
    };
    let page = compile_teletext_local_program_with_objects(
        display,
        &main,
        &objects,
        &allocations,
        selected,
        budget(),
    )
    .unwrap();
    let actual = words(page.packet_bodies());
    assert_eq!(
        &actual[..7],
        &[
            (40, 17, 2),
            (63, 31, 6),
            (40, 21, 2),
            (63, 16, 71),
            (56, 19, 15),
            (8, 0, 5),
            (63, 31, 5)
        ]
    );
    for global in [None, Some(TeletextRawDataPageNumber::new(8, 0xa1).unwrap())] {
        let error = compile_teletext_local_program_with_objects(
            display,
            &main,
            &objects,
            &allocations,
            TeletextResolvedObjectPageLinks {
                public: None,
                global,
            },
            budget(),
        )
        .unwrap_err();
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: Some(caller),
                command: 0
            })
        );
        assert_eq!(
            error.reason,
            TeletextLocalObjectProgramEncodingFailure::Lowering(
                TeletextLocalProgramEncodingFailure::ObjectSelection {
                    target: child,
                    selected: global
                }
            )
        );
    }
}

//! Exact AP identity elision, physical selectors and sparse timing evidence.

use super::enhancement_program::LoweringState;
use super::*;
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
fn colour(value: u8) -> TeletextColourReference {
    TeletextColourReference::new(value).unwrap()
}
fn screen() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetScreenColour { colour: colour(5) }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn glyph(column: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    )
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn budget() -> TeletextObjectCatalogBudget {
    TeletextObjectCatalogBudget {
        max_objects: 16,
        max_commands: 512,
    }
}
fn catalog(
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(definitions, budget()).unwrap()
}
fn target() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(65535))
}
fn invoke(kind: TeletextObjectType) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: target(),
        object_type: kind,
        offset: None,
    }
}
fn definition(
    kind: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: target(),
        object_type: kind,
        program: program(commands),
    }
}
fn compile(
    main: &TeletextEnhancementProgram,
    objects: &TeletextEnhancementObjectCatalog,
    limits: TeletextObjectCatalogBudget,
) -> Result<EncodedTeletextLocalObjectProgram, TeletextLocalObjectProgramEncodingError> {
    let allocations = allocate_teletext_resource_objects(objects, &[], 16).unwrap();
    compile_teletext_local_program_with_objects(
        owner(),
        main,
        objects,
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        limits,
    )
}
fn source(object: Option<TeletextObjectIdentity>, command: usize) -> TeletextEnhancementSource {
    TeletextEnhancementSource { object, command }
}
fn fields(word: u32) -> (u32, u32, u32) {
    (word & 63, (word >> 6) & 31, word >> 11)
}
fn lower(state: &mut LoweringState, command: TeletextEnhancementCommand) -> Vec<(u32, u32, u32)> {
    state
        .lower(command, None)
        .unwrap()
        .words()
        .iter()
        .copied()
        .map(fields)
        .collect()
}

// Literal Hamming table, odd parity checks and independently selected data bits.
fn words(bodies: &[[u8; 40]]) -> Vec<(u32, u32, u32)> {
    const HAM: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    bodies
        .iter()
        .enumerate()
        .flat_map(|(designation, body)| {
            assert_eq!(body[0], HAM[designation]);
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
                let data = [
                    3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
                ]
                .into_iter()
                .enumerate()
                .fold(0, |data, (bit, position)| {
                    data | (((raw >> (position - 1)) & 1) << bit)
                });
                fields(data)
            })
        })
        .collect()
}

#[test]
fn initial_repeated_identity_has_no_words_but_consumes_raw_command_budget() {
    let main = program(vec![position(0, 0); 3]);
    let saved = main.clone();
    let objects = catalog(vec![]);
    let encoded = compile(&main, &objects, budget()).unwrap();
    assert_eq!(encoded.main_triplet_count(), 0);
    assert!(encoded.command_positions().is_empty());
    assert_eq!(words(encoded.packet_bodies()), vec![(63, 31, 7); 13]);
    assert_eq!(
        encoded.packet_bodies(),
        compile(&program(vec![]), &objects, budget())
            .unwrap()
            .packet_bodies()
    );
    let mut limits = budget();
    limits.max_commands = 2;
    assert_eq!(
        compile(&main, &objects, limits).unwrap_err().reason,
        TeletextLocalObjectProgramEncodingFailure::CommandBudget { maximum: 2 }
    );
    assert_eq!(
        compile_teletext_local_program(owner(), &main, 2)
            .unwrap_err()
            .reason,
        TeletextLocalProgramEncodingFailure::CommandBudget { maximum: 2 }
    );
    assert_eq!(main, saved);
}

#[test]
fn every_local_type_preserves_marker_body_and_authored_ordinals() {
    for (kind, invocation_mode, marker_mode) in [
        (TeletextObjectType::Active, 17, 21),
        (TeletextObjectType::Adaptive, 18, 22),
        (TeletextObjectType::Passive, 19, 23),
    ] {
        let main = program(vec![
            position(0, 0),
            position(0, 0),
            position(1, 8),
            invoke(kind),
        ]);
        let objects = catalog(vec![definition(
            kind,
            vec![position(0, 0), position(0, 0), glyph(0)],
        )]);
        let saved = (main.clone(), objects.clone());
        let encoded = compile(&main, &objects, budget()).unwrap();
        let clean = compile(
            &program(vec![position(1, 8), invoke(kind)]),
            &catalog(vec![definition(kind, vec![glyph(0)])]),
            budget(),
        )
        .unwrap();
        assert_eq!(encoded.packet_bodies(), clean.packet_bodies());
        assert_eq!(
            &words(encoded.packet_bodies())[..6],
            &[
                (41, 4, 8),
                (40, invocation_mode, 3),
                (63, 31, 6),
                (40, marker_mode, 3),
                (0, 9, 65),
                (63, 31, 5)
            ]
        );
        assert_eq!(encoded.main_triplet_count(), 2);
        assert_eq!(encoded.definition_positions()[0].absolute_index(), 3);
        assert_eq!(encoded.definition_positions()[0].object_number(), 3);
        let positions: Vec<_> = encoded
            .command_positions()
            .iter()
            .map(|item| (item.source(), item.first_word(), item.semantic_word()))
            .collect();
        assert_eq!(
            positions,
            [
                (source(None, 2), 0, 0),
                (source(None, 3), 1, 1),
                (source(Some(target()), 2), 4, 4)
            ]
        );
        assert_eq!((main, objects), saved);
    }
}

#[test]
fn identity_only_definitions_keep_markers_without_fabricated_body_word() {
    for kind in [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ] {
        let main = program(vec![position(0, 0)]);
        let objects = catalog(vec![definition(kind, vec![position(0, 0); 2])]);
        let encoded = compile(&main, &objects, budget()).unwrap();
        assert_eq!(encoded.main_triplet_count(), 0);
        assert!(encoded.command_positions().is_empty());
        assert_eq!(encoded.definition_positions()[0].absolute_index(), 1);
        let data = words(encoded.packet_bodies());
        assert_eq!(data[0], (63, 31, 6));
        assert_eq!(data[1].2, 1);
        assert!(data[2..].iter().all(|&item| item == (63, 31, 5)));
        let mut limits = budget();
        limits.max_commands = 2;
        assert_eq!(
            compile(&main, &objects, limits).unwrap_err().reason,
            TeletextLocalObjectProgramEncodingFailure::CommandBudget { maximum: 2 }
        );
    }
}

fn drcs(table: TeletextDrcsTableScope) -> TeletextEnhancementCommand {
    at(
        0,
        TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                table,
                subtable: TeletextDrcsSubtableIndex::new(7).unwrap(),
                slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
            },
        },
    )
}

#[test]
fn identity_uses_enhanced_ap_and_keeps_header_colour_and_both_drcs_latches() {
    let mut state = LoweringState::new_with_compatibility(colour(11));
    assert!(lower(&mut state, position(0, 0)).is_empty());
    assert_eq!(lower(&mut state, screen()), [(41, 0, 5)]);
    assert!(lower(&mut state, position(0, 0)).is_empty());
    assert_eq!(
        lower(&mut state, drcs(TeletextDrcsTableScope::Normal)),
        [(40, 24, 0x57), (0, 13, 0x6f)]
    );
    assert!(lower(&mut state, position(0, 0)).is_empty());
    assert_eq!(
        lower(&mut state, drcs(TeletextDrcsTableScope::Normal)),
        [(63, 7, 11), (0, 13, 0x6f)]
    );
    assert_eq!(
        lower(&mut state, drcs(TeletextDrcsTableScope::Global)),
        [(40, 24, 0x17), (0, 13, 47)]
    );
    assert!(lower(&mut state, position(0, 0)).is_empty());
    assert_eq!(
        lower(&mut state, drcs(TeletextDrcsTableScope::Global)),
        [(63, 7, 11), (0, 13, 47)]
    );
    assert_eq!(
        lower(&mut state, drcs(TeletextDrcsTableScope::Normal)),
        [(0, 13, 0x6f)]
    );
}

#[test]
fn forward_header_positions_preserve_logical_order_and_restore_only_the_emitted_ap() {
    for column in [8, 39] {
        let mut state = LoweringState::new_with_compatibility(colour(11));
        assert!(lower(&mut state, position(0, column)).is_empty());
        assert!(lower(&mut state, position(0, column)).is_empty());
        assert_eq!(lower(&mut state, screen()), [(41, 0, 5)]);
        assert_eq!(
            state.lower(glyph(column - 1), None).err().unwrap(),
            TeletextLocalProgramEncodingFailure::ActivePositionOrder {
                previous: (0, column),
                requested: (0, column - 1),
            }
        );
        assert_eq!(
            lower(
                &mut state,
                at(
                    column,
                    TeletextColumnOperation::G2 {
                        code: TeletextGlyphCode::new(0x23).unwrap(),
                    }
                ),
            ),
            [(63, 7, 11), (u32::from(column), 15, 0x23)]
        );
        assert!(lower(&mut state, position(0, column)).is_empty());
        assert_eq!(
            lower(&mut state, glyph(column)),
            [(u32::from(column), 9, 65)]
        );
    }
}

#[test]
fn deferred_header_row_colour_checks_authored_order_before_resetting_both_positions() {
    let row_colour = |row| TeletextEnhancementCommand::SetRowColour {
        row,
        colour: colour(3),
        following_rows: false,
    };
    let mut state = LoweringState::new_with_compatibility(colour(11));
    assert!(lower(&mut state, position(0, 8)).is_empty());
    assert_eq!(
        state.lower(row_colour(0), None).err().unwrap(),
        TeletextLocalProgramEncodingFailure::ActivePositionOrder {
            previous: (0, 8),
            requested: (0, 0),
        }
    );
    assert_eq!(lower(&mut state, row_colour(1)), [(41, 1, 3)]);
    assert_eq!(lower(&mut state, glyph(0)), [(0, 9, 65)]);
    assert_eq!(
        state.lower(position(0, 8), None).err().unwrap(),
        TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering {
            position: TeletextEnhancementPosition::new(0, 8).unwrap(),
        }
    );
    assert_eq!(lower(&mut state, glyph(1)), [(1, 9, 65)]);
}

#[test]
fn deferred_header_keeps_drcs_latches_and_never_invents_a_late_header_reset() {
    let mut state = LoweringState::new_with_compatibility(colour(11));
    assert!(lower(&mut state, position(0, 8)).is_empty());
    let at_column = |column, table| {
        let TeletextEnhancementCommand::AtColumn { operation, .. } = drcs(table) else {
            unreachable!()
        };
        at(column, operation)
    };
    assert_eq!(
        lower(&mut state, at_column(8, TeletextDrcsTableScope::Normal)),
        [(40, 24, 0x57), (8, 13, 0x6f)]
    );
    assert!(lower(&mut state, position(0, 9)).is_empty());
    assert_eq!(
        lower(&mut state, at_column(9, TeletextDrcsTableScope::Global)),
        [(40, 24, 0x17), (9, 13, 47)]
    );
    assert!(lower(&mut state, position(0, 10)).is_empty());
    assert_eq!(
        lower(&mut state, at_column(10, TeletextDrcsTableScope::Normal)),
        [(10, 13, 0x6f)]
    );
    assert_eq!(
        lower(&mut state, at_column(10, TeletextDrcsTableScope::Global)),
        [(10, 13, 47)]
    );
    assert_eq!(lower(&mut state, screen()), [(41, 0, 5)]);
    assert!(lower(&mut state, position(0, 11)).is_empty());
    assert_eq!(
        state
            .lower(
                at(
                    11,
                    TeletextColumnOperation::G2 {
                        code: TeletextGlyphCode::new(0x23).unwrap(),
                    }
                ),
                None
            )
            .err()
            .unwrap(),
        TeletextLocalProgramEncodingFailure::NeedsCompatibilityRowLowering { position: (0, 11) }
    );
    assert_eq!(
        lower(&mut state, at_column(11, TeletextDrcsTableScope::Normal)),
        [(11, 13, 0x6f)]
    );
}

#[test]
fn other_row_zero_moves_remain_located_and_failure_keeps_state() {
    for column in [1, 39] {
        let mut state = LoweringState::new();
        assert!(lower(&mut state, position(0, column)).is_empty());
        assert_eq!(
            lower(&mut state, glyph(column)),
            [(u32::from(column), 9, 65)]
        );
    }
    let mut state = LoweringState::new();
    assert!(lower(&mut state, position(0, 0)).is_empty());
    lower(&mut state, position(1, 0));
    assert_eq!(
        state.lower(position(0, 0), None).err().unwrap(),
        TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering {
            position: TeletextEnhancementPosition::new(0, 0).unwrap()
        }
    );
    assert_eq!(lower(&mut state, glyph(1)), [(1, 9, 65)]);
    let main = program(vec![position(0, 0), position(1, 0), position(0, 8)]);
    let saved = main.clone();
    let error = compile(&main, &catalog(vec![]), budget()).unwrap_err();
    assert_eq!(error.location, Some(source(None, 2)));
    assert_eq!(
        error.reason,
        TeletextLocalObjectProgramEncodingFailure::Lowering(
            TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering {
                position: TeletextEnhancementPosition::new(0, 8).unwrap()
            }
        )
    );
    assert_eq!(main, saved);
}

#[test]
fn relative_forward_position_local_types_keep_exact_words_and_sparse_sources() {
    for (kind, invocation_mode, marker_mode) in [
        (TeletextObjectType::Active, 17, 21),
        (TeletextObjectType::Adaptive, 18, 22),
        (TeletextObjectType::Passive, 19, 23),
    ] {
        let fg = at(8, TeletextColumnOperation::Foreground { colour: colour(3) });
        let main = program(vec![position(1, 0), invoke(kind), glyph(1)]);
        let objects = catalog(vec![definition(
            kind,
            vec![position(0, 8), position(0, 8), fg, glyph(8)],
        )]);
        let saved = (main.clone(), objects.clone());
        let actual = compile(&main, &objects, budget()).unwrap();
        let expected = compile(
            &main,
            &catalog(vec![definition(kind, vec![fg, glyph(8)])]),
            budget(),
        )
        .unwrap();
        assert_eq!(actual.packet_bodies(), expected.packet_bodies());
        assert_eq!(
            &words(actual.packet_bodies())[..8],
            &[
                (41, 4, 0),
                (40, invocation_mode, 4),
                (1, 9, 65),
                (63, 31, 6),
                (40, marker_mode, 4),
                (8, 0, 3),
                (8, 9, 65),
                (63, 31, 5),
            ]
        );
        assert_eq!(actual.definition_positions()[0].absolute_index(), 4);
        let sources: Vec<_> = actual
            .command_positions()
            .iter()
            .map(|record| (record.source(), record.first_word(), record.semantic_word()))
            .collect();
        assert_eq!(
            sources,
            [
                (source(None, 0), 0, 0),
                (source(None, 1), 1, 1),
                (source(None, 2), 2, 2),
                (source(Some(target()), 2), 5, 5),
                (source(Some(target()), 3), 6, 6),
            ]
        );
        assert_eq!((main, objects), saved);
    }
}

#[test]
fn relative_forward_position_unused_local_definition_starts_fresh() {
    let unused = TeletextObjectIdentity::local(owner(), TeletextObjectId::new(65534));
    let main = program(vec![position(1, 0), invoke(TeletextObjectType::Passive)]);
    let objects = catalog(vec![
        definition(TeletextObjectType::Passive, vec![position(2, 3), glyph(3)]),
        TeletextEnhancementObjectDefinition {
            identity: unused,
            object_type: TeletextObjectType::Passive,
            program: program(vec![position(0, 8), glyph(8)]),
        },
    ]);
    let actual = compile(&main, &objects, budget()).unwrap();
    assert_eq!(actual.definition_positions().len(), 2);
    assert_eq!(actual.definition_positions()[1].target(), unused);
    assert_eq!(
        &words(actual.packet_bodies())[..9],
        &[
            (41, 4, 0),
            (40, 19, 3),
            (63, 31, 6),
            (40, 23, 3),
            (42, 4, 3),
            (3, 9, 65),
            (40, 23, 6),
            (8, 9, 65),
            (63, 31, 5),
        ]
    );
    let unused_sources: Vec<_> = actual
        .command_positions()
        .iter()
        .filter(|record| record.source().object == Some(unused))
        .map(|record| (record.source(), record.semantic_word()))
        .collect();
    assert_eq!(unused_sources, [(source(Some(unused), 1), 7)]);
}

#[test]
fn relative_forward_position_nested_call_requires_real_parent_ap() {
    let outer = TeletextObjectIdentity::local(owner(), TeletextObjectId::new(65534));
    let outer_call = TeletextEnhancementCommand::InvokeObject {
        target: outer,
        object_type: TeletextObjectType::Adaptive,
        offset: None,
    };
    let main = program(vec![position(1, 0), outer_call]);
    let make = |materialized| {
        let mut commands = vec![position(0, 8)];
        if materialized {
            commands.push(at(
                8,
                TeletextColumnOperation::Foreground { colour: colour(3) },
            ));
        }
        commands.push(invoke(TeletextObjectType::Passive));
        catalog(vec![
            TeletextEnhancementObjectDefinition {
                identity: outer,
                object_type: TeletextObjectType::Adaptive,
                program: program(commands),
            },
            definition(TeletextObjectType::Passive, vec![glyph(0)]),
        ])
    };
    let error = compile(&main, &make(false), budget()).unwrap_err();
    assert_eq!(error.location, Some(source(Some(outer), 1)));
    assert_eq!(
        error.reason,
        TeletextLocalObjectProgramEncodingFailure::Lowering(
            TeletextLocalProgramEncodingFailure::NeedsRowZeroLowering {
                position: TeletextEnhancementPosition::new(0, 8).unwrap(),
            }
        )
    );
    let actual = compile(&main, &make(true), budget()).unwrap();
    assert!(actual
        .command_positions()
        .iter()
        .any(|record| record.source() == source(Some(outer), 2)));
    assert!(!words(actual.packet_bodies())
        .iter()
        .any(|&(address, mode, _)| address == 63 && mode == 7));
    let mut state = LoweringState::new();
    assert!(lower(&mut state, position(0, 8)).is_empty());
    assert_eq!(
        state.lower(glyph(7), None).err().unwrap(),
        TeletextLocalProgramEncodingFailure::ActivePositionOrder {
            previous: (0, 8),
            requested: (0, 7),
        }
    );
    assert_eq!(lower(&mut state, glyph(8)), [(8, 9, 65)]);
}

#[test]
fn omitted_identities_keep_packet_boundary_pointer_and_later_sources_exact() {
    let mut commands = vec![position(0, 0)];
    commands.extend([screen(); 11]);
    commands.extend([
        position(0, 0),
        invoke(TeletextObjectType::Passive),
        position(0, 0),
    ]);
    let main = program(commands);
    let objects = catalog(vec![definition(
        TeletextObjectType::Passive,
        vec![position(0, 0), position(0, 0), glyph(0)],
    )]);
    let encoded = compile(&main, &objects, budget()).unwrap();
    assert_eq!(encoded.main_triplet_count(), 12);
    let marker = &encoded.definition_positions()[0];
    assert_eq!(
        (marker.absolute_index(), marker.object_number()),
        (13, 0x10)
    );
    let data = words(encoded.packet_bodies());
    assert_eq!(
        &data[11..16],
        &[
            (40, 19, 16),
            (63, 31, 6),
            (40, 23, 16),
            (0, 9, 65),
            (63, 31, 5)
        ]
    );
    let last = &encoded.command_positions()[11];
    assert_eq!(
        (last.source(), last.first_word(), last.semantic_word()),
        (source(None, 13), 11, 11)
    );
    assert_eq!(
        encoded.command_positions()[12].source(),
        source(Some(target()), 2)
    );
    assert_eq!(encoded.command_positions()[12].semantic_word(), 14);
}

#[test]
fn physical_capacity_counts_only_words_while_host_budget_counts_all_commands() {
    let mut commands = vec![position(0, 0); 16];
    commands.extend([screen(); 207]);
    let main = program(commands.clone());
    let objects = catalog(vec![]);
    let encoded = compile(&main, &objects, budget()).unwrap();
    assert_eq!(encoded.main_triplet_count(), 207);
    assert_eq!(encoded.packet_bodies().len(), 16);
    assert_eq!(words(encoded.packet_bodies())[207], (63, 31, 7));
    assert_eq!(encoded.command_positions().len(), 207);
    assert_eq!(encoded.command_positions()[0].source(), source(None, 16));
    assert_eq!(encoded.command_positions()[206].semantic_word(), 206);
    commands.push(screen());
    let oversized = program(commands);
    let error = compile(&oversized, &objects, budget()).unwrap_err();
    assert_eq!(
        error.reason,
        TeletextLocalObjectProgramEncodingFailure::Capacity {
            count: 209,
            maximum: 208
        }
    );
    assert_eq!(error.location, Some(source(None, 223)));
    let mut limits = budget();
    limits.max_commands = 222;
    assert_eq!(
        compile(&main, &objects, limits).unwrap_err().reason,
        TeletextLocalObjectProgramEncodingFailure::CommandBudget { maximum: 222 }
    );
    assert_eq!(main.commands().len(), 223);
}

#[test]
fn absent_identity_positions_leave_same_emitter_c2_evidence_exact() {
    let main = program(vec![
        position(0, 0),
        position(0, 0),
        position(1, 8),
        at(8, TeletextColumnOperation::Foreground { colour: colour(4) }),
        at(
            8,
            TeletextColumnOperation::Foreground { colour: colour(13) },
        ),
    ]);
    let objects = catalog(vec![]);
    let encoded = compile(&main, &objects, budget()).unwrap();
    let trace = resolve_teletext_spatial_program(
        owner(),
        &main,
        &objects,
        TeletextSpatialTraceBudget {
            max_events: 5,
            max_invocations: 0,
        },
    )
    .unwrap();
    let positions: Vec<_> = encoded
        .command_positions()
        .iter()
        .map(|item| TeletextAttributeTransmissionPosition {
            event_ordinal: item.source().command,
            source: item.source(),
            invocation: None,
            word: item.semantic_word(),
        })
        .collect();
    assert_eq!(
        positions
            .iter()
            .map(|item| (item.event_ordinal, item.word))
            .collect::<Vec<_>>(),
        [(2, 0), (3, 1), (4, 2)]
    );
    let evidence = TeletextAttributeTransmissionEvidence::new(&trace, &positions, 5).unwrap();
    assert!(evidence.matches(&trace));
    let writer = |event_ordinal: usize| {
        let event = trace.events()[event_ordinal];
        TeletextAttributeSource::Enhancement {
            event_ordinal,
            location: event.source(),
            root_source: event.root_source(),
            invocation: event.invocation(),
            effective_start: (1, 8),
            placed_position: (1, 8),
        }
    };
    assert_eq!(
        evidence.compare(writer(4), writer(3)),
        Some(std::cmp::Ordering::Greater)
    );
    assert_eq!(evidence.compare(writer(0), writer(3)), None);
    assert_eq!(
        &words(encoded.packet_bodies())[..4],
        &[(41, 4, 8), (8, 0, 4), (8, 0, 13), (63, 31, 7)]
    );
}

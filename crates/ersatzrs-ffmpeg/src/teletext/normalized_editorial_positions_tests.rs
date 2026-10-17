//! Final-word correspondence for retained local intent, without edit authority.

use super::authored_service_tests::{input, limits, semantic_limits};
use super::*;
use ersatzrs_domain::*;

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

fn fixture(main: Option<Vec<TeletextEnhancementCommand>>, active: bool) -> TeletextSemanticService {
    let mut draft = input();
    draft.pages[0].local_program =
        main.map(|commands| TeletextEnhancementProgram::new(commands).unwrap());
    if active {
        draft.base.pages[0].rows[0].runs[0].content =
            TeletextRunContent::Text { text: "£Z".into() };
        let target = TeletextObjectIdentity::local(
            draft.base.pages[0].address,
            TeletextObjectId::new(65535),
        );
        draft.objects.push(TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: TeletextObjectType::Active,
            program: TeletextEnhancementProgram::new(vec![glyph(0)]).unwrap(),
        });
    }
    TeletextSemanticService::new(draft, semantic_limits()).unwrap()
}

fn with_augmented(
    service: &TeletextSemanticService,
    inspect: impl FnOnce(&TeletextAugmentedOriginProgram<'_>, EncodedTeletextAugmentedOriginProgram<'_>),
) {
    let owner = service.base().pages[0].address;
    let budget = limits().normalization;
    let original = compose_teletext_main_program(service, owner, budget.composition).unwrap();
    let realization =
        plan_teletext_base_compatibility_realization(&original, budget.realization).unwrap();
    let placement =
        plan_teletext_origin_carrier_placement(&realization, service.objects(), budget.placement)
            .unwrap();
    let augmented = build_teletext_augmented_origin_program(&placement, budget.assembly).unwrap();
    let allocations =
        allocate_teletext_resource_objects(augmented.catalog(), &[], budget.max_resource_objects)
            .unwrap();
    let compiled = compile_teletext_augmented_origin_program(
        service,
        &augmented,
        &allocations,
        budget.replay,
        budget.packing,
        budget.ancestry,
    )
    .unwrap();
    inspect(&augmented, compiled);
}

// Literal Hamming data positions: read the actual final packet word, not a
// re-lowered command or provisional packing ordinal.
fn word(encoded: &EncodedTeletextLocalObjectProgram, ordinal: usize) -> u32 {
    let body = &encoded.packet_bodies()[ordinal / 13];
    let offset = 1 + (ordinal % 13) * 3;
    let bytes = &body[offset..offset + 3];
    let protected = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
    assert_eq!(protected.count_ones() % 2, 1);
    for parity in [1_u32, 2, 4, 8, 16] {
        let ones: u32 = (1_u32..24)
            .filter(|bit| bit & parity != 0)
            .map(|bit| (protected >> (bit - 1)) & 1)
            .sum();
        assert_eq!(ones % 2, 1);
    }
    [
        3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
    ]
    .into_iter()
    .enumerate()
    .fold(0, |value, (bit, position)| {
        value | (((protected >> (position - 1)) & 1) << bit)
    })
}

#[test]
fn shifted_carrier_positions_retain_original_local_ordinal_and_physical_source() {
    let owner = input().base.pages[0].address;
    let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
    let service = fixture(
        Some(vec![
            position(1, 8),
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Active,
                offset: None,
            },
            position(3, 20),
            foreground(20, 4),
        ]),
        true,
    );
    let saved = service.clone();
    with_augmented(&service, |augmented, compiled| {
        assert!(!augmented.carriers().is_empty());
        let encoded = compiled.physical().encoded();
        let colour = encoded
            .command_positions()
            .iter()
            .find(|position| position.original_local_command() == Some(3))
            .unwrap();
        assert_eq!(colour.source().object, None);
        assert!(
            colour.source().command > 3,
            "generated carrier must shift physical ordinals"
        );
        assert_eq!(word(encoded, colour.semantic_word()), 0x14 | (4 << 11));
        let mut excluded = 0;
        for position in encoded.command_positions() {
            let expected = match augmented.emitter_source(position.source()) {
                Some(TeletextAugmentedOriginSource::RetainedMain {
                    provenance: TeletextMainCompositionSource::OriginalPage { command },
                    ..
                }) if position.source().object.is_none() => Some(command),
                _ => {
                    excluded += 1;
                    None
                }
            };
            assert_eq!(position.original_local_command(), expected);
        }
        assert!(excluded > 0);
        assert!(encoded
            .command_positions()
            .iter()
            .any(|position| position.source().object.is_some()));
        let owned_positions = encoded.command_positions().to_vec();
        let physical = compiled.into_physical();
        assert_eq!(physical.encoded().command_positions(), owned_positions);
    });
    let normalized =
        compile_teletext_normalized_main_program(&service, owner, limits().normalization).unwrap();
    let colour = normalized
        .encoded()
        .command_positions()
        .iter()
        .find(|position| position.original_local_command() == Some(3))
        .unwrap();
    assert_eq!(
        word(normalized.encoded(), colour.semantic_word()),
        0x14 | (4 << 11)
    );
    assert_eq!(service, saved);
}

#[test]
fn header_position_before_nonempty_object_passes_the_authored_factory() {
    use super::authored_service_tests::{declared, transmissions};

    for defined_foreground in [false, true] {
        let column = if defined_foreground { 9 } else { 8 };
        let mut draft = input();
        let owner = draft.base.pages[0].address;
        let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(65535));
        let mut commands = vec![TeletextEnhancementCommand::SetRowColour {
            row: 0,
            colour: TeletextColourReference::new(3).unwrap(),
            following_rows: false,
        }];
        if defined_foreground {
            commands.push(foreground(8, 7));
        }
        commands.extend([
            position(0, column),
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Passive,
                offset: None,
            },
        ]);
        draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
        draft.objects.push(TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![foreground(0, 2), glyph(0)]).unwrap(),
        });
        let original = declared(draft);
        let saved = original.clone();
        let encoded =
            encode_teletext_authored_service(original, &transmissions(&saved), &limits()).unwrap();
        assert_eq!(encoded.declared(), &saved);
        let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
            panic!("nonempty header object must retain authored display ownership")
        };
        let fallback = encode_teletext_page(
            &saved.semantic().base().pages[0],
            TeletextEncodingRegion::WesternLatin,
        )
        .unwrap();
        let actual = project_encoded_teletext_display(display.page(), 128).unwrap();
        let baseline = project_encoded_teletext_display(&fallback, 128).unwrap();
        assert_eq!(actual.header, baseline.header);
        let rows = |records: Vec<TeletextEncodedDisplayRecord>| {
            records
                .into_iter()
                .filter(|record| !matches!(record, TeletextEncodedDisplayRecord::Triplets { .. }))
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(actual.records), rows(baseline.records));
        for trace in [
            display.normalized().original().spatial_trace(),
            display.normalized().physical().replay().spatial_trace(),
        ] {
            let invocation = trace
                .invocations()
                .iter()
                .find(|call| call.target() == target)
                .unwrap();
            for position in [invocation.origin(), invocation.caller_position()] {
                assert_eq!((position.row(), position.column()), (0, u16::from(column)));
            }
            assert_eq!(invocation.offset(), None);
        }
        for geometry in [
            display.normalized().original().display_geometry(),
            display.normalized().physical().replay().display_geometry(),
        ] {
            let region = geometry
                .object_regions()
                .iter()
                .find(|region| region.instance().target() == target)
                .unwrap();
            assert_eq!(
                region.interval(),
                TeletextObjectAddressInterval::Known {
                    start: (0, u16::from(column)),
                    finish: (0, u16::from(column)),
                }
            );
            assert_eq!(geometry.object_open_count().maximum_known_open(), 1);
        }
    }
}

#[test]
fn forward_header_positions_pass_the_authored_factory_without_changing_transmission() {
    use super::authored_service_tests::{declared, transmissions};

    for column in [8, 39] {
        let make = |explicit_position: bool| {
            let mut draft = input();
            draft.base.pages[0].rows.push(TeletextRow {
                row: 0,
                runs: vec![TeletextRun {
                    column,
                    style: TeletextStyle::default(),
                    content: TeletextRunContent::Text { text: "£".into() },
                }],
            });
            let mut commands = vec![
                TeletextEnhancementCommand::SetScreenColour {
                    colour: TeletextColourReference::new(5).unwrap(),
                },
                TeletextEnhancementCommand::SetRowColour {
                    row: 0,
                    colour: TeletextColourReference::new(3).unwrap(),
                    following_rows: false,
                },
            ];
            if explicit_position {
                commands.extend([position(0, column), position(0, column)]);
            }
            commands.push(TeletextEnhancementCommand::AtColumn {
                column,
                operation: TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(0x23).unwrap(),
                },
            });
            draft.pages[0].local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
            declared(draft)
        };
        let original = make(true);
        let saved = original.clone();
        let actual =
            encode_teletext_authored_service(original, &transmissions(&saved), &limits()).unwrap();
        let reference = make(false);
        let expected = encode_teletext_authored_service(
            reference.clone(),
            &transmissions(&reference),
            &limits(),
        )
        .unwrap();
        assert_eq!(actual.unit_count(), expected.unit_count());
        for (actual, expected) in actual.pages().iter().zip(expected.pages()) {
            assert_eq!(actual.data_units(), expected.data_units());
        }
        assert_eq!(saved, make(true));
    }
}

#[test]
fn initial_header_prefix_points_to_semantic_operand_and_omits_identity_move() {
    let service = fixture(
        Some(vec![position(0, 0), foreground(8, 4), glyph(8)]),
        false,
    );
    let owner = service.base().pages[0].address;
    let normalized =
        compile_teletext_normalized_main_program(&service, owner, limits().normalization).unwrap();
    let encoded = normalized.encoded();
    assert!(!encoded
        .command_positions()
        .iter()
        .any(|position| position.original_local_command() == Some(0)));
    let colour = encoded
        .command_positions()
        .iter()
        .find(|position| position.original_local_command() == Some(1))
        .unwrap();
    assert_eq!(colour.semantic_word(), colour.first_word() + 1);
    assert_eq!(word(encoded, colour.first_word()) & 0x7ff, 63 | (7 << 6));
    assert_eq!(word(encoded, colour.semantic_word()), 8 | (4 << 11));
}

#[test]
fn provenance_binding_changes_no_packet_or_physical_diagnostic_positions() {
    let service = fixture(
        Some(vec![position(1, 8), foreground(8, 4), glyph(8)]),
        false,
    );
    let budget = limits().normalization;
    with_augmented(&service, |augmented, compiled| {
        let original = augmented.placement().plan().composition();
        let allocations = allocate_teletext_resource_objects(
            augmented.catalog(),
            &[],
            budget.max_resource_objects,
        )
        .unwrap();
        let unbound = compile_teletext_physical_display_program(
            &service,
            original,
            augmented.program(),
            augmented.catalog(),
            &allocations,
            budget.replay,
            budget.packing,
        )
        .unwrap();
        assert_eq!(compiled.physical().replay(), unbound.replay());
        assert_eq!(
            compiled.physical().encoded().packet_bodies(),
            unbound.encoded().packet_bodies()
        );
        let diagnostics = |encoded: &EncodedTeletextLocalObjectProgram| {
            encoded
                .command_positions()
                .iter()
                .map(|position| {
                    (
                        position.source(),
                        position.first_word(),
                        position.semantic_word(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            diagnostics(compiled.physical().encoded()),
            diagnostics(unbound.encoded())
        );
        assert!(unbound
            .encoded()
            .command_positions()
            .iter()
            .all(|position| position.original_local_command().is_none()));
        assert!(compiled
            .physical()
            .encoded()
            .command_positions()
            .iter()
            .any(|position| position.original_local_command().is_some()));
    });
}

#[test]
fn default_and_generated_base_commands_never_gain_original_local_ordinals() {
    let mut draft = input();
    draft.pages[0].local_program = None;
    draft.base.pages[0].rows[0].runs[0].content = TeletextRunContent::Text { text: "é".into() };
    // Empty selected default still emits its real call but cannot introduce
    // unresolved remote attribute ordering into this correspondence fixture.
    draft.objects[1].program = TeletextEnhancementProgram::new(vec![]).unwrap();
    let service = TeletextSemanticService::new(draft, semantic_limits()).unwrap();
    with_augmented(&service, |augmented, compiled| {
        assert!(augmented.main_sources().iter().any(|source| matches!(
            source,
            TeletextAugmentedOriginSource::RetainedMain {
                provenance: TeletextMainCompositionSource::DefaultSlot { .. },
                ..
            }
        )));
        assert!(augmented.main_sources().iter().any(|source| matches!(
            source,
            TeletextAugmentedOriginSource::RetainedMain {
                provenance: TeletextMainCompositionSource::BaseCell { .. },
                ..
            }
        )));
        assert!(!compiled.physical().encoded().command_positions().is_empty());
        assert!(compiled
            .physical()
            .encoded()
            .command_positions()
            .iter()
            .all(|position| position.original_local_command().is_none()));
    });
}

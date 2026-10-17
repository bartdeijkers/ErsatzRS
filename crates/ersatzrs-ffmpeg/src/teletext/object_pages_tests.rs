//! Independent EN 300 706 pointer, definition and complete packet byte oracles.

use super::*;
use ersatzrs_domain::*;

fn resource(kind: TeletextResourceKind, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}

fn target(resource: TeletextResourceIdentity, id: u16) -> TeletextObjectIdentity {
    let id = TeletextObjectId::new(id);
    if resource.kind() == TeletextResourceKind::Gpop {
        TeletextObjectIdentity::global(resource, id).unwrap()
    } else {
        TeletextObjectIdentity::public(resource, id).unwrap()
    }
}

fn definition(
    resource: TeletextResourceIdentity,
    id: u16,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: target(resource, id),
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}

fn catalog(items: Vec<TeletextEnhancementObjectDefinition>) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        items,
        TeletextObjectCatalogBudget {
            max_objects: 128,
            max_commands: 2_000,
        },
    )
    .unwrap()
}

fn transmission(last_subpage: bool) -> TeletextObjectPageTransmission {
    TeletextObjectPageTransmission {
        counter: TeletextResourceUpdateCounter::new(7).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        mode: TeletextResourceTransmissionMode::Serial,
        last_subpage,
    }
}

fn encode(
    resource: TeletextResourceIdentity,
    catalog: &TeletextEnhancementObjectCatalog,
    last_subpage: bool,
) -> EncodedTeletextObjectPage {
    let allocations = allocate_teletext_resource_objects(catalog, &[], 128).unwrap();
    encode_teletext_object_page(
        resource,
        catalog,
        &allocations,
        transmission(last_subpage),
        2_000,
    )
    .unwrap()
}

// Literal normative codewords, independent of production protection helpers.
fn nibble(wire: u8) -> u8 {
    const HAMMING: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    u8::try_from(
        HAMMING
            .iter()
            .position(|value| *value == wire.reverse_bits())
            .unwrap(),
    )
    .unwrap()
}

fn packet(unit: &[u8; 46]) -> (u8, u8) {
    assert_eq!(&unit[..4], &[2, 44, 0xe0, 0xe4]);
    let address = nibble(unit[4]) | (nibble(unit[5]) << 4);
    (address & 7, address >> 3)
}

fn words(unit: &[u8; 46]) -> [u32; 13] {
    std::array::from_fn(|index| {
        let bytes: [u8; 3] = std::array::from_fn(|byte| unit[7 + index * 3 + byte].reverse_bits());
        let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
        assert_eq!(raw.count_ones() % 2, 1);
        for parity in [1_u32, 2, 4, 8, 16] {
            let ones: u32 = (1_u32..24)
                .filter(|position| position & parity != 0)
                .map(|position| (raw >> (position - 1)) & 1)
                .sum();
            assert_eq!(ones % 2, 1);
        }
        [
            3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
        ]
        .into_iter()
        .enumerate()
        .fold(0, |data, (bit, position)| {
            data | (((raw >> (position - 1)) & 1) << bit)
        })
    })
}

fn fields(word: u32) -> (u32, u32, u32) {
    (word & 63, (word >> 6) & 31, word >> 11)
}

fn row(page: &EncodedTeletextObjectPage, number: u8, designation: u8) -> &[u8; 46] {
    page.data_units()
        .iter()
        .find(|unit| packet(unit).1 == number && (number != 26 || nibble(unit[6]) == designation))
        .unwrap()
}

fn body_words(page: &EncodedTeletextObjectPage) -> Vec<u32> {
    page.data_units()
        .iter()
        .filter(|unit| {
            let number = packet(unit).1;
            (3..=26).contains(&number) && (number > 4 || nibble(unit[6]) == 0)
        })
        .flat_map(words)
        .collect()
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

#[test]
fn all_scopes_types_s1_and_pointer_halves_have_exact_definition_destinations() {
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        for s1 in 0..16 {
            for (type_code, object_type) in [
                (1, TeletextObjectType::Active),
                (2, TeletextObjectType::Adaptive),
                (3, TeletextObjectType::Passive),
            ] {
                let owner = resource(kind, s1);
                let input = catalog(
                    (0..32)
                        .map(|id| definition(owner, 0xffe0 + id, object_type, vec![]))
                        .collect(),
                );
                let page = encode(owner, &input, true);
                assert_eq!(page.definition_positions().len(), 32);
                assert_eq!(page.header().last_packet().get(), 7);
                let content = body_words(&page);
                for (ordinal, pos) in page.definition_positions().iter().enumerate() {
                    assert_eq!(pos.target(), input.definitions()[ordinal].identity);
                    assert_eq!(pos.object_type(), object_type);
                    assert_eq!(usize::from(pos.absolute_index()), 26 + ordinal);
                    let pointer_packet = u8::try_from(ordinal / 8 + 1).unwrap();
                    let pointers = words(row(&page, pointer_packet, 0));
                    let triplet = 3 * ((ordinal % 8) / 2) + type_code;
                    let pointer = if ordinal % 2 == 0 {
                        pointers[triplet] & 511
                    } else {
                        pointers[triplet] >> 9
                    };
                    assert_eq!(usize::try_from(pointer).unwrap(), 26 + ordinal);
                    let (address, mode, data) = fields(content[ordinal]);
                    assert_eq!(
                        (address >> 3) & 3,
                        1,
                        "definition usage is Level 2.5, not invocation scope"
                    );
                    assert_eq!(address & 4, 0);
                    assert_eq!(address & 3, u32::try_from(ordinal / 8).unwrap());
                    assert_eq!(mode, u32::try_from(20 + type_code).unwrap());
                    assert_eq!(data & 15, u32::from(s1));
                    assert_eq!((data >> 4) & 1, u32::try_from(ordinal % 2).unwrap());
                    assert_eq!((data >> 5) & 3, u32::try_from((ordinal % 8) / 2).unwrap());
                    assert_eq!(u32::from(pos.object_number()), ((address & 3) << 7) | data);
                }
                assert!(content[32..]
                    .iter()
                    .all(|value| fields(*value) == (63, 31, 3)));
            }
        }
    }
}

#[test]
fn mixed_types_reserved_triplets_and_unused_pointer_halves_remain_distinct() {
    let owner = resource(TeletextResourceKind::Pop, 0);
    let input = catalog(vec![
        definition(owner, 65535, TeletextObjectType::Passive, vec![]),
        definition(owner, 1, TeletextObjectType::Active, vec![]),
        definition(owner, 2, TeletextObjectType::Adaptive, vec![]),
        definition(owner, 3, TeletextObjectType::Active, vec![]),
    ]);
    let page = encode(owner, &input, false);
    let first = words(row(&page, 1, 0));
    assert_eq!(first[0], 0);
    assert_eq!(first[1], 1 | (3 << 9));
    assert_eq!(first[2], 2 | (511 << 9));
    assert_eq!(first[3], 511 << 9);
    assert!(first[4..].iter().all(|value| *value == 0x3ffff));
    let second = words(row(&page, 2, 0));
    assert_eq!(second[0], 0);
    assert!(second[1..].iter().all(|value| *value == 0x3ffff));
    assert_eq!(nibble(row(&page, 1, 0)[6]), 1);
    assert!(body_words(&page)[4..]
        .iter()
        .all(|word| fields(*word) == (63, 31, 1)));
}

#[test]
fn mandatory_function_packet_and_empty_page_header_preserve_wire_order_and_lifecycle() {
    for (kind, function) in [
        (TeletextResourceKind::Pop, 3),
        (TeletextResourceKind::Gpop, 2),
    ] {
        let owner = resource(kind, 15);
        let page = encode(owner, &catalog(vec![]), true);
        assert_eq!(page.data_units().len(), 5);
        assert_eq!(
            page.data_units().iter().map(packet).collect::<Vec<_>>(),
            [(0, 0), (0, 28), (0, 1), (0, 2), (0, 3)]
        );
        let function_words = words(row(&page, 28, 0));
        assert_eq!(nibble(row(&page, 28, 0)[6]), 0);
        assert_eq!(function_words[0], 0xffa0 | function);
        assert!(function_words[1..].iter().all(|value| *value == 0));
        assert_eq!(page.header().physical_subcode(), 0x037f);
        let header = page.data_units()[0];
        let n: [u8; 8] = std::array::from_fn(|i| nibble(header[6 + i]));
        assert_eq!(n[2], 15);
        assert_eq!(n[3] & 7, 7);
        assert_eq!(n[4] & 15, 3);
        assert_eq!(n[5] & 3, 0);
        assert_eq!(n[3] >> 3, 1, "changed first update erases old data");
        assert_eq!(n[6] & 1, 1, "changed update sets C8");
        assert_eq!(
            n[7] & 6,
            0,
            "full first is neither partial nor continuation"
        );
        assert!(page.definition_positions().is_empty());
        assert!(page.command_positions().is_empty());
        assert!(body_words(&page)
            .iter()
            .all(|word| fields(*word) == (63, 31, 3)));
    }
}

#[test]
fn header_nn_tracks_last_encoding_row_across_x25_and_x26_designations() {
    let owner = resource(TeletextResourceKind::Gpop, 2);
    for (commands, nn, last_packet, designation) in [
        (11, 3, 3, 0),
        (12, 4, 4, 0),
        (297, 25, 25, 0),
        (298, 26, 26, 0),
        (505, 41, 26, 15),
    ] {
        let input = catalog(vec![definition(
            owner,
            65535,
            TeletextObjectType::Active,
            vec![foreground(); commands],
        )]);
        let page = encode(owner, &input, true);
        assert_eq!(page.header().last_packet().get(), nn);
        let last = page.data_units().last().unwrap();
        assert_eq!(packet(last), (0, last_packet));
        assert_eq!(nibble(last[6]), designation);
        assert_eq!(fields(words(last)[12]), (63, 31, 3));
        assert!(page.data_units().len() <= 43);
    }
}

#[test]
fn all_pointer_extents_accept_exact_physical_capacity_and_reject_next_word() {
    let owner = resource(TeletextResourceKind::Pop, 0);
    for (count, capacity, first_absolute) in
        [(1_u16, 507_usize, 0_u16), (17, 494, 13), (25, 481, 26)]
    {
        let command_count = capacity - usize::from(count) - 1;
        for extra in [0, 1] {
            let input = catalog(
                (0..count)
                    .map(|id| {
                        definition(
                            owner,
                            id,
                            TeletextObjectType::Active,
                            if id == 0 {
                                vec![foreground(); command_count + extra]
                            } else {
                                vec![]
                            },
                        )
                    })
                    .collect(),
            );
            let allocations = allocate_teletext_resource_objects(&input, &[], 128).unwrap();
            let result =
                encode_teletext_object_page(owner, &input, &allocations, transmission(true), 2_000);
            if extra == 0 {
                let page = result.unwrap();
                assert_eq!(page.header().last_packet().get(), 41);
                assert_eq!(page.data_units().len(), 43);
                assert_eq!(
                    page.definition_positions()[0].absolute_index(),
                    first_absolute
                );
                if count > 1 {
                    // Final definition immediately precedes termination, even
                    // when pointer extensions occupy physical X3/X4.
                    assert_eq!(
                        page.definition_positions().last().unwrap().absolute_index(),
                        505
                    );
                }
                assert_eq!(fields(words(row(&page, 26, 15))[12]), (63, 31, 3));
                assert_eq!(body_words(&page).len(), capacity);
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.resource, owner);
                assert!(error.target.is_some());
                assert_eq!(
                    error.reason,
                    TeletextObjectPageEncodingFailure::Capacity {
                        count: capacity + 1,
                        maximum: capacity
                    }
                );
            }
        }
    }
}

#[test]
fn command_budget_precedes_lowering_and_counts_only_the_exact_resource() {
    let owner = resource(TeletextResourceKind::Pop, 0);
    let other = resource(TeletextResourceKind::Gpop, 0);
    let input = catalog(vec![
        definition(owner, 1, TeletextObjectType::Active, vec![position(0, 8)]),
        definition(owner, 3, TeletextObjectType::Active, vec![foreground()]),
        definition(
            other,
            2,
            TeletextObjectType::Active,
            vec![foreground(); 500],
        ),
    ]);
    let allocations = allocate_teletext_resource_objects(&input, &[], 128).unwrap();
    let error = encode_teletext_object_page(owner, &input, &allocations, transmission(true), 0)
        .unwrap_err();
    assert_eq!(error.target, Some(target(owner, 1)));
    assert_eq!(error.command, None);
    assert_eq!(
        error.reason,
        TeletextObjectPageEncodingFailure::CommandBudget { maximum: 0 }
    );
    let error = encode_teletext_object_page(owner, &input, &allocations, transmission(true), 1)
        .unwrap_err();
    assert_eq!(error.target, Some(target(owner, 3)));
    assert_eq!(error.command, None);
    assert_eq!(
        error.reason,
        TeletextObjectPageEncodingFailure::CommandBudget { maximum: 1 }
    );
    let encoded =
        encode_teletext_object_page(owner, &input, &allocations, transmission(true), 2).unwrap();
    assert_eq!(encoded.definition_positions().len(), 2);
    assert_eq!(encoded.command_positions().len(), 1);
    assert_eq!(
        encoded.command_positions()[0].source().object,
        Some(target(owner, 3))
    );
}

#[test]
fn incomplete_or_mistyped_catalogue_allocation_pairs_fail_without_rebinding() {
    let owner = resource(TeletextResourceKind::Pop, 0);
    let original = catalog(vec![definition(
        owner,
        65535,
        TeletextObjectType::Active,
        vec![],
    )]);
    let allocations = allocate_teletext_resource_objects(&original, &[], 128).unwrap();
    let empty = catalog(vec![]);
    let error = encode_teletext_object_page(owner, &empty, &allocations, transmission(true), 0)
        .unwrap_err();
    assert_eq!(error.target, Some(target(owner, 65535)));
    assert_eq!(
        error.reason,
        TeletextObjectPageEncodingFailure::MissingDefinition
    );
    let changed = catalog(vec![definition(
        owner,
        65535,
        TeletextObjectType::Passive,
        vec![],
    )]);
    let error = encode_teletext_object_page(owner, &changed, &allocations, transmission(true), 0)
        .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextObjectPageEncodingFailure::Type {
            expected: TeletextObjectType::Active,
            actual: TeletextObjectType::Passive
        }
    );
    let empty_allocations = allocate_teletext_resource_objects(&empty, &[], 0).unwrap();
    let error =
        encode_teletext_object_page(owner, &original, &empty_allocations, transmission(true), 0)
            .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextObjectPageEncodingFailure::MissingAllocation
    );
    assert_eq!(error.target, Some(target(owner, 65535)));
    let other_s1 = catalog(vec![definition(
        resource(TeletextResourceKind::Pop, 1),
        65535,
        TeletextObjectType::Active,
        vec![],
    )]);
    let other_allocations = allocate_teletext_resource_objects(&other_s1, &[], 1).unwrap();
    let error =
        encode_teletext_object_page(owner, &original, &other_allocations, transmission(true), 0)
            .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextObjectPageEncodingFailure::MissingAllocation
    );
    assert_eq!(error.target, Some(target(owner, 65535)));
}

#[test]
fn adaptive_and_passive_colour_rejections_retain_exact_object_and_command() {
    let owner = resource(TeletextResourceKind::Pop, 0);
    for object_type in [TeletextObjectType::Adaptive, TeletextObjectType::Passive] {
        for colour_command in [
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
            let input = catalog(vec![definition(
                owner,
                65535,
                object_type,
                vec![position(1, 8), colour_command],
            )]);
            let allocations = allocate_teletext_resource_objects(&input, &[], 1).unwrap();
            let error =
                encode_teletext_object_page(owner, &input, &allocations, transmission(true), 2)
                    .unwrap_err();
            assert_eq!(error.resource, owner);
            assert_eq!(error.target, Some(target(owner, 65535)));
            assert_eq!(error.command, Some(1));
            assert_eq!(
                error.reason,
                TeletextObjectPageEncodingFailure::ForbiddenColour { object_type }
            );
        }
    }
}

fn drcs_command(column: u8, scope: TeletextDrcsTableScope) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                table: scope,
                subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
            },
        },
    }
}

#[test]
fn definitions_reset_ap_and_both_drcs_selectors_instead_of_inheriting_previous_object() {
    let owner = resource(TeletextResourceKind::Pop, 0);
    let input = catalog(vec![
        definition(
            owner,
            1,
            TeletextObjectType::Active,
            vec![
                position(20, 38),
                drcs_command(38, TeletextDrcsTableScope::Global),
                drcs_command(38, TeletextDrcsTableScope::Normal),
            ],
        ),
        definition(
            owner,
            2,
            TeletextObjectType::Active,
            vec![
                position(1, 8),
                drcs_command(8, TeletextDrcsTableScope::Global),
                drcs_command(8, TeletextDrcsTableScope::Normal),
            ],
        ),
    ]);
    let before = input.clone();
    let page = encode(owner, &input, true);
    assert_eq!(input, before);
    assert_eq!(page.definition_positions()[1].absolute_index(), 6);
    let actual: Vec<_> = body_words(&page).into_iter().map(fields).collect();
    assert_eq!(
        &actual[..12],
        &[
            (40, 21, 0),
            (60, 4, 38),
            (40, 24, 16),
            (38, 13, 47),
            (40, 24, 80),
            (38, 13, 111),
            (40, 21, 16),
            (41, 4, 8),
            (40, 24, 16),
            (8, 13, 47),
            (40, 24, 80),
            (8, 13, 111)
        ]
    );
    assert_eq!(actual[12], (63, 31, 3));
    assert_eq!(
        command_spans(&page),
        [
            (source(target(owner, 1), 0), 1, 1),
            (source(target(owner, 1), 1), 2, 3),
            (source(target(owner, 1), 2), 4, 5),
            (source(target(owner, 2), 0), 7, 7),
            (source(target(owner, 2), 1), 8, 9),
            (source(target(owner, 2), 2), 10, 11),
        ]
    );
}

#[test]
fn nested_invocation_uses_exact_other_resource_map_and_immediate_origin_modifier() {
    let owner = resource(TeletextResourceKind::Pop, 3);
    let global = resource(TeletextResourceKind::Gpop, 15);
    let child = target(global, 65535);
    let mut items: Vec<_> = (0..32)
        .map(|id| definition(global, 65504 + id, TeletextObjectType::Passive, vec![]))
        .collect();
    items.push(definition(
        owner,
        65535,
        TeletextObjectType::Active,
        vec![
            position(1, 8),
            TeletextEnhancementCommand::InvokeObject {
                target: child,
                object_type: TeletextObjectType::Passive,
                offset: Some(TeletextObjectOffset::new(23, 71).unwrap()),
            },
            foreground(),
        ],
    ));
    let input = catalog(items);
    let page = encode(owner, &input, true);
    assert_eq!(page.definition_positions().len(), 1);
    let actual: Vec<_> = body_words(&page).into_iter().map(fields).collect();
    assert_eq!(
        &actual[..6],
        &[
            (40, 21, 3),
            (41, 4, 8),
            (63, 16, 71),
            (59, 19, 127),
            (8, 0, 5),
            (63, 31, 3)
        ]
    );
    assert_eq!(
        command_spans(&page),
        [
            (source(target(owner, 65535), 0), 1, 1),
            (source(target(owner, 65535), 1), 2, 3),
            (source(target(owner, 65535), 2), 4, 4),
        ]
    );
}

#[test]
fn resource_purpose_rejects_non_object_pages_before_making_units() {
    let owner = resource(TeletextResourceKind::Drcs, 0);
    let input = catalog(vec![]);
    let allocations = allocate_teletext_resource_objects(&input, &[], 0).unwrap();
    let error = encode_teletext_object_page(owner, &input, &allocations, transmission(true), 0)
        .unwrap_err();
    assert_eq!(error.resource, owner);
    assert_eq!(error.target, None);
    assert_eq!(error.command, None);
    assert_eq!(error.reason, TeletextObjectPageEncodingFailure::Kind);
}

fn command_spans(
    page: &EncodedTeletextObjectPage,
) -> Vec<(TeletextEnhancementSource, usize, usize)> {
    page.command_positions()
        .iter()
        .map(|position| {
            (
                position.source(),
                position.first_word(),
                position.semantic_word(),
            )
        })
        .collect()
}

fn source(identity: TeletextObjectIdentity, command: usize) -> TeletextEnhancementSource {
    TeletextEnhancementSource {
        object: Some(identity),
        command,
    }
}

#[test]
fn command_positions_omit_identity_moves_preserve_ordinals_and_raw_budget() {
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        let owner = resource(kind, 15);
        for object_type in [
            TeletextObjectType::Active,
            TeletextObjectType::Adaptive,
            TeletextObjectType::Passive,
        ] {
            let input = catalog(vec![definition(
                owner,
                65535,
                object_type,
                vec![position(0, 0), position(0, 0), foreground()],
            )]);
            let saved = input.clone();
            let page = encode(owner, &input, true);
            let clean = encode(
                owner,
                &catalog(vec![definition(
                    owner,
                    65535,
                    object_type,
                    vec![foreground()],
                )]),
                true,
            );
            assert_eq!(page.data_units(), clean.data_units());
            assert_eq!(page.definition_positions(), clean.definition_positions());
            assert_eq!(
                command_spans(&page),
                [(source(target(owner, 65535), 2), 1, 1)]
            );
            assert_eq!(fields(words(row(&page, 3, 0))[1]), (8, 0, 5));
            assert_eq!(fields(words(row(&page, 3, 0))[2]), (63, 31, 3));
            let allocations = allocate_teletext_resource_objects(&input, &[], 128).unwrap();
            let failure =
                encode_teletext_object_page(owner, &input, &allocations, transmission(true), 2)
                    .unwrap_err();
            assert_eq!(failure.target, Some(target(owner, 65535)));
            assert_eq!(failure.command, None);
            assert_eq!(
                failure.reason,
                TeletextObjectPageEncodingFailure::CommandBudget { maximum: 2 }
            );
            assert_eq!(input, saved);
        }
    }
}

#[test]
fn command_positions_include_first_x3_pointer_extensions_without_marker_records() {
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        let owner = resource(kind, 7);
        for (definitions, origin, physical_row) in
            [(1_u16, 0_usize, 3_u8), (17, 13, 4), (25, 26, 5)]
        {
            let input = catalog(
                (0..definitions)
                    .map(|id| {
                        definition(
                            owner,
                            id,
                            TeletextObjectType::Active,
                            if id == 0 { vec![foreground()] } else { vec![] },
                        )
                    })
                    .collect(),
            );
            let page = encode(owner, &input, true);
            assert_eq!(
                usize::from(page.definition_positions()[0].absolute_index()),
                origin
            );
            assert_eq!(
                command_spans(&page),
                [(source(target(owner, 0), 0), origin + 1, origin + 1)]
            );
            let actual = words(row(&page, physical_row, 0));
            assert_eq!(fields(actual[0]), (40, 21, 7));
            assert_eq!(fields(actual[1]), (8, 0, 5));
            assert_eq!(page.definition_positions().len(), usize::from(definitions));
        }
    }
}

#[test]
fn command_positions_match_literal_x25_x26_and_last_designation_words() {
    let owner = resource(TeletextResourceKind::Gpop, 2);
    let input = catalog(vec![definition(
        owner,
        65535,
        TeletextObjectType::Active,
        vec![foreground(); 505],
    )]);
    let page = encode(owner, &input, true);
    assert_eq!(page.command_positions().len(), 505);
    for (ordinal, absolute, packet_number, designation, triplet) in [
        (0_usize, 1_usize, 3_u8, 0_u8, 1_usize),
        (297, 298, 25, 0, 12),
        (298, 299, 26, 0, 0),
        (311, 312, 26, 1, 0),
        (504, 505, 26, 15, 11),
    ] {
        let position = page.command_positions()[ordinal];
        assert_eq!(position.source(), source(target(owner, 65535), ordinal));
        assert_eq!(position.first_word(), absolute);
        assert_eq!(position.semantic_word(), absolute);
        assert_eq!(
            fields(words(row(&page, packet_number, designation))[triplet]),
            (8, 0, 5)
        );
    }
    assert_eq!(fields(words(row(&page, 3, 0))[0]), (40, 21, 2));
    assert_eq!(fields(words(row(&page, 26, 15))[12]), (63, 31, 3));
}

#[test]
fn command_positions_follow_actual_default_reserved_assignment_not_catalog_order() {
    let owner = resource(TeletextResourceKind::Pop, 9);
    let input = catalog(vec![
        definition(owner, 10, TeletextObjectType::Active, vec![foreground()]),
        definition(owner, 20, TeletextObjectType::Active, vec![foreground()]),
    ]);
    let allocations = allocate_teletext_resource_objects(
        &input,
        &[TeletextDefaultObjectSelection {
            target: target(owner, 20),
            object_type: TeletextObjectType::Active,
        }],
        128,
    )
    .unwrap();
    let page = encode_teletext_object_page(owner, &input, &allocations, transmission(true), 2_000)
        .unwrap();
    assert_eq!(
        command_spans(&page),
        [
            (source(target(owner, 20), 0), 1, 1),
            (source(target(owner, 10), 0), 3, 3),
        ]
    );
    let actual = words(row(&page, 3, 0));
    assert_eq!(fields(actual[0]), (40, 21, 9));
    assert_eq!(fields(actual[1]), (8, 0, 5));
    assert_eq!(fields(actual[2]), (40, 21, 25));
    assert_eq!(fields(actual[3]), (8, 0, 5));
    assert_eq!(fields(actual[4]), (63, 31, 3));
}

#[test]
fn compound_drcs_range_crosses_x25_to_x26_and_names_the_glyph_not_selector() {
    let owner = resource(TeletextResourceKind::Pop, 6);
    let mut commands = vec![position(1, 8)];
    commands.extend(std::iter::repeat_n(foreground(), 296));
    commands.push(drcs_command(8, TeletextDrcsTableScope::Normal));
    let input = catalog(vec![definition(
        owner,
        65535,
        TeletextObjectType::Active,
        commands,
    )]);
    let page = encode(owner, &input, true);
    assert_eq!(page.command_positions().len(), 298);
    let position = page.command_positions()[297];
    assert_eq!(position.source(), source(target(owner, 65535), 297));
    assert_eq!(position.first_word(), 298);
    assert_eq!(position.semantic_word(), 299);
    assert_eq!(fields(words(row(&page, 25, 0))[12]), (40, 24, 80));
    assert_eq!(fields(words(row(&page, 26, 0))[0]), (8, 13, 111));
    assert_eq!(fields(words(row(&page, 26, 0))[1]), (63, 31, 3));
}

//! Actual writer inputs with literal field/extent oracles; no receiver repair.
use super::*;
use crate::teletext::{
    allocate_teletext_resource_objects, encode_teletext_object_page,
    encode_teletext_resource_header,
    packets::{hamming_24_18, hamming_8_4},
    TeletextObjectPageTransmission, TeletextResourceFullUpdate,
};
use ersatzrs_domain::{
    TeletextColourReference, TeletextColumnOperation, TeletextEnhancementCommand,
    TeletextEnhancementObjectCatalog, TeletextEnhancementObjectDefinition,
    TeletextEnhancementProgram, TeletextObjectCatalogBudget, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectType, TeletextRawDataPageNumber,
    TeletextResourceIdentity, TeletextResourceSubpageIndex, TeletextResourceUpdateCounter,
};

fn owner(kind: TeletextResourceKind) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap()
}
fn foreground() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 8,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(5).unwrap(),
        },
    }
}
fn definition(
    resource: TeletextResourceIdentity,
    id: u16,
    object_type: TeletextObjectType,
    count: usize,
) -> TeletextEnhancementObjectDefinition {
    let id = TeletextObjectId::new(id);
    let identity = if resource.kind() == TeletextResourceKind::Pop {
        TeletextObjectIdentity::public(resource, id).unwrap()
    } else {
        TeletextObjectIdentity::global(resource, id).unwrap()
    };
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: TeletextEnhancementProgram::new(vec![foreground(); count]).unwrap(),
    }
}
fn encoded(
    resource: TeletextResourceIdentity,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    mode: TeletextResourceTransmissionMode,
    changed: bool,
) -> EncodedTeletextObjectPage {
    let catalog = TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 128,
            max_commands: 2_000,
        },
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 128).unwrap();
    encode_teletext_object_page(
        resource,
        &catalog,
        &allocations,
        TeletextObjectPageTransmission {
            counter: TeletextResourceUpdateCounter::new(7).unwrap(),
            update: if changed {
                TeletextResourceFullUpdate::Changed
            } else {
                TeletextResourceFullUpdate::Unchanged
            },
            mode,
            last_subpage: true,
        },
        2_000,
    )
    .unwrap()
}
fn sample() -> EncodedTeletextObjectPage {
    let resource = owner(TeletextResourceKind::Pop);
    encoded(
        resource,
        vec![definition(resource, 65535, TeletextObjectType::Passive, 1)],
        TeletextResourceTransmissionMode::Serial,
        true,
    )
}
fn project(
    page: &EncodedTeletextObjectPage,
    units: &[[u8; 46]],
) -> Result<TeletextEncodedObjectProjection, TeletextEncodedDisplayProjectionError> {
    project_units(page, units, 43)
}
fn replace_word(unit: &mut [u8; 46], slot: usize, word: u32) {
    for (target, byte) in unit[7 + slot * 3..10 + slot * 3]
        .iter_mut()
        .zip(hamming_24_18(word))
    {
        *target = byte.reverse_bits();
    }
}

#[test]
fn complete_object_projection_retains_both_purposes_all_types_and_native_lifecycle() {
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        for mode in [
            TeletextResourceTransmissionMode::Serial,
            TeletextResourceTransmissionMode::Parallel,
        ] {
            for changed in [false, true] {
                let resource = owner(kind);
                let definitions = [
                    TeletextObjectType::Active,
                    TeletextObjectType::Adaptive,
                    TeletextObjectType::Passive,
                ]
                .into_iter()
                .enumerate()
                .map(|(ordinal, kind)| {
                    definition(resource, 65535 - u16::try_from(ordinal).unwrap(), kind, 1)
                })
                .collect();
                let page = encoded(resource, definitions, mode, changed);
                let positions = page.definition_positions().to_vec();
                let commands = page.command_positions().to_vec();
                let original_units = page.data_units().to_vec();
                let authored = EncodedTeletextAuthoredPage::Object(page.clone());
                let result = project_encoded_teletext_object(&authored, 5).unwrap();
                assert_eq!(result.resource_header, page.header());
                assert_eq!(result.transmission_mode, mode);
                assert_eq!(result.header.wire_page, 0xa0);
                assert_eq!(result.header.subcode, 0x037f);
                assert_eq!(result.header.controls.erase_page(), changed);
                assert_eq!(result.pointer_rows, 2);
                assert_eq!(result.packets.len(), 3);
                assert_eq!(
                    result.function_words[0],
                    0xffa0 | u32::from(kind.function_code())
                );
                assert_eq!(result.function_words[1..], [0; 12]);
                assert_eq!(result.packets[0].designation, 1);
                assert_eq!(result.packets[1].designation, 1);
                assert_eq!(result.packets[2].designation, 0);
                assert_eq!(result.packets[0].words[0], 0);
                for (ordinal, kind) in [1_u32, 2, 3].into_iter().enumerate() {
                    assert_eq!(
                        result.packets[0].words[usize::try_from(kind).unwrap()],
                        (511 << 9) | u32::try_from(ordinal * 2).unwrap()
                    );
                    assert_eq!(
                        result.packets[2].words[ordinal * 2],
                        0x28 | ((0x14 | kind) << 6) | (0x0f << 11)
                    );
                    assert_eq!(result.packets[2].words[ordinal * 2 + 1], 8 | (5 << 11));
                }
                assert_eq!(&result.packets[2].words[6..], &[0x1fff; 7]);
                assert_eq!(page.data_units(), original_units);
                assert_eq!(page.definition_positions(), positions);
                assert_eq!(page.command_positions(), commands);
                assert!(!format!("{result:?}").contains("function_words"));
                assert!(!format!("{:?}", result.packets[0]).contains("words"));
            }
        }
    }
}

#[test]
fn pointer_extensions_and_maximum507_content_keep_first_x3_offsets() {
    let resource = owner(TeletextResourceKind::Pop);
    for (count, capacity, pointer_rows, first_offset) in [
        (1_u16, 507_usize, 2_u8, 0_u16),
        (17, 494, 3, 13),
        (25, 481, 4, 26),
    ] {
        let definitions = (0..count)
            .map(|id| {
                definition(
                    resource,
                    65535 - id,
                    TeletextObjectType::Active,
                    if id == 0 {
                        capacity - usize::from(count) - 1
                    } else {
                        0
                    },
                )
            })
            .collect();
        let page = encoded(
            resource,
            definitions,
            TeletextResourceTransmissionMode::Serial,
            true,
        );
        let result = project(&page, page.data_units()).unwrap();
        assert_eq!(result.pointer_rows, pointer_rows);
        assert_eq!(result.packets.len(), 41);
        assert_eq!(
            page.definition_positions()[0].absolute_index(),
            first_offset
        );
        assert_eq!(result.packets.last().unwrap().logical_row, 41);
        assert_eq!(result.packets.last().unwrap().designation, 15);
        assert_eq!(result.packets.last().unwrap().words[12], 0x1fff);
        let body = &result.packets[usize::from(pointer_rows)..];
        assert_eq!(body.len() * 13, capacity);
        assert_eq!(body[0].logical_row, pointer_rows + 1);
        assert_eq!(body[0].designation, 0);
        for position in page.command_positions() {
            let offset = position.semantic_word();
            let packet = &result.packets[2 + offset / 13];
            assert_eq!(usize::from(packet.logical_row), 3 + offset / 13);
            assert_eq!(packet.words[offset % 13], 8 | (5 << 11));
        }
    }
}

#[test]
fn empty_object_resource_preserves_unused_pointers_and_actual_termination() {
    for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
        let page = encoded(
            owner(kind),
            vec![],
            TeletextResourceTransmissionMode::Parallel,
            false,
        );
        let result = project(&page, page.data_units()).unwrap();
        assert_eq!(result.pointer_rows, 2);
        assert_eq!(result.resource_header.last_packet().get(), 3);
        for packet in &result.packets[..2] {
            assert_eq!(packet.words[0], 0);
            assert_eq!(packet.words[1..], [0x3ffff; 12]);
        }
        assert_eq!(result.packets[2].words, [0x1fff; 13]);
    }
}

#[test]
fn object_projection_bounds_precede_allocation_and_reject_incomplete_inventory() {
    let page = sample();
    for (units, maximum) in [
        (page.data_units(), 0),
        (page.data_units(), 4),
        (&[][..], 43),
    ] {
        assert_eq!(
            project_units(&page, units, maximum).unwrap_err().failure,
            Failure::Budget
        );
    }
    let mut units = page.data_units().to_vec();
    units.push(units[4]);
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        Failure::Identity
    );
}

#[test]
fn object_inverse_rejects_single_bit_protection_and_parity_damage_without_correction() {
    let page = sample();
    for (index, byte) in [
        (0, 4),
        (0, 6),
        (1, 4),
        (1, 6),
        (1, 7),
        (2, 6),
        (2, 7),
        (4, 7),
    ] {
        let mut units = page.data_units().to_vec();
        units[index][byte] ^= 1;
        let error = project(&page, &units).unwrap_err();
        assert_eq!(
            error.failure,
            Failure::Protection,
            "unit {index}, byte {byte}"
        );
        assert_eq!(error.unit, Some(index));
    }
    let mut units = page.data_units().to_vec();
    units[0][14] ^= 1;
    let error = project(&page, &units).unwrap_err();
    assert_eq!(error.failure, Failure::Parity);
    assert_eq!(
        (error.unit, error.row, error.column),
        (Some(0), Some(0), Some(8))
    );
}

#[test]
fn object_inverse_refuses_clean_function_coding_and_native_header_identity_changes() {
    let page = sample();
    for (slot, word) in [(0, 0xffa2), (0, 0xff83), (1, 1)] {
        let mut units = page.data_units().to_vec();
        replace_word(&mut units[1], slot, word);
        let error = project(&page, &units).unwrap_err();
        assert_eq!(error.failure, Failure::Identity);
        assert_eq!(error.unit, Some(1));
    }
    let metadata = TeletextResourceHeaderMetadata::new(
        page.header().identity(),
        page.header().last_packet(),
        TeletextResourceUpdateCounter::new(6).unwrap(),
        page.header().transmission(),
    )
    .unwrap();
    for replacement in [
        encode_teletext_resource_header(metadata, page.transmission_mode()),
        encode_teletext_resource_header(page.header(), TeletextResourceTransmissionMode::Parallel),
    ] {
        let mut units = page.data_units().to_vec();
        units[0] = replacement;
        assert_eq!(
            project(&page, &units).unwrap_err().failure,
            Failure::Identity
        );
    }
}

#[test]
fn object_inverse_refuses_wrong_pointer_content_designations_and_packet_order() {
    let page = sample();
    for (index, designation) in [(1, 1), (2, 0), (4, 1)] {
        let mut units = page.data_units().to_vec();
        units[index][6] = hamming_8_4(designation).reverse_bits();
        let error = project(&page, &units).unwrap_err();
        assert_eq!(error.failure, Failure::UnsupportedPacket);
        assert_eq!(error.unit, Some(index));
    }
    let mut units = page.data_units().to_vec();
    units.swap(2, 3);
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        Failure::UnsupportedPacket
    );
    units[2] = units[0];
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        Failure::HeaderOrder
    );
    let mut units = page.data_units().to_vec();
    units[2][0] = 3;
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        Failure::Identity
    );
    let mut units = page.data_units().to_vec();
    units[2][2] = 0;
    assert_eq!(
        project(&page, &units).unwrap_err().failure,
        Failure::Envelope
    );
}

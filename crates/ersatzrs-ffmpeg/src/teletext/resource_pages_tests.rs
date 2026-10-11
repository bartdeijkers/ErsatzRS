//! Literal header fixtures and independently decoded resource packet sequences.

use ersatzrs_domain::*;

use super::source_header::{observe_dvb_teletext_header, TeletextHeaderObservation};
use super::*;

fn identity(
    kind: TeletextResourceKind,
    magazine: u8,
    page: u8,
    s1: u8,
) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(magazine, page).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}

fn metadata(
    owner: TeletextResourceIdentity,
    last: u8,
    counter: u8,
    state: TeletextResourceTransmission,
) -> TeletextResourceHeaderMetadata {
    TeletextResourceHeaderMetadata::new(
        owner,
        TeletextResourceLastPacket::new(last).unwrap(),
        TeletextResourceUpdateCounter::new(counter).unwrap(),
        state,
    )
    .unwrap()
}

/// Independent normative Hamming8/4 table, after undoing DVB bit order.
fn nibble(wire: u8) -> u8 {
    const WORDS: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    u8::try_from(
        WORDS
            .iter()
            .position(|word| *word == wire.reverse_bits())
            .unwrap(),
    )
    .unwrap()
}

fn packet(unit: &[u8; 46]) -> (u8, u8) {
    assert_eq!(&unit[..4], &[2, 44, 0xe0, 0xe4]);
    let address = nibble(unit[4]) | (nibble(unit[5]) << 4);
    (address & 7, address >> 3)
}

fn body(unit: &[u8; 46]) -> [u8; 40] {
    std::array::from_fn(|offset| unit[offset + 6].reverse_bits())
}

/// Independent Hamming24/18 parity equations and data-bit positions.
fn triplet(bytes: &[u8]) -> u32 {
    let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0]);
    assert_eq!(word.count_ones() % 2, 1);
    for parity in [1_u32, 2, 4, 8, 16] {
        let count: u32 = (1..24)
            .filter(|position| position & parity != 0)
            .map(|position| (word >> (position - 1)) & 1)
            .sum();
        assert_eq!(count % 2, 1);
    }
    [
        3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
    ]
    .into_iter()
    .enumerate()
    .fold(0, |data, (bit, position)| {
        data | (((word >> (position - 1)) & 1) << bit)
    })
}

#[test]
fn literal_changed_drcs_header_preserves_raw_page_and_resource_flags() {
    let model = metadata(
        identity(TeletextResourceKind::Drcs, 8, 0xa0, 15),
        24,
        0,
        TeletextResourceTransmission::FullChangedFirst,
    );
    let unit = encode_teletext_resource_header(model, TeletextResourceTransmissionMode::Serial);
    assert_eq!(
        &unit[6..14],
        &[0xa8, 0x31, 0x57, 0x0b, 0x0b, 0x40, 0x57, 0x40]
    );
    assert_eq!(&unit[14..], &[0x04; 32]);
    assert_eq!(packet(&unit), (0, 0));
    let TeletextHeaderObservation::Header(observed) = observe_dvb_teletext_header(&unit).unwrap()
    else {
        panic!("expected resource header")
    };
    assert_eq!(observed.wire_page, 0xa0);
    assert_eq!(observed.subcode, 0x180f);
    assert_eq!(observed.controls.raw(), 0xf9);
    assert_eq!(observed.corrected_header_bytes, 0);
    assert_eq!(observed.packet_address.corrected_address_bytes, 0);
}

#[test]
fn resource_header_geometry_covers_magazines_subpages_counters_last_packets_and_states() {
    use TeletextResourceTransmission as State;
    // Independent table B.1 literals: C4,C8,C12,C13.
    let states = [
        (State::FullUnchangedFirst, [0, 0, 0, 0]),
        (State::FullUnchangedContinuation, [0, 0, 1, 0]),
        (State::FullChangedFirst, [1, 1, 0, 0]),
        (State::FullChangedContinuation, [0, 1, 1, 0]),
        (State::PartialFirst, [0, 1, 0, 1]),
        (State::PartialContinuation, [0, 1, 1, 1]),
    ];
    for magazine in 1..=8 {
        for s1 in 0..16 {
            for counter in 0..8 {
                for last in 1..=41 {
                    for (state, [c4, c8, c12, c13]) in states {
                        for mode in [
                            TeletextResourceTransmissionMode::Serial,
                            TeletextResourceTransmissionMode::Parallel,
                        ] {
                            let unit = encode_teletext_resource_header(
                                metadata(
                                    identity(TeletextResourceKind::Pop, magazine, 0xa0, s1),
                                    last,
                                    counter,
                                    state,
                                ),
                                mode,
                            );
                            assert_eq!(packet(&unit), (magazine & 7, 0));
                            let n = std::array::from_fn::<_, 8, _>(|i| nibble(unit[6 + i]));
                            assert_eq!(
                                n,
                                [
                                    0,
                                    10,
                                    s1,
                                    counter | (c4 << 3),
                                    last & 15,
                                    last >> 4,
                                    0b1101 | (c8 << 1),
                                    u8::from(mode == TeletextResourceTransmissionMode::Serial)
                                        | (c12 << 1)
                                        | (c13 << 2)
                                ]
                            );
                        }
                    }
                }
            }
        }
    }
}

fn drcs(
    kind: TeletextResourceKind,
    glyphs: Vec<TeletextDrcsGlyphDefinition>,
) -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(identity(kind, 8, 0xa0, 15), glyphs).unwrap()
}

fn glyph(slot: u8) -> TeletextDrcsGlyphDefinition {
    TeletextDrcsGlyphDefinition {
        slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
        bitmap: TeletextDrcsBitmap::new([0x801; 10]).unwrap(),
    }
}

#[test]
fn every_drcs_slot_derives_actual_last_row_after_x28_and_preserves_sparse_bodies() {
    for kind in [TeletextResourceKind::Drcs, TeletextResourceKind::Gdrcs] {
        for slot in 0..48 {
            for update in [
                TeletextResourceFullUpdate::Changed,
                TeletextResourceFullUpdate::Unchanged,
            ] {
                let table = drcs(kind, vec![glyph(slot)]);
                let page = encode_teletext_drcs_page(
                    &table,
                    TeletextResourceUpdateCounter::new(7).unwrap(),
                    update,
                    TeletextResourceTransmissionMode::Serial,
                )
                .unwrap();
                let units = page.data_units();
                assert_eq!(units.len(), 3);
                assert_eq!(
                    units.iter().map(packet).collect::<Vec<_>>(),
                    [(0, 0), (0, 28), (0, 1 + slot / 2)]
                );
                assert_eq!(page.header().last_packet().get(), 1 + slot / 2);
                assert_eq!(page.header().identity(), table.identity());
                assert_eq!(page.header().update_counter().get(), 7);
                assert_eq!(
                    page.transmission_mode(),
                    TeletextResourceTransmissionMode::Serial
                );
                let TeletextHeaderObservation::Header(header) =
                    observe_dvb_teletext_header(&units[0]).unwrap()
                else {
                    panic!("expected resource header")
                };
                assert_eq!(header.subcode, (u16::from(1 + slot / 2) << 8) | 0x7f);
                assert_eq!(
                    header.controls.erase_page(),
                    update == TeletextResourceFullUpdate::Changed
                );
                let expected = encode_teletext_drcs_bodies(&table);
                assert_eq!(body(&units[1]), expected.modes);
                assert_eq!(
                    body(&units[2]),
                    expected.rows[usize::from(slot / 2)].unwrap()
                );
            }
        }
    }
}

#[test]
fn empty_drcs_has_legal_space_row_and_no_defined_glyphs() {
    let table = drcs(TeletextResourceKind::Gdrcs, vec![]);
    let page = encode_teletext_drcs_page(
        &table,
        TeletextResourceUpdateCounter::new(0).unwrap(),
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Parallel,
    )
    .unwrap();
    assert_eq!(page.header().last_packet().get(), 1);
    assert_eq!(
        page.data_units().iter().map(packet).collect::<Vec<_>>(),
        [(0, 0), (0, 28), (0, 1)]
    );
    assert_eq!(body(&page.data_units()[2]), [0x20; 40]);
    let modes = body(&page.data_units()[1]);
    let words: Vec<_> = modes[1..].chunks_exact(3).map(triplet).collect();
    assert_eq!(words[0], 4);
    for slot in 0..48 {
        let mode = (0..4).fold(0, |mode, bit| {
            let offset = 18 + 4 * slot + bit;
            mode | (((words[offset / 18] >> (offset % 18)) & 1) << bit)
        });
        assert_eq!(mode, 15);
    }
}

#[test]
fn full_drcs_rows_are_ascending_bounded_and_input_order_independent() {
    let table = drcs(
        TeletextResourceKind::Drcs,
        (0..48).rev().map(glyph).collect(),
    );
    let page = encode_teletext_drcs_page(
        &table,
        TeletextResourceUpdateCounter::new(3).unwrap(),
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap();
    assert_eq!(page.data_units().len(), 26);
    assert_eq!(page.header().last_packet().get(), 24);
    assert_eq!(
        page.data_units()[2..]
            .iter()
            .map(|unit| packet(unit).1)
            .collect::<Vec<_>>(),
        (1..=24).collect::<Vec<_>>()
    );
}

fn mot() -> TeletextMotLevel25Declaration {
    let target = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(8).unwrap(),
    };
    let object = TeletextMotObjectLink::new(
        target,
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let drcs = TeletextMotDrcsLink::new(target, TeletextMotSubpageExtent::new(0).unwrap(), false);
    TeletextMotLevel25Declaration::new(
        identity(TeletextResourceKind::Mot, 2, 0xfe, 7),
        vec![],
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn mot_complete_sequence_preserves_rows_and_optional_function_compatibility_prefix() {
    let model = mot();
    let allocations = [[None; 2]; 8];
    let page = encode_teletext_mot_page(
        &model,
        &allocations,
        TeletextResourceUpdateCounter::new(5).unwrap(),
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap();
    let units = page.data_units();
    assert_eq!(units.len(), 19);
    assert_eq!(
        units.iter().map(|unit| packet(unit).1).collect::<Vec<_>>(),
        [0, 28, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 19, 20, 21]
    );
    assert_eq!(page.header().physical_subcode(), 0x1557);
    assert!(units.iter().all(|unit| packet(unit).0 == 2));
    let declaration = body(&units[1]);
    assert_eq!(declaration[0], 0x15);
    let words: Vec<_> = declaration[1..].chunks_exact(3).map(triplet).collect();
    assert_eq!(words[0], 0x0ffb6);
    assert_eq!(&words[1..], &[0; 12]);
    for (unit, packet) in units[2..]
        .iter()
        .zip(encode_teletext_mot_bodies(&model, &allocations).unwrap())
    {
        assert_eq!(body(unit), packet.body);
    }
    let invalid = TeletextMotDefaultAllocation {
        target: TeletextObjectIdentity::global(
            identity(TeletextResourceKind::Gpop, 2, 0xa0, 0),
            TeletextObjectId::new(1),
        )
        .unwrap(),
        object_type: TeletextObjectType::Active,
        selector: 0,
    };
    let mut allocations = allocations;
    allocations[0][0] = Some(invalid);
    let error = encode_teletext_mot_page(
        &model,
        &allocations,
        TeletextResourceUpdateCounter::new(5).unwrap(),
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap_err();
    assert!(
        matches!(error, TeletextResourcePageEncodingError::Mot(TeletextMotEncodingError {
        owner, link:0, slot:0, reason:TeletextMotAllocationFailure::Unexpected }) if owner==model.identity())
    );
}

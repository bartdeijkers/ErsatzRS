//! Independently decoded MOT bodies; not complete resource or client acceptance.

use ersatzrs_domain::*;

use super::*;

fn unused() -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(8).unwrap(),
    }
}

fn object(
    target: TeletextX27LinkTarget,
    defaults: [TeletextMotDefaultObject; 2],
) -> TeletextMotObjectLink {
    TeletextMotObjectLink::new(
        target,
        TeletextMotSubpageExtent::new(15).unwrap(),
        true,
        TeletextMotFallback::new(15).unwrap(),
        defaults,
    )
}

fn declaration(
    associations: Vec<TeletextMotPageAssociation>,
    global: TeletextMotObjectLink,
) -> TeletextMotLevel25Declaration {
    let inactive = object(
        unused(),
        [
            TeletextMotDefaultObject::NoObject { raw_number: 0xa5 },
            TeletextMotDefaultObject::Dormant {
                object_type: TeletextObjectType::Adaptive,
                raw_number: 0xc3,
            },
        ],
    );
    let drcs = TeletextMotDrcsLink::new(unused(), TeletextMotSubpageExtent::new(9).unwrap(), true);
    TeletextMotLevel25Declaration::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            TeletextRawDataPageNumber::new(2, 0xfe).unwrap(),
            TeletextResourceSubpageIndex::new(7).unwrap(),
        )
        .unwrap(),
        associations,
        TeletextMotLevel25Links::new(
            global,
            [inactive; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0x81, 0xa5, 0x3c),
        )
        .unwrap(),
    )
    .unwrap()
}

fn inactive_declaration(
    associations: Vec<TeletextMotPageAssociation>,
) -> TeletextMotLevel25Declaration {
    declaration(
        associations,
        object(
            unused(),
            [
                TeletextMotDefaultObject::NoObject { raw_number: 0xa5 },
                TeletextMotDefaultObject::Dormant {
                    object_type: TeletextObjectType::Adaptive,
                    raw_number: 0xc3,
                },
            ],
        ),
    )
}

fn selection(nibble: u8) -> TeletextMotAssociationSelection {
    let public = nibble & 7;
    TeletextMotAssociationSelection::new(
        nibble & 8 != 0,
        if public == 0 {
            None
        } else {
            Some(TeletextMotPublicLinkIndex::new(nibble & 7).unwrap())
        },
    )
}

/// Fixed normative Hamming8/4 codewords, independent of the production writer.
fn decode(body: &[u8; 40]) -> [u8; 40] {
    const CODEWORDS: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    body.map(|byte| u8::try_from(CODEWORDS.iter().position(|word| *word == byte).unwrap()).unwrap())
}

#[test]
fn all_raw_associations_follow_figure20_rows_including_reserved_ff() {
    let associations = (0_u8..=255)
        .map(|page| {
            TeletextMotPageAssociation::new(page, selection(page & 15), selection(page >> 4))
        })
        .collect();
    let model = inactive_declaration(associations);
    assert!(model.association(0xff).is_none());
    let bodies = encode_teletext_mot_bodies(&model, &[[None; 2]; 8]).unwrap();
    assert_eq!(
        bodies.map(|packet| packet.row),
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 19, 20, 21]
    );
    for packet in &bodies[..14] {
        let data = decode(&packet.body);
        let pairs = if packet.row <= 8 {
            20
        } else if packet.row <= 13 {
            18
        } else {
            6
        };
        for pair in 0..pairs {
            // Reconstruct the row listings instead of repeating scatter division.
            let page = if packet.row <= 8 {
                ((2 * (packet.row - 1) + pair / 10) << 4) | (pair % 10)
            } else if packet.row <= 13 {
                ((3 * (packet.row - 9) + pair / 6) << 4) | (10 + pair % 6)
            } else {
                0xf0 | (10 + pair)
            };
            assert_eq!(data[usize::from(pair) * 2], page & 15);
            assert_eq!(data[usize::from(pair) * 2 + 1], page >> 4);
        }
        assert!(data[usize::from(pairs) * 2..]
            .iter()
            .all(|nibble| *nibble == 0));
    }
    for (page, row, pair) in [
        (0x00, 1, 0),
        (0x09, 1, 9),
        (0x0a, 9, 0),
        (0x0f, 9, 5),
        (0x10, 1, 10),
        (0x99, 5, 19),
        (0x9a, 12, 0),
        (0xa0, 6, 0),
        (0xfe, 14, 4),
        (0xff, 14, 5),
    ] {
        let packet = bodies.iter().find(|packet| packet.row == row).unwrap();
        assert_eq!(
            &decode(&packet.body)[pair * 2..pair * 2 + 2],
            &[page & 15, page >> 4]
        );
    }
}

#[test]
fn sparse_padding_ignored_selectors_reserved_fallback_and_counts_are_preserved() {
    let model = inactive_declaration(vec![TeletextMotPageAssociation::new(
        0xff,
        selection(13),
        selection(7),
    )]);
    let bodies = encode_teletext_mot_bodies(&model, &[[None; 2]; 8]).unwrap();
    assert!(bodies[..13]
        .iter()
        .all(|packet| decode(&packet.body) == [0; 40]));
    let row14 = decode(&bodies[13].body);
    assert_eq!(&row14[10..12], &[13, 7]);
    assert!(row14[..10].iter().chain(&row14[12..]).all(|n| *n == 0));
    for packet in &bodies[14..16] {
        for record in decode(&packet.body).chunks_exact(10) {
            assert_eq!(record, &[8, 15, 15, 15, 15, 8, 5, 10, 3, 12]);
        }
    }
    let drcs = decode(&bodies[16].body);
    for record in drcs[..32].chunks_exact(4) {
        assert_eq!(record, &[8, 15, 15, 9]);
    }
    assert_eq!(&drcs[32..], &[0, 0, 1, 8, 5, 10, 12, 3]);
}

fn active_model(subpage: u8) -> (TeletextMotLevel25Declaration, TeletextObjectIdentity) {
    let page = TeletextRawDataPageNumber::new(8, 0xa5).unwrap();
    let target = TeletextObjectIdentity::global(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Gpop,
            page,
            TeletextResourceSubpageIndex::new(subpage).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(u16::MAX),
    )
    .unwrap();
    let model = declaration(
        vec![],
        object(
            TeletextX27LinkTarget::Page { page },
            [
                TeletextMotDefaultObject::Object {
                    target,
                    object_type: TeletextObjectType::Passive,
                },
                TeletextMotDefaultObject::NoObject { raw_number: 0xa5 },
            ],
        ),
    );
    (model, target)
}

#[test]
fn every_physical_selector_preserves_s1_without_truncating_semantic_ids() {
    for selector in 0_u8..=255 {
        let (model, target) = active_model(selector & 15);
        let mut allocations = [[None; 2]; 8];
        allocations[0][0] = Some(TeletextMotDefaultAllocation {
            target,
            object_type: TeletextObjectType::Passive,
            selector,
        });
        let packets = encode_teletext_mot_bodies(&model, &allocations).unwrap();
        assert_eq!(
            &decode(&packets[14].body)[..10],
            &[8, 10, 5, 15, 15, 3, selector & 15, selector >> 4, 5, 10]
        );
    }
}

#[test]
fn allocation_failures_are_located_and_no_partial_bodies_are_returned() {
    let (model, target) = active_model(7);
    let mut allocations = [[None; 2]; 8];
    assert_eq!(
        encode_teletext_mot_bodies(&model, &allocations).unwrap_err(),
        TeletextMotEncodingError {
            owner: model.identity(),
            link: 0,
            slot: 0,
            reason: TeletextMotAllocationFailure::Missing
        }
    );
    let allocation = TeletextMotDefaultAllocation {
        target,
        object_type: TeletextObjectType::Passive,
        selector: 0xe7,
    };
    for (bad, reason) in [
        (
            TeletextMotDefaultAllocation {
                selector: 0xe8,
                ..allocation
            },
            TeletextMotAllocationFailure::Subpage,
        ),
        (
            TeletextMotDefaultAllocation {
                object_type: TeletextObjectType::Active,
                ..allocation
            },
            TeletextMotAllocationFailure::TargetOrType,
        ),
        (
            TeletextMotDefaultAllocation {
                target: active_model(8).1,
                ..allocation
            },
            TeletextMotAllocationFailure::TargetOrType,
        ),
    ] {
        allocations[0][0] = Some(bad);
        assert_eq!(
            encode_teletext_mot_bodies(&model, &allocations).unwrap_err(),
            TeletextMotEncodingError {
                owner: model.identity(),
                link: 0,
                slot: 0,
                reason
            }
        );
    }
    allocations[0][0] = Some(allocation);
    allocations[7][1] = Some(allocation);
    assert_eq!(
        encode_teletext_mot_bodies(&model, &allocations).unwrap_err(),
        TeletextMotEncodingError {
            owner: model.identity(),
            link: 7,
            slot: 1,
            reason: TeletextMotAllocationFailure::Unexpected
        }
    );
}

fn distinct_object_link(ordinal: usize) -> TeletextMotObjectLink {
    let page = TeletextRawDataPageNumber::new(
        u8::try_from(ordinal).unwrap() + 1,
        0xa0 + u8::try_from(ordinal).unwrap(),
    )
    .unwrap();
    let identity = TeletextResourceIdentity::new(
        if ordinal == 0 {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        page,
        TeletextResourceSubpageIndex::new(u8::try_from(ordinal).unwrap()).unwrap(),
    )
    .unwrap();
    let defaults = std::array::from_fn(|slot| {
        let id = TeletextObjectId::new(65000 + u16::try_from(slot).unwrap());
        let target = if ordinal == 0 {
            TeletextObjectIdentity::global(identity, id).unwrap()
        } else {
            TeletextObjectIdentity::public(identity, id).unwrap()
        };
        TeletextMotDefaultObject::Object {
            target,
            object_type: if slot == 0 {
                TeletextObjectType::Active
            } else {
                TeletextObjectType::Adaptive
            },
        }
    });
    TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page { page },
        TeletextMotSubpageExtent::new(u8::try_from(ordinal).unwrap()).unwrap(),
        ordinal % 2 == 1,
        TeletextMotFallback::new(u8::try_from(ordinal).unwrap()).unwrap(),
        defaults,
    )
}

#[test]
fn distinct_bank_slots_keep_targets_extents_defaults_and_absolute_magazines() {
    let object_links: [TeletextMotObjectLink; 8] = std::array::from_fn(distinct_object_link);
    let drcs_links: [TeletextMotDrcsLink; 8] = std::array::from_fn(|ordinal| {
        TeletextMotDrcsLink::new(
            TeletextX27LinkTarget::Page {
                page: TeletextRawDataPageNumber::new(
                    8 - u8::try_from(ordinal).unwrap(),
                    0xb0 + u8::try_from(ordinal).unwrap(),
                )
                .unwrap(),
            },
            TeletextMotSubpageExtent::new(15 - u8::try_from(ordinal).unwrap()).unwrap(),
            ordinal % 2 == 0,
        )
    });
    let model = TeletextMotLevel25Declaration::new(
        inactive_declaration(vec![]).identity(),
        vec![],
        TeletextMotLevel25Links::new(
            object_links[0],
            std::array::from_fn(|i| object_links[i + 1]),
            drcs_links[0],
            std::array::from_fn(|i| drcs_links[i + 1]),
            TeletextMotEnhancementCounts::new(0x5a, 0xc3, 0xe7),
        )
        .unwrap(),
    )
    .unwrap();
    let allocations = std::array::from_fn(|ordinal| {
        std::array::from_fn(|slot| {
            let TeletextMotDefaultObject::Object {
                target,
                object_type,
            } = object_links[ordinal].defaults()[slot]
            else {
                panic!("fixture has active default");
            };
            Some(TeletextMotDefaultAllocation {
                target,
                object_type,
                selector: (u8::try_from(slot).unwrap() * 0xf0) | u8::try_from(ordinal).unwrap(),
            })
        })
    });
    let packets = encode_teletext_mot_bodies(&model, &allocations).unwrap();
    for ordinal in 0..8 {
        let data = decode(&packets[14 + ordinal / 4].body);
        let index = 10 * (ordinal % 4);
        let o = u8::try_from(ordinal).unwrap();
        assert_eq!(
            &data[index..index + 10],
            &[(o + 1) & 7 | ((o % 2) << 3), 10, o, o, o, 9, o, 0, o, 15]
        );
        let drcs = decode(&packets[16].body);
        assert_eq!(
            &drcs[4 * ordinal..4 * ordinal + 4],
            &[(8 - o) & 7 | ((1 - o % 2) << 3), 11, o, 15 - o]
        );
    }
    assert_eq!(
        &decode(&packets[16].body)[32..],
        &[0, 0, 10, 5, 3, 12, 7, 14]
    );
}

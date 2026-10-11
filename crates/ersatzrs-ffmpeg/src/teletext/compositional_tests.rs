//! Independently decoded X27/4 fields; not a complete service/client fixture.

use ersatzrs_domain::*;

use super::encode_teletext_compositional_links;

fn owner(magazine: u8) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(u16::from(magazine) * 100).unwrap(),
        subcode: TeletextSubcode::new(0x10).unwrap(),
    }
}

fn links(
    target: TeletextX27LinkTarget,
    validity: TeletextX27LinkValidity,
    mask: u16,
) -> TeletextX27Level25Links {
    let link = TeletextX27LinkDeclaration::new(
        target,
        validity,
        TeletextSubpageRequirementMask::new(mask),
    )
    .unwrap();
    TeletextX27Level25Links::new(link, link, link, link).unwrap()
}

fn page(magazine: u8, page: u8) -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Page {
        page: TeletextRawDataPageNumber::new(magazine, page).unwrap(),
    }
}

fn null(magazine: u8) -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(magazine).unwrap(),
    }
}

/// Fixed data positions and parity equations, independent of the production packer.
fn decode(body: &[u8; 40]) -> [u32; 13] {
    assert_eq!(body[0], 0x64); // Normative Hamming8/4 code for designation four.
    std::array::from_fn(|triplet| {
        let start = 1 + triplet * 3;
        let word = u32::from_le_bytes([body[start], body[start + 1], body[start + 2], 0]);
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
    })
}

#[test]
fn literal_two_triplet_fixture_and_canonical_unused_tail_are_preserved() {
    let real = links(page(8, 0xa5), TeletextX27LinkValidity::Level25Only, 0x8001);
    assert_eq!(
        decode(&encode_teletext_compositional_links(owner(2), &real)),
        [
            0x29544, 0x20004, 0x29545, 0x20004, 0x29546, 0x20004, 0x29547, 0x20004, 0x3c7c8,
            0x3fffc, 0x3c7c9, 0x3fffc, 0
        ],
    );
    let explicit_null = links(null(8), TeletextX27LinkValidity::Level25And35, 0xffff);
    assert_eq!(
        &decode(&encode_teletext_compositional_links(
            owner(2),
            &explicit_null
        ))[..8],
        &[0x3d7cc, 0x3fffc, 0x3d7cd, 0x3fffc, 0x3d7ce, 0x3fffc, 0x3d7cf, 0x3fffc],
    );
}

#[test]
fn all_magazine_pairs_use_xor_for_real_and_explicit_null_targets() {
    for own_magazine in 1_u8..=8 {
        for target_magazine in 1_u8..=8 {
            for target in [page(target_magazine, 0xa5), null(target_magazine)] {
                let links = links(target, TeletextX27LinkValidity::Level25Only, 0xffff);
                let words = decode(&encode_teletext_compositional_links(
                    owner(own_magazine),
                    &links,
                ));
                for slot in 0..4 {
                    let relative = u8::try_from((words[slot * 2] >> 11) & 7).unwrap();
                    let recovered = (own_magazine & 7) ^ relative;
                    assert_eq!(if recovered == 0 { 8 } else { recovered }, target_magazine);
                    assert_eq!(relative, (own_magazine & 7) ^ (target_magazine & 7));
                    if own_magazine == target_magazine {
                        assert_eq!(relative, 0);
                    }
                }
            }
        }
    }
}

#[test]
fn every_requirement_mask_roundtrips_without_zero_or_all_normalization() {
    for mask in 0_u16..=u16::MAX {
        let links = links(page(2, 0xa0), TeletextX27LinkValidity::Level25Only, mask);
        let words = decode(&encode_teletext_compositional_links(owner(2), &links));
        for slot in 0..4 {
            assert_eq!(words[slot * 2 + 1] & 3, 0);
            assert_eq!(words[slot * 2 + 1] >> 2, u32::from(mask));
        }
    }
}

#[test]
fn every_validity_and_page_byte_keep_fixed_roles_nibbles_and_compatibility_bits() {
    for validity in [
        TeletextX27LinkValidity::Level25Only,
        TeletextX27LinkValidity::Level35Only,
        TeletextX27LinkValidity::Level25And35,
    ] {
        let expected_validity = match validity {
            TeletextX27LinkValidity::Level25Only => 1,
            TeletextX27LinkValidity::Level35Only => 2,
            TeletextX27LinkValidity::Level25And35 => 3,
        };
        for page_byte in 0_u8..=0xfc {
            let links = links(page(2, page_byte), validity, 0);
            let words = decode(&encode_teletext_compositional_links(owner(7), &links));
            for slot in 0..4 {
                let word = words[slot * 2];
                assert_eq!(word & 3, u32::try_from(slot).unwrap());
                assert_eq!((word >> 2) & 3, expected_validity);
                assert_eq!((word >> 4) & 3, 0); // Writer's reserved-bit convention.
                assert_eq!((word >> 6) & 15, u32::from(page_byte & 15));
                assert_eq!((word >> 10) & 1, 1);
                assert_eq!(word >> 14, u32::from(page_byte >> 4));
                assert_eq!(words[slot * 2 + 1], 0);
            }
        }
    }
}

#[test]
fn distinct_role_targets_masks_and_applicability_remain_independent() {
    let declarations: Vec<_> = (1_u8..=4)
        .map(|magazine| {
            TeletextX27LinkDeclaration::new(
                page(magazine, 0xa0 | magazine),
                if magazine == 1 {
                    TeletextX27LinkValidity::Level25Only
                } else if magazine == 2 {
                    TeletextX27LinkValidity::Level35Only
                } else {
                    TeletextX27LinkValidity::Level25And35
                },
                TeletextSubpageRequirementMask::new(1 << magazine),
            )
            .unwrap()
        })
        .collect();
    let links = TeletextX27Level25Links::new(
        declarations[0],
        declarations[1],
        declarations[2],
        declarations[3],
    )
    .unwrap();
    let words = decode(&encode_teletext_compositional_links(owner(8), &links));
    for slot in 0..4 {
        let magazine = u32::try_from(slot + 1).unwrap();
        assert_eq!(words[slot * 2] & 3, u32::try_from(slot).unwrap());
        assert_eq!((words[slot * 2] >> 6) & 15, magazine);
        assert_eq!((words[slot * 2] >> 11) & 7, magazine);
        assert_eq!(words[slot * 2 + 1] >> 2, 1 << magazine);
    }
}

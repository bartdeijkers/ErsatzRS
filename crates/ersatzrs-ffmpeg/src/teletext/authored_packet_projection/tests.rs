use ersatzrs_domain::*;

use super::*;
use crate::teletext::packets::{data_unit, odd_parity};
use crate::teletext::{encode_teletext_page, TeletextEncodingRegion};

fn draft() -> TeletextPageDraft {
    TeletextPageDraft {
        address: TeletextPageAddress {
            page: TeletextPageNumber::new(200).unwrap(),
            subcode: TeletextSubcode::new(0x1234).unwrap(),
        },
        kind: TeletextPageKind::FullPage,
        language: TeletextLanguage::new("nld".into()).unwrap(),
        national_option: TeletextNationalOption::default(),
        rows: vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 0,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text {
                    text: "PRIVATE, DATA".into(),
                },
            }],
        }],
        navigation: Vec::new(),
    }
}

fn encoded() -> EncodedTeletextPage {
    encode_teletext_page(&draft(), TeletextEncodingRegion::WesternLatin).unwrap()
}

fn projection_with_units(
    page: &EncodedTeletextPage,
    units: &[[u8; 46]],
) -> Result<TeletextEncodedDisplayProjection, TeletextEncodedDisplayProjectionError> {
    project_units(page, units, 100)
}

#[test]
fn actual_encoder_projects_exact_header_rows_enhancements_and_native_order() {
    let page = encoded();
    let result = project_encoded_teletext_display(&page, 27).unwrap();
    assert_eq!(result.address, page.address());
    assert_eq!(result.kind, TeletextPageKind::FullPage);
    assert_eq!(result.language, *page.language());
    assert_eq!(result.header.subcode, 0x1234);
    assert_eq!(result.header.controls.raw(), 0x81);
    assert_eq!(result.records.len(), 27);
    assert_eq!(
        result.records[0],
        TeletextEncodedDisplayRecord::HeaderText([b' '; 32])
    );
    assert!(
        matches!(result.records[1], TeletextEncodedDisplayRecord::Triplets {
        row: 28, designation: 0, words
    } if words[0] == 0x3c000)
    );
    assert!(
        matches!(result.records[2], TeletextEncodedDisplayRecord::Triplets {
        row: 26, designation: 0, words
    } if words == [0x7ff; 13])
    );
    let mut expected = [b' '; 40];
    expected[..13].copy_from_slice(b"PRIVATE, DATA");
    assert_eq!(
        result.records[3],
        TeletextEncodedDisplayRecord::BaseRow {
            row: 1,
            cells: expected
        }
    );
    assert_eq!(
        result.records[26],
        TeletextEncodedDisplayRecord::BaseRow {
            row: 24,
            cells: [b' '; 40]
        }
    );
    for debug in [format!("{result:?}"), format!("{:?}", result.records)] {
        assert!(!debug.contains("PRIVATE"));
        assert!(!debug.contains("DATA"));
        assert!(!debug.contains("words:"));
        assert!(!debug.contains("cells:"));
    }
}

#[test]
fn exact_header_flags_are_retained_and_kind_identity_is_checked() {
    for (kind, flags) in [
        (TeletextPageKind::FullPage, 0x81),
        (TeletextPageKind::Newsflash, 0x8b),
        (TeletextPageKind::Subtitle, 0x8d),
    ] {
        let mut value = draft();
        value.kind = kind;
        value.rows.clear();
        let page = encode_teletext_page(&value, TeletextEncodingRegion::WesternLatin).unwrap();
        let projection = project_encoded_teletext_display(&page, 100).unwrap();
        assert_eq!(projection.kind, kind);
        assert_eq!(projection.header.controls.raw(), flags);
        let mut units = page.data_units().to_vec();
        units[0][0] = if kind == TeletextPageKind::Subtitle {
            2
        } else {
            3
        };
        assert_eq!(
            projection_with_units(&page, &units).unwrap_err().failure,
            TeletextEncodedDisplayProjectionFailure::Identity
        );
    }
    let page = encoded();
    let mut units = page.data_units().to_vec();
    // Clean Hamming codewords for a different subcode and for a subtitle flag.
    units[0][8] = hamming_8_4(5).reverse_bits();
    assert_eq!(
        projection_with_units(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    units[0] = page.data_units()[0];
    units[0][11] = hamming_8_4(9).reverse_bits();
    assert_eq!(
        projection_with_units(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
}

#[test]
fn editorial_links_retain_six_slots_exact_subcodes_and_raw_crc() {
    let mut value = draft();
    value.navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Red,
        target: TeletextNavigationTarget {
            page: TeletextPageNumber::new(801).unwrap(),
            subcode: Some(TeletextSubcode::new(0x1234).unwrap()),
        },
    });
    let page = encode_teletext_page(&value, TeletextEncodingRegion::WesternLatin).unwrap();
    let projection = project_encoded_teletext_display(&page, 100).unwrap();
    let TeletextEncodedDisplayRecord::EditorialLinks { nibbles, crc } = projection.records[1]
    else {
        panic!("expected X/27/0, not Hamming triplets");
    };
    assert_eq!(&nibbles[..7], &[0, 1, 0, 4, 3, 2, 5]);
    assert_eq!(&nibbles[7..13], &[15, 15, 15, 7, 15, 3]);
    assert_eq!(nibbles[37], 15);
    assert_eq!(
        crc,
        [
            page.data_units()[1][44].reverse_bits(),
            page.data_units()[1][45].reverse_bits()
        ]
    );
    // CRC is raw data: preserving it must not require odd parity or Hamming coding.
    let mut units = page.data_units().to_vec();
    units[1][44] = 0;
    units[1][45] = 0xff;
    let projection = projection_with_units(&page, &units).unwrap();
    assert!(matches!(
        projection.records[1],
        TeletextEncodedDisplayRecord::EditorialLinks { crc: [0, 255], .. }
    ));
    for column in 0..38 {
        for bit in 0..8 {
            let mut damaged = units.clone();
            damaged[1][6 + column] ^= 1 << bit;
            let failure = projection_with_units(&page, &damaged).unwrap_err();
            assert_eq!(
                failure.failure,
                TeletextEncodedDisplayProjectionFailure::Protection
            );
            assert_eq!(failure.row, Some(27));
            assert_eq!(failure.column, Some(u8::try_from(column).unwrap()));
        }
    }
}

#[test]
fn literal_hamming_oracles_preserve_words_and_reject_all_single_bit_damage() {
    // Literal EN 300 706 Hamming 24/18 vectors, also retained by the older
    // independent presentation tests. No encoder constructs these oracle bytes.
    for (bytes, expected) in [([0x8b, 0x80, 0x00], 0), ([0x8b, 0x80, 0x78], 0x3c000)] {
        assert_eq!(exact_triplet(bytes), Some(expected));
        for byte in 0..3 {
            for bit in 0..8 {
                let mut damaged = bytes;
                damaged[byte] ^= 1 << bit;
                assert_eq!(exact_triplet(damaged), None);
            }
        }
    }
    let codewords = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    for (nibble, byte) in (0..16).zip(codewords) {
        assert_eq!(exact_nibble(byte), Some(nibble));
        for bit in 0..8 {
            assert_eq!(exact_nibble(byte ^ (1 << bit)), None);
        }
    }
    for data in 0..(1 << 18) {
        assert_eq!(exact_triplet(hamming_24_18(data)), Some(data));
    }
}

#[test]
fn x26_x27_four_and_x28_preserve_literal_triplets_and_locate_damage() {
    let page = encoded();
    for row in [26, 27, 28] {
        let designation = if row == 27 { 4 } else { 0 };
        let mut body = [0; 40];
        body[0] = if designation == 4 { 0x64 } else { 0x15 };
        for word in body[1..].chunks_exact_mut(3) {
            word.copy_from_slice(&[0x8b, 0x80, 0x78]);
        }
        let units = [page.data_units()[0], data_unit(2, row, &body, false)];
        let projection = projection_with_units(&page, &units).unwrap();
        assert_eq!(
            projection.records[1],
            TeletextEncodedDisplayRecord::Triplets {
                row,
                designation,
                words: [0x3c000; 13]
            }
        );
        for column in 0..40 {
            for bit in 0..8 {
                let mut damaged = units;
                damaged[1][column + 6] ^= 1 << bit;
                let failure = projection_with_units(&page, &damaged).unwrap_err();
                assert_eq!(
                    failure.failure,
                    TeletextEncodedDisplayProjectionFailure::Protection
                );
                assert_eq!(failure.unit, Some(1));
                assert_eq!(failure.row, Some(row));
                assert_eq!(
                    failure.column,
                    Some(if column == 0 {
                        0
                    } else {
                        u8::try_from(1 + ((column - 1) / 3) * 3).unwrap()
                    })
                );
            }
        }
    }
}

#[test]
fn header_and_body_parity_damage_never_becomes_a_masked_editable_cell() {
    let page = encoded();
    for (unit, columns, row) in [(0, 8..40, 0), (3, 0..40, 1)] {
        for column in columns {
            for bit in 0..8 {
                let mut units = page.data_units().to_vec();
                units[unit][column + 6] ^= 1 << bit;
                let failure = projection_with_units(&page, &units).unwrap_err();
                assert_eq!(
                    failure.failure,
                    TeletextEncodedDisplayProjectionFailure::Parity
                );
                assert_eq!(failure.unit, Some(unit));
                assert_eq!(failure.row, Some(row));
                assert_eq!(failure.column, Some(u8::try_from(column).unwrap()));
                assert!(!format!("{failure:?}").contains("PRIVATE"));
                assert!(!failure.to_string().contains("PRIVATE"));
            }
        }
    }
}

#[test]
fn corrected_addresses_and_headers_wrong_identity_and_additional_headers_are_rejected() {
    let page = encoded();
    for column in 4..14 {
        for bit in 0..8 {
            let mut units = page.data_units().to_vec();
            units[0][column] ^= 1 << bit;
            assert_eq!(
                projection_with_units(&page, &units).unwrap_err().failure,
                TeletextEncodedDisplayProjectionFailure::Protection
            );
        }
    }
    let body = [odd_parity(b' '); 40];
    for unit in [data_unit(3, 1, &body, false), data_unit(2, 1, &body, true)] {
        assert_eq!(
            projection_with_units(&page, &[page.data_units()[0], unit])
                .unwrap_err()
                .failure,
            TeletextEncodedDisplayProjectionFailure::Identity
        );
    }
    assert_eq!(
        projection_with_units(&page, &[page.data_units()[3], page.data_units()[0]])
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::HeaderOrder
    );
    assert_eq!(
        projection_with_units(&page, &[page.data_units()[0], page.data_units()[0]])
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::HeaderOrder
    );
    let mut units = page.data_units().to_vec();
    units[0][6] = hamming_8_4(1).reverse_bits();
    assert_eq!(
        projection_with_units(&page, &units).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    for column in 4..6 {
        let mut units = page.data_units().to_vec();
        units[1][column] ^= 1;
        assert_eq!(
            projection_with_units(&page, &units).unwrap_err().failure,
            TeletextEncodedDisplayProjectionFailure::Protection
        );
    }
}

#[test]
fn budget_envelopes_unsupported_records_and_packet_twenty_five_are_explicit() {
    let page = encoded();
    for limit in [0, 26] {
        let failure = project_encoded_teletext_display(&page, limit).unwrap_err();
        assert_eq!(
            failure.failure,
            TeletextEncodedDisplayProjectionFailure::Budget
        );
        assert_eq!(failure.unit, None);
    }
    assert_eq!(
        projection_with_units(&page, &[]).unwrap_err().failure,
        TeletextEncodedDisplayProjectionFailure::Budget
    );
    for column in 0..4 {
        let mut units = page.data_units().to_vec();
        units[1][column] = 0;
        assert_eq!(
            projection_with_units(&page, &units).unwrap_err().failure,
            TeletextEncodedDisplayProjectionFailure::Envelope
        );
    }
    let body = [odd_parity(b' '); 40];
    let units = [page.data_units()[0], data_unit(2, 25, &body, false)];
    assert_eq!(
        projection_with_units(&page, &units).unwrap().records[1],
        TeletextEncodedDisplayRecord::BaseRow {
            row: 25,
            cells: [b' '; 40]
        }
    );
    for row in 29..=31 {
        let units = [page.data_units()[0], data_unit(2, row, &body, false)];
        assert_eq!(
            projection_with_units(&page, &units).unwrap_err().failure,
            TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
        );
    }
    for designation in 1..4 {
        let mut body = body;
        body[0] = hamming_8_4(designation);
        let units = [page.data_units()[0], data_unit(2, 27, &body, false)];
        assert_eq!(
            projection_with_units(&page, &units).unwrap_err().failure,
            TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
        );
    }
}

fn literal_magazine_unit(magazine: u8) -> [u8; 46] {
    let mut body = [0; 40];
    body[0] = 0x15;
    for word in body[1..].chunks_exact_mut(3) {
        word.copy_from_slice(&[0x8b, 0x80, 0x78]);
    }
    data_unit(magazine % 8, 29, &body, false)
}

#[test]
fn actual_service_magazine_projection_keeps_absent_page_x28_ownership() {
    use crate::teletext::authored_service_tests::{declared, input, limits, transmissions};
    use crate::teletext::{encode_teletext_authored_service, EncodedTeletextAuthoredPage};

    let original = declared(input());
    let tx = transmissions(&original);
    let service = encode_teletext_authored_service(original, &tx, &limits()).unwrap();
    let magazines = project_encoded_teletext_magazines(&service, 1).unwrap();
    assert_eq!(magazines.len(), 1);
    assert_eq!(magazines[0].magazine, 2);
    assert_eq!(magazines[0].designation, 0);
    assert_eq!(magazines[0].words[0], 0x3c080);
    for (word_index, &word) in magazines[0].words.iter().enumerate() {
        let bytes = hamming_24_18(word);
        let column = 7 + word_index * 3;
        assert_eq!(
            bytes,
            std::array::from_fn(|index| service.magazine_units()[0][column + index].reverse_bits())
        );
    }
    let EncodedTeletextAuthoredPage::CompiledDisplay(first) = &service.pages()[0] else {
        panic!("expected fixture's compiled display");
    };
    let display = project_encoded_teletext_display(first.page(), 100).unwrap();
    assert!(!display.records.iter().any(|record| matches!(
        record,
        TeletextEncodedDisplayRecord::Triplets { row: 28, .. }
    )));
    assert!(!format!("{:?}", magazines[0]).contains("words"));
}

#[test]
fn literal_magazine_records_allow_empty_services_and_preserve_magazine_eight() {
    assert!(project_magazine_units(&[], [false; 8], 1)
        .unwrap()
        .is_empty());
    assert_eq!(
        project_magazine_units(&[], [false; 8], 0)
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::Budget
    );
    let units = [literal_magazine_unit(2), literal_magazine_unit(8)];
    let expected = [false, true, false, false, false, false, false, true];
    let records = project_magazine_units(&units, expected, 2).unwrap();
    assert_eq!(
        records,
        vec![
            TeletextEncodedMagazineRecord {
                magazine: 2,
                designation: 0,
                words: [0x3c000; 13]
            },
            TeletextEncodedMagazineRecord {
                magazine: 8,
                designation: 0,
                words: [0x3c000; 13]
            },
        ]
    );
    assert_eq!(
        project_magazine_units(&units, expected, 1)
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::Budget
    );
    assert_eq!(
        project_magazine_units(&[], expected, 2)
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
}

#[test]
fn magazine_projection_rejects_damage_duplicates_and_unowned_or_unknown_packets() {
    let original = literal_magazine_unit(2);
    let expected = [false, true, false, false, false, false, false, false];
    for column in 4..46 {
        for bit in 0..8 {
            let mut damaged = original;
            damaged[column] ^= 1 << bit;
            let failure = project_magazine_units(&[damaged], expected, 1).unwrap_err();
            assert_eq!(
                failure.failure,
                TeletextEncodedDisplayProjectionFailure::Protection
            );
            assert_eq!(failure.unit, Some(0));
            assert_eq!(failure.row, Some(29));
        }
    }
    assert_eq!(
        project_magazine_units(&[original, original], expected, 2)
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    assert_eq!(
        project_magazine_units(&[original], [false; 8], 1)
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    for column in 0..4 {
        let mut damaged = original;
        damaged[column] = 0;
        assert_eq!(
            project_magazine_units(&[damaged], expected, 1)
                .unwrap_err()
                .failure,
            TeletextEncodedDisplayProjectionFailure::Envelope
        );
    }
    let mut subtitle = original;
    subtitle[0] = 3;
    assert_eq!(
        project_magazine_units(&[subtitle], expected, 1)
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::Identity
    );
    let mut unknown_designation = original;
    unknown_designation[6] = 0x02_u8.reverse_bits();
    assert_eq!(
        project_magazine_units(&[unknown_designation], expected, 1)
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
    );
    let mut body = [0; 40];
    body[0] = 0x15;
    for row in [26, 28, 30, 31] {
        let unit = data_unit(2, row, &body, false);
        assert_eq!(
            project_magazine_units(&[unit], expected, 1)
                .unwrap_err()
                .failure,
            TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
        );
    }
    let page = encoded();
    assert_eq!(
        project_magazine_units(&[page.data_units()[0]], expected, 1)
            .unwrap_err()
            .failure,
        TeletextEncodedDisplayProjectionFailure::UnsupportedPacket
    );
}

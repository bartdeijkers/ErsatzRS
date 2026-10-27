//! Body compatibility and independent bitfield checks, without decoder readiness claims.

use ersatzrs_domain::*;

use super::{encode_teletext_page, presentation::*, TeletextEncodingError, TeletextEncodingRegion};

/// Extract data using fixed Hamming 24/18 positions, checking every parity equation.
/// This reader deliberately does not call the production encoder or bit writer.
pub(super) fn decode(body: &[u8; 40]) -> [u32; 13] {
    assert_eq!(body[0], 0x15); // Normative Hamming 8/4 code for designation zero.
    std::array::from_fn(|index| {
        let start = 1 + index * 3;
        let word = u32::from_le_bytes([body[start], body[start + 1], body[start + 2], 0]);
        assert_eq!(word.count_ones() % 2, 1);
        for parity in [1_u32, 2, 4, 8, 16] {
            let count: u32 = (1_u32..24)
                .filter(|position| position & parity != 0)
                .map(|position| (word >> (position - 1)) & 1)
                .sum();
            assert_eq!(count % 2, 1);
        }
        let positions = [
            3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
        ];
        positions
            .into_iter()
            .enumerate()
            .fold(0, |data, (bit, position)| {
                data | (((word >> (position - 1)) & 1) << bit)
            })
    })
}

pub(super) fn field(words: &[u32; 13], offset: usize, width: usize) -> u32 {
    (0..width).fold(0, |value, bit| {
        let position = offset + bit;
        value | (((words[position / 18] >> (position % 18)) & 1) << bit)
    })
}

fn page(national: u8) -> TeletextPageDraft {
    TeletextPageDraft {
        address: TeletextPageAddress {
            page: TeletextPageNumber::new(200).unwrap(),
            subcode: TeletextSubcode::new(1).unwrap(),
        },
        kind: TeletextPageKind::FullPage,
        language: TeletextLanguage::new("nld".to_owned()).unwrap(),
        national_option: TeletextNationalOption::new(national).unwrap(),
        rows: vec![],
        navigation: vec![],
    }
}

#[test]
fn all_seven_legacy_declarations_preserve_literal_pre_refactor_packet_bodies() {
    // Former latin_designation output: only the first triplet varies with national bits.
    let first_triplets = [
        [0x8b, 0x80, 0x78],
        [0x03, 0x88, 0xf8],
        [0x02, 0x90, 0x78],
        [0x8a, 0x98, 0xf8],
        [0x01, 0xa0, 0x78],
        [0x89, 0xa8, 0xf8],
        [0x88, 0xb0, 0x78],
    ];
    let suffix = [
        0x34, 0x40, 0x07, 0x26, 0xff, 0x00, 0xe0, 0x5f, 0xff, 0x54, 0xc0, 0x35, 0x8d, 0x80, 0x2b,
        0x19, 0x7c, 0x6e, 0x61, 0xcc, 0x3f, 0xbe, 0xf7, 0x6f, 0xf6, 0x5f, 0x3b, 0x75, 0xff, 0x7e,
        0x65, 0x7f, 0xee, 0xed, 0x80, 0x00,
    ];
    for national in 0_u8..7 {
        let declaration =
            western_latin_v1_presentation(TeletextNationalOption::new(national).unwrap()).unwrap();
        let mut expected = [0; 40];
        expected[0] = 0x15;
        expected[1..4].copy_from_slice(&first_triplets[usize::from(national)]);
        expected[4..].copy_from_slice(&suffix);
        assert_eq!(encode_presentation(&declaration), expected);
        let encoded =
            encode_teletext_page(&page(national), TeletextEncodingRegion::WesternLatin).unwrap();
        // Empty legacy pages retain header, X28, termination-only X26, all24bodyrows.
        assert_eq!(encoded.data_units().len(), 27);
        let body = std::array::from_fn(|index| encoded.data_units()[1][index + 6].reverse_bits());
        assert_eq!(body, expected);
        let words = decode(&body);
        assert_eq!(field(&words, 0, 7), 0);
        assert_eq!(field(&words, 7, 7), u32::from(national));
        assert_eq!(field(&words, 14, 7), 127);
        assert_eq!(field(&words, 21, 7), 0);
        assert_eq!(field(&words, 220, 14), 0);
    }
}

#[test]
fn reserved_legacy_national_option_keeps_a_located_error_instead_of_panicking() {
    let reserved = TeletextNationalOption::new(7).unwrap();
    assert_eq!(
        western_latin_v1_presentation(reserved),
        Err(TeletextCharsetValueError::PageDesignation { code: 7 })
    );
    let page = page(7);
    assert_eq!(
        encode_teletext_page(&page, TeletextEncodingRegion::WesternLatin),
        Err(TeletextEncodingError::NationalOption {
            address: page.address
        })
    );
}

#[test]
fn asymmetric_palette_preserves_entry_order_rgb_component_order_and_triplet_crossings() {
    let mut declaration = western_latin_v1_presentation(TeletextNationalOption::default()).unwrap();
    declaration.palette = TeletextLevel25Palette {
        clut2: std::array::from_fn(|index| {
            TeletextRgb::new([
                u8::try_from(index).unwrap(),
                15 - u8::try_from(index).unwrap(),
                3,
            ])
            .unwrap()
        }),
        clut3: std::array::from_fn(|index| {
            TeletextRgb::new([
                14 - u8::try_from(index).unwrap(),
                2,
                2 * u8::try_from(index).unwrap(),
            ])
            .unwrap()
        }),
    };
    let words = decode(&encode_presentation(&declaration));
    for (entry, rgb) in declaration
        .palette
        .clut2
        .iter()
        .chain(&declaration.palette.clut3)
        .enumerate()
    {
        for (component, value) in rgb.get().into_iter().enumerate() {
            assert_eq!(
                field(&words, 28 + entry * 12 + component * 4, 4),
                u32::from(value)
            );
        }
    }
    // The last colour spans triplet12 and triplet13; subsequent fields remain separate.
    assert_eq!(field(&words, 208, 12), 7 | (2 << 4) | (14 << 8));
    assert_eq!(field(&words, 220, 14), 0);
}

#[test]
fn every_assigned_primary_and_secondary_code_preserves_raw_aliases_and_none_sentinel() {
    let mut declaration = western_latin_v1_presentation(TeletextNationalOption::default()).unwrap();
    for code in 0_u8..=255 {
        if let Ok(designation) = TeletextPageCharsetDesignation::new(code) {
            declaration.primary = designation;
            declaration.secondary = Some(designation);
            let words = decode(&encode_presentation(&declaration));
            assert_eq!(field(&words, 7, 7), u32::from(code));
            assert_eq!(field(&words, 14, 7), u32::from(code));
            assert_eq!(field(&words, 0, 7), 0);
        }
    }
    declaration.secondary = None;
    assert_eq!(
        field(&decode(&encode_presentation(&declaration)), 14, 7),
        127
    );
}

#[test]
fn panel_flags_status_and_raw_width_preserve_every_encodable_combination() {
    let mut declaration = western_latin_v1_presentation(TeletextNationalOption::default()).unwrap();
    for width in 0_u8..16 {
        for left in [false, true] {
            for right in [false, true] {
                for (status, bit) in [
                    (TeletextPanelStatus::Level35Only, 0),
                    (TeletextPanelStatus::Level25And35, 1),
                ] {
                    declaration.panels = TeletextSidePanels {
                        left,
                        right,
                        status,
                        width: TeletextPanelWidth::new(width).unwrap(),
                    };
                    let words = decode(&encode_presentation(&declaration));
                    assert_eq!(field(&words, 21, 1), u32::from(left));
                    assert_eq!(field(&words, 22, 1), u32::from(right));
                    assert_eq!(field(&words, 23, 1), bit);
                    assert_eq!(field(&words, 24, 4), u32::from(width));
                    assert_eq!(field(&words, 28, 4), 15);
                }
            }
        }
    }
}

#[test]
fn all_eight_remappings_use_the_normative_table_four_wire_codes() {
    let mut declaration = western_latin_v1_presentation(TeletextNationalOption::default()).unwrap();
    let cases = [
        (TeletextColourRemapping::FgClut0BgClut0, 0),
        (TeletextColourRemapping::FgClut0BgClut1, 1),
        (TeletextColourRemapping::FgClut0BgClut2, 2),
        (TeletextColourRemapping::FgClut1BgClut1, 3),
        (TeletextColourRemapping::FgClut1BgClut2, 4),
        (TeletextColourRemapping::FgClut2BgClut1, 5),
        (TeletextColourRemapping::FgClut2BgClut2, 6),
        (TeletextColourRemapping::FgClut2BgClut3, 7),
    ];
    declaration.screen_colour = TeletextColourReference::new(31).unwrap();
    declaration.row_colour = TeletextColourReference::new(19).unwrap();
    declaration.black_background_substitution = true;
    for (remapping, code) in cases {
        declaration.remapping = remapping;
        let words = decode(&encode_presentation(&declaration));
        assert_eq!(field(&words, 231, 3), code);
        assert_eq!(field(&words, 220, 5), 31);
        assert_eq!(field(&words, 225, 5), 19);
        assert_eq!(field(&words, 230, 1), 1);
    }
}

#[test]
fn all_screen_row_colours_and_substitution_bits_remain_independent() {
    let mut declaration = western_latin_v1_presentation(TeletextNationalOption::default()).unwrap();
    declaration.remapping = TeletextColourRemapping::FgClut2BgClut3;
    for screen in 0_u8..32 {
        for row in 0_u8..32 {
            for substitution in [false, true] {
                declaration.screen_colour = TeletextColourReference::new(screen).unwrap();
                declaration.row_colour = TeletextColourReference::new(row).unwrap();
                declaration.black_background_substitution = substitution;
                let words = decode(&encode_presentation(&declaration));
                assert_eq!(field(&words, 220, 5), u32::from(screen));
                assert_eq!(field(&words, 225, 5), u32::from(row));
                assert_eq!(field(&words, 230, 1), u32::from(substitution));
                assert_eq!(field(&words, 231, 3), 7);
            }
        }
    }
}

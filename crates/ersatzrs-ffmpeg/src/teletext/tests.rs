use ersatzrs_domain::*;

use super::{layout, packets, *};

fn page(number: u16) -> TeletextPageDraft {
    TeletextPageDraft {
        address: TeletextPageAddress {
            page: TeletextPageNumber::new(number).unwrap(),
            subcode: TeletextSubcode::new(0x10).unwrap(),
        },
        kind: TeletextPageKind::FullPage,
        language: TeletextLanguage::new("nld".into()).unwrap(),
        national_option: TeletextNationalOption::default(),
        rows: Vec::new(),
        navigation: Vec::new(),
    }
}

fn text(column: u8, text: &str, style: TeletextStyle) -> TeletextRun {
    TeletextRun {
        column,
        style,
        content: TeletextRunContent::Text { text: text.into() },
    }
}

fn encode(page: &TeletextPageDraft) -> Result<EncodedTeletextPage, TeletextEncodingError> {
    encode_teletext_page(page, TeletextEncodingRegion::WesternLatin)
}

fn body(unit: &[u8; 46]) -> [u8; 40] {
    std::array::from_fn(|index| unit[index + 6].reverse_bits())
}

fn packet(page: &EncodedTeletextPage, number: u8) -> &[u8; 46] {
    let address = page.address().page.wire_magazine() | (number << 3);
    page.data_units()
        .iter()
        .find(|unit| {
            unit[4] == packets::hamming_8_4(address & 15).reverse_bits()
                && unit[5] == packets::hamming_8_4(address >> 4).reverse_bits()
        })
        .unwrap()
}

#[test]
fn protection_matches_normative_vectors_and_odd_parity() {
    let reference = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    for (input, expected) in (0_u8..16).zip(reference) {
        assert_eq!(packets::hamming_8_4(input), expected);
    }
    for data in 0_u8..128 {
        assert_eq!(packets::odd_parity(data) & 127, data);
        assert_eq!(packets::odd_parity(data).count_ones() % 2, 1);
    }
    // Check every possible triplet against independent parity equations.
    for input in 0_u32..(1 << 18) {
        let [a, b, c] = packets::hamming_24_18(input);
        let word = u32::from_le_bytes([a, b, c, 0]);
        assert_eq!(word.count_ones() % 2, 1);
        for parity in [1_u32, 2, 4, 8, 16] {
            let count: u32 = (1..24)
                .filter(|position| position & parity != 0)
                .map(|position| (word >> (position - 1)) & 1)
                .sum();
            assert_eq!(count % 2, 1);
        }
        let mut recovered = 0;
        let mut bit = 0;
        for position in 1_u32..24 {
            if !position.is_power_of_two() {
                recovered |= ((word >> (position - 1)) & 1) << bit;
                bit += 1;
            }
        }
        assert_eq!(recovered, input);
    }
}

#[test]
fn style_commands_do_not_overwrite_glyphs_or_header_fields() {
    let mut value = page(200);
    let cyan = TeletextStyle {
        foreground: TeletextColour::Cyan,
        ..TeletextStyle::default()
    };
    value.rows = vec![TeletextRow {
        row: 1,
        runs: vec![text(0, "DATA", cyan)],
    }];
    assert!(matches!(
        encode(&value),
        Err(TeletextEncodingError::ControlCells { column: 0, .. })
    ));
    value.rows[0].runs[0].column = 1;
    let result = encode(&value).unwrap();
    let row = body(packet(&result, 1)).map(|byte| byte & 127);
    assert_eq!(&row[..5], b"\x06DATA");
    value.rows[0].row = 0;
    value.rows[0].runs[0].column = 8;
    assert!(matches!(
        encode(&value),
        Err(TeletextEncodingError::ControlCells { row: 0, .. })
    ));
    value.rows[0].runs[0].column = 9;
    assert!(encode(&value).is_ok());
}

#[test]
fn colour_clears_conceal_and_background_transitions_restore_foreground() {
    let mut value = page(200);
    value.rows = vec![TeletextRow {
        row: 1,
        runs: vec![
            text(
                1,
                "A",
                TeletextStyle {
                    conceal: true,
                    ..TeletextStyle::default()
                },
            ),
            text(3, "B", TeletextStyle::default()),
            text(
                7,
                "C",
                TeletextStyle {
                    foreground: TeletextColour::Yellow,
                    background: TeletextColour::Blue,
                    ..TeletextStyle::default()
                },
            ),
        ],
    }];
    let result = layout::compile(&value).unwrap();
    assert_eq!(&result.rows[1][..8], b"\x18A\x07B\x04\x1d\x03C");
    value.rows[0].runs[2].column = 6;
    assert!(matches!(
        encode(&value),
        Err(TeletextEncodingError::ControlCells { column: 6, .. })
    ));
}

#[test]
fn subtitle_boxing_and_type_are_explicit() {
    let mut value = page(888);
    value.kind = TeletextPageKind::Subtitle;
    value.rows = vec![TeletextRow {
        row: 1,
        runs: vec![text(2, "Caption", TeletextStyle::default())],
    }];
    assert!(matches!(
        encode(&value),
        Err(TeletextEncodingError::HiddenContent { .. })
    ));
    value.rows[0].runs[0].style.boxed = true;
    let result = encode(&value).unwrap();
    assert!(result.data_units().iter().all(|unit| unit[0] == 3));
    assert_eq!(
        &body(packet(&result, 1)).map(|byte| byte & 127)[..9],
        b"\x0b\x0bCaption"
    );
    value.kind = TeletextPageKind::Newsflash;
    assert!(encode(&value)
        .unwrap()
        .data_units()
        .iter()
        .all(|unit| unit[0] == 2));
}

#[test]
fn mosaic_patterns_and_separation_retain_all_six_bits() {
    let mut value = page(200);
    value.rows = vec![TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 2,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Mosaics {
                cells: vec![
                    TeletextMosaic::new(0).unwrap(),
                    TeletextMosaic::new(32).unwrap(),
                    TeletextMosaic::new(63).unwrap(),
                ],
                separated: true,
            },
        }],
    }];
    let result = layout::compile(&value).unwrap();
    assert_eq!(&result.rows[1][..5], &[0x17, 0x1a, 0x20, 0x60, 0x7f]);
}

#[test]
fn unicode_requires_exact_tables_and_never_silently_normalizes() {
    let mut value = page(200);
    value.rows = vec![TeletextRow {
        row: 1,
        runs: vec![text(0, "éöĉŁ →", TeletextStyle::default())],
    }];
    let result = layout::compile(&value).unwrap();
    let enhancements = result.legacy_enhancements();
    assert!(enhancements.contains(&layout::triplet(0, 0x12, b'e')));
    assert!(enhancements.contains(&layout::triplet(2, 0x13, b'c')));
    for invalid in ["📺", "e\u{301}", "\u{212b}", "|", "€"] {
        value.rows[0].runs[0].content = TeletextRunContent::Text {
            text: invalid.into(),
        };
        assert!(
            matches!(encode(&value), Err(TeletextEncodingError::Character { .. })),
            "{invalid}"
        );
    }
    value.national_option = TeletextNationalOption::new(7).unwrap();
    assert!(matches!(
        encode(&value),
        Err(TeletextEncodingError::NationalOption { .. })
    ));
}

#[test]
fn links_retain_relative_magazine_and_actual_subcode() {
    let mut value = page(200);
    value.navigation = vec![TeletextNavigationLink {
        role: TeletextNavigationRole::Red,
        target: TeletextNavigationTarget {
            page: TeletextPageNumber::new(801).unwrap(),
            subcode: Some(TeletextSubcode::new(0x1234).unwrap()),
        },
    }];
    let encoded = encode(&value).unwrap();
    let links = body(packet(&encoded, 27));
    let expected = [1, 0, 4, 3, 2, 5]; // Relative magazine 2 XOR 0 sets M2.
    assert_eq!(&links[1..7], &expected.map(packets::hamming_8_4));
    value.navigation[0].target.subcode = Some(TeletextSubcode::new(0x3f7f).unwrap());
    assert!(matches!(
        encode(&value),
        Err(TeletextEncodingError::LinkSubcode { .. })
    ));
}

#[test]
fn packets_have_exact_identity_erase_serial_flags_and_reversed_national_bits() {
    let mut value = page(800);
    value.national_option = TeletextNationalOption::new(1).unwrap();
    let encoded = encode(&value).unwrap();
    assert_eq!(encoded.address(), value.address);
    let header = body(&encoded.data_units()[0]);
    assert_eq!(
        &header[..8],
        &[0, 0, 0, 9, 0, 0, 0, 9].map(packets::hamming_8_4)
    );
    assert_eq!(
        encoded.data_units()[0][4],
        packets::hamming_8_4(0).reverse_bits()
    );
    assert!(encoded
        .data_units()
        .iter()
        .all(|unit| unit[..4] == [2, 44, 0xe0, 0xe4]));
}

#[test]
fn enhancements_fail_as_a_whole_when_designation_capacity_is_exhausted() {
    let mut value = page(200);
    for row in 1..=5 {
        value.rows.push(TeletextRow {
            row,
            runs: vec![text(0, &"é".repeat(40), TeletextStyle::default())],
        });
    }
    value.rows.push(TeletextRow {
        row: 6,
        runs: vec![text(0, "é", TeletextStyle::default())],
    });
    let maximum = encode(&value).unwrap();
    assert_eq!(maximum.data_units().len(), 25 + 1 + 16);
    value.rows[5].runs[0].content = TeletextRunContent::Text {
        text: "éé".into()
    };
    assert!(matches!(
        encode(&value),
        Err(TeletextEncodingError::Enhancements { .. })
    ));
}

#[test]
fn page_sets_require_real_targets_and_never_return_partial_encoding() {
    let mut draft = TeletextPageSetDraft {
        language: TeletextLanguage::new("nld".into()).unwrap(),
        initial_page: TeletextPageNumber::new(200).unwrap(),
        pages: vec![page(200), page(201)],
    };
    let ready = encode_teletext_page_set(&draft, TeletextEncodingRegion::WesternLatin).unwrap();
    assert_eq!(ready.initial_page(), draft.initial_page);
    assert_eq!(ready.language(), &draft.language);
    assert_eq!(ready.pages().len(), 2);
    draft.pages[1].rows = vec![TeletextRow {
        row: 1,
        runs: vec![text(0, "📺", TeletextStyle::default())],
    }];
    assert!(
        matches!(encode_teletext_page_set(&draft, TeletextEncodingRegion::WesternLatin), Err(TeletextEncodingError::Character { address, .. }) if address.page.get() == 201)
    );
    draft.pages[1].rows.clear();
    draft.pages[0].navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Index,
        target: TeletextNavigationTarget {
            page: TeletextPageNumber::new(202).unwrap(),
            subcode: None,
        },
    });
    assert!(matches!(
        encode_teletext_page_set(&draft, TeletextEncodingRegion::WesternLatin),
        Err(TeletextEncodingError::Structure(
            TeletextStructureError::MissingTarget { .. }
        ))
    ));
}

#[test]
fn authored_page_wire_snapshot() {
    use std::fmt::Write as _;
    let mut value = page(200);
    value.rows = vec![TeletextRow {
        row: 1,
        runs: vec![text(
            1,
            "Café",
            TeletextStyle {
                foreground: TeletextColour::Cyan,
                ..TeletextStyle::default()
            },
        )],
    }];
    let encoded = encode(&value).unwrap();
    let mut wire = String::new();
    for unit in encoded.data_units() {
        for byte in unit {
            write!(wire, "{byte:02x}").unwrap();
        }
        wire.push('\n');
    }
    insta::assert_snapshot!("authored_page_wire", wire);
}

#[tokio::test]
#[ignore = "requires exact ERSATZRS_TELETEXT_TEST_FFMPEG and ERSATZRS_TELETEXT_TEST_PYTHON"]
async fn real_runtime_decodes_authored_unicode_styles_and_subtitles() {
    let ffmpeg = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_FFMPEG"])
        .expect("select exact runtime");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .expect("select Python fixture tool");
    let mut full = page(200);
    full.rows = vec![
        TeletextRow {
            row: 1,
            runs: vec![text(
                1,
                "Authored café Noël",
                TeletextStyle {
                    foreground: TeletextColour::Cyan,
                    ..TeletextStyle::default()
                },
            )],
        },
        TeletextRow {
            row: 2,
            runs: vec![text(
                1,
                "élève Čaj ÆØß",
                TeletextStyle {
                    foreground: TeletextColour::Green,
                    ..TeletextStyle::default()
                },
            )],
        },
    ];
    let mut caption = page(888);
    caption.kind = TeletextPageKind::Subtitle;
    caption.rows = vec![TeletextRow {
        row: 1,
        runs: vec![text(
            2,
            "Authored caption",
            TeletextStyle {
                boxed: true,
                ..TeletextStyle::default()
            },
        )],
    }];
    let raw: Vec<u8> = [encode(&full).unwrap(), encode(&caption).unwrap()]
        .iter()
        .flat_map(|page| page.data_units().iter().flatten().copied())
        .collect();
    let script = test_manifest_dir!().join("../../scripts/calibration/wrap-authored-teletext.py");
    let stream =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &raw, 128 * 1024)
            .await
            .unwrap();
    let decoded = crate::teletext_probe::run_probe(
        &ffmpeg,
        crate::teletext_probe::TEXT_PROBE_ARGUMENTS,
        &stream,
        64 * 1024,
    )
    .await
    .unwrap();
    let text = String::from_utf8(decoded).unwrap();
    for expected in ["Authored café Noël", "élève Čaj ÆØß", "Authored caption"] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    let bitmap = crate::teletext_probe::run_probe(
        &ffmpeg,
        crate::teletext_probe::BITMAP_PROBE_ARGUMENTS,
        &stream,
        492 * 250 * 3,
    )
    .await
    .unwrap();
    assert_eq!(bitmap.len(), 492 * 250 * 3);
    let cyan = bitmap
        .chunks_exact(3)
        .filter(|pixel| pixel[0] < 40 && pixel[1] > 180 && pixel[2] > 180)
        .count();
    let green = bitmap
        .chunks_exact(3)
        .filter(|pixel| pixel[0] < 40 && pixel[1] > 180 && pixel[2] < 40)
        .count();
    assert!(
        cyan > 10 && green > 10,
        "missing authored colours: cyan={cyan}, green={green}"
    );
    assert_national_subsets(&ffmpeg, &python, &script).await;
}

async fn assert_national_subsets(ffmpeg: &str, python: &str, script: &std::path::Path) {
    let mut full = page(200);
    let accents = "ÀÁÂÃÄÅÇÈÉÊËÌÍÎÏÑÒÓÔÕÖÙÚÛÜÝÿČŠŽąęŁŒ";
    let national_examples = [
        "£$@←½→↑#—¼‖¾÷",
        "#$§ÄÖÜ^_°äöüß",
        "#¤ÉÄÖÅÜ_éäöåü",
        "£$é°ç→↑#ùàòèì",
        "éïàëêùî#èâôûç",
        "ç$¡áéíóú¿üñèà",
        "#ůčťžýířéáěúš",
    ];
    for (option, national) in (0..7).zip(national_examples) {
        full.national_option = TeletextNationalOption::new(option).unwrap();
        full.rows = vec![TeletextRow {
            row: 1,
            runs: vec![self::text(0, accents, TeletextStyle::default())],
        }];
        full.rows.push(TeletextRow {
            row: 2,
            runs: vec![self::text(0, national, TeletextStyle::default())],
        });
        let encoded = encode(&full).unwrap();
        let raw: Vec<u8> = encoded.data_units().iter().flatten().copied().collect();
        let stream =
            crate::teletext_probe::run_probe(python, &[script.to_str().unwrap()], &raw, 128 * 1024)
                .await
                .unwrap();
        let decoded = crate::teletext_probe::run_probe(
            ffmpeg,
            crate::teletext_probe::TEXT_PROBE_ARGUMENTS,
            &stream,
            64 * 1024,
        )
        .await
        .unwrap();
        let decoded = String::from_utf8(decoded).unwrap();
        assert!(
            decoded.contains(accents),
            "national option {option} lost characters: {decoded}"
        );
        assert!(
            decoded.contains(national),
            "national option {option} changed its table: {decoded}"
        );
    }
}

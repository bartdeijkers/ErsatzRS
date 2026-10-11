//! Positioned Western Latin glyph intent and unchanged legacy packet literals.

use ersatzrs_domain::*;

use super::layout;
use super::*;

fn page() -> TeletextPageDraft {
    TeletextPageDraft {
        address: TeletextPageAddress {
            page: TeletextPageNumber::new(200).unwrap(),
            subcode: TeletextSubcode::new(0x1234).unwrap(),
        },
        kind: TeletextPageKind::FullPage,
        language: TeletextLanguage::new("nld".into()).unwrap(),
        national_option: TeletextNationalOption::new(1).unwrap(),
        rows: vec![],
        navigation: vec![],
    }
}

fn text(column: u8, text: &str) -> TeletextRun {
    TeletextRun {
        column,
        style: TeletextStyle::default(),
        content: TeletextRunContent::Text { text: text.into() },
    }
}

fn row(row: u8, column: u8, value: &str) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![text(column, value)],
    }
}

fn diacritic(code: u8, mark: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0Diacritic {
        code: TeletextGlyphCode::new(code).unwrap(),
        diacritic: TeletextDiacriticIndex::new(mark).unwrap(),
    }
}

fn g2(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G2 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}

#[test]
fn unsorted_runs_produce_sorted_positioned_unicode_intent_and_exact_fallback_cells() {
    let mut input = page();
    input.rows = vec![
        row(24, 3, "™"),
        TeletextRow {
            row: 1,
            runs: vec![text(20, "Ö"), text(0, "éĉ™@#")],
        },
        row(0, 8, "é"),
    ];
    let original = input.clone();
    let compiled = layout::compile(&input).unwrap();
    assert_eq!(input, original);
    let actual: Vec<_> = compiled
        .base_glyphs()
        .iter()
        .map(|glyph| {
            (
                glyph.position().row(),
                glyph.position().column(),
                TeletextColumnOperation::from(glyph),
            )
        })
        .collect();
    assert_eq!(
        actual,
        [
            (0, 8, diacritic(b'e', 2)),
            (1, 0, diacritic(b'e', 2)),
            (1, 1, diacritic(b'c', 3)),
            (1, 2, g2(0x54)),
            // Literal mode10 special '*' denotes @, not an ordinary G0 '*'.
            (1, 3, diacritic(b'*', 0)),
            (24, 3, g2(0x54)),
        ]
    );
    let mut expected = [[b' '; 40]; 25];
    expected[0][8] = b'e';
    expected[1][..5].copy_from_slice(b"ec  #");
    expected[1][20] = 0x5c; // National subset Ö requires no X26 event.
    assert_eq!(compiled.rows, expected);
    // Independent original address/mode/data literals: exact row0 and row24
    // selectors remain legacy encoding details, not context-free composer moves.
    assert_eq!(
        compiled.legacy_enhancements(),
        [0x001ff, 0x32c88, 0x00129, 0x32c80, 0x31cc1, 0x2a3c2, 0x15403, 0x00128, 0x2a3c3,]
    );
}

#[test]
fn direct_national_subset_characters_emit_no_derived_events_or_selectors() {
    let mut input = page();
    input.rows = vec![row(1, 0, "#$§ÄÖÜ^_°äöüß")];
    let compiled = layout::compile(&input).unwrap();
    assert!(compiled.base_glyphs().is_empty());
    assert!(compiled.legacy_enhancements().is_empty());
    assert_eq!(
        &compiled.rows[1][..13],
        &[0x23, 0x24, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e,]
    );
}

#[test]
fn ordinary_ascii_g0_positions_ambiguous_with_national_subset_keep_no_mark_mode10() {
    let mut input = page();
    input.rows = vec![row(1, 0, "@[\\]`{~")];
    let compiled = layout::compile(&input).unwrap();
    let actual: Vec<_> = compiled
        .base_glyphs()
        .iter()
        .map(TeletextColumnOperation::from)
        .collect();
    assert_eq!(
        actual,
        [
            diacritic(b'*', 0),
            diacritic(b'[', 0),
            diacritic(b'\\', 0),
            diacritic(b']', 0),
            diacritic(b'`', 0),
            diacritic(b'{', 0),
            diacritic(b'~', 0),
        ]
    );
    assert_eq!(&compiled.rows[1][..7], b"       ");
}

// Decode emitted packet words using independent normative protection positions.
fn packet_words(unit: &[u8; 46]) -> [u32; 13] {
    std::array::from_fn(|index| {
        let raw = u32::from_le_bytes([
            unit[7 + index * 3].reverse_bits(),
            unit[8 + index * 3].reverse_bits(),
            unit[9 + index * 3].reverse_bits(),
            0,
        ]);
        assert_eq!(raw.count_ones() % 2, 1);
        for parity in [1_u32, 2, 4, 8, 16] {
            let count: u32 = (1_u32..24)
                .filter(|bit| bit & parity != 0)
                .map(|bit| (raw >> (bit - 1)) & 1)
                .sum();
            assert_eq!(count % 2, 1);
        }
        [
            3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 22, 23,
        ]
        .into_iter()
        .enumerate()
        .fold(0, |value, (bit, position)| {
            value | (((raw >> (position - 1)) & 1) << bit)
        })
    })
}

fn enhancement_units(page: &EncodedTeletextPage) -> Vec<&[u8; 46]> {
    const CODEWORDS: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    page.data_units()
        .iter()
        .filter(|unit| {
            let low = CODEWORDS
                .iter()
                .position(|value| *value == unit[4].reverse_bits())
                .unwrap();
            let high = CODEWORDS
                .iter()
                .position(|value| *value == unit[5].reverse_bits())
                .unwrap();
            (low | (high << 4)) >> 3 == 26
        })
        .collect()
}

#[test]
fn legacy_packet_builder_retains_literal_header_body_selectors_and_zero_termination() {
    let mut input = page();
    input.rows = vec![row(24, 3, "™"), row(0, 8, "é"), row(1, 0, "@")];
    let encoded = encode_teletext_page(&input, TeletextEncodingRegion::WesternLatin).unwrap();
    let units = enhancement_units(&encoded);
    assert_eq!(units.len(), 1);
    assert_eq!(units[0][6].reverse_bits(), 0x15); // Designation zero.
    assert_eq!(
        packet_words(units[0]),
        [
            0x001ff, 0x32c88, 0x00129, 0x15400, 0x00128, 0x2a3c3, 0x007ff, 0x007ff, 0x007ff,
            0x007ff, 0x007ff, 0x007ff, 0x007ff,
        ]
    );
}

#[test]
fn capacity_counts_row_selectors_and_zero_termination_without_changing_error_priority() {
    let mut input = page();
    input.rows = (1..=5)
        .map(|number| row(number, 0, &"é".repeat(40)))
        .collect();
    input.rows.push(row(6, 0, "é"));
    let compiled = layout::compile(&input).unwrap();
    assert_eq!(compiled.base_glyphs().len(), 201);
    assert_eq!(compiled.legacy_enhancements().len(), 207);
    let encoded = encode_teletext_page(&input, TeletextEncodingRegion::WesternLatin).unwrap();
    let units = enhancement_units(&encoded);
    assert_eq!(units.len(), 16);
    assert_eq!(packet_words(units[15])[12], 0x007ff);
    input.rows[5].runs[0].content = TeletextRunContent::Text {
        text: "éé".into()
    };
    assert!(
        matches!(layout::compile(&input), Err(TeletextEncodingError::Enhancements { address })
        if address == input.address)
    );
    // A later unsupported scalar still wins over the late aggregate capacity check.
    input.rows.push(row(7, 0, "📺"));
    assert_eq!(
        layout::compile(&input).err(),
        Some(TeletextEncodingError::Character {
            address: input.address,
            row: 7,
            column: 0,
            codepoint: 0x1f4fa,
        })
    );
}

#[test]
fn located_character_control_cell_and_reserved_national_diagnostics_are_preserved() {
    let mut input = page();
    input.rows = vec![row(1, 7, "é📺")];
    assert_eq!(
        layout::compile(&input).err(),
        Some(TeletextEncodingError::Character {
            address: input.address,
            row: 1,
            column: 8,
            codepoint: 0x1f4fa,
        })
    );
    input.rows = vec![row(1, 0, "é")];
    input.rows[0].runs[0].style.foreground = TeletextColour::Red;
    assert_eq!(
        layout::compile(&input).err(),
        Some(TeletextEncodingError::ControlCells {
            address: input.address,
            row: 1,
            column: 0,
        })
    );
    input.national_option = TeletextNationalOption::new(7).unwrap();
    assert_eq!(
        layout::compile(&input).err(),
        Some(TeletextEncodingError::NationalOption {
            address: input.address,
        })
    );
}

#[test]
fn mosaics_keep_spacing_controls_and_never_become_unicode_glyph_events() {
    let mut input = page();
    input.rows = vec![TeletextRow {
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
    let compiled = layout::compile(&input).unwrap();
    assert!(compiled.base_glyphs().is_empty());
    assert_eq!(&compiled.rows[1][..5], &[0x17, 0x1a, 0x20, 0x60, 0x7f]);
}

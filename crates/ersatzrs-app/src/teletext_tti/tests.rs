use super::*;

const BUDGET: TeletextTtiBudget = TeletextTtiBudget {
    max_bytes: 8192,
    max_records: 100,
};

fn roundtrip(records: Vec<TeletextTtiRecord>) -> (TeletextTtiDocument, Vec<u8>) {
    let document = TeletextTtiDocument::checked_from_records(records, BUDGET).unwrap();
    let bytes = write_teletext_tti(&document, BUDGET).unwrap();
    assert_eq!(parse_teletext_tti(&bytes, BUDGET).unwrap(), document);
    (document, bytes)
}

fn page() -> TeletextTtiRecord {
    TeletextTtiRecord::PageNumber {
        page: 0x200,
        editor_ordinal: 10,
    }
}

#[test]
fn controls_comma_and_trailing_spaces_roundtrip_without_masking() {
    let mut cells = [b' '; 40];
    cells[..6].copy_from_slice(&[0x00, 0x1b, 0x1f, b',', b'A', 0x7f]);
    let (_, bytes) = roundtrip(vec![page(), TeletextTtiRecord::BaseRow { row: 25, cells }]);
    assert!(bytes.windows(7).any(|part| part == b"\x1b@\x1b[\x1b_,"));
    assert!(bytes.ends_with(&[vec![b' '; 34], b"\r\n".to_vec()].concat()));
}

#[test]
fn decimal_editor_ordinals_and_wide_raw_metadata_are_preserved() {
    let input = b"PN,20010\r\nSC,0010\r\nPS,FFFF\r\nRE,18446744073709551615\r\nCT,18446744073709551615,C\r\nFL,000,0FF,8FF,FFF,200,206\r\n";
    let document = parse_teletext_tti(input, BUDGET).unwrap();
    assert_eq!(document.records()[0], page());
    assert_eq!(document.records()[1], TeletextTtiRecord::SubpageOrdinal(10));
    assert_eq!(write_teletext_tti(&document, BUDGET).unwrap(), input);
    let (_, bytes) = roundtrip(vec![
        TeletextTtiRecord::PageNumber {
            page: 0x200,
            editor_ordinal: 100,
        },
        TeletextTtiRecord::SubpageOrdinal(100),
    ]);
    assert_eq!(bytes, b"PN,200100\r\nSC,0100\r\n");
}

#[test]
fn editor_page_ordinal_wrap_does_not_rewrite_full_sc_ordinal() {
    let bytes = b"PN,20000\r\nSC,0256\r\nPN,20001\r\nSC,0001\r\nPN,20001\r\nSC,0257\r\n";
    let document = parse_teletext_tti(bytes, BUDGET).unwrap();
    assert_eq!(
        document.records()[0],
        TeletextTtiRecord::PageNumber {
            page: 0x200,
            editor_ordinal: 0
        }
    );
    assert_eq!(
        document.records()[1],
        TeletextTtiRecord::SubpageOrdinal(256)
    );
    assert_eq!(write_teletext_tti(&document, BUDGET).unwrap(), bytes);
}

#[test]
fn actual_editor_calibration_bytes_and_x26_words_are_retained() {
    // Literal native Save As output from the pinned editor calibration in
    // docs/reference/teletext-design-interchange.md, SHA256 1cfa16ca...5209db.
    // This is synthetic editor evidence, not an ErsatzRS export/import proof.
    let bytes = include_bytes!("fixtures/qtm-1.0-saved-calibration.tti");
    assert_eq!(bytes.len(), 563);
    let document = parse_teletext_tti(bytes, BUDGET).unwrap();
    assert_eq!(
        write_teletext_tti(&document, BUDGET).unwrap(),
        bytes.as_slice()
    );
    let TeletextTtiRecord::Packet {
        row,
        designation,
        words,
    } = &document.records()[7]
    else {
        panic!("expected original X26 packet");
    };
    assert_eq!((*row, *designation), (26, 0));
    let unpack = |word: u32| (word & 0x3f, (word >> 6) & 0x1f, word >> 11);
    assert_eq!(unpack(words[0]), (41, 4, 0));
    assert_eq!(unpack(words[1]), (5, 9, 65));
    assert!(words[2..].iter().all(|&word| unpack(word) == (63, 31, 127)));
}

#[test]
fn magazine_packet_scope_and_packet_number_are_not_repaired() {
    let records = vec![
        TeletextTtiRecord::PageNumber {
            page: 0x2ff,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::Packet {
            row: 29,
            designation: 15,
            words: [0x3ffff; 13],
        },
    ];
    let (_, bytes) = roundtrip(records);
    assert!(bytes.windows(6).any(|part| part == b"OL,29,"));
    assert!(!bytes.windows(6).any(|part| part == b"OL,28,"));
    let error = TeletextTtiDocument::checked_from_records(
        vec![
            page(),
            TeletextTtiRecord::Packet {
                row: 29,
                designation: 0,
                words: [0; 13],
            },
        ],
        BUDGET,
    )
    .unwrap_err();
    assert_eq!(error.kind, TeletextTtiErrorKind::InvalidMagazineScope);
    assert_eq!(
        (error.line, error.row, error.column),
        (2, Some(29), Some(1))
    );
}

#[test]
fn unknown_blank_duplicate_and_ambiguous_identity_records_are_rejected() {
    for (bytes, kind) in [
        (
            b"PN,20000\r\nXX,private text\r\n".as_slice(),
            TeletextTtiErrorKind::UnknownRecord,
        ),
        (
            b"PN,20000\r\n\r\n".as_slice(),
            TeletextTtiErrorKind::BlankRecord,
        ),
        (
            b"PN,20000\r\nPS,8000\r\nPS,8000\r\n".as_slice(),
            TeletextTtiErrorKind::DuplicateRecord,
        ),
        (
            b"PN,20000\r\nPN,20000\r\n".as_slice(),
            TeletextTtiErrorKind::DuplicateRecord,
        ),
        (
            b"PN,20000\r\nPN,20101\r\n".as_slice(),
            TeletextTtiErrorKind::DifferentPage,
        ),
        (
            b"SC,0000\r\nPN,20000\r\n".as_slice(),
            TeletextTtiErrorKind::InvalidOrder,
        ),
    ] {
        assert_eq!(parse_teletext_tti(bytes, BUDGET).unwrap_err().kind, kind);
    }
}

#[test]
fn truncated_parity_bytes_and_invalid_escapes_report_payload_positions() {
    let prefix = b"PN,20000\r\nOL,1,";
    for (payload, kind, column) in [
        (vec![b' '; 39], TeletextTtiErrorKind::InvalidRowWidth, 40),
        (
            [vec![0xa0], vec![b' '; 39]].concat(),
            TeletextTtiErrorKind::InvalidCharacter,
            1,
        ),
        (
            [b"\x1b?".to_vec(), vec![b' '; 39]].concat(),
            TeletextTtiErrorKind::InvalidEscape,
            1,
        ),
        (
            [vec![b' '; 39], vec![0x1b]].concat(),
            TeletextTtiErrorKind::InvalidEscape,
            40,
        ),
        (
            [vec![0x01], vec![b' '; 39]].concat(),
            TeletextTtiErrorKind::InvalidCharacter,
            1,
        ),
    ] {
        let bytes = [prefix.to_vec(), payload, b"\r\n".to_vec()].concat();
        let error = parse_teletext_tti(&bytes, BUDGET).unwrap_err();
        assert_eq!(
            (error.kind, error.line, error.row, error.column),
            (kind, 2, Some(1), Some(column))
        );
    }
    let bytes = [
        b"PN,20000\r\nOL,26,".to_vec(),
        vec![0x40; 39],
        b"\r\n".to_vec(),
    ]
    .concat();
    assert_eq!(
        parse_teletext_tti(&bytes, BUDGET).unwrap_err().kind,
        TeletextTtiErrorKind::InvalidPacket
    );
}

#[test]
fn host_bounds_overflow_and_invalid_direct_values_are_atomic() {
    assert_eq!(
        parse_teletext_tti(
            b"PN,20000\r\n",
            TeletextTtiBudget {
                max_bytes: 0,
                ..BUDGET
            }
        )
        .unwrap_err()
        .kind,
        TeletextTtiErrorKind::InvalidBudget
    );
    assert_eq!(
        parse_teletext_tti(
            b"PN,20000\r\n",
            TeletextTtiBudget {
                max_bytes: 5,
                ..BUDGET
            }
        )
        .unwrap_err()
        .kind,
        TeletextTtiErrorKind::BytesExceeded
    );
    assert_eq!(
        parse_teletext_tti(
            b"PN,20000\r\nSC,0000\r\n",
            TeletextTtiBudget {
                max_records: 1,
                ..BUDGET
            }
        )
        .unwrap_err()
        .kind,
        TeletextTtiErrorKind::RecordsExceeded
    );
    for bytes in [
        b"PN,20000\r\nRE,18446744073709551616\r\n".as_slice(),
        b"PN,20000\r\nPS,10000\r\n".as_slice(),
    ] {
        assert_eq!(
            parse_teletext_tti(bytes, BUDGET).unwrap_err().kind,
            TeletextTtiErrorKind::Overflow
        );
    }
    assert_eq!(
        parse_teletext_tti(b"PN,20000\r\nCT,0,T\r\n", BUDGET)
            .unwrap_err()
            .kind,
        TeletextTtiErrorKind::InvalidValue
    );
    let error = TeletextTtiDocument::checked_from_records(
        vec![
            page(),
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words: [0x40000; 13],
            },
        ],
        BUDGET,
    )
    .unwrap_err();
    assert_eq!(error.kind, TeletextTtiErrorKind::InvalidPacket);
    let document = parse_teletext_tti(b"PN,20000\r\n", BUDGET).unwrap();
    assert_eq!(
        write_teletext_tti(
            &document,
            TeletextTtiBudget {
                max_bytes: 9,
                ..BUDGET
            }
        )
        .unwrap_err()
        .kind,
        TeletextTtiErrorKind::BytesExceeded
    );
}

#[test]
fn debug_and_errors_do_not_reveal_authored_content() {
    let document = TeletextTtiDocument::checked_from_records(
        vec![
            TeletextTtiRecord::Description("AUTHORED SECRET".to_owned()),
            page(),
        ],
        BUDGET,
    )
    .unwrap();
    assert!(!format!("{document:?} {:?}", document.records()).contains("AUTHORED SECRET"));
    let error = parse_teletext_tti(b"PN,20000\r\nXX,AUTHORED SECRET\r\n", BUDGET).unwrap_err();
    assert!(!format!("{error:?} {error}").contains("AUTHORED SECRET"));
}

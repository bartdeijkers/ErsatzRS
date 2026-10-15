use super::*;

fn record(tag: u8, entries: &[[u8; 5]]) -> Vec<u8> {
    let mut bytes = vec![tag, u8::try_from(entries.len() * 5).unwrap()];
    bytes.extend(entries.iter().flatten());
    bytes
}

fn entry(language: [u8; 3], kind: u8, magazine: u8, page: u8) -> [u8; 5] {
    [
        language[0],
        language[1],
        language[2],
        (kind << 3) | magazine,
        page,
    ]
}

#[test]
fn unknown_records_preserve_wire_descriptor_and_entry_ordinals() {
    let mut bytes = vec![0x52, 1, 7];
    bytes.extend(record(
        0x56,
        &[entry(*b"eng", 1, 2, 0), entry(*b"nld", 2, 0, 0x88)],
    ));
    bytes.extend([0x80, 0]);
    bytes.extend(record(0x46, &[entry(*b"deu", 5, 1, 0x99)]));
    let entries = parse_source_teletext_descriptors(&bytes).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(
        entries
            .iter()
            .map(|entry| (
                entry.descriptor_tag,
                entry.descriptor_ordinal,
                entry.entry_ordinal
            ))
            .collect::<Vec<_>>(),
        [(0x56, 1, 0), (0x56, 1, 1), (0x46, 3, 0)]
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.language.as_str())
            .collect::<Vec<_>>(),
        ["eng", "nld", "deu"]
    );
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.display_page().unwrap().get())
            .collect::<Vec<_>>(),
        [200, 888, 199]
    );
}

#[test]
fn repeated_addresses_and_distinct_wire_tags_are_never_deduplicated() {
    let raw = entry(*b"nld", 2, 0, 0x88);
    let mut bytes = record(0x56, &[raw, raw]);
    bytes.extend(record(0x56, &[raw]));
    bytes.extend(record(0x46, &[raw]));
    let entries = parse_source_teletext_descriptors(&bytes).unwrap();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].entry_ordinal, 0);
    assert_eq!(entries[1].entry_ordinal, 1);
    assert_eq!(entries[2].descriptor_ordinal, 1);
    assert_eq!(entries[3].descriptor_tag, 0x46);
    assert!(entries
        .iter()
        .all(|entry| entry.display_page().unwrap().get() == 888));
}

#[test]
fn subtitle_types_two_and_five_keep_their_exact_raw_types() {
    let entries = parse_source_teletext_descriptors(&record(
        0x56,
        &[entry(*b"eng", 2, 0, 0x88), entry(*b"eng", 5, 0, 0x88)],
    ))
    .unwrap();
    assert_eq!(entries[0].descriptor_type, 2);
    assert_eq!(entries[1].descriptor_type, 5);
    assert_eq!(entries[0].display_page(), entries[1].display_page());
}

#[test]
fn reserved_types_magazine_eight_and_nondecimal_pages_survive() {
    let entries = parse_source_teletext_descriptors(&record(
        0x56,
        &[
            entry(*b"eng", 0, 0, 0),
            entry(*b"eng", 31, 7, 0xfa),
            entry(*b"eng", 6, 0, 0x9a),
            entry(*b"eng", 4, 1, 0xff),
        ],
    ))
    .unwrap();
    assert_eq!(entries[0].wire_magazine, 0);
    assert_eq!(entries[0].descriptor_type, 0);
    assert_eq!(entries[0].display_page().unwrap().get(), 800);
    assert_eq!(entries[1].descriptor_type, 31);
    assert_eq!(entries[1].wire_page, 0xfa);
    for raw in &entries[1..] {
        assert_eq!(raw.display_page(), None);
    }
}

#[test]
fn truncated_unknown_or_known_tlv_records_reject_the_entire_list() {
    for bytes in [
        vec![0x56],
        vec![0x46],
        vec![0x80],
        vec![0x80, 2, 1],
        vec![0x56, 5, b'e', b'n', b'g', 0x10],
    ] {
        assert_eq!(
            parse_source_teletext_descriptors(&bytes),
            Err(TeletextSourceDescriptorError::Framing)
        );
    }
    let mut late = record(0x56, &[entry(*b"eng", 2, 0, 0x88)]);
    late.push(0x80);
    assert_eq!(
        parse_source_teletext_descriptors(&late),
        Err(TeletextSourceDescriptorError::Framing)
    );
}

#[test]
fn both_tags_reject_partial_entries_even_after_a_valid_descriptor() {
    for tag in [0x56, 0x46] {
        for length in [1_u8, 2, 3, 4, 6, 9] {
            let mut bytes = record(0x56, &[entry(*b"eng", 1, 2, 0)]);
            bytes.extend([tag, length]);
            bytes.extend(std::iter::repeat_n(0, usize::from(length)));
            assert_eq!(
                parse_source_teletext_descriptors(&bytes),
                Err(TeletextSourceDescriptorError::PayloadLength)
            );
        }
    }
}

#[test]
fn invalid_iso_language_never_returns_earlier_valid_entries() {
    for language in [
        *b"ENG",
        *b"en1",
        *b"e g",
        [0, b'n', b'g'],
        [0xff, b'n', b'g'],
    ] {
        let bytes = record(
            0x56,
            &[entry(*b"nld", 2, 0, 0x88), entry(language, 2, 0, 0x88)],
        );
        assert_eq!(
            parse_source_teletext_descriptors(&bytes),
            Err(TeletextSourceDescriptorError::Language)
        );
    }
}

#[test]
fn empty_and_unknown_only_lists_do_not_infer_teletext_presence() {
    for bytes in [
        vec![],
        vec![0x80, 0],
        vec![0x56, 0, 0x46, 0],
        vec![0x80, 3, 0, 0xff, 0],
    ] {
        assert_eq!(parse_source_teletext_descriptors(&bytes), Ok(vec![]));
    }
}

#[test]
fn complete_descriptor_bytes_accept_the_exact_existing_acquisition_bound() {
    let mut bytes = record(0x56, &[entry(*b"eng", 2, 0, 0x88)]);
    // Fill with complete unknown TLVs, retaining the initial known descriptor.
    while bytes.len() + 257 <= MAX_SOURCE_TELETEXT_HEADER_DESCRIPTOR_BYTES {
        bytes.extend([0x80, 255]);
        bytes.extend(std::iter::repeat_n(0, 255));
    }
    let remaining = MAX_SOURCE_TELETEXT_HEADER_DESCRIPTOR_BYTES - bytes.len();
    assert!(remaining >= 2);
    bytes.extend([0x80, u8::try_from(remaining - 2).unwrap()]);
    bytes.extend(std::iter::repeat_n(0, remaining - 2));
    assert_eq!(bytes.len(), MAX_SOURCE_TELETEXT_HEADER_DESCRIPTOR_BYTES);
    assert_eq!(parse_source_teletext_descriptors(&bytes).unwrap().len(), 1);
    bytes.push(0);
    assert_eq!(
        parse_source_teletext_descriptors(&bytes),
        Err(TeletextSourceDescriptorError::ResourceLimit)
    );
}

#[test]
fn maximum_descriptor_payload_retains_every_five_byte_entry() {
    let entries = vec![entry(*b"eng", 31, 0, 0xff); 51];
    let bytes = record(0x56, &entries);
    assert_eq!(bytes[1], 255);
    let parsed = parse_source_teletext_descriptors(&bytes).unwrap();
    assert_eq!(parsed.len(), 51);
    assert_eq!(parsed[50].entry_ordinal, 50);
    assert_eq!(parsed[50].descriptor_type, 31);
    assert_eq!(parsed[50].wire_page, 0xff);
}

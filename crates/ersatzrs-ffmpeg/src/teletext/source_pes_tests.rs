use super::*;

const ZERO_PTS: [u8; 5] = [0x21, 0, 1, 0, 1];

fn framed_unit(id: u8, length: u8) -> Vec<u8> {
    let mut bytes = vec![0x5a; 2 + usize::from(length)];
    bytes[..2].copy_from_slice(&[id, length]);
    bytes
}

fn supplied_pes(units: &[u8], pts: Option<[u8; 5]>) -> Vec<u8> {
    let size = 46 + units.len();
    assert!(size.is_multiple_of(184));
    let declared = u16::try_from(size - 6).unwrap();
    let mut bytes = vec![0xff; size];
    bytes[..9].copy_from_slice(&[0, 0, 1, 0xbd, 0, 0, 0x84, 0, 0x24]);
    bytes[4..6].copy_from_slice(&declared.to_be_bytes());
    if let Some(pts) = pts {
        bytes[7] = 0x80;
        bytes[9..14].copy_from_slice(&pts);
    }
    bytes[45] = 0x10;
    bytes[46..].copy_from_slice(units);
    bytes
}

fn fixture() -> Vec<u8> {
    let units = [
        framed_unit(2, 44),
        framed_unit(3, 44),
        framed_unit(0xff, 44),
    ]
    .concat();
    supplied_pes(&units, Some(ZERO_PTS))
}

fn pad_units(units: &mut Vec<u8>) {
    let mut padding = (184 - (46 + units.len()) % 184) % 184;
    if padding == 1 {
        padding += 184;
    }
    while padding > 0 {
        let mut size = padding.min(257);
        if padding - size == 1 {
            size -= 1;
        }
        units.extend(framed_unit(0xff, u8::try_from(size - 2).unwrap()));
        padding -= size;
    }
}

#[test]
fn complete_packet_preserves_units_and_borrows_every_input_slice() {
    let bytes = fixture();
    let units = {
        let parsed = SuppliedTeletextPes::parse(&bytes).unwrap();
        assert!(std::ptr::eq(parsed.bytes().as_ptr(), bytes.as_ptr()));
        assert_eq!(parsed.bytes(), bytes);
        assert_eq!(parsed.data_identifier(), 0x10);
        parsed.units().collect::<Vec<_>>()
    };
    assert_eq!(
        units
            .iter()
            .map(SuppliedTeletextDataUnit::id)
            .collect::<Vec<_>>(),
        [2, 3, 0xff]
    );
    let mut offset = 46;
    for unit in units {
        assert_eq!(unit.length(), 44);
        assert!(std::ptr::eq(
            unit.bytes().as_ptr(),
            bytes[offset..].as_ptr()
        ));
        assert!(std::ptr::eq(
            unit.payload().as_ptr(),
            bytes[offset + 2..].as_ptr()
        ));
        assert_eq!(unit.payload(), &bytes[offset + 2..offset + 46]);
        if unit.id() == 0xff {
            assert!(unit.teletext_unit().is_none());
        } else {
            let typed = unit.teletext_unit().unwrap();
            assert!(std::ptr::eq(typed.as_ptr(), bytes[offset..].as_ptr()));
            // Framing alone does not check this deliberately invalid envelope.
            assert_eq!(typed[2], 0x5a);
        }
        offset += unit.bytes().len();
    }
    assert_eq!(offset, bytes.len());
}

#[test]
fn independent_pts_vectors_preserve_zero_high_bit_and_full_33_bit_range() {
    for (wire, ticks) in [
        (ZERO_PTS, 0),
        ([0x21, 0, 5, 0xbf, 0x21], 90_000),
        ([0x29, 0, 1, 0, 1], 1_u64 << 32),
        ([0x2f, 0xff, 0xff, 0xff, 0xff], (1_u64 << 33) - 1),
    ] {
        let mut bytes = fixture();
        bytes[9..14].copy_from_slice(&wire);
        assert_eq!(
            SuppliedTeletextPes::parse(&bytes)
                .unwrap()
                .pts()
                .unwrap()
                .ticks(),
            ticks
        );
    }
}

#[test]
fn absent_pts_is_distinct_from_present_zero() {
    let mut bytes = fixture();
    assert_eq!(
        SuppliedTeletextPes::parse(&bytes)
            .unwrap()
            .pts()
            .unwrap()
            .ticks(),
        0
    );
    bytes[7] = 0;
    bytes[9..45].fill(0xff);
    assert_eq!(SuppliedTeletextPes::parse(&bytes).unwrap().pts(), None);
}

#[test]
fn pts_requires_its_exact_prefix_and_all_three_marker_bits() {
    for prefix in 0_u8..16 {
        let mut bytes = fixture();
        bytes[9] = (prefix << 4) | 1;
        let result = SuppliedTeletextPes::parse(&bytes);
        if prefix == 2 {
            assert!(result.is_ok());
        } else {
            assert_eq!(result, Err(TeletextPesError::Timestamp));
        }
    }
    for index in [9, 11, 13] {
        let mut bytes = fixture();
        bytes[index] &= 0xfe;
        assert_eq!(
            SuppliedTeletextPes::parse(&bytes),
            Err(TeletextPesError::Timestamp)
        );
    }
}

#[test]
fn every_truncated_prefix_is_rejected_without_exposing_partial_units() {
    let bytes = fixture();
    for end in 0..bytes.len() {
        assert_eq!(
            SuppliedTeletextPes::parse(&bytes[..end]),
            Err(TeletextPesError::PacketLength)
        );
    }
    assert!(SuppliedTeletextPes::parse(&bytes).is_ok());
}

#[test]
fn packet_length_must_be_bounded_exact_and_an_integral_184_bytes() {
    for declared in [0_u16, 177, 179, u16::MAX] {
        let mut bytes = fixture();
        bytes[4..6].copy_from_slice(&declared.to_be_bytes());
        assert_eq!(
            SuppliedTeletextPes::parse(&bytes),
            Err(TeletextPesError::PacketLength)
        );
    }
    let mut bytes = fixture();
    bytes.push(0xff);
    bytes[4..6].copy_from_slice(&179_u16.to_be_bytes());
    assert_eq!(
        SuppliedTeletextPes::parse(&bytes),
        Err(TeletextPesError::PacketLength)
    );
    let mut bytes = fixture();
    bytes.extend_from_slice(&fixture());
    assert_eq!(
        SuppliedTeletextPes::parse(&bytes),
        Err(TeletextPesError::PacketLength)
    );
}

#[test]
fn header_requires_start_code_stream_markers_alignment_and_clear_scrambling() {
    for (index, value, error) in [
        (0, 1, TeletextPesError::StartCode),
        (1, 1, TeletextPesError::StartCode),
        (2, 2, TeletextPesError::StartCode),
        (3, 0xbe, TeletextPesError::StreamId),
        (6, 0x04, TeletextPesError::HeaderMarker),
        (6, 0xc4, TeletextPesError::HeaderMarker),
        (6, 0x80, TeletextPesError::Alignment),
        (6, 0x94, TeletextPesError::Scrambled),
        (6, 0xa4, TeletextPesError::Scrambled),
        (6, 0xb4, TeletextPesError::Scrambled),
        (8, 0x23, TeletextPesError::HeaderDataLength),
        (8, 0x25, TeletextPesError::HeaderDataLength),
    ] {
        let mut bytes = fixture();
        bytes[index] = value;
        assert_eq!(SuppliedTeletextPes::parse(&bytes), Err(error));
    }
    for flags in 0_u8..8 {
        let mut bytes = fixture();
        bytes[6] |= ((flags & 4) << 1) | (flags & 3);
        assert!(SuppliedTeletextPes::parse(&bytes).is_ok());
    }
}

#[test]
fn all_other_optional_headers_are_explicitly_unsupported() {
    for flags in 0_u8..=255 {
        if matches!(flags, 0 | 0x80) {
            continue;
        }
        let mut bytes = fixture();
        bytes[7] = flags;
        assert_eq!(
            SuppliedTeletextPes::parse(&bytes),
            Err(TeletextPesError::UnsupportedHeader)
        );
    }
}

#[test]
fn header_stuffing_is_checked_after_the_supported_optional_fields() {
    for pts in [None, Some(ZERO_PTS)] {
        let original = fixture();
        let bytes = supplied_pes(&original[46..], pts);
        let start = if pts.is_some() { 14 } else { 9 };
        for index in start..45 {
            let mut damaged = bytes.clone();
            damaged[index] = 0xfe;
            assert_eq!(
                SuppliedTeletextPes::parse(&damaged),
                Err(TeletextPesError::HeaderStuffing)
            );
        }
    }
}

#[test]
fn raw_identifier_is_preserved_within_its_supported_range() {
    for identifier in 0_u8..=255 {
        let mut bytes = fixture();
        bytes[45] = identifier;
        let result = SuppliedTeletextPes::parse(&bytes);
        if (0x10..=0x1f).contains(&identifier) {
            assert_eq!(result.unwrap().data_identifier(), identifier);
        } else {
            assert_eq!(result, Err(TeletextPesError::DataIdentifier));
        }
    }
}

#[test]
fn malformed_late_units_and_unframed_tail_reject_the_complete_packet() {
    for id in [2, 3] {
        let mut bytes = fixture();
        bytes[138] = id;
        bytes[139] = 43;
        assert_eq!(
            SuppliedTeletextPes::parse(&bytes),
            Err(TeletextPesError::TeletextUnitLength)
        );
    }
    let mut bytes = fixture();
    bytes[139] = 45;
    assert_eq!(
        SuppliedTeletextPes::parse(&bytes),
        Err(TeletextPesError::UnitBoundary)
    );
    let mut bytes = fixture();
    bytes[46] = 0xff;
    bytes[47] = 135;
    assert_eq!(
        SuppliedTeletextPes::parse(&bytes),
        Err(TeletextPesError::UnitBoundary)
    );
}

#[test]
fn stuffing_and_unknown_units_preserve_every_encoded_length_and_arbitrary_payload() {
    for id in [0xff, 0x99] {
        for length in 0_u8..=255 {
            let mut units = framed_unit(id, length);
            pad_units(&mut units);
            let bytes = supplied_pes(&units, None);
            let parsed = SuppliedTeletextPes::parse(&bytes).unwrap();
            let first = parsed.units().next().unwrap();
            assert_eq!(first.id(), id);
            assert_eq!(first.length(), length);
            assert_eq!(first.payload(), vec![0x5a; usize::from(length)]);
            assert!(first.teletext_unit().is_none());
            assert_eq!(
                parsed.units().map(|unit| unit.bytes().len()).sum::<usize>(),
                units.len()
            );
        }
    }
}

#[test]
fn largest_packet_supports_both_maximal_teletext_and_maximal_raw_unit_counts() {
    let units = framed_unit(2, 44).repeat(1_423);
    let mut bytes = supplied_pes(&units, Some(ZERO_PTS));
    assert_eq!(bytes.len(), MAX_PES_BYTES);
    let parsed = SuppliedTeletextPes::parse(&bytes).unwrap();
    assert_eq!(parsed.units().count(), 1_423);
    assert!(parsed.units().all(|unit| unit.teletext_unit().is_some()));
    bytes[46..]
        .chunks_exact_mut(2)
        .for_each(|pair| pair.copy_from_slice(&[0xff, 0]));
    let parsed = SuppliedTeletextPes::parse(&bytes).unwrap();
    assert_eq!(parsed.units().count(), 32_729);
    let mut iterator = parsed.units();
    assert_eq!(iterator.by_ref().count(), 32_729);
    assert_eq!(iterator.next(), None);
    assert_eq!(iterator.next(), None);
    bytes.resize(MAX_PES_BYTES + 184, 0xff);
    assert_eq!(
        SuppliedTeletextPes::parse(&bytes),
        Err(TeletextPesError::PacketLength)
    );
}

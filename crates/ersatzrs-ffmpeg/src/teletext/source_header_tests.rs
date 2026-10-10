use super::*;

// EN 300 706 section 8.2 codewords, independent from the production encoder.
const CODEWORDS: [u8; 16] = [
    0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea,
];

fn supplied_unit(packet: u8, magazine: u8, nibbles: [u8; 8]) -> [u8; 46] {
    let mut unit = [0_u8; 46];
    unit[..4].copy_from_slice(&[2, 44, 0xe0, 0xe4]);
    let address = magazine | (packet << 3);
    unit[4] = CODEWORDS[usize::from(address & 15)].reverse_bits();
    unit[5] = CODEWORDS[usize::from(address >> 4)].reverse_bits();
    for (byte, nibble) in unit[6..14].iter_mut().zip(nibbles) {
        *byte = CODEWORDS[usize::from(nibble)].reverse_bits();
    }
    // Header text deliberately has invalid odd parity. This metadata observer
    // neither decodes it nor certifies that any body/display text is usable.
    unit
}

fn fixture_header() -> [u8; 46] {
    // Independent page 200, subcode 1234, erase and serial flags, raw option 000.
    supplied_unit(0, 2, [0, 0, 4, 11, 2, 1, 0, 1])
}

fn header(unit: &[u8; 46]) -> SuppliedTeletextHeader {
    let TeletextHeaderObservation::Header(header) = observe_dvb_teletext_header(unit).unwrap()
    else {
        panic!("expected supplied page header");
    };
    header
}

#[test]
fn independent_header_fixture_retains_actual_raw_identity_and_controls() {
    let observed = header(&fixture_header());
    assert_eq!(
        observed.packet_address.data_unit_kind,
        TeletextDataUnitKind::NonSubtitle
    );
    assert!(observed.packet_address.field_parity);
    assert_eq!(observed.packet_address.line_offset, 0);
    assert_eq!(observed.packet_address.wire_magazine, 2);
    assert_eq!(observed.packet_address.packet_number, 0);
    assert_eq!(observed.wire_page, 0);
    assert_eq!(observed.display_page().unwrap().get(), 200);
    assert_eq!(observed.subcode, 0x1234);
    assert_eq!(observed.controls.raw(), 0x81);
    assert!(observed.controls.erase_page());
    assert!(!observed.controls.subtitle());
    assert_eq!(observed.controls.c12_c14_bits(), 0);
    assert_eq!(observed.corrected_header_bytes, 0);
    assert_eq!(observed.packet_address.corrected_address_bytes, 0);
}

#[test]
fn normative_hamming_vectors_accept_exact_and_correct_every_single_bit_error() {
    for (value, codeword) in (0_u8..16).zip(CODEWORDS) {
        assert_eq!(decode_hamming(codeword), Some((value, false)));
        for bit in 0..8 {
            assert_eq!(decode_hamming(codeword ^ (1 << bit)), Some((value, true)));
        }
    }
}

#[test]
fn normative_hamming_vectors_reject_every_double_bit_error() {
    for codeword in CODEWORDS {
        for first in 0..8 {
            for second in first + 1..8 {
                assert_eq!(
                    decode_hamming(codeword ^ (1 << first) ^ (1 << second)),
                    None
                );
            }
        }
    }
}

#[test]
fn complete_units_correct_address_and_header_bits_without_changing_identity() {
    let original = fixture_header();
    let expected = header(&original);
    for byte in 4..14 {
        for bit in 0..8 {
            let mut damaged = original;
            damaged[byte] ^= 1 << bit;
            let mut observed = header(&damaged);
            if byte < 6 {
                assert_eq!(observed.packet_address.corrected_address_bytes, 1);
                assert_eq!(observed.corrected_header_bytes, 0);
                observed.packet_address.corrected_address_bytes = 0;
            } else {
                assert_eq!(observed.packet_address.corrected_address_bytes, 0);
                assert_eq!(observed.corrected_header_bytes, 1);
                observed.corrected_header_bytes = 0;
            }
            assert_eq!(observed, expected);
        }
    }
    let mut damaged = original;
    for byte in &mut damaged[4..14] {
        *byte ^= 1;
    }
    let observed = header(&damaged);
    assert_eq!(observed.packet_address.corrected_address_bytes, 2);
    assert_eq!(observed.corrected_header_bytes, 8);
}

#[test]
fn complete_units_reject_detected_double_errors_in_each_protected_field() {
    for byte in 4..14 {
        let mut damaged = fixture_header();
        damaged[byte] ^= 3;
        assert_eq!(
            observe_dvb_teletext_header(&damaged),
            Err(if byte < 6 {
                TeletextHeaderError::AddressProtection
            } else {
                TeletextHeaderError::HeaderProtection
            })
        );
    }
}

#[test]
fn both_fields_and_every_valid_line_offset_are_accepted() {
    for parity in [0, 0x20] {
        for offset in std::iter::once(0).chain(7..=22) {
            let mut unit = fixture_header();
            unit[2] = 0xc0 | parity | offset;
            let observed = header(&unit);
            assert_eq!(observed.packet_address.field_parity, parity != 0);
            assert_eq!(observed.packet_address.line_offset, offset);
        }
    }
}

#[test]
fn invalid_recognized_envelope_is_rejected_before_protection_decoding() {
    for (index, value, error) in [
        (1, 43, TeletextHeaderError::Length),
        (2, 0xa0, TeletextHeaderError::ReservedEnvelope),
        (3, 0x27, TeletextHeaderError::FramingCode),
    ] {
        let mut unit = fixture_header();
        unit[index] = value;
        unit[4] ^= 3; // A double-bit address error cannot hide envelope failure.
        assert!(decode_hamming(unit[4].reverse_bits()).is_none());
        assert_eq!(observe_dvb_teletext_header(&unit), Err(error));
    }
    for offset in (1..=6).chain(23..=31) {
        let mut unit = fixture_header();
        unit[2] = 0xe0 | offset;
        assert_eq!(
            observe_dvb_teletext_header(&unit),
            Err(TeletextHeaderError::LineOffset)
        );
    }
}

#[test]
fn unknown_and_stuffing_identifiers_never_interpret_payload() {
    for data_unit_id in [0, 1, 4, 0x7f, 0x80, 0xfe, 0xff] {
        let mut unit = [0_u8; 46];
        unit[0] = data_unit_id;
        assert_eq!(
            observe_dvb_teletext_header(&unit),
            Ok(TeletextHeaderObservation::NotTeletext { data_unit_id })
        );
    }
}

#[test]
fn nonheaders_decode_only_packet_address_without_reading_header_bytes() {
    for packet_number in 1..=31 {
        let mut unit = supplied_unit(packet_number, 0, [0; 8]);
        // Detected double-bit header errors must remain deliberately ignored.
        unit[6..14].fill((CODEWORDS[0] ^ 3).reverse_bits());
        assert!(decode_hamming(unit[6].reverse_bits()).is_none());
        unit[4] ^= 1;
        let TeletextHeaderObservation::NotHeader(address) =
            observe_dvb_teletext_header(&unit).unwrap()
        else {
            panic!("nonheader parsed as header");
        };
        assert_eq!(address.packet_number, packet_number);
        assert_eq!(address.wire_magazine, 0);
        assert_eq!(address.corrected_address_bytes, 1);
    }
}

#[test]
fn all_control_flags_and_subcode_widths_remain_separate_raw_values() {
    // All control bits set; maximum subcode; non-display hexadecimal page FA.
    let observed = header(&supplied_unit(0, 7, [10, 15, 15, 15, 15, 15, 15, 15]));
    assert_eq!(observed.wire_page, 0xfa);
    assert!(observed.display_page().is_none());
    assert_eq!(observed.subcode, 0x3f7f);
    assert_eq!(observed.controls.raw(), 0x7ff);
    assert!(observed.controls.erase_page());
    assert!(observed.controls.subtitle());
    assert_eq!(observed.controls.c12_c14_bits(), 7);
    for (nibble_index, bit, control_bit) in [
        (3, 3, 0),
        (5, 2, 1),
        (5, 3, 2),
        (6, 0, 3),
        (6, 1, 4),
        (6, 2, 5),
        (6, 3, 6),
        (7, 0, 7),
        (7, 1, 8),
        (7, 2, 9),
        (7, 3, 10),
    ] {
        let mut nibbles = [0; 8];
        nibbles[nibble_index] = 1 << bit;
        let observed = header(&supplied_unit(0, 1, nibbles));
        assert_eq!(observed.subcode, 0);
        assert_eq!(observed.controls.raw(), 1 << control_bit);
    }
}

#[test]
fn c12_c14_are_reported_in_header_order_without_charset_inference() {
    for (last_nibble, raw_bits) in [(1, 0), (3, 4), (5, 2), (9, 1), (15, 7)] {
        let observed = header(&supplied_unit(0, 0, [8, 8, 0, 0, 0, 0, 0, last_nibble]));
        assert_eq!(observed.controls.c12_c14_bits(), raw_bits);
        assert_eq!(observed.display_page().unwrap().get(), 888);
    }
}

#[test]
fn supplied_unit_type_does_not_infer_or_override_c6() {
    let non_subtitle_unit_with_c6 = supplied_unit(0, 2, [0, 0, 0, 0, 0, 8, 0, 0]);
    let observed = header(&non_subtitle_unit_with_c6);
    assert_eq!(
        observed.packet_address.data_unit_kind,
        TeletextDataUnitKind::NonSubtitle
    );
    assert!(observed.controls.subtitle());
    let mut subtitle_unit_without_c6 = fixture_header();
    subtitle_unit_without_c6[0] = 3;
    let observed = header(&subtitle_unit_without_c6);
    assert_eq!(
        observed.packet_address.data_unit_kind,
        TeletextDataUnitKind::Subtitle
    );
    assert!(!observed.controls.subtitle());
}

#[test]
fn ff_terminators_and_prohibited_null_address_are_distinct() {
    for nibbles in [[15, 15, 0, 0, 0, 0, 1, 1], [15, 15, 14, 15, 15, 15, 15, 15]] {
        let TeletextHeaderObservation::Terminator(observed) =
            observe_dvb_teletext_header(&supplied_unit(0, 2, nibbles)).unwrap()
        else {
            panic!("FF terminator became acquired page");
        };
        assert_eq!(observed.wire_page, 0xff);
        assert!(observed.display_page().is_none());
    }
    assert_eq!(
        observe_dvb_teletext_header(&supplied_unit(0, 2, [15; 8])),
        Err(TeletextHeaderError::ProhibitedNullAddress)
    );
    let observed = header(&supplied_unit(0, 2, [14, 15, 15, 15, 15, 15, 15, 15]));
    assert_eq!(observed.wire_page, 0xfe);
    assert_eq!(observed.subcode, 0x3f7f);
    assert!(observed.display_page().is_none());
}

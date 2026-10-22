use super::*;
use crate::teletext::source_header::{observe_dvb_teletext_header, TeletextDataUnitKind};
use crate::teletext::source_observation::SuppliedTeletextHeaderObserver;
use ersatzrs_domain::{
    SourceTeletextHeaderProgram, SourceTeletextHeaderWindow, SourceTeletextObservedHeader,
    SourceTeletextPageReadiness,
};

// EN 300 706 Hamming(8,4) codewords, independent from the packet encoder.
const CODEWORDS: [u8; 16] = [
    0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd, 0xea,
];

fn packet(number: u8, magazine: u8) -> [u8; 46] {
    let mut raw = [0x20_u8.reverse_bits(); 46];
    raw[..4].copy_from_slice(&[3, 44, 0xe0, 0xe4]);
    let address = magazine | (number << 3);
    raw[4] = CODEWORDS[usize::from(address & 15)].reverse_bits();
    raw[5] = CODEWORDS[usize::from(address >> 4)].reverse_bits();
    raw
}

fn header(magazine: u8, page: u8, subcode: u16, serial: bool) -> [u8; 46] {
    let mut raw = packet(0, magazine);
    let [sub_low, sub_high] = subcode.to_le_bytes();
    let nibbles = [
        page & 15,
        page >> 4,
        sub_low & 15,
        ((sub_low >> 4) & 7) | 8, // C4: an observed erased acquisition.
        sub_high & 15,
        (sub_high >> 4) & 3,
        0,
        u8::from(serial),
    ];
    for (byte, nibble) in raw[6..14].iter_mut().zip(nibbles) {
        *byte = CODEWORDS[usize::from(nibble)].reverse_bits();
    }
    raw
}

fn context(number: u8, magazine: u8, designation: u8, values: [u8; 4]) -> [u8; 46] {
    let mut raw = packet(number, magazine);
    raw[6] = CODEWORDS[usize::from(designation)].reverse_bits();
    let [function, coding, primary, secondary] = values;
    let mut words = [0_u32; 13];
    words[0] = u32::from(function)
        | (u32::from(coding) << 4)
        | (u32::from(primary) << 7)
        | (u32::from(secondary & 15) << 14);
    words[1] = u32::from(secondary >> 4);
    for (target, word) in raw[7..].chunks_exact_mut(3).zip(words) {
        for (byte, coded) in target.iter_mut().zip(hamming_24_18(word)) {
            *byte = coded.reverse_bits();
        }
    }
    raw
}

fn capture(units: &[[u8; 46]]) -> (SourceTeletextPageWindow, SourceTeletextHeaderWindow) {
    let mut collector = SourceTeletextPageCollector::new();
    let mut headers = Vec::new();
    for (index, raw) in units.iter().enumerate() {
        let observation = observe_dvb_teletext_header(raw).unwrap();
        let ordinal = headers.len();
        if let TeletextHeaderObservation::Header(header) = observation {
            headers.push(SourceTeletextObservedHeader {
                wire_magazine: header.packet_address.wire_magazine,
                wire_page: header.wire_page,
                subcode: header.subcode,
                controls: header.controls.raw(),
                subtitle_data_unit: header.packet_address.data_unit_kind
                    == TeletextDataUnitKind::Subtitle,
                field_parity: header.packet_address.field_parity,
                line_offset: header.packet_address.line_offset,
                corrected_address_bytes: header.packet_address.corrected_address_bytes,
                corrected_header_bytes: header.corrected_header_bytes,
                completion_packet: u32::try_from(index).unwrap(),
                pes_ordinal: u32::try_from(index).unwrap(),
                unit_ordinal: 0,
                pts: None,
            });
        }
        collector.observe(
            raw,
            observation,
            ordinal,
            SourceTeletextUnitPosition {
                completion_packet: index,
                pes_ordinal: index,
                unit_ordinal: 0,
                pts: None,
            },
        );
    }
    let headers = SourceTeletextHeaderWindow {
        transport_stream_id: 1,
        pat_version: 0,
        psi_fingerprint: [7; 32],
        packet_count: u32::try_from(units.len()).unwrap(),
        complete_pes: u32::try_from(units.len()).unwrap(),
        unobserved_prefix: false,
        ended_at_eof: true,
        programs: vec![SourceTeletextHeaderProgram {
            program_number: 1,
            pmt_pid: 0x1000,
            pcr_pid: 0x101,
            pmt_version: 0,
            stream_type: 6,
            descriptors: Vec::new(),
        }],
        headers,
    };
    let pages = collector.finish();
    pages.validate(&headers).unwrap();
    crate::teletext::validate_source_page_observations(&pages, &headers).unwrap();
    (pages, headers)
}

#[test]
fn persisted_raw_headers_context_and_mode_must_reproduce_the_typed_facts() {
    let (pages, headers) = capture(&[
        context(29, 1, 0, [0, 0, 0, 127]),
        header(1, 0x24, 1, false),
        packet(1, 1),
        header(1, 0x25, 0, false),
    ]);
    for kind in 0..3 {
        let mut inconsistent = pages.clone();
        match kind {
            0 => inconsistent.units[1].data[6] = CODEWORDS[5].reverse_bits(),
            1 => inconsistent.pages[0].context.primary = Some(1),
            _ => inconsistent.transmission_mode_conflict = true,
        }
        // These are structurally valid claims, but not what the bytes declare.
        inconsistent.validate(&headers).unwrap();
        assert!(
            crate::teletext::validate_source_page_observations(&inconsistent, &headers).is_err()
        );
    }
}

#[test]
fn parallel_pages_keep_repeated_subcodes_and_exact_interleaved_bodies() {
    let units = [
        packet(1, 1), // A prefix body is never attached to a future header.
        header(1, 0x24, 0x1234, false),
        header(2, 0x25, 1, false),
        packet(2, 1),
        packet(3, 2),
        header(1, 0x24, 0x1235, false),
        packet(4, 2),
        packet(5, 1),
        header(2, 0x25, 1, false),
        header(1, 0x24, 0x1235, false),
    ];
    let (pages, headers) = capture(&units);
    assert_eq!(pages.pages.len(), 5);
    assert_eq!(pages.pages[0].body_units, [3]);
    assert_eq!(pages.pages[0].closing_unit, Some(5));
    assert_eq!(pages.pages[1].body_units, [4, 6]);
    assert_eq!(pages.pages[1].closing_unit, Some(8));
    assert_eq!(pages.pages[2].body_units, [7]);
    assert_eq!(pages.pages[2].closing_unit, Some(9));
    assert_eq!(headers.headers[0].subcode, 0x1234);
    assert_eq!(headers.headers[2].subcode, 0x1235);
    assert_eq!(headers.headers[4].subcode, 0x1235);
    assert_eq!(pages.unassociated_body_units, 1);
    assert!(!pages.transmission_mode_conflict);
    for (captured, original) in pages.units.iter().zip(units) {
        assert_eq!(captured.data, original);
    }
    assert_eq!(
        pages.readiness(&headers).unwrap()[4],
        SourceTeletextPageReadiness::Open
    );
}

#[test]
fn serial_headers_and_terminators_close_without_creating_a_fictitious_page() {
    let units = [
        header(1, 0x24, 1, true),
        packet(1, 1),
        header(2, 0x25, 2, true),
        packet(2, 1), // Serial page1 was closed by magazine2.
        packet(3, 2),
        header(3, 0xff, 1, true), // Time filling terminates the serial page.
        packet(4, 2),
    ];
    let (pages, _) = capture(&units);
    assert_eq!(pages.pages.len(), 2);
    assert_eq!(pages.pages[0].closing_unit, Some(2));
    assert_eq!(pages.pages[0].body_units, [1]);
    assert_eq!(pages.pages[1].closing_unit, Some(5));
    assert_eq!(pages.pages[1].body_units, [4]);
    assert_eq!(pages.unassociated_body_units, 2);
    assert_eq!(pages.units[5].data, units[5]);
    assert!(!pages.transmission_mode_conflict);

    let (conflict, headers) = capture(&[
        header(1, 0x24, 1, true),
        packet(1, 1),
        header(2, 0x25, 2, false),
    ]);
    assert!(conflict.transmission_mode_conflict);
    assert_eq!(
        conflict.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::PriorState
    );
    let (terminator_conflict, _) = capture(&[
        header(1, 0x24, 1, true),
        packet(1, 1),
        header(2, 0xff, 1, false),
    ]);
    assert_eq!(terminator_conflict.pages.len(), 1);
    assert_eq!(terminator_conflict.pages[0].closing_unit, Some(2));
    assert!(terminator_conflict.transmission_mode_conflict);
}

#[test]
fn eof_and_no_body_never_manufacture_a_closed_page_or_charset() {
    let (open, headers) = capture(&[header(1, 0x24, 1, false), packet(1, 1)]);
    assert!(headers.ended_at_eof);
    assert_eq!(open.pages[0].closing_unit, None);
    assert_eq!(
        open.readiness(&headers).unwrap(),
        [SourceTeletextPageReadiness::Open]
    );
    let (empty, headers) = capture(&[header(1, 0x24, 1, false), header(1, 0x24, 2, false)]);
    assert!(empty.pages[0].body_units.is_empty());
    assert_eq!(empty.pages[0].context, SourceTeletextPageContext::default());
    assert_eq!(
        empty.readiness(&headers).unwrap()[0],
        SourceTeletextPageReadiness::ContextUnknown
    );
}

#[test]
fn magazine_context_snapshots_do_not_retroactively_change_closed_pages() {
    let units = [
        context(29, 1, 4, [0, 0, 0x20, 0x7f]),
        header(1, 0x24, 1, false),
        packet(1, 1),
        header(1, 0x24, 2, false),
        context(29, 1, 0, [0, 0, 0x40, 0x44]),
        context(29, 1, 4, [0, 0, 0x55, 0x57]),
        packet(1, 1),
        header(1, 0x24, 3, false),
    ];
    let (pages, _) = capture(&units);
    let first = &pages.pages[0].context;
    assert_eq!(first.primary, Some(0x20));
    assert_eq!(first.secondary, Some(0x7f));
    assert_eq!(
        first.charset_origin,
        Some(SourceTeletextCharsetOrigin::Magazine29_4)
    );
    assert_eq!(first.context_unit, Some(0));
    let second = &pages.pages[1].context;
    assert_eq!(second.primary, Some(0x40));
    assert_eq!(second.secondary, Some(0x44));
    assert_eq!(
        second.charset_origin,
        Some(SourceTeletextCharsetOrigin::Magazine29_0)
    );
    assert_eq!(second.context_unit, Some(4));
}

#[test]
fn page_context_takes_precedence_without_normalizing_raw_designations() {
    for (first, second) in [(0, 4), (4, 0)] {
        let units = [
            context(29, 1, 0, [0, 0, 0x20, 0x7f]),
            header(1, 0x24, 1, false),
            context(28, 1, first, [0, 0, first + 0x40, 0x7f]),
            context(28, 1, second, [0, 0, second + 0x40, 0x7f]),
            header(1, 0x24, 2, false),
        ];
        let (pages, _) = capture(&units);
        assert_eq!(pages.pages[0].context.primary, Some(0x40));
        assert_eq!(pages.pages[0].context.secondary, Some(0x7f));
        assert_eq!(
            pages.pages[0].context.charset_origin,
            Some(SourceTeletextCharsetOrigin::Page28_0)
        );
    }
    let (page4, _) = capture(&[
        context(29, 1, 0, [0, 0, 0x20, 0x7f]),
        header(1, 0x24, 1, false),
        context(28, 1, 4, [0, 0, 0x55, 0x7f]),
        header(1, 0x24, 2, false),
    ]);
    assert_eq!(
        page4.pages[0].context.charset_origin,
        Some(SourceTeletextCharsetOrigin::Page28_4)
    );
    assert_eq!(page4.pages[0].context.primary, Some(0x55));
}

#[test]
fn every_context_triplet_is_checked_and_dirty_overrides_never_borrow_fallback() {
    for triplet in 0..13 {
        let mut dirty = context(28, 1, 0, [0, 0, 0x40, 0x44]);
        dirty[7 + triplet * 3] ^= 3;
        let (pages, _) = capture(&[
            context(29, 1, 0, [0, 0, 0x20, 0x7f]),
            header(1, 0x24, 1, false),
            context(28, 1, 4, [0, 0, 0x55, 0x7f]),
            dirty,
            header(1, 0x24, 2, false),
        ]);
        let actual = &pages.pages[0].context;
        assert!(actual.protection_error);
        assert_eq!(actual.primary, None);
        assert_eq!(actual.page_function, None);
        assert_eq!(
            actual.charset_origin,
            Some(SourceTeletextCharsetOrigin::Page28_0)
        );
        assert_eq!(actual.context_unit, Some(3));
    }
    let mut designation = context(28, 1, 0, [0, 0, 0x40, 0x7f]);
    designation[6] ^= 3;
    let (pages, _) = capture(&[
        context(29, 1, 0, [0, 0, 0x20, 0x7f]),
        header(1, 0x24, 1, false),
        context(28, 1, 0, [0, 0, 0x40, 0x7f]),
        designation,
        context(28, 1, 4, [0, 0, 0x55, 0x7f]),
        header(1, 0x24, 2, false),
    ]);
    assert!(pages.pages[0].context.protection_error);
    assert_eq!(pages.pages[0].context.charset_origin, None);
    assert_eq!(pages.pages[0].context.primary, None);
}

#[test]
fn non_display_function_and_coding_retain_raw_data_without_display_parity_claims() {
    for (function, coding) in [(1, 0), (0, 1), (15, 7)] {
        let mut body = packet(1, 1);
        body[6..].fill(0); // Legal retained raw coding, not odd-parity display text.
        let (pages, headers) = capture(&[
            context(29, 1, 0, [function, coding, 0x20, 0x7f]),
            header(1, 0x24, 1, false),
            body,
            header(1, 0x24, 2, false),
        ]);
        assert_eq!(pages.pages[0].context.page_function, Some(function));
        assert_eq!(pages.pages[0].context.page_coding, Some(coding));
        assert_eq!(pages.pages[0].context.primary, None);
        assert_eq!(pages.pages[0].context.secondary, None);
        assert_eq!(pages.units[2].data, body);
        assert_eq!(
            pages.readiness(&headers).unwrap()[0],
            SourceTeletextPageReadiness::NeedsEnhancementContext
        );
    }
    let (page_override, _) = capture(&[
        context(29, 1, 0, [0, 0, 0x20, 0x7f]),
        header(1, 0x24, 1, false),
        context(28, 1, 0, [1, 0, 0x40, 0x44]),
        header(1, 0x24, 2, false),
    ]);
    assert_eq!(page_override.pages[0].context.page_function, Some(1));
    assert_eq!(page_override.pages[0].context.primary, None);
    assert_eq!(
        page_override.pages[0].context.charset_origin,
        Some(SourceTeletextCharsetOrigin::Page28_0)
    );
    for values in [[4, 1, 0x40, 0x44], [0, 0, 0x20, 0x7f]] {
        let (drcs, _) = capture(&[
            context(29, 1, 0, [0, 0, 0x20, 0x7f]),
            header(1, 0x24, 1, false),
            context(28, 1, 3, values),
            header(1, 0x24, 2, false),
        ]);
        assert_eq!(drcs.pages[0].context.page_function, Some(values[0]));
        assert_eq!(drcs.pages[0].context.page_coding, Some(values[1]));
        assert_eq!(drcs.pages[0].context.primary, None);
        assert_eq!(drcs.pages[0].context.secondary, None);
        assert_eq!(
            drcs.pages[0].context.charset_origin,
            Some(SourceTeletextCharsetOrigin::Page28_3)
        );
    }
}

#[test]
fn source_hamming24_corrects_single_bits_and_rejects_double_bits() {
    for (bytes, expected) in [([0x8b, 0x80, 0], 0), ([0x74, 0x7f, 0xff], 0x3ffff)] {
        assert_eq!(decode_hamming_24(bytes), Some(expected));
        for bit in 0..24 {
            let mut damaged = bytes;
            damaged[bit / 8] ^= 1 << (bit % 8);
            assert_eq!(decode_hamming_24(damaged), Some(expected));
            for second in bit + 1..24 {
                let mut double = damaged;
                double[second / 8] ^= 1 << (second % 8);
                assert_eq!(decode_hamming_24(double), None);
            }
        }
    }
}

#[test]
fn page_capacity_is_inclusive_then_fences_all_body_evidence() {
    let mut units = vec![header(1, 0x24, 1, false)];
    units.resize(MAX_SOURCE_TELETEXT_CAPTURED_UNITS, packet(1, 1));
    let (at_bound, _) = capture(&units);
    assert_eq!(at_bound.units.len(), MAX_SOURCE_TELETEXT_CAPTURED_UNITS);
    assert!(!at_bound.capacity_exhausted);
    units.push(header(2, 0x25, 1, true));
    units.push(packet(1, 2));
    let (exhausted, headers) = capture(&units);
    assert_eq!(headers.headers.len(), 2);
    assert!(exhausted.capacity_exhausted);
    assert!(exhausted.transmission_mode_conflict);
    assert!(exhausted.units.is_empty() && exhausted.pages.is_empty());
    assert_eq!(exhausted.unassociated_body_units, 0);

    let headers = vec![header(1, 0x24, 1, false); MAX_SOURCE_TELETEXT_CAPTURED_PAGES];
    let (at_page_bound, _) = capture(&headers);
    assert_eq!(
        at_page_bound.pages.len(),
        MAX_SOURCE_TELETEXT_CAPTURED_PAGES
    );
    assert!(!at_page_bound.capacity_exhausted);
}

#[test]
fn opt_in_observer_preserves_the_existing_header_result_and_aggregate_discard() {
    let bytes = include_str!("../../tests/fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|value| u8::from_str_radix(value, 16).unwrap())
        .collect::<Vec<_>>();
    let mut original = SuppliedTeletextHeaderObserver::new(0x100).unwrap();
    let mut pages = SuppliedTeletextHeaderObserver::new_with_pages(0x100).unwrap();
    let mut discarded = SuppliedTeletextHeaderObserver::new_with_pages(0x100).unwrap();
    for packet in bytes.chunks_exact(188) {
        let packet = packet.try_into().unwrap();
        original.push(packet).unwrap();
        pages.push(packet).unwrap();
        discarded.push(packet).unwrap();
    }
    assert_eq!(original.retained_page_unit_count(), 0);
    assert!(pages.retained_page_unit_count() > 0);
    discarded.discard_page_evidence_for_capacity();
    assert_eq!(discarded.retained_page_unit_count(), 0);
    let original = original.finish().unwrap();
    let pages = pages.finish().unwrap();
    let mut discarded = discarded.finish().unwrap();
    assert!(original.page_observations().is_none());
    assert_eq!(original.headers(), pages.headers());
    assert_eq!(original.headers(), discarded.headers());
    assert_eq!(original.packets(), pages.packets());
    assert_eq!(original.complete_pes(), pages.complete_pes());
    assert!(!pages.page_observations().unwrap().capacity_exhausted);
    assert!(
        discarded
            .take_page_observations()
            .unwrap()
            .capacity_exhausted
    );
    assert!(discarded.page_observations().is_none());
}

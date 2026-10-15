use super::*;

// Independent Python calibration producer, same exact first cycle used by the
// public source_assembly tests. No production packet encoder builds this oracle.
fn packets() -> Vec<[u8; 188]> {
    let bytes = include_str!("../../tests/fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|value| u8::from_str_radix(value, 16).unwrap())
        .collect::<Vec<_>>();
    bytes
        .chunks_exact(188)
        .map(|p| p.try_into().unwrap())
        .collect()
}

#[test]
fn source_observation_retains_actual_occurrences_and_supplied_positions() {
    let mut observer = SuppliedTeletextHeaderObserver::new(0x100).unwrap();
    for packet in packets() {
        observer.push(&packet).unwrap();
    }
    let window = observer.finish().unwrap();
    assert_eq!(window.elementary_pid(), 0x100);
    assert_eq!(window.packets(), 16);
    assert_eq!(window.complete_pes(), 7);
    assert!(!window.has_unobserved_prefix());
    let expected = [
        (200, 90_000, 129),
        (201, 108_000, 129),
        (888, 126_000, 141),
        (899, 162_000, 129),
    ];
    for (record, (page, pts, controls)) in window.headers().iter().zip(expected) {
        assert_eq!(record.header.display_page().unwrap().get(), page);
        assert_eq!(record.pts.unwrap().ticks(), pts);
        assert_eq!(record.header.controls.raw(), controls);
        assert_eq!(record.unit_ordinal, 0);
        assert_eq!(record.header.corrected_header_bytes, 0);
        assert_eq!(record.header.packet_address.corrected_address_bytes, 0);
    }
    assert_eq!(window.headers().len(), 4);
    assert!(window
        .headers()
        .windows(2)
        .all(|pair| pair[0].completion_packet < pair[1].completion_packet
            && pair[0].pes_ordinal < pair[1].pes_ordinal));
}

#[test]
fn source_observation_never_converts_no_pid_or_unseen_prefix_into_inventory() {
    let mut absent = SuppliedTeletextHeaderObserver::new(0x101).unwrap();
    for packet in packets() {
        absent.push(&packet).unwrap();
    }
    let absent = absent.finish().unwrap();
    assert!(absent.headers().is_empty());
    assert_eq!(absent.complete_pes(), 0);
    let mut prefix = SuppliedTeletextHeaderObserver::new(0x100).unwrap();
    let mut packet = [0xff; 188];
    packet[..4].copy_from_slice(&[0x47, 1, 0, 0x10]);
    prefix.push(&packet).unwrap();
    let prefix = prefix.finish().unwrap();
    assert!(prefix.has_unobserved_prefix());
    assert!(prefix.headers().is_empty());
}

#[test]
fn source_observation_protection_failure_discards_earlier_headers_and_stays_failed() {
    let mut data = packets();
    // First header PES starts at byte4; DVB unit starts after 46 PES bytes.
    // Corrupt the second header's first protected page byte with a double error.
    let second = 7; // Calibration page201 follows the first page200/body pair.
    data[second][4 + 46 + 6] ^= 3;
    let mut observer = SuppliedTeletextHeaderObserver::new(0x100).unwrap();
    for packet in &data[..second] {
        observer.push(packet).unwrap();
    }
    assert_eq!(observer.window.headers.len(), 1);
    let error = observer.push(&data[second]).unwrap_err();
    assert!(matches!(error, TeletextSourceObservationError::Header(_)));
    assert!(observer.window.headers.is_empty());
    assert_eq!(observer.push(&data[second]), Err(error));
    assert_eq!(observer.finish().unwrap_err(), error);
}

#[test]
fn source_observation_packet_bound_is_inclusive_and_sticky() {
    let mut observer = SuppliedTeletextHeaderObserver::new(0x100).unwrap();
    let mut other = [0xff; 188];
    other[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    for _ in 0..MAX_SOURCE_OBSERVATION_PACKETS {
        observer.push(&other).unwrap();
    }
    assert_eq!(
        observer.push(&other),
        Err(TeletextSourceObservationError::Packets)
    );
    assert_eq!(
        observer.finish().unwrap_err(),
        TeletextSourceObservationError::Packets
    );
}

#[test]
fn source_observation_header_bound_counts_repeated_occurrences_without_truncation() {
    let cycle = packets();
    let mut observer = SuppliedTeletextHeaderObserver::new(0x100).unwrap();
    let mut cc = 0_u8;
    for _ in 0..MAX_SOURCE_OBSERVATION_HEADERS / 4 {
        for mut packet in cycle.clone() {
            if packet[1] & 0x1f == 1 && packet[2] == 0 {
                let counter = if packet[3] & 0x10 != 0 {
                    let counter = cc;
                    cc = (cc + 1) & 15;
                    counter
                } else {
                    cc.wrapping_sub(1) & 15
                };
                packet[3] = (packet[3] & 0xf0) | counter;
            }
            observer.push(&packet).unwrap();
        }
    }
    assert_eq!(
        observer.window.headers.len(),
        MAX_SOURCE_OBSERVATION_HEADERS
    );
    for mut packet in cycle {
        if packet[1] & 0x1f == 1 && packet[2] == 0 {
            let counter = if packet[3] & 0x10 != 0 {
                let counter = cc;
                cc = (cc + 1) & 15;
                counter
            } else {
                cc.wrapping_sub(1) & 15
            };
            packet[3] = (packet[3] & 0xf0) | counter;
        }
        if observer.push(&packet).is_err() {
            break;
        }
    }
    assert_eq!(
        observer.finish().unwrap_err(),
        TeletextSourceObservationError::Headers
    );
}

#[test]
fn source_observation_finish_refuses_a_window_cut_inside_selected_pes() {
    let mut observer = SuppliedTeletextHeaderObserver::new(0x100).unwrap();
    let mut data = packets();
    data[3][8..10].copy_from_slice(&362_u16.to_be_bytes());
    for packet in &data[..4] {
        observer.push(packet).unwrap();
    }
    assert_eq!(
        observer.finish().unwrap_err(),
        TeletextSourceObservationError::Assembly(TeletextAssemblyError::Incomplete)
    );
}

#[test]
fn source_observation_termination_and_unknown_units_are_not_pages() {
    for unknown in [false, true] {
        let mut data = packets();
        let unit = &mut data[3][50..96];
        if unknown {
            unit[0] = 0x99;
        } else {
            // Normative Hamming(8,4) values 15/15/1 in DVB bit order:
            // FF:0001 is termination, not the prohibited FF:3F7F address.
            unit[6] = 0x57;
            unit[7] = 0x57;
            unit[8] = 0x40;
        }
        let mut observer = SuppliedTeletextHeaderObserver::new(0x100).unwrap();
        for packet in data {
            observer.push(&packet).unwrap();
        }
        let window = observer.finish().unwrap();
        assert_eq!(window.complete_pes(), 7);
        assert_eq!(window.headers().len(), 3);
        assert_eq!(
            window.headers()[0].header.display_page().unwrap().get(),
            201
        );
    }
}

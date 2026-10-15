use super::*;
use ersatzrs_ffmpeg::teletext::source_observation::SuppliedTeletextHeaderObserver;

const PID: u16 = 0x100;
const PMT: u16 = 0x1000;

fn fixture() -> Vec<[u8; 188]> {
    include_str!("../../../../../ersatzrs-ffmpeg/tests/fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|value| u8::from_str_radix(value, 16).unwrap())
        .collect::<Vec<_>>()
        .chunks_exact(188)
        .map(|bytes| bytes.try_into().unwrap())
        .collect()
}

fn collector(page: u16, start_pts: i64, end_pts: i64) -> PagePreludeCollector {
    PagePreludeCollector::new(
        PID,
        PMT,
        TeletextPageNumber::new(page).unwrap(),
        SourceNativeEpoch::from_first_demux_timestamps(Some(90_000), None),
        start_pts * 300,
        end_pts * 300,
    )
    .unwrap()
}

fn header(page: u16, pts: u64, erase: bool, cc: u8) -> [u8; 188] {
    let mut raw = fixture()[if page == 200 { 3 } else { 11 }];
    raw[3] = 0x10 | cc;
    raw[13..18].copy_from_slice(&[
        0x21 | u8::try_from((pts >> 29) & 0x0e).unwrap(),
        u8::try_from((pts >> 22) & 0xff).unwrap(),
        u8::try_from(((pts >> 14) & 0xfe) | 1).unwrap(),
        u8::try_from((pts >> 7) & 0xff).unwrap(),
        u8::try_from(((pts << 1) & 0xfe) | 1).unwrap(),
    ]);
    // Supplied calibration S2/C4 has zero subcode nibble and C4 set. These
    // are the normative Hamming 8/4 codewords for 8 and 0, in DVB bit order.
    raw[4 + 46 + 9] = if erase { 0x0b } else { 0xa8 };
    raw
}

fn feed(collector: &mut PagePreludeCollector, packets: &[[u8; 188]]) {
    for (ordinal, raw) in packets.iter().enumerate() {
        collector
            .push(raw, u64::try_from(ordinal).unwrap())
            .unwrap();
    }
}

fn with_psi(selected: &[[u8; 188]]) -> Vec<[u8; 188]> {
    let mut packets = fixture()[..2].to_vec();
    packets.extend_from_slice(selected);
    packets
}

#[test]
fn latest_prior_c4_is_retained_across_incremental_requested_page_updates() {
    let mut observation = collector(200, 110_000, 150_000);
    feed(
        &mut observation,
        &with_psi(&[
            header(200, 90_000, true, 0),
            header(200, 95_000, false, 1),
            header(200, 100_000, true, 2),
            header(200, 105_000, false, 3),
            header(200, 120_000, true, 4),
        ]),
    );
    let witness = observation.finish().unwrap().unwrap();
    assert_eq!(witness.source_pusi_packet(), 4);
    assert_eq!(witness.completion_source_packet(), 4);
    assert_eq!(witness.raw_pts().ticks(), 100_000);
    assert_eq!(witness.native_pts_ticks(), 30_000_000);
    assert_eq!(witness.header().display_page().unwrap().get(), 200);
    assert!(witness.header().controls.erase_page());
}

#[test]
fn first_future_c4_is_retained_only_inside_the_requested_end() {
    // The independent fixture first supplies page200 at 90000, then page888
    // at 126000. Other pages do not invent earlier page888 state.
    let mut observation = collector(888, 120_000, 126_000);
    feed(&mut observation, &fixture());
    let witness = observation.finish().unwrap().unwrap();
    assert_eq!(witness.source_pusi_packet(), 11);
    assert_eq!(witness.raw_pts().ticks(), 126_000);
    assert_eq!(witness.psi_start_packet(), 0);

    let mut outside = collector(888, 120_000, 125_999);
    feed(&mut outside, &fixture());
    assert!(outside.finish().unwrap().is_none());

    let mut repeated = collector(888, 120_000, 150_000);
    feed(
        &mut repeated,
        &with_psi(&[header(888, 126_000, true, 0), header(888, 140_000, true, 1)]),
    );
    assert_eq!(
        repeated.finish().unwrap().unwrap().raw_pts().ticks(),
        126_000
    );
}

#[test]
fn incremental_unknown_state_prefix_and_missing_pts_cannot_be_repaired_by_later_reset() {
    let mut prefix = header(200, 85_000, true, 15);
    prefix[1] &= !0x40;
    let mut missing = header(888, 85_000, true, 15);
    missing[11] = 0;
    missing[13..18].fill(0xff);
    for before in [prefix, missing, header(200, 85_000, false, 15)] {
        let mut observation = collector(200, 100_000, 150_000);
        feed(
            &mut observation,
            &with_psi(&[before, header(200, 90_000, true, 0)]),
        );
        assert!(observation.finish().unwrap().is_none());
    }
}

#[test]
fn duplicate_pusi_does_not_move_the_actual_multpacket_pes_start() {
    let mut first = header(200, 90_000, true, 0);
    first[8..10].copy_from_slice(&362_u16.to_be_bytes());
    let mut last = [0xff; 188];
    last[..4].copy_from_slice(&[0x47, 1, 0, 0x11]);
    for stuffing in last[4..].chunks_exact_mut(46) {
        stuffing[0] = 0xff;
        stuffing[1] = 44;
    }
    let mut observation = collector(200, 100_000, 150_000);
    feed(&mut observation, &with_psi(&[first, first, last]));
    let witness = observation.finish().unwrap().unwrap();
    assert_eq!(witness.source_pusi_packet(), 2);
    assert_eq!(witness.completion_source_packet(), 4);
}

#[test]
fn locator_requires_preceding_pat_and_pmt_and_caches_pat_at_pmt_start() {
    let packets = fixture();
    let selected = header(200, 90_000, true, 0);
    for incomplete in [
        vec![selected],
        vec![packets[0], selected],
        vec![packets[1], selected],
        vec![packets[1], packets[0], selected],
    ] {
        let mut observation = collector(200, 100_000, 150_000);
        feed(&mut observation, &incomplete);
        assert!(observation.finish().unwrap().is_none());
    }
    let mut observation = collector(200, 100_000, 150_000);
    feed(
        &mut observation,
        &[packets[0], packets[1], packets[0], selected],
    );
    assert_eq!(observation.finish().unwrap().unwrap().psi_start_packet(), 0);
}

#[test]
fn decreasing_selected_pts_and_unknown_epoch_remain_unknown() {
    let mut observation = collector(200, 100_000, 150_000);
    feed(
        &mut observation,
        &with_psi(&[header(200, 90_000, true, 0), header(888, 89_999, true, 1)]),
    );
    assert!(observation.finish().unwrap().is_none());
    assert!(PagePreludeCollector::new(
        PID,
        PMT,
        TeletextPageNumber::new(200).unwrap(),
        SourceNativeEpoch::unknown(),
        1,
        2,
    )
    .is_none());
}

#[test]
fn malformed_transport_or_protected_headers_and_incomplete_pes_remain_errors() {
    let mut broken_header = header(200, 90_000, true, 0);
    broken_header[4 + 46 + 6] ^= 3;
    let mut broken_cc = header(200, 90_001, true, 2);
    broken_cc[3] = 0x12;
    for selected in [
        vec![broken_header],
        vec![header(200, 90_000, true, 0), broken_cc],
    ] {
        let mut observation = collector(200, 100_000, 150_000);
        let mut failed = false;
        for (ordinal, raw) in with_psi(&selected).iter().enumerate() {
            failed |= observation
                .push(raw, u64::try_from(ordinal).unwrap())
                .is_err();
        }
        assert!(failed);
        assert!(observation.finish().is_err());
    }
    let mut incomplete = header(200, 90_000, true, 0);
    incomplete[8..10].copy_from_slice(&362_u16.to_be_bytes());
    let mut observation = collector(200, 100_000, 150_000);
    feed(&mut observation, &with_psi(&[incomplete]));
    assert!(observation.finish().is_err());
}

#[test]
fn omitted_physical_packets_cannot_form_a_whole_source_prelude() {
    let mut observation = collector(200, 100_000, 150_000);
    assert!(observation.push(&fixture()[0], 1).is_err());
    assert!(observation.push(&fixture()[0], 0).is_err());
    assert!(observation.finish().is_err());
}

fn observed_headers(packets: &[[u8; 188]]) -> SuppliedTeletextHeaderWindow {
    let mut observer = SuppliedTeletextHeaderObserver::new(PID).unwrap();
    for raw in packets {
        observer.push(raw).unwrap();
    }
    observer.finish().unwrap()
}

#[test]
fn exact_anchor_match_accounts_for_actual_snapshot_start_and_prefix_omissions() {
    let packets = fixture();
    let mut observation = collector(888, 120_000, 150_000);
    feed(&mut observation, &packets);
    let witness = observation.finish().unwrap().unwrap();
    assert!(witness.is_retained_in(&observed_headers(&packets), 0, 0));
    // Starting at physical packet8 omits the three selected-PID packets8–10
    // before page888's PES start11. This tests header matching only, not PSI.
    let shifted = observed_headers(&packets[11..]);
    assert!(witness.is_retained_in(&shifted, 8, 3));
    assert!(!witness.is_retained_in(&shifted, 8, 2));
    assert!(!witness.is_retained_in(&shifted, 12, 0));
    assert!(!witness.is_retained_in(&shifted, 0, u32::MAX));
}

#[test]
fn omitted_anchor_cannot_be_replaced_by_a_later_same_header_and_pts() {
    let packets = with_psi(&[header(888, 126_000, true, 0)]);
    let mut observation = collector(888, 120_000, 150_000);
    feed(&mut observation, &packets);
    let witness = observation.finish().unwrap().unwrap();
    let empty = observed_headers(&packets[..2]);
    assert!(!witness.is_retained_in(&empty, 0, 0));

    let mut later = packets[..2].to_vec();
    let mut unrelated = [0xff; 188];
    unrelated[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    later.extend_from_slice(&[unrelated, packets[2]]);
    let later = observed_headers(&later);
    assert_eq!(later.headers()[0].pts, Some(witness.raw_pts()));
    assert_eq!(later.headers()[0].header, witness.header());
    assert!(!witness.is_retained_in(&later, 0, 0));
}

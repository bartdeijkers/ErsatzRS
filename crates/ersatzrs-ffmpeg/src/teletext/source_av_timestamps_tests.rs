use super::{
    SourceAvPesHeader, SourceAvStreamIdentity, SourceAvStreamKind, SourceAvTimestampError,
    SourceAvTimestampObserver,
};

const PID: u16 = 256;
const WRAP: u64 = 1_u64 << 33;

fn identity(kind: SourceAvStreamKind) -> SourceAvStreamIdentity {
    SourceAvStreamIdentity {
        stream_index: 3,
        pid: PID,
        kind,
    }
}

// Independently write the five-byte H.222.0 timestamp, including all markers.
fn timestamp(value: u64, prefix: u8) -> [u8; 5] {
    assert!(value < WRAP);
    [
        (prefix << 4) | u8::try_from(((value >> 30) & 7) << 1).unwrap() | 1,
        u8::try_from((value >> 22) & 255).unwrap(),
        u8::try_from(((value >> 15) & 127) << 1).unwrap() | 1,
        u8::try_from((value >> 7) & 255).unwrap(),
        u8::try_from((value & 127) << 1).unwrap() | 1,
    ]
}

fn header(pts: Option<u64>, dts: Option<u64>, extra: usize) -> Vec<u8> {
    assert!(pts.is_some() || dts.is_none());
    let flags = if dts.is_some() {
        0xc0
    } else if pts.is_some() {
        0x80
    } else {
        0
    };
    let mut bytes = vec![0, 0, 1, 0xe0, 0, 0, 0x80, flags, 0];
    if let Some(pts) = pts {
        bytes.extend_from_slice(&timestamp(pts, if dts.is_some() { 3 } else { 2 }));
    }
    if let Some(dts) = dts {
        bytes.extend_from_slice(&timestamp(dts, 1));
    }
    bytes.resize(bytes.len() + extra, 0xff);
    bytes[8] = u8::try_from(bytes.len() - 9).unwrap();
    // A declared payload need not be retained or completed by a header observer.
    let length = u16::try_from(bytes.len() - 6 + 100).unwrap();
    bytes[4..6].copy_from_slice(&length.to_be_bytes());
    bytes
}

fn packet(pid: u16, counter: u8, start: bool, payload: &[u8]) -> [u8; 188] {
    assert!(payload.len() <= 184);
    let mut raw = [0xff; 188];
    raw[0] = 0x47;
    raw[1] = u8::try_from(pid >> 8).unwrap() | if start { 0x40 } else { 0 };
    raw[2] = u8::try_from(pid & 255).unwrap();
    raw[3] = counter;
    match payload.len() {
        184 => raw[3] |= 0x10,
        0 => {
            raw[3] |= 0x20;
            raw[4] = 183;
            raw[5] = 0;
        }
        length => {
            raw[3] |= 0x30;
            raw[4] = u8::try_from(183 - length).unwrap();
            if raw[4] != 0 {
                raw[5] = 0;
            }
        }
    }
    raw[188 - payload.len()..].copy_from_slice(payload);
    raw
}

fn feed_split(bytes: &[u8], split: usize) -> (SourceAvTimestampObserver, SourceAvPesHeader) {
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 500).unwrap();
    let mut emitted = Vec::new();
    let mut counter = 0;
    for part in [&bytes[..split], &bytes[split..]] {
        for payload in part.chunks(184) {
            let raw = packet(PID, counter & 15, counter == 0, payload);
            if let Some(header) = observer.push(&raw).unwrap() {
                emitted.push(header);
            }
            counter += 1;
            observer
                .push(&packet(8191, 0, false, &[0xff; 184]))
                .unwrap();
        }
    }
    assert_eq!(emitted.len(), 1);
    (observer, emitted[0])
}

fn assert_fenced(observer: &mut SourceAvTimestampObserver, error: SourceAvTimestampError) {
    assert_eq!(observer.report(), Err(error));
    assert_eq!(observer.finish(), Err(error));
    assert_eq!(observer.push(&packet(8191, 0, false, &[])), Err(error));
}

#[test]
fn every_optional_header_split_preserves_raw_pts_dts_and_original_positions() {
    for bytes in [
        header(Some(127_680), None, 0),
        header(Some(129_600), Some(126_000), 0),
        header(Some(WRAP - 1), Some(0), 245),
    ] {
        for split in 1..bytes.len() {
            let (mut observer, actual) = feed_split(&bytes, split);
            let report = observer.finish().unwrap();
            assert_eq!(actual.source_pusi_packet, 500);
            assert_eq!(
                actual.header_completion_packet,
                500 + report.packets_seen - 2
            );
            assert_eq!(actual.stream_id, 0xe0);
            assert_eq!(
                actual.pts,
                report.first_pts.map(|sample| sample.raw_ticks_90khz)
            );
            assert_eq!(
                actual.dts,
                report.first_dts.map(|sample| sample.raw_ticks_90khz)
            );
            assert_eq!(report.headers_seen, 1);
            assert_eq!(report.headers_without_timestamps, 0);
            assert!(!report.pending_header);
            assert!(!report.unobserved_prefix);
            assert_eq!(observer.finish(), Ok(report));
        }
    }
}

#[test]
fn split_timestamps_match_independent_wire_values_including_wrap_and_reorder() {
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    for (counter, (pts, dts)) in [(900, 800), (850, 810), (WRAP - 1, 820), (0, 830)]
        .into_iter()
        .enumerate()
    {
        let bytes = header(Some(pts), Some(dts), 0);
        let completed = observer
            .push(&packet(PID, u8::try_from(counter).unwrap(), true, &bytes))
            .unwrap()
            .unwrap();
        assert_eq!(completed.pts, Some(pts));
        assert_eq!(completed.dts, Some(dts));
    }
    let report = observer.finish().unwrap();
    assert_eq!(report.first_pts.unwrap().raw_ticks_90khz, 900);
    assert_eq!(report.last_pts.unwrap().raw_ticks_90khz, 0);
    assert_eq!(report.first_dts.unwrap().raw_ticks_90khz, 800);
    assert_eq!(report.last_dts.unwrap().raw_ticks_90khz, 830);
}

#[test]
fn missing_timestamps_remain_unknown_and_first_dts_is_independent_of_first_pts() {
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Audio), 40).unwrap();
    assert_eq!(observer.report().unwrap().first_pts, None);
    for (counter, (pts, dts)) in [
        (None, None),
        (Some(0), None),
        (Some(5), Some(2)),
        (None, None),
    ]
    .into_iter()
    .enumerate()
    {
        let mut bytes = header(pts, dts, 0);
        bytes[3] = 0xbd; // Private-stream audio is not rejected as non-audio.
        let actual = observer
            .push(&packet(PID, u8::try_from(counter).unwrap(), true, &bytes))
            .unwrap()
            .unwrap();
        assert_eq!((actual.pts, actual.dts), (pts, dts));
    }
    let report = observer.finish().unwrap();
    assert_eq!(report.headers_seen, 4);
    assert_eq!(report.headers_without_timestamps, 2);
    assert_eq!(report.first_pts.unwrap().source_pusi_packet, 41);
    assert_eq!(report.first_pts.unwrap().raw_ticks_90khz, 0);
    assert_eq!(report.first_dts.unwrap().source_pusi_packet, 42);
    assert_eq!(report.last_pts.unwrap().source_pusi_packet, 42);
}

#[test]
fn zero_length_video_and_unfinished_av_payload_require_only_complete_header() {
    let mut bytes = header(Some(90_000), None, 0);
    bytes[4..6].copy_from_slice(&[0, 0]);
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    assert!(observer
        .push(&packet(PID, 15, true, &bytes))
        .unwrap()
        .is_some());
    observer.push(&packet(PID, 0, false, &[0xff; 184])).unwrap();
    assert_eq!(observer.finish().unwrap().headers_seen, 1);
    let mut audio = SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Audio), 0).unwrap();
    assert_eq!(
        audio.push(&packet(PID, 0, true, &bytes)),
        Err(SourceAvTimestampError::PesHeader)
    );
    assert_fenced(&mut audio, SourceAvTimestampError::PesHeader);
}

#[test]
fn duplicate_packets_do_not_emit_twice_or_extend_pending_header() {
    let bytes = header(Some(129_600), Some(126_000), 0);
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    let first = packet(PID, 15, true, &bytes[..11]);
    assert_eq!(observer.push(&first).unwrap(), None);
    assert_eq!(observer.push(&first).unwrap(), None);
    let final_packet = packet(PID, 0, false, &bytes[11..]);
    let actual = observer.push(&final_packet).unwrap().unwrap();
    assert_eq!(actual.source_pusi_packet, 0);
    assert_eq!(actual.header_completion_packet, 2);
    assert_eq!(observer.push(&final_packet).unwrap(), None);
    let report = observer.finish().unwrap();
    assert_eq!(report.headers_seen, 1);
    assert_eq!(report.packets_seen, 4);
}

#[test]
fn payload_continuity_is_checked_after_headers_and_on_repeated_counter_changes() {
    let bytes = header(Some(1), None, 0);
    for completed in [false, true] {
        let mut observer =
            SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
        let initial = if completed { &bytes[..] } else { &bytes[..5] };
        let first = packet(PID, 0, true, initial);
        observer.push(&first).unwrap();
        let mut changed = first;
        changed[187] ^= 1;
        assert_eq!(
            observer.push(&changed),
            Err(SourceAvTimestampError::Continuity)
        );
        assert_fenced(&mut observer, SourceAvTimestampError::Continuity);
    }
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    observer.push(&packet(PID, 0, true, &bytes)).unwrap();
    assert_eq!(
        observer.push(&packet(PID, 2, false, &[0xff; 184])),
        Err(SourceAvTimestampError::Continuity)
    );
    assert_fenced(&mut observer, SourceAvTimestampError::Continuity);
}

#[test]
fn adaptation_only_packets_preserve_pending_header_and_payload_counter() {
    let bytes = header(Some(90_000), None, 0);
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 20).unwrap();
    observer.push(&packet(PID, 7, false, &[])).unwrap(); // No earlier payload CC to compare.
    observer.push(&packet(PID, 0, true, &bytes[..4])).unwrap();
    observer.push(&packet(PID, 0, false, &[])).unwrap();
    assert!(observer.report().unwrap().pending_header);
    let actual = observer
        .push(&packet(PID, 1, false, &bytes[4..]))
        .unwrap()
        .unwrap();
    assert_eq!(actual.source_pusi_packet, 21);
    assert_eq!(actual.header_completion_packet, 23);
    assert_eq!(observer.finish().unwrap().headers_seen, 1);
}

#[test]
fn pre_start_payload_is_unobserved_but_other_programs_cannot_supply_headers() {
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    let bytes = header(Some(7), None, 0);
    observer.push(&packet(300, 0, true, &bytes)).unwrap();
    observer.push(&packet(PID, 0, false, &[0xff; 184])).unwrap();
    let report = observer.report().unwrap();
    assert!(report.unobserved_prefix);
    assert_eq!(report.headers_seen, 0);
    assert_eq!(report.first_pts, None);
    let actual = observer
        .push(&packet(PID, 1, true, &bytes))
        .unwrap()
        .unwrap();
    assert_eq!(actual.source_pusi_packet, 2);
    assert_eq!(actual.pts, Some(7));
}

#[test]
fn incomplete_header_finish_and_interrupted_start_are_sticky() {
    let bytes = header(Some(7), Some(2), 0);
    for length in 1..bytes.len() {
        let mut observer =
            SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
        assert_eq!(
            observer
                .push(&packet(PID, 0, true, &bytes[..length]))
                .unwrap(),
            None
        );
        assert!(observer.report().unwrap().pending_header);
        assert_eq!(
            observer.finish(),
            Err(SourceAvTimestampError::IncompleteHeader)
        );
        assert_fenced(&mut observer, SourceAvTimestampError::IncompleteHeader);
    }
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    observer.push(&packet(PID, 0, true, &bytes[..5])).unwrap();
    assert_eq!(
        observer.push(&packet(PID, 1, true, &bytes)),
        Err(SourceAvTimestampError::UnexpectedStart)
    );
    assert_fenced(&mut observer, SourceAvTimestampError::UnexpectedStart);
}

#[test]
fn timestamp_flags_prefixes_and_all_markers_are_checked_strictly() {
    let original = header(Some(127_680), Some(126_000), 0);
    let mut malformed = Vec::new();
    for (offset, value) in [(7, 0x40), (9, 0x21), (14, 0x21)] {
        let mut bytes = original.clone();
        bytes[offset] = value;
        malformed.push(bytes);
    }
    for offset in [9, 11, 13, 14, 16, 18] {
        let mut bytes = original.clone();
        bytes[offset] &= !1;
        malformed.push(bytes);
    }
    let mut pts_only_prefix = header(Some(0), None, 0);
    pts_only_prefix[9] = 0x31;
    malformed.push(pts_only_prefix);
    for bytes in malformed {
        let mut observer =
            SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
        assert_eq!(
            observer.push(&packet(PID, 0, true, &bytes)),
            Err(SourceAvTimestampError::Timestamp)
        );
        assert_fenced(&mut observer, SourceAvTimestampError::Timestamp);
    }
}

#[test]
fn malformed_pes_prefix_markers_scrambling_lengths_and_unparsed_stream_fail() {
    let original = header(Some(0), None, 0);
    for (offset, value) in [(0, 1), (6, 0), (6, 0xb0), (3, 0xbe), (5, 1)] {
        let mut bytes = original.clone();
        bytes[offset] = value;
        let mut observer =
            SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
        assert_eq!(
            observer.push(&packet(PID, 0, true, &bytes)),
            Err(SourceAvTimestampError::PesHeader)
        );
        assert_fenced(&mut observer, SourceAvTimestampError::PesHeader);
    }
    let mut bytes = original;
    bytes[8] = 4;
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    assert_eq!(
        observer.push(&packet(PID, 0, true, &bytes)),
        Err(SourceAvTimestampError::Timestamp)
    );
}

#[test]
fn discontinuity_is_explicit_initial_only_even_without_payload() {
    let bytes = header(Some(1), None, 0);
    for payload in [&[][..], &bytes[..]] {
        let mut flagged = packet(PID, 0, !payload.is_empty(), payload);
        flagged[5] |= 0x80;
        let mut observer =
            SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
        assert_eq!(
            observer.push(&flagged),
            Err(SourceAvTimestampError::Discontinuity)
        );
        assert_fenced(&mut observer, SourceAvTimestampError::Discontinuity);
        let mut fresh =
            SourceAvTimestampObserver::new_source_epoch(identity(SourceAvStreamKind::Video), 0)
                .unwrap();
        fresh.push(&packet(300, 0, false, &[])).unwrap();
        fresh.push(&flagged).unwrap();
        assert_eq!(
            fresh.push(&flagged),
            Err(SourceAvTimestampError::Discontinuity)
        );
        assert_fenced(&mut fresh, SourceAvTimestampError::Discontinuity);
    }
    let mut fresh =
        SourceAvTimestampObserver::new_source_epoch(identity(SourceAvStreamKind::Video), 0)
            .unwrap();
    fresh.push(&packet(PID, 0, false, &[])).unwrap();
    let mut flagged = packet(PID, 1, true, &bytes);
    flagged[5] |= 0x80;
    assert_eq!(
        fresh.push(&flagged),
        Err(SourceAvTimestampError::Discontinuity)
    );
}

#[test]
fn invalid_transport_and_start_without_payload_fence_the_source() {
    for (offset, value) in [(0, 0), (1, 0x81), (3, 0), (3, 0xa0), (4, 184)] {
        let mut raw = packet(PID, 0, false, &[]);
        raw[offset] = value;
        let mut observer =
            SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
        assert_eq!(observer.push(&raw), Err(SourceAvTimestampError::Framing));
        assert_fenced(&mut observer, SourceAvTimestampError::Framing);
    }
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    assert_eq!(
        observer.push(&packet(PID, 0, true, &[])),
        Err(SourceAvTimestampError::UnexpectedStart)
    );
}

#[test]
fn identity_absolute_positions_and_counters_are_checked_and_closed_is_sticky() {
    for stream in [
        SourceAvStreamIdentity {
            stream_index: -1,
            ..identity(SourceAvStreamKind::Audio)
        },
        SourceAvStreamIdentity {
            pid: 31,
            ..identity(SourceAvStreamKind::Video)
        },
        SourceAvStreamIdentity {
            pid: 8191,
            ..identity(SourceAvStreamKind::Video)
        },
    ] {
        assert_eq!(
            SourceAvTimestampObserver::new(stream, 0).unwrap_err(),
            SourceAvTimestampError::Identity
        );
    }
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), u64::MAX).unwrap();
    assert_eq!(
        observer.push(&packet(8191, 0, false, &[])),
        Err(SourceAvTimestampError::Arithmetic)
    );
    assert_fenced(&mut observer, SourceAvTimestampError::Arithmetic);
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    observer.report.headers_seen = u64::MAX;
    assert_eq!(
        observer.push(&packet(PID, 0, true, &header(Some(0), None, 0))),
        Err(SourceAvTimestampError::Arithmetic)
    );
    assert_fenced(&mut observer, SourceAvTimestampError::Arithmetic);
    let mut observer =
        SourceAvTimestampObserver::new(identity(SourceAvStreamKind::Video), 0).unwrap();
    assert_eq!(observer.finish().unwrap().first_pts, None);
    assert_eq!(
        observer.push(&packet(8191, 0, false, &[])),
        Err(SourceAvTimestampError::Closed)
    );
    assert_fenced(&mut observer, SourceAvTimestampError::Closed);
}

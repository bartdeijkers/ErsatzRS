use super::*;
use crate::ffprobe::teletext_origin::parse_source_origin;

fn collector(start: i64, end: i64, max_packets: u64, max_ticks: u64) -> CueCollector {
    let origin = parse_source_origin(
        br#"{"format":{"format_name":"mpegts","start_time":"0.000000"},"streams":[]}"#,
    )
    .unwrap();
    CueCollector::new(
        SourceTeletextCueRequest::new(start, end, max_packets, max_ticks).unwrap(),
        &origin,
        SourceNativeEpoch::from_first_demux_timestamps(Some(0), Some(0)),
    )
    .unwrap()
}

fn report(position: u64, ticks: u64) -> SourceTeletextClockReport {
    let first = SourceTeletextClockSample {
        source_packet: 0,
        raw_ticks: 0,
        elapsed_ticks: 0,
    };
    SourceTeletextClockReport {
        first_source_packet: 0,
        packets_seen: position + 1,
        first: Some(first),
        last: Some(SourceTeletextClockSample {
            source_packet: position,
            raw_ticks: ticks,
            elapsed_ticks: ticks,
        }),
        pcr_samples: position + 1,
        wraps: 0,
    }
}

#[test]
fn cue_request_requires_positive_effective_interval_and_explicit_unambiguous_limits() {
    for (start, end, packets, ticks) in [
        (-1, 1, 1, 1),
        (1, 1, 1, 1),
        (2, 1, 1, 1),
        (0, 1, 0, 1),
        (0, 1, 1, 0),
        (0, 1, 1, (1_u64 << 33) * 150),
    ] {
        assert_eq!(
            SourceTeletextCueRequest::new(start, end, packets, ticks),
            None
        );
    }
    let request = SourceTeletextCueRequest::new(40_000_000, 52_000_000, 134, 2_700_000).unwrap();
    assert_eq!(
        (request.start_microseconds(), request.end_microseconds()),
        (40_000_000, 52_000_000)
    );
}

#[test]
fn bookends_include_the_format_numeric_enclosure_without_pcr_interpolation() {
    let mut cue = collector(1_000, 2_000, 10, 100_000);
    assert_eq!(cue.target_ticks, (26_973, 54_027));
    for (position, ticks) in [(0, 0), (2, 26_970), (4, 27_000), (6, 54_000)] {
        cue.push(report(position, ticks));
    }
    assert!(!cue.invalid);
    assert_eq!(cue.before.unwrap().source_packet, 2);
    assert_eq!(
        cue.after, None,
        "rounded requested end cannot invent a bounding PCR"
    );
    cue.push(report(8, 54_030));
    assert_eq!(cue.after.unwrap().source_packet, 8);
    // Later observations must not move the first actual end bookend.
    cue.push(report(100, 999_999));
    assert_eq!(cue.after.unwrap().source_packet, 8);
    assert!(!cue.invalid);
}

#[test]
fn missing_start_or_end_pcr_never_becomes_coverage() {
    let mut no_start = collector(0, 2_000, 10, 100_000);
    no_start.push(report(0, 0));
    no_start.push(report(2, 60_000));
    assert_eq!(
        no_start.before, None,
        "numeric enclosure extends before the first real PCR"
    );
    assert!(no_start.after.is_some());
    let mut no_end = collector(1_000, 2_000, 10, 100_000);
    no_end.push(report(0, 0));
    no_end.push(report(2, 26_970));
    assert!(no_end.before.is_some());
    assert_eq!(no_end.after, None);
}

#[test]
fn packet_and_tick_gaps_fence_provisional_bookends_stickily() {
    for next in [report(11, 1_000), report(2, 100_001)] {
        let mut cue = collector(1_000, 2_000, 10, 100_000);
        cue.push(report(0, 0));
        cue.push(next);
        assert!(cue.invalid);
        cue.push(report(12, 100_002));
        assert_eq!(cue.after, None);
    }
    let mut missing = collector(1_000, 2_000, 10, 100_000);
    let mut no_pcr = report(0, 0);
    no_pcr.first = None;
    no_pcr.last = None;
    no_pcr.packets_seen = 11;
    missing.push(no_pcr);
    assert!(missing.invalid);
    let mut tail = collector(1_000, 2_000, 10, 100_000);
    tail.push(report(0, 0));
    let mut tail_report = report(0, 0);
    tail_report.packets_seen = 12;
    tail.push(tail_report);
    assert!(
        tail.invalid,
        "actual non-PCR packets consume the gap allowance"
    );
}

#[test]
fn unknown_writer_precision_origin_or_unrepresentable_ticks_gain_no_target() {
    let epoch = SourceNativeEpoch::from_first_demux_timestamps(Some(0), Some(0));
    let request = SourceTeletextCueRequest::new(1_000, 2_000, 10, 100_000).unwrap();
    for start in [None, Some("N/A"), Some("0.0"), Some("8796093.022209")] {
        let mut json = serde_json::json!({"format":{"format_name":"mpegts"},"streams":[]});
        if let Some(start) = start {
            json["format"]["start_time"] = start.into();
        }
        let origin = parse_source_origin(&serde_json::to_vec(&json).unwrap()).unwrap();
        assert!(CueCollector::new(request, &origin, epoch).is_none());
    }
    let origin =
        parse_source_origin(br#"{"format":{"start_time":"0.000000"},"streams":[]}"#).unwrap();
    let request = SourceTeletextCueRequest::new(i64::MAX - 1, i64::MAX, 10, 100_000).unwrap();
    assert!(CueCollector::new(request, &origin, epoch).is_none());
}

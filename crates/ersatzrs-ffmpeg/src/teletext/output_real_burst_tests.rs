//! Opt-in evidence harness: a real constant-rate `FFmpeg` capture through the
//! output gate.
//!
//! Set `ERSATZRS_TELETEXT_BURST_TS` to a single-program `-muxrate` capture
//! (PCR on PID `0x100`) and `ERSATZRS_TELETEXT_BURST_MUXRATE` to its muxrate.
//! `ERSATZRS_TELETEXT_BURST_OUT` optionally receives the delivered stream for
//! independent decoding (for example with the `libzvbi` teletext decoder).
//! The pump is emulated with one-interval windows and a bounded look-ahead of
//! already received packets, as the host pump reads ahead during delivery.

use std::time::Duration;

use ersatzrs_domain::*;
use mpeg2ts_reader::packet::Packet;

use super::*;
use crate::teletext::{encode_teletext_page_set, TeletextEncodingRegion};

fn full_page(number: u16, title: &str) -> TeletextPageDraft {
    TeletextPageDraft {
        address: TeletextPageAddress {
            page: TeletextPageNumber::new(number).unwrap(),
            subcode: TeletextSubcode::new(0).unwrap(),
        },
        kind: TeletextPageKind::FullPage,
        language: TeletextLanguage::new("nld".into()).unwrap(),
        national_option: TeletextNationalOption::default(),
        rows: (1..=23)
            .map(|row| TeletextRow {
                row,
                runs: vec![TeletextRun {
                    column: 1,
                    style: TeletextStyle::default(),
                    content: TeletextRunContent::Text {
                        text: format!("{title} row {row:02}"),
                    },
                }],
            })
            .collect(),
        navigation: Vec::new(),
    }
}

fn pid(raw: &[u8; 188]) -> u16 {
    u16::from(Packet::new(raw).pid())
}

#[test]
#[ignore = "needs a real FFmpeg capture; see module documentation"]
// Opt-in evidence input, like the packaged-FFmpeg capability tests.
#[allow(clippy::disallowed_methods)]
fn real_ffmpeg_cbr_bursts_carry_teletext_without_av_changes() {
    let path = std::env::var("ERSATZRS_TELETEXT_BURST_TS").expect("capture path");
    let muxrate: u64 = std::env::var("ERSATZRS_TELETEXT_BURST_MUXRATE")
        .expect("muxrate")
        .parse()
        .unwrap();
    let bytes = std::fs::read(path).unwrap();
    let raw: Vec<[u8; 188]> = bytes
        .chunks_exact(188)
        .map(|chunk| chunk.try_into().unwrap())
        .collect();
    let set = encode_teletext_page_set(
        &TeletextPageSetDraft {
            initial_page: TeletextPageNumber::new(200).unwrap(),
            language: TeletextLanguage::new("nld".into()).unwrap(),
            pages: vec![full_page(200, "Index"), full_page(201, "Guide")],
        },
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    let config = TeletextOutputConfig {
        program: 1,
        elementary_pid: 512,
        clock: TeletextClockConfig {
            pcr_pid: 256,
            muxrate_bps: muxrate,
            rate_tolerance_ppm: 1000,
            pcr_jitter_ticks: 14,
            max_pcr_gap_packets: 400,
            max_pcr_gap_ticks: 2_700_000,
            min_pcr_intervals: 2,
            max_window_packets: 1024,
        },
        max_buffer_packets: 8192,
        max_hold: Duration::from_secs(2),
        playout_delay: Duration::from_millis(100),
        max_lateness: Duration::from_millis(100),
    };
    let mut gate = TeletextOutputGate::new(config, set).unwrap();
    // About 100 ms of received-but-not-due output, as with the host pump.
    let lookahead = usize::try_from(muxrate / (10 * 188 * 8)).unwrap();
    let (mut pushed, mut emitted, mut intervals) = (0_usize, 0_usize, 1_usize);
    let mut now = Duration::ZERO;
    let mut output = Vec::with_capacity(raw.len());
    let (mut pes, mut windows, mut empty) = (0_usize, 0_usize, 0_usize);
    loop {
        let target = (emitted + lookahead * 2).min(raw.len());
        if pushed < target {
            gate.push(&raw[pushed..target].concat(), now).unwrap();
            pushed = target;
        }
        let Some(end) = gate.window_end(intervals).unwrap() else {
            assert!(pushed < raw.len(), "no clock anchor in the capture");
            continue;
        };
        let end = usize::try_from(end).unwrap();
        if end + lookahead > raw.len() {
            break;
        }
        let prepared = gate
            .prepare_window(end - emitted, now)
            .unwrap_or_else(|error| panic!("window at packet {emitted}: {error}"));
        if !prepared {
            intervals += 1;
            continue;
        }
        intervals = 1;
        let schedule = gate.pending_schedule().unwrap();
        let window = gate.pending_packets(schedule.first_due()).unwrap().to_vec();
        now = schedule.last_due();
        gate.acknowledge_window(schedule, now).unwrap();
        let carried = window
            .iter()
            .filter(|raw| pid(raw) == 512 && Packet::new(*raw).payload_unit_start_indicator())
            .count();
        pes += carried;
        windows += 1;
        empty += usize::from(carried == 0);
        emitted = end;
        output.extend_from_slice(&window);
    }
    // Audio, video and PCR packets are byte-identical; only nulls and the
    // rewritten PMT may differ.
    for (before, after) in raw.iter().zip(&output) {
        if ![0x1fff, 0x1000].contains(&pid(before)) {
            assert_eq!(before, after);
        }
    }
    assert!(pes > 0, "no teletext was carried");
    println!(
        "delivered {} packets in {windows} windows; {pes} teletext PES; {empty} windows without teletext",
        output.len()
    );
    if let Ok(out) = std::env::var("ERSATZRS_TELETEXT_BURST_OUT") {
        std::fs::write(out, output.concat()).unwrap();
    }
}

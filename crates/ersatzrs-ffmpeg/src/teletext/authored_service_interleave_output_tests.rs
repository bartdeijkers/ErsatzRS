//! Physical resource deadlines and acknowledged completion across internal refreshes.

use super::*;
use crate::teletext::carousel::authored_service_carousel_tests::many_displays;

const DISPLAYS: usize = 499;
const SOURCE_FRAMES: u64 = 1_100;

struct AcquisitionUnit {
    bytes: [u8; 46],
    header: bool,
    delivered: Vec<(u64, Duration)>,
}

struct InterleaveOutput {
    gate: TeletextOutputGate,
    raw: Vec<[u8; 188]>,
    units: Vec<AcquisitionUnit>,
    position: usize,
    now: Duration,
    continuity: u8,
    terminator: [u8; 46],
    terminators: usize,
}

fn packet_pid(packet: &[u8; 188]) -> u16 {
    (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2])
}

impl InterleaveOutput {
    fn new() -> Self {
        let service = many_displays(DISPLAYS, true);
        let mut units: Vec<_> = service
            .magazine_units()
            .iter()
            .map(|&bytes| AcquisitionUnit {
                bytes,
                header: false,
                delivered: Vec::new(),
            })
            .collect();
        assert_eq!(units.len(), 1);
        for page in &service.pages()[DISPLAYS..] {
            units.extend(page.data_units().iter().enumerate().map(|(index, &bytes)| {
                AcquisitionUnit {
                    bytes,
                    header: index == 0,
                    delivered: Vec::new(),
                }
            }));
        }
        assert!(units.iter().any(|unit| unit.header));
        let terminator = crate::teletext::packets::terminator(
            service
                .declared()
                .semantic()
                .base()
                .initial_page
                .wire_magazine(),
        );
        Self {
            gate: TeletextOutputGate::new_authored(config(), service).unwrap(),
            raw: source(SOURCE_FRAMES),
            units,
            position: 0,
            now: Duration::ZERO,
            continuity: 0,
            terminator,
            terminators: 0,
        }
    }

    fn prepare(&mut self, frame: u64) -> TeletextOutputSchedule {
        let stop = end(frame + 1);
        let mut window = self.raw[self.position..stop].to_vec();
        if frame >= 500 {
            // Consume early null slots with valid adaptation-only packets on the
            // existing PCR/AV PID. PCR samples and their positions remain intact.
            // The later acquisition therefore has a genuinely later physical
            // position inside its window, rather than merely a later batch PTS.
            let mut consumed = 0;
            for packet in &mut window {
                if packet_pid(packet) == 8_191 && consumed < 40 {
                    packet[..6].copy_from_slice(&[0x47, 1, 44, 0x20, 183, 0]);
                    consumed += 1;
                }
            }
            assert_eq!(consumed, 40);
        }
        self.raw[self.position..stop].copy_from_slice(&window);
        assert!(window.len() <= config().max_buffer_packets);
        self.gate.push(&bytes(&window), self.now).unwrap();
        assert!(self.gate.prepare_window(window.len(), self.now).unwrap());
        self.position = stop;
        self.gate.pending_schedule().unwrap()
    }

    fn deliver(&mut self, frame: u64, schedule: TeletextOutputSchedule) -> Vec<[u8; 188]> {
        let output = self
            .gate
            .pending_packets(schedule.first_due())
            .unwrap()
            .to_vec();
        let start = self.position - output.len();
        for (original, actual) in self.raw[start..self.position].iter().zip(&output) {
            if !matches!(packet_pid(original), 8_191 | 3_337) {
                assert_eq!(original, actual, "source/PCR packets changed");
            }
        }
        self.record_units(frame, &output, &schedule);
        let completes = self.gate.pending.as_ref().unwrap().cycle.completes_cycle;
        assert_eq!(self.terminators, usize::from(completes));
        assert!(!self.gate.completed_cycle);
        self.now = schedule.last_due();
        self.gate.acknowledge_window(schedule, self.now).unwrap();
        output
    }

    fn record_units(
        &mut self,
        frame: u64,
        output: &[[u8; 188]],
        schedule: &TeletextOutputSchedule,
    ) {
        let packets: Vec<_> = output
            .iter()
            .enumerate()
            .filter(|(_, packet)| packet_pid(packet) == config().elementary_pid)
            .collect();
        assert!(!packets.is_empty());
        assert!(packets.len() <= 8);
        let mut pes = Vec::new();
        for (ordinal, (_, packet)) in packets.iter().enumerate() {
            assert_eq!(packet[3] & 15, self.continuity);
            self.continuity = (self.continuity + 1) & 15;
            assert_eq!(packet[1] & 0x40 != 0, ordinal == 0);
            assert_eq!(packet[3] & 0x30, 0x10);
            pes.extend_from_slice(&packet[4..]);
        }
        assert_eq!(&pes[..4], &[0, 0, 1, 0xbd]);
        assert_eq!(
            usize::from(u16::from_be_bytes([pes[4], pes[5]])) + 6,
            pes.len()
        );
        assert_eq!(pes[45], 0x10);
        for (index, bytes) in pes[46..].chunks_exact(46).enumerate() {
            if bytes == self.terminator {
                self.terminators += 1;
            }
            let Some(unit) = self.units.iter_mut().find(|unit| {
                bytes
                    .iter()
                    .zip(unit.bytes)
                    .enumerate()
                    .all(|(column, (actual, expected))| {
                        (unit.header && matches!(column, 9 | 12)) || *actual == expected
                    })
            }) else {
                continue;
            };
            // Use delivery of the unit's final byte, including units straddling
            // TS payloads. The per-packet physical schedule is authoritative.
            let packet = (46 + index * 46 + 45) / 184;
            let due = schedule.packet_due(packets[packet].0).unwrap();
            unit.delivered.push((frame, due));
        }
    }

    fn refreshed(&self) -> bool {
        self.units.iter().all(|unit| unit.delivered.len() >= 2)
    }

    fn assert_repetition(&self) {
        assert!(self.refreshed());
        for unit in &self.units {
            for pair in unit.delivered.windows(2) {
                assert!(pair[1].1.checked_sub(pair[0].1).unwrap() <= Duration::from_secs(20));
                assert!(pair[1].0 - pair[0].0 <= 999);
                let first_offset = pair[0]
                    .1
                    .checked_sub(config().playout_delay + Duration::from_millis(pair[0].0 * 20))
                    .unwrap();
                let repeated_offset = pair[1]
                    .1
                    .checked_sub(config().playout_delay + Duration::from_millis(pair[1].0 * 20))
                    .unwrap();
                assert!(repeated_offset > first_offset + Duration::from_millis(10));
            }
        }
    }
}

#[test]
fn long_cycle_refreshes_resources_at_physical_deadlines_before_acknowledged_completion() {
    let mut proof = InterleaveOutput::new();
    let mut refreshed = false;
    for frame in 0..SOURCE_FRAMES {
        let schedule = proof.prepare(frame);
        let completes = proof.gate.pending.as_ref().unwrap().cycle.completes_cycle;
        assert!(!proof.gate.completed_cycle);
        if completes {
            assert!(refreshed);
            assert!(proof.gate.carousel.as_ref().unwrap().is_cycle_boundary());
            // Preparing and exposing the FF-containing window do not certify
            // its delivery. Only acknowledgement establishes completion.
            proof.gate.pending_packets(schedule.first_due()).unwrap();
            assert!(!proof.gate.completed_cycle);
        }
        drop(proof.deliver(frame, schedule));
        if proof.refreshed() {
            refreshed = true;
            proof.assert_repetition();
        }
        if completes {
            assert!(proof.gate.completed_cycle);
            proof.gate.finish(proof.now).unwrap();
            return;
        }
        assert!(!proof.gate.completed_cycle);
        assert!(!proof.gate.carousel.as_ref().unwrap().is_cycle_boundary());
    }
    panic!("complete display cycle was not delivered");
}

#[test]
fn eof_after_acknowledged_internal_refresh_still_rejects_incomplete_display_cycle() {
    let mut proof = InterleaveOutput::new();
    for frame in 0..SOURCE_FRAMES {
        let schedule = proof.prepare(frame);
        assert!(!proof.gate.pending.as_ref().unwrap().cycle.completes_cycle);
        drop(proof.deliver(frame, schedule));
        assert!(!proof.gate.completed_cycle);
        if proof.refreshed() {
            proof.assert_repetition();
            assert!(!proof.gate.carousel.as_ref().unwrap().is_cycle_boundary());
            assert_eq!(
                proof.gate.prepare_final_window(proof.now),
                Err(TeletextOutputError::Unterminated)
            );
            assert!(proof.gate.pending_schedule().is_err());
            return;
        }
    }
    panic!("internal acquisition refresh was not delivered");
}

fn complete_stream() -> Vec<u8> {
    let mut proof = InterleaveOutput::new();
    let mut stream = Vec::new();
    for frame in 0..SOURCE_FRAMES {
        let schedule = proof.prepare(frame);
        let completes = proof.gate.pending.as_ref().unwrap().cycle.completes_cycle;
        assert!(!proof.gate.completed_cycle);
        stream.extend(proof.deliver(frame, schedule).into_iter().flatten());
        assert!(stream.len() <= 16 * 1_024 * 1_024);
        if completes {
            proof.assert_repetition();
            assert!(proof.gate.completed_cycle);
            assert_eq!(proof.terminators, 1);
            proof.gate.finish(proof.now).unwrap();
            return stream;
        }
        assert!(!proof.gate.completed_cycle);
    }
    panic!("complete transport cycle was not delivered");
}

async fn analyze(executable: &str, stream: &[u8]) -> serde_json::Value {
    let output = crate::teletext_probe::run_probe(executable, &["--json"], stream, 128 * 1_024)
        .await
        .unwrap();
    serde_json::from_slice(&output).unwrap()
}

fn teletext_pid(report: &serde_json::Value) -> &serde_json::Value {
    report["pids"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pid| pid["id"] == 512)
        .unwrap()
}

#[tokio::test]
#[ignore = "Requires explicit TSDuck tsanalyze executable; synthetic PSI/PCR, no AV/client acceptance"]
async fn interleaved_paced_service_passes_tsduck_and_detects_counter_damage() {
    let executable = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_TSANALYZE"])
        .expect("set the TSDuck tsanalyze executable");
    let stream = complete_stream();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_INTERLEAVED_TS"])
    {
        std::fs::write(path, &stream).unwrap();
    }
    let report = analyze(&executable, &stream).await;
    assert_eq!(report["ts"]["packets"]["invalid-syncs"], 0);
    assert_eq!(report["ts"]["packets"]["transport-errors"], 0);
    let pid = teletext_pid(&report);
    assert_eq!(pid["pes-stream-id"], 0xbd);
    assert_eq!(pid["invalid-pes-prefix"], 0);
    assert_eq!(pid["packets"]["discontinuities"], 0);
    assert_eq!(pid["packets"]["pts-leap"], 0);
    assert_eq!(pid["packets"]["pcr-leap"], 0);
    assert!(pid["pes"].as_u64().unwrap() > 0);
    let mut damaged = stream;
    let packet = damaged
        .chunks_exact_mut(188)
        .filter(|packet| packet[1] & 0x1f == 2 && packet[2] == 0 && packet[3] & 0x30 == 0x10)
        .nth(10)
        .unwrap();
    packet[3] ^= 3;
    let report = analyze(&executable, &damaged).await;
    assert!(
        teletext_pid(&report)["packets"]["discontinuities"]
            .as_u64()
            .unwrap()
            > 0
    );
}

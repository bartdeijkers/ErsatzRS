//! One real AV source and gate across signalling replacement/withdrawal/re-enable.
//! Sink times are simulated; this is not asynchronous delivery or client acceptance.

use std::{ops::Range, sync::Arc};

use super::*;
use crate::teletext::{output::transition_tests::simple_with_text, psi_framing::SectionAssembler};
use mpeg2ts_reader::mpegts_crc;

struct Forwarding<'a> {
    gate: TeletextOutputGate,
    source: &'a [[u8; 188]],
    read: usize,
    output: Vec<[u8; 188]>,
    now: Duration,
}

impl<'a> Forwarding<'a> {
    fn new(source: &'a [[u8; 188]], service: Arc<EncodedTeletextAuthoredService>) -> Self {
        Self {
            gate: TeletextOutputGate::new_authored_shared(
                TeletextOutputConfig {
                    program: 10,
                    elementary_pid: 512,
                    clock: fixture_clock_policy(256),
                    max_buffer_packets: 1024,
                    max_hold: Duration::from_millis(500),
                    playout_delay: Duration::from_millis(100),
                    max_lateness: Duration::from_millis(20),
                },
                service,
            )
            .unwrap(),
            source,
            read: 0,
            output: Vec::with_capacity(source.len()),
            now: Duration::ZERO,
        }
    }

    fn read_until(&mut self, end: usize) {
        while self.read < end {
            let stop = (self.read + 256).min(self.source.len());
            let bytes: Vec<u8> = self.source[self.read..stop]
                .iter()
                .flatten()
                .copied()
                .collect();
            self.gate.push(&bytes, self.now).unwrap();
            self.read = stop;
        }
    }

    fn acknowledge(&mut self) -> TeletextOutputAcknowledgement {
        let schedule = self.gate.pending_schedule().unwrap();
        self.output
            .extend_from_slice(self.gate.pending_packets(schedule.first_due()).unwrap());
        self.now = schedule.last_due();
        let receipt = self
            .gate
            .acknowledge_window_with_receipt(schedule, self.now)
            .unwrap();
        assert_eq!(
            receipt.end_packet(),
            u64::try_from(self.output.len()).unwrap()
        );
        receipt
    }

    fn next(&mut self) -> Option<TeletextOutputAcknowledgement> {
        let mut previous_end = 0;
        let mut ordinary_fallback = false;
        for intervals in 1..=50 {
            let end = if ordinary_fallback {
                self.gate.window_end(intervals)
            } else {
                self.gate.transition_window_end(intervals)
            };
            let Some(end) = end.unwrap() else {
                self.read_until(256.min(self.source.len()));
                continue;
            };
            let end = usize::try_from(end).unwrap();
            if end > self.source.len() {
                return None;
            }
            assert!(
                end > previous_end,
                "unchanged rejected prefix cannot make progress"
            );
            self.read_until(end.max(256).min(self.source.len()));
            if self
                .gate
                .prepare_window(end - self.output.len(), self.now)
                .unwrap()
            {
                return Some(self.acknowledge());
            }
            previous_end = end;
            // A cycle cap cannot truncate a raw source PMT. Extend through an
            // ordinary bounded interval after false readiness, preserving all
            // bytes; a window ending in another cycle is not a transition receipt.
            ordinary_fallback = true;
        }
        panic!("real source did not admit a bounded output window");
    }

    fn cycle(&mut self) -> TeletextOutputAcknowledgement {
        for _ in 0..200 {
            let receipt = self
                .next()
                .expect("source ended before a complete phase cycle");
            if receipt.is_cycle_boundary() && receipt.contains_complete_cycle() {
                return receipt;
            }
        }
        panic!("authored phase exceeded bounded cycle extent");
    }

    fn announcement(&mut self) -> TeletextOutputAcknowledgement {
        let start = self.output.len();
        assert!(self.gate.signalling_transition_pending());
        let receipt = self
            .next()
            .expect("source ended before signalling announcement");
        assert!(receipt.signalling_announced());
        assert!(!self.gate.signalling_transition_pending());
        assert_no_authored_pid(&self.output[start..]);
        receipt
    }

    fn finish(mut self) -> Vec<[u8; 188]> {
        while self.next().is_some() {}
        self.read_until(self.source.len());
        if self.gate.prepare_final_window(self.now).unwrap() {
            self.acknowledge();
        }
        self.gate.finish(self.now).unwrap();
        assert_eq!(self.output.len(), self.source.len());
        self.output
    }
}

fn assert_no_authored_pid(packets: &[[u8; 188]]) {
    assert!(packets
        .iter()
        .all(|raw| u16::from(Packet::new(raw).pid()) != 512));
}

fn sections(packets: &[[u8; 188]]) -> Vec<Vec<u8>> {
    let mut assembler = SectionAssembler::default();
    let mut sections = Vec::new();
    for raw in packets {
        let packet = Packet::new(raw);
        if u16::from(packet.pid()) == 4096 {
            sections.extend(assembler.push(&packet).unwrap());
        }
    }
    assert!(!assembler.is_pending());
    sections
}

fn expected_pmt(source: &[u8], version: u8, language: Option<&[u8; 3]>) -> Vec<u8> {
    let mut section = source[..source.len() - 4].to_vec();
    if let Some(language) = language {
        // One private stream on PID512, initial-page descriptor for page200.
        section.extend_from_slice(&[6, 0xe2, 0, 0xf0, 7, 0x56, 5]);
        section.extend_from_slice(language);
        section.extend_from_slice(&[0x0a, 0]);
    }
    let size = u16::try_from(section.len() + 1).unwrap().to_be_bytes();
    section[1] = 0xb0 | size[0];
    section[2] = size[1];
    section[5] = 0xc1 | (version << 1);
    section.extend_from_slice(&mpegts_crc::sum32(&section).to_be_bytes());
    section
}

fn verify_pmt_transitions(input: &[[u8; 188]], output: &[[u8; 188]]) {
    let source = sections(input).into_iter().next().unwrap();
    assert_eq!((source[5] >> 1) & 31, 0);
    let expected = [
        expected_pmt(&source, 1, Some(b"nld")),
        expected_pmt(&source, 2, Some(b"fra")),
        expected_pmt(&source, 3, None),
        expected_pmt(&source, 4, Some(b"nld")),
    ];
    let mut seen = Vec::new();
    for section in sections(output) {
        assert_eq!(mpegts_crc::sum32(&section), 0);
        let version = usize::from((section[5] >> 1) & 31);
        assert!((1..=4).contains(&version));
        assert_eq!(section, expected[version - 1]);
        if seen.last() != Some(&version) {
            seen.push(version);
        }
    }
    assert_eq!(seen, [1, 2, 3, 4]);
    let mut previous = None;
    for raw in output {
        let packet = Packet::new(raw);
        if u16::from(packet.pid()) == 4096 && packet.payload().is_some() {
            let counter = packet.continuity_counter().count();
            if let Some(previous) = previous {
                assert_eq!(counter, (previous + 1) & 15);
            }
            previous = Some(counter);
        }
    }
}

fn transition_output(input: &[[u8; 188]]) -> (Vec<[u8; 188]>, [Range<usize>; 3]) {
    let initial = simple_with_text("nld", None, "ALPHA");
    let changed = simple_with_text("fra", None, "BRAVO");
    let mut output = Forwarding::new(input, Arc::clone(&initial));
    output.cycle();
    let receipt = output.cycle();
    let first_end = output.output.len();
    output
        .gate
        .replace_authored_at_acknowledged_boundary(&receipt, &changed, output.now)
        .unwrap();
    output.announcement();
    output.cycle();
    let receipt = output.cycle();
    let changed_end = output.output.len();
    output
        .gate
        .withdraw_at_acknowledged_boundary(&receipt, output.now)
        .unwrap();
    let mut receipt = output.announcement();
    for _ in 0..5 {
        receipt = output.next().unwrap();
        assert!(receipt.is_cycle_boundary());
    }
    let restart = output.output.len();
    assert_no_authored_pid(&output.output[changed_end..restart]);
    let withdrawn = sections(&output.output[changed_end..restart]);
    assert!(!withdrawn.is_empty());
    let expected = expected_pmt(&sections(input)[0], 3, None);
    assert!(withdrawn.iter().all(|section| section == &expected));
    output
        .gate
        .replace_authored_at_acknowledged_boundary(&receipt, &initial, output.now)
        .unwrap();
    output.announcement();
    output.cycle();
    output.cycle();
    let restart_end = output.output.len();
    (
        output.finish(),
        [0..first_end, first_end..changed_end, restart..restart_end],
    )
}

async fn verify_transport(executable: &str, output: &[u8]) {
    let analysis = crate::teletext_probe::run_probe(executable, &["--json"], output, 128 * 1024)
        .await
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&analysis).unwrap();
    assert_eq!(
        report["ts"]["packets"]["total"].as_u64().unwrap(),
        u64::try_from(output.len() / 188).unwrap()
    );
    assert!(
        report["ts"]["pcr-bitrate"]
            .as_u64()
            .unwrap()
            .abs_diff(MUXRATE)
            < 2000
    );
    for field in ["invalid-syncs", "transport-errors"] {
        assert_eq!(report["ts"]["packets"][field], 0);
    }
    for pid in report["pids"].as_array().unwrap() {
        for field in ["discontinuities", "pts-leap", "pcr-leap"] {
            assert_eq!(pid["packets"][field], 0, "PID {} {field}", pid["id"]);
        }
    }
    let authored = report["pids"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pid| pid["id"] == 512)
        .unwrap();
    assert_eq!(authored["pes-stream-id"], 0xbd);
    assert_eq!(authored["invalid-pes-prefix"], 0);
    for expected in [256, 257, 258, 4096, 4097, 512] {
        assert!(report["pids"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pid| pid["id"] == expected));
    }
    // Historical packets during withdrawal need not be referenced by that
    // phase's latest PMT; aggregate PID inventory is not a withdrawal oracle.
}

async fn verify_counter_negative(executable: &str, output: &[u8]) {
    let mut damaged = output.to_vec();
    let packet = damaged
        .chunks_exact_mut(188)
        .filter(|packet| {
            (u16::from(packet[1] & 0x1f) << 8) | u16::from(packet[2]) == 512
                && packet[3] & 0x10 != 0
        })
        .nth(10)
        .expect("actual transition capture must contain enough authored PES packets");
    packet[3] ^= 3;
    let analysis = crate::teletext_probe::run_probe(executable, &["--json"], &damaged, 128 * 1024)
        .await
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&analysis).unwrap();
    let pid = report["pids"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pid| pid["id"] == 512)
        .unwrap();
    assert!(pid["packets"]["discontinuities"].as_u64().unwrap() > 0);
}

async fn verify_text(executable: &str, output: &[[u8; 188]], phases: [Range<usize>; 3]) {
    for (phase, expected) in phases.into_iter().zip(["ALPHA", "BRAVO", "ALPHA"]) {
        let bytes: Vec<u8> = output[phase].iter().flatten().copied().collect();
        let decoded = crate::teletext_probe::run_probe(
            executable,
            crate::teletext_probe::TEXT_PROBE_ARGUMENTS,
            &bytes,
            64 * 1024,
        )
        .await
        .unwrap();
        let text = String::from_utf8(decoded).unwrap();
        assert!(
            text.contains(expected),
            "independent decoder missed {expected}: {text}"
        );
    }
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9/libzvbi and TSDuck tsanalyze executables"]
async fn real_runtime_gate_replaces_withdraws_and_reenables_without_changing_av() {
    let ffmpeg = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_FFMPEG"])
        .expect("set the exact FFmpeg executable");
    let tsanalyze = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_TSANALYZE"])
        .expect("set the TSDuck tsanalyze executable");
    let source = crate::teletext_probe::run_probe(&ffmpeg, SOURCE_ARGS, &[], 4 * 1024 * 1024)
        .await
        .unwrap();
    let input: Vec<[u8; 188]> = source
        .chunks_exact(188)
        .map(|chunk| chunk.try_into().unwrap())
        .collect();
    assert_eq!(input.len() * 188, source.len());
    let (output, phases) = transition_output(&input);
    assert_eq!(
        verify_fixture_clock(&output, 256).unwrap(),
        verify_fixture_clock(&input, 256).unwrap()
    );
    for (before, after) in input.iter().zip(&output) {
        if ![8191, 4096].contains(&u16::from(Packet::new(before).pid())) {
            assert_eq!(before, after);
        }
    }
    verify_pmt_transitions(&input, &output);
    verify_text(&ffmpeg, &output, phases).await;
    let bytes: Vec<u8> = output.iter().flatten().copied().collect();
    let (before, after) = tokio::join!(
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &source, 256 * 1024),
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &bytes, 256 * 1024)
    );
    assert_eq!(
        before.unwrap(),
        after.unwrap(),
        "decoded AV frames/timestamps differ"
    );
    verify_transport(&tsanalyze, &bytes).await;
    verify_counter_negative(&tsanalyze, &bytes).await;
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_OUTPUT_TRANSITION_TS"])
    {
        std::fs::write(path, bytes).unwrap();
    }
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9 and TSDuck; simulated sink, not HTTP or warm-client acceptance"]
async fn real_runtime_populated_mot_retires_before_successor_without_changing_av() {
    use crate::teletext::authored_service_tests as authored;
    use crate::teletext::source_header::{observe_dvb_teletext_header, TeletextHeaderObservation};

    let ffmpeg = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_FFMPEG"])
        .expect("set the exact FFmpeg executable");
    let tsanalyze = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_TSANALYZE"])
        .expect("set the TSDuck tsanalyze executable");
    let source = crate::teletext_probe::run_probe(&ffmpeg, SOURCE_ARGS, &[], 4 * 1024 * 1024)
        .await
        .unwrap();
    let input: Vec<[u8; 188]> = source
        .chunks_exact(188)
        .map(|chunk| chunk.try_into().unwrap())
        .collect();
    assert_eq!(input.len() * 188, source.len());
    let mut declaration = authored::input();
    declaration.magazines.clear();
    let declaration = authored::declared(declaration);
    let transmissions = authored::transmissions(&declaration);
    let old = Arc::new(
        encode_teletext_authored_service(declaration, &transmissions, &authored::limits()).unwrap(),
    );
    let next = simple_with_text("fra", None, "BRAVO");
    let mut output = Forwarding::new(&input, old);
    let receipt = output.cycle();
    output
        .gate
        .replace_authored_at_acknowledged_boundary(&receipt, &next, output.now)
        .unwrap();
    output.announcement();
    let start = output.output.len();
    for _ in 0..3 {
        let receipt = output.next().unwrap();
        assert!(
            !receipt.contains_complete_cycle(),
            "retirement does not deliver the successor"
        );
        assert!(!receipt.is_cycle_boundary());
    }
    let retirement = warm_phase_units(&output.output[start..]);
    assert_eq!(retirement.len(), 20);
    assert!(
        matches!(observe_dvb_teletext_header(&retirement[0]).unwrap(),
        TeletextHeaderObservation::Header(header) if header.wire_page == 0xfe && header.subcode & 15 == 9)
    );
    assert!(matches!(
        observe_dvb_teletext_header(&retirement[19]).unwrap(),
        TeletextHeaderObservation::Terminator(_)
    ));
    let end = output.output.len();
    output.cycle();
    let output = output.finish();
    assert_no_mot_in_tail(&output[end..]);
    assert_eq!(
        verify_fixture_clock(&output, 256).unwrap(),
        verify_fixture_clock(&input, 256).unwrap()
    );
    for (before, after) in input.iter().zip(&output) {
        if ![8191, 4096].contains(&u16::from(Packet::new(before).pid())) {
            assert_eq!(before, after);
        }
    }
    let bytes: Vec<u8> = output.iter().flatten().copied().collect();
    let (before, after) = tokio::join!(
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &source, 256 * 1024),
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &bytes, 256 * 1024)
    );
    assert_eq!(
        before.unwrap(),
        after.unwrap(),
        "decoded AV frames/timestamps differ"
    );
    verify_transport(&tsanalyze, &bytes).await;
    verify_counter_negative(&tsanalyze, &bytes).await;
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_OUTPUT_TRANSITION_TS"])
    {
        std::fs::write(path, bytes).unwrap();
    }
}

fn assert_no_mot_in_tail(packets: &[[u8; 188]]) {
    use crate::teletext::source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent};
    use crate::teletext::source_header::{observe_dvb_teletext_header, TeletextHeaderObservation};

    // Stream the entire tail through the existing per-PES bound. The short
    // phase collector's unit cap must not be raised for an ongoing response.
    let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
    for packet in packets {
        if let TeletextAssemblyEvent::Complete(pes) = assembler.push(packet).unwrap() {
            for unit in pes.units().filter_map(|unit| unit.teletext_unit()) {
                assert!(!matches!(observe_dvb_teletext_header(unit).unwrap(),
                    TeletextHeaderObservation::Header(header) if header.wire_page == 0xfe));
            }
        }
    }
    assembler.finish().unwrap();
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9/libzvbi and TSDuck; initially-Off real AV epoch, not live-client acceptance"]
async fn real_runtime_initial_absence_activates_without_rewriting_source_or_resetting_clock() {
    let ffmpeg = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_FFMPEG"])
        .expect("set the exact FFmpeg executable");
    let tsanalyze = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_TSANALYZE"])
        .expect("set the TSDuck tsanalyze executable");
    let source = crate::teletext_probe::run_probe(&ffmpeg, SOURCE_ARGS, &[], 4 * 1024 * 1024)
        .await
        .unwrap();
    let input: Vec<[u8; 188]> = source
        .chunks_exact(188)
        .map(|chunk| chunk.try_into().unwrap())
        .collect();
    assert_eq!(input.len() * 188, source.len());
    let service = simple_with_text("nld", None, "ALPHA");
    let mut output = Forwarding {
        gate: TeletextOutputGate::new_absent(TeletextOutputConfig {
            program: 10,
            elementary_pid: 512,
            clock: fixture_clock_policy(256),
            max_buffer_packets: 1024,
            max_hold: Duration::from_millis(500),
            playout_delay: Duration::from_millis(100),
            max_lateness: Duration::from_millis(20),
        })
        .unwrap(),
        source: &input,
        read: 0,
        output: Vec::with_capacity(input.len()),
        now: Duration::ZERO,
    };
    let mut receipt = output.next().unwrap();
    for _ in 0..9 {
        receipt = output.next().unwrap();
    }
    assert_eq!(output.output, input[..output.output.len()]);
    assert_no_authored_pid(&output.output);
    let inactive_end = output.output.len();
    output
        .gate
        .replace_authored_at_acknowledged_boundary(&receipt, &service, output.now)
        .unwrap();
    output.announcement();
    output.cycle();
    output.cycle();
    let active_end = output.output.len();
    let output = output.finish();
    assert_eq!(
        verify_fixture_clock(&output, 256).unwrap(),
        verify_fixture_clock(&input, 256).unwrap()
    );
    for (before, after) in input.iter().zip(&output) {
        if ![8191, 4096].contains(&u16::from(Packet::new(before).pid())) {
            assert_eq!(before, after);
        }
    }
    verify_initial_absence_pmt_transition(&input, &output);
    let active: Vec<u8> = output[inactive_end..active_end]
        .iter()
        .flatten()
        .copied()
        .collect();
    let text = crate::teletext_probe::run_probe(
        &ffmpeg,
        crate::teletext_probe::TEXT_PROBE_ARGUMENTS,
        &active,
        64 * 1024,
    )
    .await
    .unwrap();
    assert!(String::from_utf8(text).unwrap().contains("ALPHA"));
    let bytes: Vec<u8> = output.iter().flatten().copied().collect();
    let (before, after) = tokio::join!(
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &source, 256 * 1024),
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &bytes, 256 * 1024),
    );
    assert_eq!(
        before.unwrap(),
        after.unwrap(),
        "initial-Off activation changed decoded AV/timestamps"
    );
    verify_transport(&tsanalyze, &bytes).await;
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_OUTPUT_TRANSITION_TS"])
    {
        std::fs::write(path, bytes).unwrap();
    }
}

fn verify_initial_absence_pmt_transition(input: &[[u8; 188]], output: &[[u8; 188]]) {
    let original = sections(input)[0].clone();
    let expected = expected_pmt(&original, 1, Some(b"nld"));
    let mut versions = Vec::new();
    for section in sections(output) {
        assert_eq!(mpegts_crc::sum32(&section), 0);
        let version = (section[5] >> 1) & 31;
        assert_eq!(
            section,
            if version == 0 {
                original.clone()
            } else {
                expected.clone()
            }
        );
        if versions.last() != Some(&version) {
            versions.push(version);
        }
    }
    assert_eq!(versions, [0, 1]);
}

fn warm_runtime_services() -> (
    Arc<EncodedTeletextAuthoredService>,
    Arc<EncodedTeletextAuthoredService>,
    serde_json::Value,
) {
    use crate::teletext::output::transition_tests::{
        warm_magazine_defaults, warm_magazine_service,
    };
    use ersatzrs_domain::{
        TeletextEffectivePresentation, TeletextPageCharsetDesignation, TeletextPresentationSource,
        TeletextRgb,
    };
    let cyrillic = warm_magazine_defaults();
    let mut greek = cyrillic;
    greek.primary = TeletextPageCharsetDesignation::new(0x37).unwrap();
    greek.secondary = Some(TeletextPageCharsetDesignation::new(0x24).unwrap());
    greek.palette.clut2[0] = TeletextRgb::new([2, 4, 6]).unwrap();
    greek.palette.clut2[7] = TeletextRgb::new([3, 10, 14]).unwrap();
    let old = warm_magazine_service(
        &[(200, 0, None), (200, 16, None), (201, 0, None)],
        Some(cyrillic),
    );
    let next = warm_magazine_service(
        &[
            (200, 0, Some(greek)),
            (200, 16, Some(greek)),
            (201, 0, Some(greek)),
        ],
        None,
    );
    assert_eq!(
        old.declared().semantic().base(),
        next.declared().semantic().base()
    );
    assert!(!old.magazine_units().is_empty());
    assert!(next.magazine_units().is_empty());
    for (service, source, defaults) in [
        (&old, TeletextPresentationSource::Magazine, cyrillic),
        (&next, TeletextPresentationSource::Page, greek),
    ] {
        for page in &service.declared().semantic().base().pages {
            assert_eq!(
                service
                    .declared()
                    .semantic()
                    .effective_presentation(page.address),
                Some(TeletextEffectivePresentation::Extension {
                    source,
                    defaults: &defaults,
                })
            );
        }
    }
    let context = serde_json::json!({
        "unchanged_base":old.declared().semantic().base(),
        "old":{"presentation_source":"magazine","magazine":2,"defaults":cyrillic},
        "replacement":{"presentation_source":"page","defaults":greek},
        "addresses":[{"page":200,"subcode":0},{"page":200,"subcode":16},{"page":201,"subcode":0}],
        "row":1,"columns":[1,2],"level1_text":"AB",
        "old_expected_g0":[0x0410,0x0411],"replacement_expected_g0":[0x0391,0x0392]
    });
    (old, next, context)
}

fn warm_phase_units(packets: &[[u8; 188]]) -> Vec<[u8; 46]> {
    use crate::teletext::source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent};
    let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
    let mut units = Vec::new();
    for packet in packets {
        match assembler.push(packet).unwrap() {
            TeletextAssemblyEvent::Complete(pes) => {
                units.extend(pes.units().filter_map(|unit| unit.teletext_unit().copied()));
                assert!(units.len() <= 512, "bounded phase exceeded its unit budget");
            }
            TeletextAssemblyEvent::UnobservedPrefix => {
                panic!("acknowledged phase begins inside a selected PES");
            }
            _ => {}
        }
    }
    // A complete acknowledged cycle must end after its real closing PES. Do
    // not repair, filter or synthesize an acquisition boundary for the decoder.
    assembler.finish().unwrap();
    units
}

fn warm_runtime_phase(
    output: &mut Forwarding<'_>,
    expected: &mut TeletextCarousel,
    name: &str,
) -> (TeletextOutputAcknowledgement, serde_json::Value) {
    let start = output.output.len();
    output.cycle();
    let receipt = output.cycle();
    assert!(receipt.is_cycle_boundary() && receipt.contains_complete_cycle());
    let end = output.output.len();
    let actual = warm_phase_units(&output.output[start..end]);
    let mut model = Vec::new();
    for _ in 0..2 {
        let mut completed = false;
        for _ in 0..128 {
            let batch = expected.next_batch().unwrap();
            model.extend_from_slice(batch.data_units());
            if batch.kind() == TeletextCarouselBatchKind::Terminator {
                completed = true;
                break;
            }
        }
        assert!(
            completed,
            "bounded reference model failed to complete a cycle"
        );
    }
    assert_eq!(actual, model, "entire ordered {name} phase model differs");
    let proof = serde_json::json!({
        "name":name,"start_packet":start,"end_packet":end,
        "acknowledged_end_packet":receipt.end_packet(),"complete_cycles":2,
        "sink_elapsed_ns":u64::try_from(output.now.as_nanos()).unwrap(),
        "complete_pes_boundary":true,"complete_ordered_model_equal":true,
        "units_sha256":warm_runtime_sha256(&actual.iter().flatten().copied().collect::<Vec<_>>()),
        "unit_count":actual.len(),"units":actual.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()
    });
    (receipt, proof)
}

fn warm_runtime_output(
    input: &[[u8; 188]],
    old: &Arc<EncodedTeletextAuthoredService>,
    next: &Arc<EncodedTeletextAuthoredService>,
) -> (Vec<[u8; 188]>, Vec<serde_json::Value>) {
    let old_model = old.declared().clone();
    let next_model = next.declared().clone();
    let mut forwarding = Forwarding::new(input, Arc::clone(old));
    let mut expected = TeletextCarousel::new_authored_shared(Arc::clone(old)).unwrap();
    let (receipt, before) = warm_runtime_phase(&mut forwarding, &mut expected, "old");
    let boundary = forwarding.output.len();
    let elapsed = forwarding.now;
    forwarding
        .gate
        .replace_authored_at_acknowledged_boundary(&receipt, next, elapsed)
        .unwrap();
    assert_eq!(forwarding.output.len(), boundary);
    assert_eq!(forwarding.now, elapsed);
    assert!(!forwarding.gate.signalling_transition_pending());
    // Replacement starts its own first/repeat header sequence, while the real
    // gate retains the source epoch, packet positions, PMT and presentation clock.
    expected = TeletextCarousel::new_authored_shared(Arc::clone(next)).unwrap();
    let (_, replacement) = warm_runtime_phase(&mut forwarding, &mut expected, "replacement");
    let (_, repeated) = warm_runtime_phase(&mut forwarding, &mut expected, "repeat");
    let output = forwarding.finish();
    assert_eq!(old.declared(), &old_model);
    assert_eq!(next.declared(), &next_model);
    (output, vec![before, replacement, repeated])
}

fn warm_runtime_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(digest, "{byte:02x}").unwrap();
    }
    digest
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9 and TSDuck; real AV gate only, persistent decoder and live clients remain separate"]
async fn real_runtime_warm_m29_replacement_uses_explicit_greek_page_context_without_changing_av() {
    let directory =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_M29_RUNTIME_DIR"])
            .expect("set a new runtime proof directory");
    let directory = std::path::Path::new(&directory);
    std::fs::create_dir(directory).unwrap();
    let ffmpeg = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_FFMPEG"])
        .expect("set the exact FFmpeg executable");
    let tsanalyze = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_TSANALYZE"])
        .expect("set the TSDuck tsanalyze executable");
    let source = crate::teletext_probe::run_probe(&ffmpeg, SOURCE_ARGS, &[], 4 * 1024 * 1024)
        .await
        .unwrap();
    let input: Vec<[u8; 188]> = source
        .chunks_exact(188)
        .map(|packet| packet.try_into().unwrap())
        .collect();
    assert_eq!(input.len() * 188, source.len());
    let (old, next, declared_context) = warm_runtime_services();
    let (output, phases) = warm_runtime_output(&input, &old, &next);
    assert_eq!(
        verify_fixture_clock(&output, 256).unwrap(),
        verify_fixture_clock(&input, 256).unwrap()
    );
    for (before, after) in input.iter().zip(&output) {
        if ![8191, 4096].contains(&u16::from(Packet::new(before).pid())) {
            assert_eq!(before, after, "original source packet moved or changed");
        }
    }
    let expected = expected_pmt(&sections(&input)[0], 1, Some(b"nld"));
    for section in sections(&output) {
        assert_eq!(mpegts_crc::sum32(&section), 0);
        assert_eq!(
            section, expected,
            "same-descriptor replacement changed the PMT"
        );
    }
    let bytes: Vec<u8> = output.iter().flatten().copied().collect();
    let (before, after) = tokio::join!(
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &source, 256 * 1024),
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &bytes, 256 * 1024)
    );
    assert_eq!(
        before.unwrap(),
        after.unwrap(),
        "decoded AV frames/timestamps differ"
    );
    verify_transport(&tsanalyze, &bytes).await;
    verify_counter_negative(&tsanalyze, &bytes).await;
    let proof = serde_json::to_vec(&serde_json::json!({
        "probe_version":1,"scope":"one real AV source and gate; simulated sink times",
        "source_sha256":warm_runtime_sha256(&source),"output_sha256":warm_runtime_sha256(&bytes),
        "packet_count":output.len(),"program":10,"pcr_pid":256,"teletext_pid":512,
        "declared_context":declared_context,"phases":phases,
        "same_acknowledged_boundary_replacement":true,"unchanged_source_positions":true,
        "unchanged_clock":true,"unchanged_pmt_version":1,"decoded_av_equal":true,
        "tsduck_transport_passed":true,"counter_negative_detected":true,
        "persistent_decoder_proven":false,"ordinary_http_proven":false,"actual_client_proven":false
    }))
    .unwrap();
    assert!(proof.len() <= 256 * 1024);
    std::fs::write(directory.join("source.ts"), source).unwrap();
    std::fs::write(directory.join("output.ts"), bytes).unwrap();
    std::fs::write(directory.join("proof.json"), proof).unwrap();
}

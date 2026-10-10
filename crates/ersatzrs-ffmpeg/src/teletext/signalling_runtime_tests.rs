use mpeg2ts_reader::packet::Packet;
use std::time::Duration;

use super::{
    signalling_tests::{page, pages},
    *,
};

const MUXRATE: u64 = 5_000_000;
const SOURCE_ARGS: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-nostdin",
    "-f",
    "lavfi",
    "-i",
    "testsrc2=size=160x90:rate=25",
    "-f",
    "lavfi",
    "-i",
    "sine=frequency=440:sample_rate=48000",
    "-t",
    "6",
    "-map",
    "0:v:0",
    "-map",
    "1:a:0",
    "-map",
    "1:a:0",
    "-program",
    "program_num=10:st=0:st=1",
    "-program",
    "program_num=11:st=0:st=2",
    "-c:v",
    "mpeg2video",
    "-b:v",
    "200k",
    "-c:a",
    "aac",
    "-b:a",
    "64k",
    "-muxrate",
    "5000000",
    "-pcr_period",
    "20",
    "-f",
    "mpegts",
    "pipe:1",
];
const AV_DIGEST_ARGS: &[&str] = &[
    "-hide_banner",
    "-loglevel",
    "error",
    "-nostdin",
    "-f",
    "mpegts",
    "-i",
    "pipe:0",
    "-map",
    "0:v:0",
    "-map",
    "0:a",
    "-f",
    "framemd5",
    "pipe:1",
];

#[tokio::test]
#[ignore = "requires exact FFmpeg 9/libzvbi and TSDuck tsanalyze executables"]
async fn real_runtime_adds_signalled_pages_without_changing_av_packets() {
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
    let set = fixture_pages();
    let service = TeletextServiceDescriptor::from_page_set(&set);
    let mut discovery = TeletextPsiDiscovery::new(10, 512).unwrap();
    let mut admitted = None;
    for (index, raw) in input.iter().enumerate() {
        discovery.push(raw).unwrap();
        match discovery.plan(&service) {
            Ok(_) => {
                admitted = Some(index + 1);
                break;
            }
            Err(TeletextPsiError::Incomplete) => {}
            Err(error) => panic!("invalid source admission: {error}"),
        }
    }
    let mut monitor = discovery.into_monitor(&service).unwrap();
    for window in input[admitted.unwrap()..].chunks(256) {
        // New source bytes only; the host retains prefixes while false.
        monitor.inspect_window(window).unwrap();
    }
    monitor.finish().unwrap();
    let plan = monitor.plan().clone();
    assert_eq!(plan.program_number(), 10);
    let original_clock = verify_fixture_clock(&input, plan.pcr_pid()).unwrap();
    assert!(original_clock.ready);
    assert!(original_clock.validated_intervals > 200);
    let (clock_index, anchor) = source_clock(&input, plan.pcr_pid());
    verify_removed_slot_fails(&input, plan.pcr_pid(), clock_index);
    let mut output = input.clone();
    let mut installer = TeletextPmtInstaller::new(plan.clone());
    let installation = installer.install(&mut output).unwrap();
    assert!(installation.consumed_null_packets > 0);
    assert_eq!(installation.pmt_packets, 2 * installation.repetitions);
    assert!(
        installation.repetitions > 16,
        "fixture does not exercise counter wrap"
    );
    inject_fixture_carousel(&mut output, clock_index, anchor, set, &plan);
    assert_eq!(
        verify_fixture_clock(&output, plan.pcr_pid()).unwrap(),
        original_clock,
        "installation changed programme clock/rate evidence"
    );
    for (original, modified) in input.iter().zip(&output) {
        let pid = u16::from(Packet::new(original).pid());
        if pid != 0x1fff && pid != plan.pmt_pid() {
            assert_eq!(original, modified, "changed PID {pid}");
        }
    }
    let output: Vec<u8> = output.into_iter().flatten().collect();
    assert_eq!(output.len(), source.len());
    let decoded = crate::teletext_probe::run_probe(
        &ffmpeg,
        crate::teletext_probe::TEXT_PROBE_ARGUMENTS,
        &output,
        64 * 1024,
    )
    .await
    .unwrap();
    let text = String::from_utf8(decoded).unwrap();
    for expected in ["Signalled café", "Signalled caption"] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    let (original_av, modified_av) = tokio::join!(
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &source, 256 * 1024),
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &output, 256 * 1024)
    );
    assert_eq!(
        original_av.unwrap(),
        modified_av.unwrap(),
        "decoded AV frames/timestamps differ"
    );
    let analysis = crate::teletext_probe::run_probe(&tsanalyze, &["--json"], &output, 128 * 1024)
        .await
        .unwrap();
    verify_analysis(&serde_json::from_slice(&analysis).unwrap());
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_MULTIPLEX_ARTIFACT"])
    {
        std::fs::write(path, output).unwrap();
    }
}

fn fixture_pages() -> EncodedTeletextPageSet {
    let mut drafts = vec![page(200, 1, "nld", false), page(888, 0, "nld", true)];
    drafts.extend((850..=880).map(|number| page(number, 0, "nld", true)));
    pages(drafts)
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9/libzvbi and TSDuck tsanalyze executables"]
async fn real_runtime_output_gate_produces_valid_preserved_prefix() {
    verify_real_output_gate(false).await;
}

#[tokio::test]
#[ignore = "requires exact FFmpeg 9/libzvbi and TSDuck tsanalyze executables"]
async fn real_runtime_output_gate_drains_finite_source_without_changing_av() {
    verify_real_output_gate(true).await;
}

async fn verify_real_output_gate(finite_eof: bool) {
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
    let output = compose_fixture_output(&input, finite_eof);
    if finite_eof {
        assert_eq!(output.len(), input.len());
    }
    assert_eq!(
        verify_fixture_clock(&output, 256).unwrap(),
        verify_fixture_clock(&input[..output.len()], 256).unwrap()
    );
    for (before, after) in input.iter().zip(&output) {
        if ![8191, 4096].contains(&u16::from(Packet::new(before).pid())) {
            assert_eq!(before, after);
        }
    }
    let source: Vec<u8> = input[..output.len()].iter().flatten().copied().collect();
    let output: Vec<u8> = output.into_iter().flatten().collect();
    let (original_av, modified_av) = tokio::join!(
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &source, 256 * 1024),
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &output, 256 * 1024)
    );
    assert_eq!(original_av.unwrap(), modified_av.unwrap());
    let decoded = crate::teletext_probe::run_probe(
        &ffmpeg,
        crate::teletext_probe::TEXT_PROBE_ARGUMENTS,
        &output,
        64 * 1024,
    )
    .await
    .unwrap();
    let text = String::from_utf8(decoded).unwrap();
    assert!(text.contains("Signalled café"));
    assert!(text.contains("Signalled caption"));
    let analysis = crate::teletext_probe::run_probe(&tsanalyze, &["--json"], &output, 128 * 1024)
        .await
        .unwrap();
    verify_analysis(&serde_json::from_slice(&analysis).unwrap());
}

// Exercise gate composition with bounded incremental reads and a simulated
// monotonic sink acknowledgement. This is not asynchronous pacing/client proof.
fn compose_fixture_output(input: &[[u8; 188]], finite_eof: bool) -> Vec<[u8; 188]> {
    let set = fixture_pages();
    let mut carousel = TeletextCarousel::new(set.clone());
    let mut cycle_frames = 0_u64;
    loop {
        cycle_frames += 1;
        if carousel.next_batch().unwrap().kind() == TeletextCarouselBatchKind::Terminator {
            break;
        }
    }
    let (anchor, _) = source_clock(input, 256);
    let boundary =
        |frame: u64| anchor + 1 + usize::try_from(frame * MUXRATE / (50 * 188 * 8)).unwrap();
    let final_frame = if finite_eof {
        (1..=u64::try_from(input.len()).unwrap())
            .take_while(|&frame| boundary(frame) <= input.len())
            .last()
            .unwrap()
    } else {
        cycle_frames * 2
    };
    let extent = if finite_eof {
        input.len()
    } else {
        boundary(final_frame)
    };
    assert!(extent <= input.len());
    let mut gate = TeletextOutputGate::new(
        TeletextOutputConfig {
            program: 10,
            elementary_pid: 512,
            clock: fixture_clock_policy(256),
            max_buffer_packets: 1024,
            max_hold: Duration::from_millis(500),
            playout_delay: Duration::from_millis(100),
            max_lateness: Duration::from_millis(20),
        },
        set,
    )
    .unwrap();
    let mut read = 0;
    let mut output = Vec::new();
    let mut now = Duration::ZERO;
    for frame in 1..=final_frame {
        let end = boundary(frame);
        while read < end.max(256).min(extent) {
            let stop = (read + 256).min(extent);
            let bytes: Vec<u8> = input[read..stop].iter().flatten().copied().collect();
            gate.push(&bytes, now).unwrap();
            read = stop;
        }
        if !gate.prepare_window(end - output.len(), now).unwrap() {
            assert!(
                finite_eof || frame != final_frame,
                "final calibration boundary was not admitted"
            );
            continue;
        }
        let schedule = gate.pending_schedule().unwrap();
        now = schedule.last_due();
        output.extend_from_slice(gate.pending_packets(schedule.first_due()).unwrap());
        gate.acknowledge_window(schedule, now).unwrap();
    }
    if finite_eof {
        while read < extent {
            let stop = (read + 256).min(extent);
            let bytes: Vec<u8> = input[read..stop].iter().flatten().copied().collect();
            gate.push(&bytes, now).unwrap();
            read = stop;
        }
        if gate.prepare_final_window(now).unwrap() {
            let schedule = gate.pending_schedule().unwrap();
            now = schedule.last_due();
            output.extend_from_slice(gate.pending_packets(schedule.first_due()).unwrap());
            gate.acknowledge_window(schedule, now).unwrap();
        }
    }
    assert_eq!(output.len(), extent);
    gate.finish(now).unwrap();
    output
}

fn verify_removed_slot_fails(input: &[[u8; 188]], pcr_pid: u16, clock_index: usize) {
    let mut missing_slot = input.to_vec();
    let null_index = input
        .iter()
        .enumerate()
        .skip(clock_index + 1)
        .find(|(_, raw)| u16::from(Packet::new(*raw).pid()) == 8191)
        .map(|(index, _)| index)
        .unwrap();
    missing_slot.remove(null_index);
    assert_eq!(
        verify_fixture_clock(&missing_slot, pcr_pid),
        Err(TeletextClockError::Muxrate),
        "removing a physical null slot did not invalidate CBR timing"
    );
}

fn verify_fixture_clock(
    input: &[[u8; 188]],
    pcr_pid: u16,
) -> Result<TeletextClockReport, TeletextClockError> {
    // Explicit fixture policy, not blanket production/platform defaults.
    let mut clock = TeletextClockValidator::new(fixture_clock_policy(pcr_pid))?;
    for window in input.chunks(256) {
        clock.observe(window)?;
    }
    clock.report()
}

fn fixture_clock_policy(pcr_pid: u16) -> TeletextClockConfig {
    TeletextClockConfig {
        pcr_pid,
        muxrate_bps: MUXRATE,
        rate_tolerance_ppm: 1000,
        pcr_jitter_ticks: 14,
        max_pcr_gap_packets: 134,
        max_pcr_gap_ticks: 1_080_000,
        min_pcr_intervals: 2,
        max_window_packets: 256,
    }
}

fn source_clock(input: &[[u8; 188]], pcr_pid: u16) -> (usize, TeletextTimestamp) {
    input
        .iter()
        .enumerate()
        .find_map(|(index, raw)| {
            let packet = Packet::new(raw);
            if u16::from(packet.pid()) != pcr_pid {
                return None;
            }
            let clock = packet.adaptation_field()?.pcr().ok()?;
            Some((index, TeletextTimestamp::new(clock.base()).unwrap()))
        })
        .expect("FFmpeg fixture PCR missing")
}

// Use physical CBR packet positions for each 20 ms deadline. PTS is the end
// of that window on the unchanged programme clock; input packets are never
// appended or re-paced. This finite fixture is not the asynchronous host owner.
fn inject_fixture_carousel(
    output: &mut [[u8; 188]],
    clock_index: usize,
    anchor: TeletextTimestamp,
    set: EncodedTeletextPageSet,
    plan: &TeletextPmtPlan,
) {
    let mut carousel = TeletextCarousel::new(set);
    let mut packetizer = TeletextPacketizer::new(plan.elementary_pid()).unwrap();
    let mut cycles = 0;
    while cycles < 4 {
        let batch = carousel.next_batch().unwrap();
        let offset = |ticks: u64| usize::try_from(ticks * MUXRATE / (90_000 * 188 * 8)).unwrap();
        let start = clock_index + 1 + offset(batch.presentation_ticks());
        let end = clock_index + 1 + offset(batch.presentation_ticks() + 1800);
        assert!(end <= output.len(), "calibration has insufficient duration");
        let pts = anchor.wrapping_add(batch.presentation_ticks() + 1800);
        plan.replace_null_packets(&mut output[start..end], &batch, pts, &mut packetizer)
            .unwrap();
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            cycles += 1;
        }
    }
}

fn verify_analysis(report: &serde_json::Value) {
    assert!(
        report["ts"]["pcr-bitrate"]
            .as_u64()
            .unwrap()
            .abs_diff(MUXRATE)
            < 2000,
        "fixture PCR does not confirm the configured CBR rate"
    );
    assert_eq!(report["ts"]["packets"]["invalid-syncs"], 0);
    assert_eq!(report["ts"]["packets"]["transport-errors"], 0);
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
    let pids = report["pids"].as_array().unwrap();
    for expected in [256, 257, 258, 4096, 4097] {
        assert!(
            pids.iter().any(|pid| pid["id"] == expected),
            "missing source PID {expected}"
        );
    }
    let pid = report["pids"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pid| pid["id"] == 512)
        .unwrap();
    assert_eq!(pid["pes-stream-id"], 0xbd);
    assert_eq!(pid["invalid-pes-prefix"], 0);
    assert_eq!(pid["language"], "nld");
    assert!(pid["description"]
        .as_str()
        .unwrap()
        .contains("Initial Teletext page"));
    assert!(pid["description"]
        .as_str()
        .unwrap()
        .contains("Teletext subtitles"));
    for pid in report["pids"].as_array().unwrap() {
        for field in ["discontinuities", "pts-leap", "pcr-leap"] {
            assert_eq!(pid["packets"][field], 0, "PID {} {field}", pid["id"]);
        }
    }
}

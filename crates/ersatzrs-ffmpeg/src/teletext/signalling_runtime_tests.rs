use mpeg2ts_reader::{packet::Packet, psi::SectionCommonHeader};

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
    "1",
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
    "0:a:0",
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
    let source = crate::teletext_probe::run_probe(&ffmpeg, SOURCE_ARGS, &[], 1024 * 1024)
        .await
        .unwrap();
    let input: Vec<[u8; 188]> = source
        .chunks_exact(188)
        .map(|chunk| chunk.try_into().unwrap())
        .collect();
    assert_eq!(input.len() * 188, source.len());
    let set = pages(vec![page(200, 1, "nld", false), page(888, 0, "nld", true)]);
    let service = TeletextServiceDescriptor::from_page_set(&set);
    let plan = TeletextPmtPlan::new(source_pmt(&input), 4096, 512, &service).unwrap();
    let (clock_index, anchor) = source_clock(&input, plan.pcr_pid());
    let mut output = input.clone();
    install_fixture_pmt(&mut output, &plan);
    inject_fixture_carousel(&mut output, clock_index, anchor, set, &plan);
    for (original, modified) in input.iter().zip(&output) {
        let pid = u16::from(Packet::new(original).pid());
        if pid != 0x1fff && pid != 4096 {
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
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &source, 64 * 1024),
        crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &output, 64 * 1024)
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

// Calibration intentionally uses FFmpeg's small one-packet PMT. Production
// PSI assembly, PAT/other-program ownership and PMT growth handling remain due.
fn source_pmt(input: &[[u8; 188]]) -> &[u8] {
    for raw in input {
        let packet = Packet::new(raw);
        if u16::from(packet.pid()) == 4096 && packet.payload_unit_start_indicator() {
            let payload = packet.payload().unwrap();
            let start = 1 + usize::from(payload[0]);
            let header = SectionCommonHeader::new(&payload[start..start + 3]);
            return &payload[start..start + 3 + header.section_length];
        }
    }
    panic!("FFmpeg fixture PMT missing");
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

fn install_fixture_pmt(output: &mut [[u8; 188]], plan: &TeletextPmtPlan) {
    for raw in output {
        let packet = Packet::new(raw);
        if u16::from(packet.pid()) == 4096 {
            assert!(packet.payload_unit_start_indicator());
            let packets = plan.packets(packet.continuity_counter().count()).unwrap();
            assert_eq!(packets.len(), 1, "calibration PMT grew beyond one packet");
            *raw = packets[0];
        }
    }
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

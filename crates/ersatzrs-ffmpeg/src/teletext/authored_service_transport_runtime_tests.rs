//! Independent PES/TS checks of actual complete-service carousel batches.
//! Calibration supplies PSI/PCR only; this is not hosted AV/client acceptance.

use super::*;

async fn analyze(executable: &str, stream: &[u8]) -> serde_json::Value {
    let output = crate::teletext_probe::run_probe(executable, &["--json"], stream, 128 * 1024)
        .await
        .unwrap();
    serde_json::from_slice(&output).unwrap()
}

fn teletext_pid(report: &serde_json::Value) -> &serde_json::Value {
    report["pids"]
        .as_array()
        .unwrap()
        .iter()
        .find(|pid| pid["id"] == 256)
        .unwrap()
}

#[tokio::test]
#[ignore = "Requires explicit TSDuck tsanalyze and Python executables"]
async fn complete_authored_carousel_pes_passes_tsduck_and_detects_counter_damage() {
    let tsanalyze = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_TSANALYZE"])
        .expect("set the TSDuck tsanalyze executable");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .expect("set the Python executable");
    let service = super::authored_service_tests::candidate();
    assert_eq!(service.magazine_units().len(), 1);
    assert_eq!(service.pages().len(), 6);
    let mut carousel = TeletextCarousel::new_authored(service).unwrap();
    let mut packetizer = TeletextPacketizer::new(0x100).unwrap();
    let anchor = TeletextTimestamp::new(90_000).unwrap();
    let mut raw = Vec::new();
    let mut cycles = 0;
    let mut magazine_batches = 0;
    while cycles < 4 {
        let batch = carousel.next_batch().unwrap();
        if batch.kind() == TeletextCarouselBatchKind::Magazine {
            magazine_batches += 1;
        }
        raw.extend(
            packetizer
                .packetize(&batch, anchor.wrapping_add(batch.presentation_ticks()))
                .into_iter()
                .flatten(),
        );
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            cycles += 1;
        }
    }
    assert_eq!(magazine_batches, 4);
    let script = test_manifest_dir!().join("../../scripts/calibration/wrap-authored-teletext.py");
    let stream = crate::teletext_probe::run_probe(
        &python,
        &[script.to_str().unwrap(), "--packetized"],
        &raw,
        256 * 1024,
    )
    .await
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_AUTHORED_CAROUSEL_TS"])
    {
        std::fs::write(path, &stream).unwrap();
    }
    let report = analyze(&tsanalyze, &stream).await;
    assert_eq!(report["ts"]["packets"]["invalid-syncs"], 0);
    assert_eq!(report["ts"]["packets"]["transport-errors"], 0);
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
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
        .filter(|packet| packet[1] & 0x1f == 1 && packet[2] == 0 && packet[3] & 0x30 == 0x10)
        .nth(10)
        .unwrap();
    packet[3] ^= 3;
    let report = analyze(&tsanalyze, &damaged).await;
    assert!(
        teletext_pid(&report)["packets"]["discontinuities"]
            .as_u64()
            .unwrap()
            > 0
    );
}

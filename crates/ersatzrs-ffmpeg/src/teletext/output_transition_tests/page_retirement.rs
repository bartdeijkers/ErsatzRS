//! Full page erasure retires X28 independently of persistent magazine defaults.

use super::*;
use crate::teletext::output::transition_tests::{warm_magazine_defaults, warm_magazine_service};
use ersatzrs_domain::{TeletextPageCharsetDesignation, TeletextRgb};

#[tokio::test]
#[ignore = "requires exact FFmpeg 9 and TSDuck; real AV gate with simulated sink times, not client acceptance"]
async fn real_runtime_page_x28_retirement_restores_header_or_retained_magazine_context() {
    let directory =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PAGE_RETIREMENT_DIR"])
            .expect("set a new page retirement proof directory");
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
    let page = warm_magazine_defaults();
    let mut magazine = page;
    magazine.primary = TeletextPageCharsetDesignation::new(0x37).unwrap();
    magazine.secondary = Some(TeletextPageCharsetDesignation::new(0x24).unwrap());
    magazine.palette.clut2[0] = TeletextRgb::new([2, 4, 6]).unwrap();
    magazine.palette.clut2[7] = TeletextRgb::new([3, 10, 14]).unwrap();
    for (name, inherited) in [("header", None), ("magazine", Some(magazine))] {
        let old = warm_magazine_service(
            &[
                (200, 0, Some(page)),
                (200, 16, Some(page)),
                (201, 0, Some(page)),
            ],
            inherited,
        );
        let next = warm_magazine_service(
            &[(200, 0, None), (200, 16, None), (201, 0, None)],
            inherited,
        );
        assert_eq!(
            old.declared().semantic().base(),
            next.declared().semantic().base()
        );
        assert_eq!(old.magazine_units(), next.magazine_units());
        assert!(next
            .declared()
            .semantic()
            .page_enhancements()
            .values()
            .all(|page| page.presentation.is_none()));
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
            assert_eq!(section, expected);
        }
        let bytes: Vec<u8> = output.iter().flatten().copied().collect();
        let (before, after) = tokio::join!(
            crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &source, 256 * 1024),
            crate::teletext_probe::run_probe(&ffmpeg, AV_DIGEST_ARGS, &bytes, 256 * 1024)
        );
        assert_eq!(
            before.unwrap(),
            after.unwrap(),
            "decoded AV/timestamps differ"
        );
        verify_transport(&tsanalyze, &bytes).await;
        verify_counter_negative(&tsanalyze, &bytes).await;
        let proof = serde_json::to_vec(&serde_json::json!({
            "probe_version":1,"scope":"one real AV source and gate; simulated sink times",
            "source_sha256":warm_runtime_sha256(&source),"output_sha256":warm_runtime_sha256(&bytes),
            "packet_count":output.len(),"program":10,"pcr_pid":256,"teletext_pid":512,
            "declared_context":{"original_page":page,"retained_magazine":inherited},"phases":phases,
            "same_acknowledged_boundary_replacement":true,"unchanged_source_positions":true,
            "unchanged_clock":true,"unchanged_pmt_version":1,"decoded_av_equal":true,
            "tsduck_transport_passed":true,"counter_negative_detected":true,
            "persistent_decoder_proven":false,"ordinary_http_proven":false,"actual_client_proven":false
        }))
        .unwrap();
        assert!(proof.len() <= 256 * 1024);
        std::fs::write(directory.join(format!("{name}-output.ts")), bytes).unwrap();
        std::fs::write(directory.join(format!("{name}-proof.json")), proof).unwrap();
    }
    std::fs::write(directory.join("source.ts"), source).unwrap();
}

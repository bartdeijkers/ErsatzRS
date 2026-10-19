//! Fixed-cell independent evidence, separate from automatic resource acquisition.

use super::*;

fn request(cells: &[serde_json::Value]) -> serde_json::Value {
    let original = fixture(true);
    check_original(&original);
    let encoded =
        encode_teletext_authored_service(original.clone(), &transmissions(&original), &limits())
            .expect(
                "foreign default and compatibility glyph must compile before independent decoding",
            );
    assert!(
        encoded.declared() == &original,
        "authored declaration changed during encoding"
    );
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[1] else {
        panic!("the compatibility glyph must use the complete compiled display");
    };
    // Preserve the complete production display. The existing ordinary cycle
    // terminator closes its acquisition; no object or resource is reconstructed.
    let mut units = page.page().data_units().to_vec();
    units.push(super::super::super::packets::terminator(2));
    serde_json::json!({
        "expected_version": "0.2.0",
        "cases": [{
            "name": "foreign_mot_default_and_base_compatibility",
            "page": 0x200,
            "subcode": 1,
            "units": units.iter().map(|unit| unit.to_vec()).collect::<Vec<_>>(),
            "cells": cells,
        }],
    })
}

async fn decode(request: &serde_json::Value) -> std::io::Result<Vec<u8>> {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-main-composition-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let input = serde_json::to_vec(request).unwrap();
    crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &input, 16 * 1024).await
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn foreign_mot_default_and_base_compatibility_decode_with_pinned_imager() {
    // Literal decoder tuples: code, repertoire, diacritic, foreground,
    // background. These distinguish the original foreign A from selected POP Z,
    // while preserving the original Level 1.5 Z and the separate G2 pound.
    let request = request(&[
        serde_json::json!({"row": 1, "column": 8,
            "l15": [90, 15, 0, 7, 0], "l25": [65, 0, 0, 7, 0]}),
        serde_json::json!({"row": 1, "column": 9,
            "l15": [90, 15, 0, 7, 0], "l25": [90, 15, 0, 7, 0]}),
        serde_json::json!({"row": 2, "column": 1,
            "l15": [35, 7, 0, 7, 0], "l25": [35, 7, 0, 7, 0]}),
        serde_json::json!({"row": 2, "column": 2,
            "l15": [90, 15, 0, 7, 0], "l25": [90, 15, 0, 7, 0]}),
    ]);
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_FOREIGN_DEFAULT_IMAGER_FIXTURE",
    ]) {
        std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    }
    let output = decode(&request)
        .await
        .expect("actual compiled foreign-default wire must satisfy independent fixed cells");
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder": "teletextimager", "decoder_version": "0.2.0",
            "decoder_commit": "2b0125b778c45ffdfa2df0b196870c093361f317",
            "cases_passed": 1, "levels": ["1.5", "2.5"]})
    );

    // Only the claimed golden changes. Every negative uses the exact same
    // successfully decoded wire and must fail the decoder's cell comparison.
    for (name, index, level, component, wrong) in [
        ("selected_POP_Z_instead_of_original_A", 0, "l25", 0, 90),
        ("changed_original_fallback_Z", 0, "l15", 0, 65),
        ("missing_G2_pound", 2, "l25", 0, 32),
        ("changed_original_foreground", 0, "l25", 3, 2),
    ] {
        let mut negative = request.clone();
        negative["cases"][0]["cells"][index][level][component] = serde_json::json!(wrong);
        let error = decode(&negative)
            .await
            .expect_err("wrong fixed-cell golden must not decode successfully");
        assert_eq!(error.kind(), std::io::ErrorKind::Other, "{name}: {error}");
        assert_eq!(error.to_string(), "teletext probe process failed", "{name}");
    }
    println!("Independent foreign A, fallback Z, G2 pound and four wrong-golden refusals passed: {report}");
}

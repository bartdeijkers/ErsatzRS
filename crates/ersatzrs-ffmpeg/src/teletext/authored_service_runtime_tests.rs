//! Full authored compiler output through the existing independent native oracle.
//! Resource ordering and explicit closing headers are test feeding, not a carousel.

use super::*;

fn compile_candidate(mode: DefaultProgram, suppressed: bool) -> EncodedTeletextAuthoredService {
    let empty = resource(0xa0, 0);
    let semantic = service_with_companions(mode, vec![empty], suppressed);
    let mot = mot();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![mot.clone()],
        vec![TeletextDisplayMotSelection {
            display: base().address,
            mot: Some(mot.identity()),
        }],
        vec![empty],
        TeletextDeclaredServiceBudget {
            max_mots: 1,
            max_displays: 1,
            max_resources: 5,
            max_defaults: 1,
            max_objects: 3,
        },
    )
    .unwrap();
    let transmissions: Vec<_> = declared
        .resource_inventory()
        .iter()
        .map(|&resource| TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(0).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: resource.logical_subpage().get() == 1,
        })
        .collect();
    encode_teletext_authored_service(
        declared,
        &transmissions,
        &super::super::authored_service_tests::limits(),
    )
    .unwrap()
}

fn compiled_case(mode: DefaultProgram, suppressed: bool) -> serde_json::Value {
    let compiled = compile_candidate(mode, suppressed);
    assert_eq!(compiled.pages().len(), 6);
    assert!(compiled.magazine_units().is_empty());
    let mut units = compiled.magazine_units().to_vec();
    for page in compiled
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        let header = page.resource_header().unwrap();
        units.extend_from_slice(page.data_units());
        units.push(terminator(header.identity().page().magazine() % 8));
    }
    let display = &compiled.pages()[0];
    if suppressed {
        assert!(matches!(
            display,
            EncodedTeletextAuthoredPage::CompiledDisplay(_)
        ));
        assert!(display.data_units().iter().any(|unit| packet(unit) == 26));
    } else {
        assert!(matches!(
            display,
            EncodedTeletextAuthoredPage::NativeDisplay(_)
        ));
        assert!(display.data_units().iter().all(|unit| packet(unit) != 26));
    }
    units.extend_from_slice(display.data_units());
    units.push(terminator(2));
    // Same A/C/D/T goldens as the separately encoded primitive calibration.
    // Nothing from the compiled service is replaced or spliced into page bodies.
    let name = match (mode, suppressed) {
        (DefaultProgram::RootOnly, false) => "authored_service_foreign_mot_root_only",
        (DefaultProgram::RootOnly, true) => "authored_service_empty_local_suppresses_root",
        (DefaultProgram::NestedSelected, false) => "authored_service_foreign_mot_nested_selected",
        (DefaultProgram::NestedSelected, true) => "authored_service_empty_local_suppresses_nested",
    };
    serde_json::json!({
        "name": name,
        "units": units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "page": 0x200,
        "subcode": 0x10,
        "cells": expected_cells(suppressed, mode),
    })
}

async fn run_compiled_cases(mode: DefaultProgram, capture_variable: &str) {
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version": "0.2.42",
        "cases": [compiled_case(mode, false), compiled_case(mode, true)],
    }))
    .unwrap();
    decode_request(&request, capture_variable).await;
}

async fn decode_request(request: &[u8], capture_variable: &str) {
    if let Some(path) = crate::locator::first_non_empty_env(&[capture_variable]) {
        std::fs::write(path, request).unwrap();
    }
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-object-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(report["decoder_version"], "0.2.42");
    assert_eq!(report["cases_passed"], 2);
    println!("Independent complete authored service decoding: {report}");
}

#[tokio::test]
#[ignore = "Requires explicitly calibrated Linux x64 libzvbi 0.2.42"]
async fn full_authored_service_native_root_and_compiled_empty_local_decode() {
    run_compiled_cases(
        DefaultProgram::RootOnly,
        "ERSATZRS_TELETEXT_TEST_AUTHORED_SERVICE_ROOT_FIXTURE",
    )
    .await;
}

#[tokio::test]
#[ignore = "Known upstream X27 parsing deficiency; use the receipted test-only correction"]
async fn full_authored_service_nested_selected_default_decode() {
    run_compiled_cases(
        DefaultProgram::NestedSelected,
        "ERSATZRS_TELETEXT_TEST_AUTHORED_SERVICE_NESTED_FIXTURE",
    )
    .await;
}

fn carousel_case(mode: DefaultProgram, suppressed: bool) -> serde_json::Value {
    let service = compile_candidate(mode, suppressed);
    let mut carousel = TeletextCarousel::new_authored(service).unwrap();
    let mut units = Vec::new();
    let mut cycles = 0;
    while cycles < 2 {
        let batch = carousel.next_batch().unwrap();
        units.extend_from_slice(batch.data_units());
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            cycles += 1;
        }
    }
    let name = match (mode, suppressed) {
        (DefaultProgram::RootOnly, false) => "authored_carousel_foreign_mot_root_only",
        (DefaultProgram::RootOnly, true) => "authored_carousel_empty_local_suppresses_root",
        (DefaultProgram::NestedSelected, false) => "authored_carousel_foreign_mot_nested_selected",
        (DefaultProgram::NestedSelected, true) => "authored_carousel_empty_local_suppresses_nested",
    };
    serde_json::json!({
        "name": name,
        "units": units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "page": 0x200,
        "subcode": 0x10,
        "cells": expected_cells(suppressed, mode),
    })
}

async fn run_carousel_cases(mode: DefaultProgram, capture_variable: &str) {
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version": "0.2.42",
        "cases": [carousel_case(mode, false), carousel_case(mode, true)],
    }))
    .unwrap();
    decode_request(&request, capture_variable).await;
}

#[tokio::test]
#[ignore = "Requires explicitly calibrated Linux x64 libzvbi 0.2.42"]
async fn full_authored_carousel_root_and_compiled_empty_local_decode() {
    run_carousel_cases(
        DefaultProgram::RootOnly,
        "ERSATZRS_TELETEXT_TEST_AUTHORED_CAROUSEL_ROOT_FIXTURE",
    )
    .await;
}

#[tokio::test]
#[ignore = "Known upstream X27 parsing deficiency; use the receipted test-only correction"]
async fn full_authored_carousel_nested_selected_default_decode() {
    run_carousel_cases(
        DefaultProgram::NestedSelected,
        "ERSATZRS_TELETEXT_TEST_AUTHORED_CAROUSEL_NESTED_FIXTURE",
    )
    .await;
}

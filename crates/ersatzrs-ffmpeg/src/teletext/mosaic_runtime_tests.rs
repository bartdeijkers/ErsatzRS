//! Actual composed-main G1 wire/raster against independent libzvbi 0.2.42.
//! Normal opaque Local mosaics only; this is not full client acceptance.

use super::*;
use crate::teletext::packets::{data_unit, terminator};
use crate::teletext::*;

const CONTIGUOUS: [u16; 10] = [4095; 10];
const SEPARATED: [u16; 10] = [0, 1950, 1950, 0, 1950, 1950, 1950, 0, 1950, 1950];

fn mosaic_run(column: u8, count: usize, separated: bool) -> TeletextRun {
    TeletextRun {
        column,
        style: TeletextStyle::default(),
        content: TeletextRunContent::Mosaics {
            cells: vec![TeletextMosaic::new(63).unwrap(); count],
            separated,
        },
    }
}

fn d5(column: u8, separated: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_height: false,
            double_width: false,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: separated,
        },
    )
}

fn units(
    service: &TeletextSemanticService,
    compiled: &EncodedTeletextComposedMainProgram,
) -> Vec<[u8; 46]> {
    let page = encode_teletext_page(
        &service.base().pages[0],
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    let mut result = Vec::new();
    let mut inserted = false;
    for unit in page.data_units() {
        let address = source_header::observe_dvb_teletext_header(unit).unwrap();
        if matches!(address, source_header::TeletextHeaderObservation::NotHeader(address)
            if address.packet_number == 26)
        {
            if !inserted {
                result.extend(
                    compiled
                        .encoded()
                        .packet_bodies()
                        .iter()
                        .map(|body| data_unit(2, 26, body, false)),
                );
                inserted = true;
            }
        } else {
            result.push(*unit);
        }
    }
    assert!(
        inserted,
        "actual base X26 must be replaced with compiled main"
    );
    result.push(terminator(2));
    result
}

fn encoded_case(
    name: &str,
    runs: Vec<TeletextRun>,
    commands: Vec<TeletextEnhancementCommand>,
    column: u8,
    separated: bool,
) -> serde_json::Value {
    let service = Fixture::new(
        vec![TeletextRow { row: 1, runs }, text(2, 1, "B")],
        Some(commands),
    )
    .build();
    let allocations = allocate_teletext_resource_objects(service.objects(), &[], 0).unwrap();
    let compiled = compile_teletext_composed_main_program(
        &service,
        owner(),
        &allocations,
        budget(),
        TeletextObjectCatalogBudget {
            max_objects: 0,
            max_commands: 8192,
        },
    )
    .unwrap();
    let image = render_teletext_composition(
        compiled.composition(),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextRasterBudget {
            max_pixels: 216_000,
            max_diagnostics: 0,
        },
    )
    .unwrap();
    let expected = if separated { SEPARATED } else { CONTIGUOUS };
    let mut pixels = Vec::with_capacity(120);
    for (y, bits) in expected.into_iter().enumerate() {
        for x in 0..12 {
            let TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) =
                image.pixel(usize::from(column) * 12 + x, 10 + y).unwrap()
            else {
                panic!("mosaic rectangle must be resolved opaque RGB4");
            };
            let literal = if bits & (1 << (11 - x)) != 0 {
                [15, 15, 15]
            } else {
                [0, 0, 0]
            };
            assert_eq!(rgb.get(), literal, "{name}: fixed bitmap at {x},{y}");
            pixels.push(rgb.get());
        }
    }
    let units = units(&service, &compiled);
    serde_json::json!({
        "name":name,"units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "page":0x200,"subcode":owner().subcode.get(),"row":1,"column":column,
        "pattern":if separated {"separated"} else {"contiguous"},"raster_rgb4":pixels,
    })
}

fn cases() -> Vec<serde_json::Value> {
    let mosaic = at(
        8,
        TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(63).unwrap(),
        },
    );
    vec![
        encoded_case(
            "enhanced_default_d5_contiguous",
            vec![mosaic_run(8, 2, true)],
            vec![position(1, 8), mosaic],
            8,
            false,
        ),
        encoded_case(
            "enhanced_d5_separated",
            vec![mosaic_run(8, 2, true)],
            vec![position(1, 8), d5(8, true), mosaic],
            8,
            true,
        ),
        encoded_case(
            "base_d5_zero_cancels_spacing",
            vec![mosaic_run(8, 2, true)],
            vec![position(1, 8), d5(8, false)],
            8,
            false,
        ),
        encoded_case(
            "later_base_spacing_reenables_separation",
            vec![
                mosaic_run(8, 2, true),
                mosaic_run(12, 2, false),
                mosaic_run(20, 2, true),
            ],
            vec![position(1, 8), d5(8, false)],
            20,
            true,
        ),
    ]
}

async fn run_native(cases: Vec<serde_json::Value>, expected_count: usize) {
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases}))
            .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_MOSAIC_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-mosaic-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder_version":"0.2.42","cases_passed":expected_count})
    );
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi 0.2.42 renderer"]
async fn composed_mosaic_d5_raster_matches_independent_native_g1_pixels() {
    run_native(cases().into_iter().take(3).collect(), 3).await;
}

#[tokio::test]
#[ignore = "Known libzvbi 0.2.42 regression: later spacing separation after explicit D5=0 stays contiguous"]
async fn composed_mosaic_later_spacing_reenables_separation_with_libzvbi() {
    run_native(vec![cases().pop().unwrap()], 1).await;
}

#[tokio::test]
#[ignore = "Requires Python 3 and an explicitly supplied pinned teletextimager 0.2.0 checkout"]
async fn composed_mosaic_spacing_decodes_with_pinned_imager() {
    let cases: Vec<_> = cases()
        .into_iter()
        .zip([24, 25, 24, 25])
        .map(|case_and_font| {
            let (case, font) = case_and_font;
            serde_json::json!({
                "name":case["name"], "page":case["page"], "subcode":case["subcode"],
                "units":case["units"], "cells":[
                    {"row":case["row"],"column":case["column"],
                     "l15":[127,25,0,7,0],"l25":[127,font,0,7,0]},
                    {"row":2,"column":1,"l15":[66,15,0,7,0],"l25":[66,15,0,7,0]}
                ]
            })
        })
        .collect();
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.0","cases":cases})).unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_MOSAIC_IMAGER_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script =
        test_manifest_dir!().join("../../scripts/e2e/teletext-main-composition-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(report["decoder"], "teletextimager");
    assert_eq!(report["decoder_version"], "0.2.0");
    assert_eq!(
        report["decoder_commit"],
        "2b0125b778c45ffdfa2df0b196870c093361f317"
    );
    assert_eq!(report["cases_passed"], 4);
    assert_eq!(report["levels"], serde_json::json!(["1.5", "2.5"]));
}

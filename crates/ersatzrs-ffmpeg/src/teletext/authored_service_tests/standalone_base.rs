//! Whole-factory standalone spacing and applicable X26, not client acceptance.

use super::*;
use serde_json::{json, Value};

fn text(row: u8, style: TeletextStyle, content: &str) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![TeletextRun {
            column: 8,
            style,
            content: TeletextRunContent::Text {
                text: content.into(),
            },
        }],
    }
}

fn mosaic(row: u8, colour: TeletextColour, separated: bool) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle {
                foreground: colour,
                ..TeletextStyle::default()
            },
            content: TeletextRunContent::Mosaics {
                cells: vec![TeletextMosaic::new(63).unwrap()],
                separated,
            },
        }],
    }
}

fn fixture_original() -> TeletextDeclaredService {
    let mut draft = input();
    draft.base.pages.truncate(1);
    draft.pages.truncate(1);
    draft.magazines.clear();
    draft.objects.clear();
    draft.drcs.clear();
    draft.base.pages[0].rows = vec![
        text(
            1,
            TeletextStyle {
                foreground: TeletextColour::Red,
                background: TeletextColour::Blue,
                boxed: true,
                flash: true,
                conceal: true,
                ..TeletextStyle::default()
            },
            "A",
        ),
        text(
            2,
            TeletextStyle {
                foreground: TeletextColour::Green,
                height: TeletextHeight::Double,
                ..TeletextStyle::default()
            },
            "DE",
        ),
        mosaic(4, TeletextColour::Magenta, true),
        mosaic(5, TeletextColour::Cyan, false),
        text(6, TeletextStyle::default(), "ZZZ"),
        text(8, TeletextStyle::default(), "BASE 200"),
    ];
    let page = &mut draft.pages[0];
    page.resources = TeletextResolvedPageResourceSelection {
        objects: TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        drcs: TeletextDrcsPageLinks::unlinked(),
        defaults: [None; 4],
    };
    page.local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(6, 8).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(35).unwrap(),
                },
            },
            TeletextEnhancementCommand::AtColumn {
                column: 9,
                operation: TeletextColumnOperation::G0Diacritic {
                    code: TeletextGlyphCode::new(65).unwrap(),
                    diacritic: TeletextDiacriticIndex::new(2).unwrap(),
                },
            },
            TeletextEnhancementCommand::AtColumn {
                column: 10,
                operation: TeletextColumnOperation::G3 {
                    code: TeletextGlyphCode::new(47).unwrap(),
                    compatibility: TeletextG3Compatibility::Level15,
                },
            },
        ])
        .unwrap(),
    );
    let mut semantic = semantic_limits();
    semantic.max_objects = 0;
    let service = TeletextSemanticService::new(draft, semantic).unwrap();
    let mut budget = declaration_limits();
    budget.max_mots = 0;
    budget.max_objects = 0;
    budget.max_resources = 0;
    let declared = TeletextDeclaredService::new(
        service,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(0),
            mot: None,
        }],
        vec![],
        budget,
    )
    .unwrap();
    assert!(declared.resource_inventory().is_empty());
    declared
}

fn fixture() -> EncodedTeletextAuthoredService {
    let declared = fixture_original();
    let expected = declared.clone();
    let encoded = encode_teletext_authored_service(declared, &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), &expected);
    assert!(encoded.magazine_units().is_empty());
    assert_eq!(encoded.pages().len(), 1);
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
        panic!("applicable X26 must use the full compiled display path")
    };
    assert_eq!(display.page().address(), owner(0));
    assert!(encoded.pages()[0]
        .data_units()
        .iter()
        .all(|unit| !matches!(packet(unit), 28 | 29)));
    encoded
}

fn cell(row: u8, column: u8, value: [u8; 6], attributes: &[Value; 6]) -> Value {
    json!({"row":row,"column":column,"l1":value,"l15":value,"l25":value,
        "attributes":{"l1":attributes,"l15":attributes,"l25":attributes}})
}

fn cells() -> Vec<Value> {
    // Literal EN300706 spacing timing: colours/flash/size SetAfter;
    // new background/conceal SetAt. Fragment tags distinguish height halves.
    let steady = [
        json!(0),
        json!(0),
        json!(false),
        json!(false),
        json!(false),
        json!(false),
    ];
    let mut result = vec![
        cell(1, 1, [32, 0, 0, 7, 0, 0], &steady),
        cell(1, 2, [32, 0, 0, 4, 4, 0], &steady),
        cell(1, 3, [32, 0, 0, 4, 4, 0], &steady),
        cell(1, 4, [32, 0, 0, 1, 4, 0], &steady),
        cell(
            1,
            6,
            [32, 0, 0, 1, 4, 0],
            &[
                json!(0),
                json!(0),
                json!(false),
                json!(true),
                json!(false),
                json!(false),
            ],
        ),
        cell(
            1,
            7,
            [32, 0, 0, 1, 4, 0],
            &[
                json!(1),
                json!(0),
                json!(true),
                json!(true),
                json!(false),
                json!(false),
            ],
        ),
        cell(
            1,
            8,
            [b'A', 15, 0, 1, 4, 0],
            &[
                json!(1),
                json!(0),
                json!(true),
                json!(true),
                json!(false),
                json!(false),
            ],
        ),
        cell(2, 7, [32, 0, 0, 2, 0, 0], &steady),
        cell(2, 8, [b'D', 15, 0, 2, 0, 1], &steady),
        cell(3, 8, [b'D', 15, 0, 2, 0, 2], &steady),
        cell(4, 8, [127, 25, 0, 5, 0, 0], &steady),
        cell(5, 8, [127, 24, 0, 6, 0, 0], &steady),
        cell(6, 8, [b'Z', 15, 0, 7, 0, 0], &steady),
        cell(6, 9, [b'Z', 15, 0, 7, 0, 0], &steady),
        cell(6, 10, [b'Z', 15, 0, 7, 0, 0], &steady),
        cell(8, 8, [b'B', 15, 0, 7, 0, 0], &steady),
    ];
    for (index, enhanced) in [
        (12, [35, 7, 0, 7, 0, 0]),
        (13, [65, 0, 2, 7, 0, 0]),
        (14, [47, 26, 0, 7, 0, 0]),
    ] {
        result[index]["l15"] = json!(enhanced);
        result[index]["l25"] = json!(enhanced);
    }
    result
}

fn request() -> Value {
    let encoded = fixture();
    let mut units = encoded.pages()[0].data_units().to_vec();
    units.push(super::super::packets::terminator(2));
    json!({"expected_version":"0.2.0","cases":[{
        "name":"standalone_base_spacing_and_x26","page":0x200,"subcode":0,
        "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),"cells":cells()}]})
}

#[test]
fn standalone_spacing_and_level15_x26_compile_without_optional_resource_banks() {
    let encoded = fixture();
    let row = |number| {
        body(
            encoded.pages()[0]
                .data_units()
                .iter()
                .find(|unit| packet(unit) == number)
                .unwrap(),
        )
        .map(|value| value & 0x7f)
    };
    assert_eq!(
        &row(1)[1..9],
        &[0x04, 0x1d, 0x01, 0x0b, 0x0b, 0x08, 0x18, b'A']
    );
    assert_eq!(&row(2)[7..10], &[0x0d, b'D', b'E']);
    assert_eq!(row(4)[8], 127);
    assert_eq!(row(5)[8], 127);
    assert_eq!(&row(6)[8..11], b"ZZZ");
    assert_eq!(&row(8)[8..16], b"BASE 200");
}

#[test]
fn standalone_conceal_reveal_and_flash_phases_preserve_essential_base_pixels() {
    const A: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];
    let encoded = fixture();
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &encoded.pages()[0] else {
        panic!("fixture must use the full compiled display")
    };
    // The established atlas A literal is an oracle, not pixels read back from
    // the renderer. Flash phase is explicitly supplied, never a timing claim.
    let mut essential = None;
    for reveal in [false, true] {
        for phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
            let raster = render_teletext_composition(
                display.normalized().original(),
                TeletextColourPixelOptions {
                    reveal_concealed: reveal,
                    flash_phase: phase,
                },
                TeletextRasterBudget {
                    max_pixels: 120_000,
                    max_diagnostics: 1400,
                },
            )
            .unwrap();
            assert_eq!((raster.width(), raster.height()), (480, 250));
            for (y, mask) in A.into_iter().enumerate() {
                for x in 0..12 {
                    let visible = reveal
                        && phase == TeletextFlashPhase::Original
                        && mask & (1 << (11 - x)) != 0;
                    let expected = if visible { [15, 0, 0] } else { [0, 0, 15] };
                    assert!(
                        matches!(raster.pixel(8*12+x,10+y),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)))
                        if rgb.get() == expected),
                        "reveal={reveal},phase={phase:?},pixel={x},{y}"
                    );
                }
            }
            let information: Vec<_> = (80..90)
                .flat_map(|y| (8 * 12..16 * 12).map(move |x| (x, y)))
                .map(|(x, y)| raster.pixel(x, y).unwrap())
                .collect();
            if let Some(before) = &essential {
                assert_eq!(before, &information);
            } else {
                essential = Some(information);
            }
        }
    }
}

async fn decode(request: &Value) -> std::io::Result<Vec<u8>> {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-base-footprint-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    crate::teletext_probe::run_probe(
        &python,
        &[script.to_str().unwrap()],
        &serde_json::to_vec(request).unwrap(),
        16 * 1024,
    )
    .await
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn standalone_spacing_height_mosaics_and_x26_decode_at_levels1_15_25() {
    let request = request();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_STANDALONE_BASE_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    }
    let output = decode(&request)
        .await
        .expect("unchanged literal base/1.5 goldens must decode");
    let report: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        json!({"decoder":"teletextimager","decoder_version":"0.2.0",
        "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317",
        "cases_passed":1,"levels":["1.5","2.5"],"fragment_tags":true,
        "level1_and_attributes_cases":1})
    );
    for (name, index, key, component, wrong) in [
        ("flash_is_not_steady", 6, "attributes", 0, json!(0)),
        ("conceal_is_not_reveal", 6, "attributes", 2, json!(false)),
        ("height_is_not_normal", 8, "l15", 5, json!(0)),
        ("level15_keeps_g2", 12, "l15", 0, json!(b'Z')),
        ("level1_keeps_base_Z", 12, "l1", 0, json!(35)),
        ("level15_keeps_diacritic", 13, "l15", 2, json!(0)),
        ("level15_keeps_smoothing_G3", 14, "l15", 1, json!(15)),
    ] {
        let mut wrong_request = request.clone();
        if key == "attributes" {
            wrong_request["cases"][0]["cells"][index][key]["l15"][component] = wrong;
        } else {
            wrong_request["cases"][0]["cells"][index][key][component] = wrong;
        }
        let error = decode(&wrong_request)
            .await
            .expect_err("wrong literal golden must fail");
        assert_eq!(error.kind(), std::io::ErrorKind::Other, "{name}");
    }
    println!("Whole-factory base spacing/height/mosaics/flash/conceal and X26 passed at1/1.5/2.5, seven wrong-golden refusals: {report}");
}

//! Actual same-service wire/raster pixels against independent libzvbi 0.2.42.
//! Opaque steady DRCS rectangles only; this is not suitable-client acceptance.

use super::*;
use crate::teletext::packets::{data_unit, terminator};
use crate::teletext::presentation::{encode_presentation, western_latin_v1_presentation};
use crate::teletext::*;

const PIXELS: [u16; 10] = [
    0x801, 0x402, 0x204, 0x108, 0x090, 0x060, 0x030, 0x018, 0x00c, 0x006,
];
const FOREGROUND: [u8; 3] = [15, 2, 3];
const BACKGROUND: [u8; 3] = [1, 14, 5];

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    size: TeletextDisplaySize,
    scope: TeletextDrcsTableScope,
    invert: bool,
    conceal: bool,
    reveal: bool,
}

fn fixture(case: Case) -> Fixture {
    let mut result = drcs_fixture(PIXELS);
    result.objects.clear();
    result.rows.push(text(2, 1, "B"));
    let reference = TeletextDrcsGlyphReference {
        table: case.scope,
        subtable: TeletextDrcsSubtableIndex::new(if case.scope == TeletextDrcsTableScope::Normal {
            15
        } else {
            0
        })
        .unwrap(),
        slot: TeletextDrcsGlyphSlot::new(if case.scope == TeletextDrcsTableScope::Normal {
            47
        } else {
            3
        })
        .unwrap(),
    };
    let table = result
        .drcs
        .iter()
        .find(|table| {
            table.identity().kind()
                == if case.scope == TeletextDrcsTableScope::Normal {
                    TeletextResourceKind::Drcs
                } else {
                    TeletextResourceKind::Gdrcs
                }
        })
        .unwrap()
        .identity();
    result.drcs = vec![TeletextDrcsSubtable::new(
        table,
        vec![TeletextDrcsGlyphDefinition {
            slot: reference.slot,
            bitmap: TeletextDrcsBitmap::new(PIXELS).unwrap(),
        }],
    )
    .unwrap()];
    result.drcs_links = TeletextDrcsPageLinks::new(
        (case.scope == TeletextDrcsTableScope::Normal).then_some(table.page()),
        (case.scope == TeletextDrcsTableScope::Global).then_some(table.page()),
    )
    .unwrap();
    result.main = Some(vec![
        position(1, 8),
        at(
            8,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(16).unwrap(),
            },
        ),
        at(
            8,
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(24).unwrap(),
            },
        ),
        at(
            8,
            TeletextColumnOperation::Attributes {
                double_width: matches!(
                    case.size,
                    TeletextDisplaySize::DoubleWidth | TeletextDisplaySize::DoubleSize
                ),
                double_height: matches!(
                    case.size,
                    TeletextDisplaySize::DoubleHeight | TeletextDisplaySize::DoubleSize
                ),
                boxed: false,
                conceal: case.conceal,
                invert: case.invert,
                underline_or_separated_mosaics: false,
            },
        ),
        at(8, TeletextColumnOperation::Drcs { reference }),
    ]);
    let mut presentation =
        western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap()).unwrap();
    presentation.palette.clut2[0] = TeletextRgb::new(FOREGROUND).unwrap();
    presentation.palette.clut3[0] = TeletextRgb::new(BACKGROUND).unwrap();
    result.presentation = Some(presentation);
    result
}

fn wire_units(
    service: &TeletextSemanticService,
    compiled: &EncodedTeletextComposedMainProgram,
) -> Vec<[u8; 46]> {
    let table = &service.drcs().subtables()[0];
    let normal = table.identity().kind() == TeletextResourceKind::Drcs;
    let counter = TeletextResourceUpdateCounter::new(1).unwrap();
    let declaration = crate::teletext::resource_runtime_tests::mot(table.identity(), normal);
    let mot = encode_teletext_mot_page(
        &declaration,
        &[[None; 2]; 8],
        counter,
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap();
    let mut units = mot.data_units().to_vec();
    units.push(terminator(2));
    for logical in 0..=table.identity().logical_subpage().get() {
        let empty;
        let current = if logical == table.identity().logical_subpage().get() {
            table
        } else {
            empty = TeletextDrcsSubtable::new(
                TeletextResourceIdentity::new(
                    table.identity().kind(),
                    table.identity().page(),
                    TeletextResourceSubpageIndex::new(logical).unwrap(),
                )
                .unwrap(),
                vec![],
            )
            .unwrap();
            &empty
        };
        units.extend_from_slice(
            encode_teletext_drcs_page(
                current,
                counter,
                TeletextResourceFullUpdate::Changed,
                TeletextResourceTransmissionMode::Serial,
            )
            .unwrap()
            .data_units(),
        );
        units.push(terminator(table.identity().page().wire_magazine()));
    }
    let page = &service.base().pages[0];
    let TeletextEffectivePresentation::Extension { defaults, .. } =
        service.effective_presentation(owner()).unwrap()
    else {
        panic!("fixture must retain explicit authored palette");
    };
    let base = encode_teletext_page(page, TeletextEncodingRegion::WesternLatin).unwrap();
    let mut inserted = false;
    for unit in base.data_units() {
        match source_header::observe_dvb_teletext_header(unit).unwrap() {
            source_header::TeletextHeaderObservation::NotHeader(address)
                if address.packet_number == 28 =>
            {
                units.push(data_unit(2, 28, &encode_presentation(defaults), false));
            }
            source_header::TeletextHeaderObservation::NotHeader(address)
                if address.packet_number == 26 =>
            {
                if !inserted {
                    units.extend(
                        compiled
                            .encoded()
                            .packet_bodies()
                            .iter()
                            .map(|body| data_unit(2, 26, body, false)),
                    );
                    inserted = true;
                }
            }
            _ => units.push(*unit),
        }
    }
    assert!(inserted);
    units.push(terminator(2));
    units
}

fn encoded_case(case: Case) -> serde_json::Value {
    let service = fixture(case).build();
    let allocations = allocate_teletext_resource_objects(service.objects(), &[], 4).unwrap();
    let mut limits = budget();
    limits.collection.max_drcs_references = 1;
    let compiled = compile_teletext_composed_main_program(
        &service,
        owner(),
        &allocations,
        limits,
        TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 8192,
        },
    )
    .unwrap();
    let options = TeletextColourPixelOptions {
        reveal_concealed: case.reveal,
        flash_phase: TeletextFlashPhase::Original,
    };
    let image = render_teletext_composition(
        compiled.composition(),
        options,
        TeletextRasterBudget {
            max_pixels: 216_000,
            max_diagnostics: 0,
        },
    )
    .unwrap();
    let wide = matches!(
        case.size,
        TeletextDisplaySize::DoubleWidth | TeletextDisplaySize::DoubleSize
    );
    let high = matches!(
        case.size,
        TeletextDisplaySize::DoubleHeight | TeletextDisplaySize::DoubleSize
    );
    let mut pixels = Vec::new();
    for y in 10..10 + if high { 20 } else { 10 } {
        for x in 96..96 + if wide { 24 } else { 12 } {
            let TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) =
                image.pixel(x, y).unwrap()
            else {
                panic!("fixture raster rectangle must be resolved opaque RGB4");
            };
            pixels.push(rgb.get());
        }
    }
    let table = &service.drcs().subtables()[0];
    let reference = compiled.composition().drcs_execution().records()[0].reference();
    assert_eq!(
        reference.subtable.get(),
        table.identity().logical_subpage().get()
    );
    let units = wire_units(&service, &compiled);
    let size = match case.size {
        TeletextDisplaySize::Normal => "normal",
        TeletextDisplaySize::DoubleWidth => "double_width",
        TeletextDisplaySize::DoubleHeight => "double_height",
        TeletextDisplaySize::DoubleSize => "double_size",
    };
    serde_json::json!({"name":case.name,"units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "page":0x200,"subcode":owner().subcode.get(),"row":1,"column":8,
        "slot":reference.slot.get(),"s1":reference.subtable.get(),
        "normal":case.scope==TeletextDrcsTableScope::Normal,"pixels":PIXELS,"size":size,
        "foreground":FOREGROUND,"background":BACKGROUND,"invert":case.invert,"conceal":case.conceal,
        "reveal":case.reveal,"raster_rgb4":pixels})
}

fn cases() -> Vec<serde_json::Value> {
    use TeletextDisplaySize as Size;
    use TeletextDrcsTableScope as Scope;
    [
        ("normal", Size::Normal, Scope::Normal, false, false, false),
        (
            "wide",
            Size::DoubleWidth,
            Scope::Normal,
            false,
            false,
            false,
        ),
        (
            "high",
            Size::DoubleHeight,
            Scope::Normal,
            false,
            false,
            false,
        ),
        (
            "double_size",
            Size::DoubleSize,
            Scope::Normal,
            false,
            false,
            false,
        ),
        ("global", Size::Normal, Scope::Global, false, false, false),
        ("inverted", Size::Normal, Scope::Global, true, false, false),
        ("concealed", Size::Normal, Scope::Global, false, true, false),
        ("revealed", Size::Normal, Scope::Global, true, true, true),
    ]
    .into_iter()
    .map(|(name, size, scope, invert, conceal, reveal)| {
        encoded_case(Case {
            name,
            size,
            scope,
            invert,
            conceal,
            reveal,
        })
    })
    .collect()
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi 0.2.42 renderer"]
async fn composed_drcs_raster_matches_independent_native_pixels() {
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases()}))
            .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_RASTER_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-raster-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder_version":"0.2.42","cases_passed":8})
    );
}

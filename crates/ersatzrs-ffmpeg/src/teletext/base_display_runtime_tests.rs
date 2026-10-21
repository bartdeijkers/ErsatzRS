//! Fixed independent cell tuples from actual encoded spacing-control rows.
//! Character code/repertoire/mark are separate; raw codes are not Unicode.
//! This is not a full renderer, enhancement resolver or suitable-client gate.

use ersatzrs_domain::*;

use super::packets::{data_unit, terminator};
use super::{encode_teletext_page, source_header, TeletextEncodingRegion};

fn run(column: u8, style: TeletextStyle, text: &str) -> TeletextRun {
    TeletextRun {
        column,
        style,
        content: TeletextRunContent::Text { text: text.into() },
    }
}

fn cell(row: u8, column: u8, l15: [u8; 5], l25: [u8; 5]) -> serde_json::Value {
    serde_json::json!({"row":row,"column":column,"l15":l15,"l25":l25})
}

fn same(row: u8, column: u8, tuple: [u8; 5]) -> serde_json::Value {
    cell(row, column, tuple, tuple)
}

fn encoded_case(
    name: &str,
    rows: Vec<TeletextRow>,
    presentation: Option<TeletextLevel25PresentationDefaults>,
    cells: &[serde_json::Value],
) -> serde_json::Value {
    let mut page = super::transport_tests::draft(200, 0, false);
    page.national_option = TeletextNationalOption::new(1).unwrap();
    page.rows = rows;
    let encoded = encode_teletext_page(&page, TeletextEncodingRegion::WesternLatin).unwrap();
    let mut units = Vec::new();
    for unit in encoded.data_units() {
        let address = source_header::observe_dvb_teletext_header(unit).unwrap();
        if matches!(address, source_header::TeletextHeaderObservation::NotHeader(address)
            if address.packet_number == 28)
        {
            if let Some(presentation) = &presentation {
                // Author a real declaration through the production encoder.
                // Every original layout/row byte and the base-derived X26 stay.
                units.push(data_unit(
                    2,
                    28,
                    &super::presentation::encode_presentation(presentation),
                    false,
                ));
                continue;
            }
        }
        units.push(*unit);
    }
    units.push(terminator(2));
    serde_json::json!({"name":name,"page":0x200,"subcode":0,
        "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),"cells":cells})
}

fn control_case() -> serde_json::Value {
    let style = TeletextStyle {
        foreground: TeletextColour::Red,
        background: TeletextColour::Blue,
        boxed: true,
        flash: true,
        conceal: true,
        ..TeletextStyle::default()
    };
    encoded_case(
        "spacing_control_cells",
        vec![
            TeletextRow {
                row: 1,
                runs: vec![run(8, style, "A")],
            },
            TeletextRow {
                row: 2,
                runs: vec![run(8, TeletextStyle::default(), "A")],
            },
        ],
        None,
        &[
            same(1, 1, [32, 0, 0, 7, 0]), // Blue foreground is SetAfter.
            same(1, 2, [32, 0, 0, 4, 4]), // NewBackground is SetAt.
            same(1, 3, [32, 0, 0, 4, 4]), // Red foreground is SetAfter.
            same(1, 4, [32, 0, 0, 1, 4]),
            same(1, 5, [32, 0, 0, 1, 4]),
            same(1, 6, [32, 0, 0, 1, 4]),
            same(1, 7, [32, 0, 0, 1, 4]),
            same(1, 8, [b'A', 15, 0, 1, 4]),
            same(2, 8, [b'A', 15, 0, 7, 0]),
        ],
    )
}

fn mosaic_case() -> serde_json::Value {
    encoded_case(
        "mosaics_and_row_reset",
        vec![
            TeletextRow {
                row: 3,
                runs: vec![
                    TeletextRun {
                        column: 6,
                        style: TeletextStyle {
                            foreground: TeletextColour::Cyan,
                            ..TeletextStyle::default()
                        },
                        content: TeletextRunContent::Mosaics {
                            cells: vec![
                                TeletextMosaic::new(1).unwrap(),
                                TeletextMosaic::new(32).unwrap(),
                            ],
                            separated: true,
                        },
                    },
                    run(10, TeletextStyle::default(), "A"),
                ],
            },
            TeletextRow {
                row: 4,
                runs: vec![run(8, TeletextStyle::default(), "A")],
            },
        ],
        None,
        &[
            same(3, 4, [32, 0, 0, 7, 0]),
            same(3, 5, [32, 0, 0, 6, 0]),
            same(3, 6, [0x21, 25, 0, 6, 0]),
            same(3, 7, [0x60, 25, 0, 6, 0]),
            same(3, 8, [32, 25, 0, 6, 0]),
            same(3, 9, [32, 0, 0, 6, 0]),
            same(3, 10, [b'A', 15, 0, 7, 0]),
            same(4, 8, [b'A', 15, 0, 7, 0]),
        ],
    )
}

fn remapped_case() -> serde_json::Value {
    let mut presentation =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    presentation.remapping = TeletextColourRemapping::FgClut0BgClut2;
    encoded_case(
        "asymmetric_new_background",
        vec![TeletextRow {
            row: 1,
            runs: vec![run(
                8,
                TeletextStyle {
                    foreground: TeletextColour::Red,
                    background: TeletextColour::Red,
                    ..TeletextStyle::default()
                },
                "A",
            )],
        }],
        Some(presentation),
        &[
            cell(1, 5, [32, 15, 0, 7, 0], [32, 15, 0, 7, 16]),
            cell(1, 6, [32, 0, 0, 7, 0], [32, 0, 0, 7, 16]),
            cell(1, 7, [32, 0, 0, 1, 1], [32, 0, 0, 1, 17]),
            cell(1, 8, [b'A', 15, 0, 1, 1], [b'A', 15, 0, 1, 17]),
        ],
    )
}

fn unicode_case() -> serde_json::Value {
    encoded_case(
        "base_unicode_and_actual_replacements",
        vec![TeletextRow {
            row: 1,
            runs: vec![run(8, TeletextStyle::default(), "£éA")],
        }],
        None,
        &[
            same(1, 8, [0x23, 7, 0, 7, 0]),
            same(1, 9, [b'e', 0, 2, 7, 0]),
            same(1, 10, [b'A', 15, 0, 7, 0]),
        ],
    )
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn actual_base_spacing_cells_decode_with_pinned_independent_teletextimager() {
    let cases = vec![
        control_case(),
        mosaic_case(),
        remapped_case(),
        unicode_case(),
    ];
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.0","cases":cases})).unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_BASE_DISPLAY_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script =
        test_manifest_dir!().join("../../scripts/e2e/teletext-main-composition-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let result =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&result).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder":"teletextimager","decoder_version":"0.2.0",
        "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317","cases_passed":4,"levels":["1.5","2.5"]})
    );
}

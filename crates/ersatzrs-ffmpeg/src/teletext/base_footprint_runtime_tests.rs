//! Actual producer fragments at two decoder levels, not a full client gate.

use ersatzrs_domain::*;

fn row(row: u8, column: u8, style: TeletextStyle, text: &str) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![TeletextRun {
            column,
            style,
            content: TeletextRunContent::Text { text: text.into() },
        }],
    }
}

fn cell(row: u8, column: u8, l15: [u8; 6], l25: [u8; 6]) -> serde_json::Value {
    serde_json::json!({"row":row,"column":column,"l15":l15,"l25":l25})
}

fn same(row: u8, column: u8, expected: [u8; 6]) -> serde_json::Value {
    cell(row, column, expected, expected)
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn actual_spacing_and_nonspacing_height_have_distinct_lower_row_fragments() {
    let spacing = super::main_composition_runtime_tests::encoded_case(
        "spacing_height_fragments",
        vec![
            row(
                1,
                8,
                TeletextStyle {
                    foreground: TeletextColour::Red,
                    background: TeletextColour::Blue,
                    height: TeletextHeight::Double,
                    ..TeletextStyle::default()
                },
                "AB",
            ),
            row(3, 8, TeletextStyle::default(), "C"),
        ],
        vec![],
        vec![],
        &[
            same(1, 7, [32, 0, 0, 1, 4, 0]), // 0D is SetAfter.
            same(1, 8, [b'A', 15, 0, 1, 4, 1]),
            same(2, 8, [b'A', 15, 0, 1, 4, 2]),
            same(2, 6, [32, 0, 0, 4, 4, 0]), // Upper normal cell: BG only.
            same(3, 8, [b'C', 15, 0, 7, 0, 0]),
        ],
    );
    let nonspacing = super::main_composition_runtime_tests::encoded_case(
        "nonspacing_height_preserves_lower_base",
        vec![
            row(1, 8, TeletextStyle::default(), "A"),
            row(
                2,
                4,
                TeletextStyle {
                    foreground: TeletextColour::Red,
                    ..TeletextStyle::default()
                },
                "B",
            ),
        ],
        vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::Attributes {
                    double_height: true,
                    double_width: false,
                    boxed: false,
                    conceal: false,
                    invert: false,
                    underline_or_separated_mosaics: false,
                },
            },
        ],
        vec![],
        &[
            cell(1, 8, [b'A', 15, 0, 7, 0, 0], [b'A', 15, 0, 7, 0, 1]),
            cell(2, 8, [32, 15, 0, 1, 0, 0], [b'A', 15, 0, 7, 0, 2]),
            same(2, 4, [b'B', 15, 0, 1, 0, 0]),
        ],
    );
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":[spacing, nonspacing]
    }))
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_BASE_FOOTPRINT_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-base-footprint-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder":"teletextimager", "decoder_version":"0.2.0",
            "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317",
            "cases_passed":2,"levels":["1.5","2.5"],"fragment_tags":true})
    );
}

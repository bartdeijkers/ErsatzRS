//! Original producer C.3 cases, decoded independently at both client levels.
//! This fixed-cell probe is separate from suitable-client display acceptance.

use super::main_composition_runtime_tests::encoded_case;
use ersatzrs_domain::*;

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}

fn size(column: u8, height: bool, width: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_height: height,
            double_width: width,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    )
}

fn text(row: u8, column: u8, text: &str) -> TeletextRow {
    TeletextRow {
        row,
        runs: vec![TeletextRun {
            column,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: text.into() },
        }],
    }
}

fn cell(row: u8, column: u8, l15: [u8; 6], l25: [u8; 6]) -> serde_json::Value {
    serde_json::json!({"row":row,"column":column,"l15":l15,"l25":l25})
}

fn overlay(
    kind: TeletextObjectType,
    row: u8,
    column: u8,
    normal: bool,
) -> (
    TeletextEnhancementCommand,
    TeletextEnhancementObjectDefinition,
) {
    let target = TeletextObjectIdentity::local(
        TeletextPageAddress {
            page: TeletextPageNumber::new(200).unwrap(),
            subcode: TeletextSubcode::new(0).unwrap(),
        },
        TeletextObjectId::new(0),
    );
    let mut commands = vec![position(row, column)];
    if normal {
        commands.push(size(column, false, false));
    }
    commands.push(at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'X').unwrap(),
        },
    ));
    (
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: kind,
            offset: None,
        },
        TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: kind,
            program: TeletextEnhancementProgram::new(commands).unwrap(),
        },
    )
}

fn covered_foreground_case() -> serde_json::Value {
    encoded_case(
        "covered_foreground_deferred_and_superseded",
        vec![text(1, 8, "ABCD")],
        vec![
            position(1, 8),
            at(
                8,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(1).unwrap(),
                },
            ),
            size(8, false, true),
            at(
                9,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(4).unwrap(),
                },
            ),
            at(
                10,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(2).unwrap(),
                },
            ),
            size(10, false, false),
        ],
        vec![],
        &[
            cell(1, 8, [b'A', 15, 0, 7, 0, 0], [b'A', 15, 0, 1, 0, 3]),
            cell(1, 9, [b'B', 15, 0, 7, 0, 0], [b'A', 15, 0, 1, 0, 4]),
            cell(1, 10, [b'C', 15, 0, 7, 0, 0], [b'C', 15, 0, 2, 0, 0]),
        ],
    )
}

fn overlay_cases() -> Vec<serde_json::Value> {
    let mut result = Vec::new();
    for (kind, column, normal, name) in [
        (
            TeletextObjectType::Adaptive,
            8,
            false,
            "adaptive_origin_inherits_width",
        ),
        (
            TeletextObjectType::Adaptive,
            8,
            true,
            "adaptive_origin_explicit_normal",
        ),
        (
            TeletextObjectType::Adaptive,
            9,
            true,
            "adaptive_right_fragment_explicit_normal",
        ),
        (
            TeletextObjectType::Passive,
            9,
            false,
            "passive_overlays_right_fragment",
        ),
    ] {
        let (invoke, definition) = overlay(kind, 1, 0, normal);
        let main = vec![
            position(1, column),
            invoke,
            position(2, 8),
            size(8, false, true),
            size(10, false, false),
        ];
        let expected = match (kind, normal, column) {
            (TeletextObjectType::Adaptive, false, _) => [b'X', 0, 0, 7, 0, 3],
            (TeletextObjectType::Adaptive, true, 8) => [b'X', 0, 0, 7, 0, 0],
            _ => [b'A', 15, 0, 7, 0, 3],
        };
        let right = match (kind, normal, column) {
            (TeletextObjectType::Adaptive, false, _) => [b'X', 0, 0, 7, 0, 4],
            (TeletextObjectType::Adaptive, true, 8) => [b'A', 15, 0, 7, 0, 4],
            _ => [b'X', 0, 0, 7, 0, 0],
        };
        result.push(encoded_case(
            name,
            vec![text(2, 8, "ABCD")],
            main,
            vec![definition],
            &[
                cell(2, 8, [b'A', 15, 0, 7, 0, 0], expected),
                cell(2, 9, [b'B', 15, 0, 7, 0, 0], right),
            ],
        ));
    }
    result
}

fn passive_bottom_case() -> serde_json::Value {
    let (invoke, definition) = overlay(TeletextObjectType::Passive, 1, 0, false);
    encoded_case(
        "passive_overlays_bottom_fragment",
        vec![text(1, 8, "A"), text(2, 8, "B")],
        vec![
            position(1, 8),
            size(8, true, false),
            invoke,
            size(9, false, false),
        ],
        vec![definition],
        &[
            cell(1, 8, [b'A', 15, 0, 7, 0, 0], [b'A', 15, 0, 7, 0, 1]),
            cell(2, 8, [b'B', 15, 0, 7, 0, 0], [b'X', 0, 0, 7, 0, 0]),
        ],
    )
}

fn cases() -> Vec<serde_json::Value> {
    let mut result = vec![covered_foreground_case()];
    result.extend(overlay_cases());
    result.push(passive_bottom_case());
    result
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn original_c3_fragments_and_object_overlays_decode_independently() {
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.0", "cases":cases(),
    }))
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_DISPLAY_RESOLUTION_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-base-footprint-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({
            "decoder":"teletextimager", "decoder_version":"0.2.0",
            "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317",
            "cases_passed":6,"levels":["1.5","2.5"],"fragment_tags":true,
        })
    );
}

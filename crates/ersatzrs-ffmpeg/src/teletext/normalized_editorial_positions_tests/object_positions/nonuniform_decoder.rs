//! Exact complete-factory bytes retain both background regions at both levels.
use super::*;

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn nonuniform_active_header_background_decodes_with_pinned_independent_imager() {
    let (mut draft, outer, _) =
        nested_header_draft(TeletextObjectType::Active, TeletextObjectType::Passive);
    // This is the unchanged source from the existing complete-factory qualifier.
    draft.base.pages[0]
        .rows
        .iter_mut()
        .find(|row| row.row == 0)
        .unwrap()
        .runs
        .push(TeletextRun {
            column: 20,
            style: TeletextStyle {
                background: TeletextColour::Blue,
                ..TeletextStyle::default()
            },
            content: TeletextRunContent::Text {
                text: "Z".repeat(20),
            },
        });
    let source = declared(draft);
    let original = source.clone();
    let service =
        encode_teletext_authored_service(source, &transmissions(&original), &limits()).unwrap();
    assert_eq!(service.declared(), &original);
    let EncodedTeletextAuthoredPage::CompiledDisplay(display) = &service.pages()[0] else {
        panic!("the exact complete authored factory must own the display bytes");
    };
    let mut units = display.page().data_units().to_vec();
    units.push(crate::teletext::packets::terminator(2));
    // Fixed Latin CLUT0 cells need no external resource or changed CLUT2 slot.
    // This does not qualify full MOT/M29 acquisition or another receiver.
    let case = serde_json::json!({
        "name":"nonuniform_active_header_background", "page":0x200, "subcode":0,
        "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "cells":[
            {"row":0,"column":8,"l15":[90,15,0,7,0],"l25":[65,0,0,2,0]},
            {"row":0,"column":20,"l15":[90,15,0,7,4],"l25":[90,15,0,7,4]},
            {"row":0,"column":39,"l15":[90,15,0,7,4],"l25":[90,15,0,7,4]},
            {"row":1,"column":8,"l15":[90,15,0,7,0],"l25":[90,15,0,7,0]},
            {"row":1,"column":9,"l15":[90,15,0,7,0],"l25":[90,15,0,7,0]}
        ]
    });
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.0","cases":[case.clone()]}))
            .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_NONUNIFORM_HEADER_FIXTURE"])
    {
        let path = std::path::Path::new(&path);
        std::fs::write(path, &request).unwrap();
        let wrong = wrong_flash_case(&case, &units, display.normalized().encoded(), outer);
        std::fs::write(
            path.with_file_name("wrong-flash-request.json"),
            serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.0","cases":[wrong]}))
                .unwrap(),
        )
        .unwrap();
    }
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-header-context-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let output =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder":"teletextimager","decoder_version":"0.2.0",
            "decoder_commit":"2b0125b778c45ffdfa2df0b196870c093361f317",
            "cases_passed":1,"levels":["1.5","2.5"],"steady_tail_cells":32})
    );
}

// The negative keeps valid Hamming/envelopes and every original cell golden.
// Only the actual derived Flash word changes, so the extra observer must reject
// a real flash mismatch rather than a corrupt transport or unsupported request.
fn wrong_flash_case(
    case: &serde_json::Value,
    units: &[[u8; 46]],
    packed: &EncodedTeletextLocalObjectProgram,
    outer: TeletextObjectIdentity,
) -> serde_json::Value {
    let position = packed
        .command_positions()
        .iter()
        .find(|position| {
            position.source().object == Some(outer)
                && position.source().command == 1
                && position.original_object_source()
                    == Some(TeletextMainCompositionSource::HeaderPosition { command: 0 })
        })
        .unwrap();
    let index = position.semantic_word();
    let body = packed.packet_bodies()[index / 13];
    let offset = 1 + index % 13 * 3;
    assert_eq!(
        &body[offset..offset + 3],
        &crate::teletext::packets::hamming_24_18(8 | (7 << 6))
    );
    let original = crate::teletext::packets::data_unit(2, 26, &body, false);
    let mut changed = body;
    changed[offset..offset + 3].copy_from_slice(&crate::teletext::packets::hamming_24_18(
        8 | (7 << 6) | (1 << 11),
    ));
    let mut corrupted = units.to_vec();
    let matches: Vec<_> = corrupted
        .iter()
        .enumerate()
        .filter_map(|(index, unit)| (*unit == original).then_some(index))
        .collect();
    assert_eq!(matches.len(), 1);
    corrupted[matches[0]] = crate::teletext::packets::data_unit(2, 26, &changed, false);
    let mut wrong = case.clone();
    wrong["units"] = serde_json::json!(corrupted
        .iter()
        .map(|unit| unit.to_vec())
        .collect::<Vec<_>>());
    wrong
}

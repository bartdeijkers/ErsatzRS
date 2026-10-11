//! Opt-in reproducer for the demonstrated libzvbi 0.2.42 local-marker fault.
//! Independent local-object rendering failed; conformance remains open. The
//! normative compiler bytes stay unchanged, and a separate export retains the
//! correct rendered-object oracle for replay against a conforming decoder.
//! The reused ctypes oracle deliberately supports Linux x64 only. Other native
//! targets skip this diagnostic; production compilation is platform independent.

use ersatzrs_domain::*;

use super::packets::{data_unit, terminator};
use super::*;

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn at(operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation,
    }
}

fn cell(column: u8, character: u8, foreground: u8, background: u8) -> serde_json::Value {
    serde_json::json!({"row":4,"column":column,"unicode":character,
        "foreground":foreground,"background":background})
}

fn case(name: &str, kind: TeletextObjectType) -> serde_json::Value {
    let mut draft = super::transport_tests::draft(200, 0, false);
    draft.rows = [2_u8, 4]
        .into_iter()
        .map(|row| TeletextRow {
            row,
            runs: vec![TeletextRun {
                column: if row == 2 { 1 } else { 8 },
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text {
                    text: if row == 2 { "B" } else { "TTTT" }.into(),
                },
            }],
        })
        .collect();
    let target = TeletextObjectIdentity::local(draft.address, TeletextObjectId::new(0xffff));
    let catalog = TeletextEnhancementObjectCatalog::new(
        vec![TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: kind,
            program: TeletextEnhancementProgram::new(vec![
                position(1, 0),
                at(TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(1).unwrap(),
                }),
                at(TeletextColumnOperation::Background {
                    colour: TeletextColourReference::new(4).unwrap(),
                }),
                at(TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                }),
            ])
            .unwrap(),
        }],
        TeletextObjectCatalogBudget {
            max_objects: 1,
            max_commands: 4,
        },
    )
    .unwrap();
    let resources = allocate_teletext_resource_objects(&catalog, &[], 1).unwrap();
    assert!(resources.entries().is_empty());
    let program = TeletextEnhancementProgram::new(vec![
        position(3, 8),
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: kind,
            offset: None,
        },
    ])
    .unwrap();
    let local = compile_teletext_local_program_with_objects(
        draft.address,
        &program,
        &catalog,
        &resources,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        TeletextObjectCatalogBudget {
            max_objects: 1,
            max_commands: 6,
        },
    )
    .unwrap();
    let base = encode_teletext_page(&draft, TeletextEncodingRegion::WesternLatin).unwrap();
    let original = base.data_units();
    assert!(
        matches!(source_header::observe_dvb_teletext_header(&original[2]).unwrap(),
        source_header::TeletextHeaderObservation::NotHeader(address) if address.packet_number == 26)
    );
    let mut units = original[..2].to_vec();
    // Replace the base encoder's empty designation zero with the ONE compiled
    // main-and-definitions sequence, preserving all original ASCII/body bytes.
    for body in local.packet_bodies() {
        units.push(data_unit(2, 26, body, false));
    }
    units.extend_from_slice(&original[3..]);
    units.push(terminator(2));
    let (foreground, background) = if kind == TeletextObjectType::Active {
        (1, 4)
    } else {
        (7, 0)
    };
    serde_json::json!({"name":name,"units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "page":0x200,"subcode":0,"cells":[cell(8,b'A',1,4),cell(9,b'T',foreground,background)]})
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi decoder"]
async fn pinned_libzvbi_0_2_42_local_definition_marker_limitation_is_reproduced() {
    let mut cases = vec![
        case("local_active", TeletextObjectType::Active),
        case("local_adaptive", TeletextObjectType::Adaptive),
        case("local_passive", TeletextObjectType::Passive),
    ];
    let conformance_request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases}))
            .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_CONFORMANCE_FIXTURE",
    ]) {
        std::fs::write(path, &conformance_request).unwrap();
    }
    // The pinned decoder stops at the definition marker instead of executing
    // its body. These exact fallback cells diagnose that version-specific
    // failure; they are not a local-object support or conformance oracle.
    for case in &mut cases {
        case["cells"] = serde_json::json!([cell(8, b'T', 7, 0), cell(9, b'T', 7, 0)]);
    }
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases}))
            .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-object-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(report["decoder_version"], "0.2.42");
    assert_eq!(report["cases_passed"], 3);
    println!("Pinned local-object decoder limitation reproduced: {report}");
}

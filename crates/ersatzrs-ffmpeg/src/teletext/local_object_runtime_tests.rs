//! Opt-in independent local-object calibration using one immutable golden input.
//! libzvbi 0.2.42 rendering failed: its limitation diagnostic is Linux x64 only.
//! The portable teletextimager check verifies the original local glyph/attribute
//! oracle. Neither check establishes full Level 2.5 or client acceptance.

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

fn conformance_cases() -> Vec<serde_json::Value> {
    vec![
        case("local_active", TeletextObjectType::Active),
        case("local_adaptive", TeletextObjectType::Adaptive),
        case("local_passive", TeletextObjectType::Passive),
    ]
}

fn conformance_request(cases: &[serde_json::Value]) -> Vec<u8> {
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases}))
            .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_LOCAL_OBJECT_CONFORMANCE_FIXTURE",
    ]) {
        std::fs::write(path, &request).unwrap();
    }
    request
}

// Intentional Linux x64 exception: the frozen public ctypes ABI oracle has
// precisely that native calibration. The separate imager test has no ABI guard.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi decoder"]
async fn pinned_libzvbi_0_2_42_local_definition_marker_limitation_is_reproduced() {
    let mut cases = conformance_cases();
    let _ = conformance_request(&cases);
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
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-object-decoder.py");
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

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn authored_local_objects_decode_with_pinned_independent_teletextimager() {
    let request = conformance_request(&conformance_cases());
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-imager-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(
        report,
        serde_json::json!({
            "decoder": "teletextimager",
            "decoder_version": "0.2.0",
            "decoder_commit": "2b0125b778c45ffdfa2df0b196870c093361f317",
            "fixture_provenance_version": "0.2.42",
            "cases_passed": 3
        })
    );
    println!("Independent local-object cells calibrated: {report}");
}

fn advanced_case(
    name: &str,
    commands: Vec<TeletextEnhancementCommand>,
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    expected_positions: &[(u8, u8)],
) -> serde_json::Value {
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
    let max_objects = definitions.len();
    let definition_commands = definitions
        .iter()
        .map(|definition| definition.program.commands().len())
        .sum::<usize>();
    let max_commands = commands.len() + definition_commands;
    let catalog = TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects,
            max_commands: definition_commands,
        },
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], max_objects).unwrap();
    assert!(allocations.entries().is_empty());
    let program = TeletextEnhancementProgram::new(commands).unwrap();
    let local = compile_teletext_local_program_with_objects(
        draft.address,
        &program,
        &catalog,
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        TeletextObjectCatalogBudget {
            max_objects,
            max_commands,
        },
    )
    .unwrap();
    assert_eq!(local.main_triplet_count(), program.commands().len());
    assert_eq!(local.definition_positions().len(), expected_positions.len());
    for ((actual, definition), &(index, number)) in local
        .definition_positions()
        .iter()
        .zip(catalog.definitions())
        .zip(expected_positions)
    {
        assert_eq!(actual.target(), definition.identity);
        assert_eq!(actual.object_type(), definition.object_type);
        assert_eq!(actual.absolute_index(), index);
        assert_eq!(actual.object_number(), number);
    }
    let base = encode_teletext_page(&draft, TeletextEncodingRegion::WesternLatin).unwrap();
    let original = base.data_units();
    assert!(
        matches!(source_header::observe_dvb_teletext_header(&original[2]).unwrap(),
        source_header::TeletextHeaderObservation::NotHeader(address) if address.packet_number == 26)
    );
    let mut units = original[..2].to_vec();
    // Replace only the original ASCII page's empty X26; every new word comes
    // from the production compiler, including definitions and selectors.
    units.extend(
        local
            .packet_bodies()
            .iter()
            .map(|body| data_unit(2, 26, body, false)),
    );
    units.extend_from_slice(&original[3..]);
    units.push(terminator(2));
    serde_json::json!({"name":name,"units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "page":0x200,"subcode":0,"cells":[cell(8,b'A',1,4),cell(9,b'T',7,0)]})
}

fn advanced_glyph() -> Vec<TeletextEnhancementCommand> {
    vec![
        at(TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(1).unwrap(),
        }),
        at(TeletextColumnOperation::Background {
            colour: TeletextColourReference::new(4).unwrap(),
        }),
        at(TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        }),
    ]
}

fn high_designation_case(name: &str, index: u8, number: u8) -> serde_json::Value {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let target = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    // Original main position + invocation + data-six termination occupy three
    // words. AP-neutral black screen commands therefore put the marker at the
    // literal index 104 (designation 8/slot 0) or 195 (designation 15/slot 0).
    let mut commands = vec![
        TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::BLACK,
        };
        usize::from(index) - 3
    ];
    commands.extend([
        position(3, 8),
        TeletextEnhancementCommand::InvokeObject {
            target,
            object_type: TeletextObjectType::Passive,
            offset: None,
        },
    ]);
    let mut body = vec![position(1, 0)];
    body.extend(advanced_glyph());
    advanced_case(
        name,
        commands,
        vec![TeletextEnhancementObjectDefinition {
            identity: target,
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(body).unwrap(),
        }],
        &[(index, number)],
    )
}

fn nested_local_case() -> serde_json::Value {
    let owner = super::transport_tests::draft(200, 0, false).address;
    let targets = [0xffff, 0x0100, 0x0000]
        .map(|id| TeletextObjectIdentity::local(owner, TeletextObjectId::new(id)));
    let kinds = [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ];
    let invoke = |index: usize| TeletextEnhancementCommand::InvokeObject {
        target: targets[index],
        object_type: kinds[index],
        offset: None,
    };
    let definitions = (0..3)
        .map(|index| TeletextEnhancementObjectDefinition {
            identity: targets[index],
            object_type: kinds[index],
            program: TeletextEnhancementProgram::new(if index == 2 {
                // This final object's initial relative AP is already 0,0.
                // Column operations need no context-dependent row-zero selector.
                advanced_glyph()
            } else {
                vec![position(1, 0), invoke(index + 1)]
            })
            .unwrap(),
        })
        .collect();
    // Root origin 2,8 plus each outer definition's 1,0 places A at 4,8.
    // Passive attributes do not alter the adjacent underlying T.
    advanced_case(
        "nested_local_active_adaptive_passive",
        vec![position(2, 8), invoke(0)],
        definitions,
        &[(3, 0x03), (6, 0x06), (9, 0x09)],
    )
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly pinned external teletextimager checkout"]
async fn advanced_local_objects_decode_with_pinned_independent_teletextimager() {
    let cases = vec![
        high_designation_case("local_passive_designation_eight", 104, 0x80),
        high_designation_case("local_passive_designation_fifteen", 195, 0xf0),
        nested_local_case(),
    ];
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases}))
            .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[
        "ERSATZRS_TELETEXT_TEST_ADVANCED_LOCAL_OBJECT_FIXTURE",
    ]) {
        std::fs::write(path, &request).unwrap();
    }
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-imager-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(
        report,
        serde_json::json!({
            "decoder": "teletextimager",
            "decoder_version": "0.2.0",
            "decoder_commit": "2b0125b778c45ffdfa2df0b196870c093361f317",
            "fixture_provenance_version": "0.2.42",
            "cases_passed": 3
        })
    );
    println!("Advanced independent local-object cells calibrated: {report}");
}

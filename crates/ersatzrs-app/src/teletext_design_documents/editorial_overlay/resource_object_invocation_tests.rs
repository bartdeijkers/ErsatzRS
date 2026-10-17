//! V16 isolates retained-call edits without widening historical program roles.
use super::resource_object_operand_tests::{identity, original};
use super::tests::{budget, bytes, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextDesignDocumentFailure, TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextResourceObjectOperandOverlay, TeletextResourceObjectProgramOverlay,
};
use serde_json::{json, Value};

#[path = "resource_object_invocation_structure_tests.rs"]
mod structure;

fn foreground(colour: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: serde_json::from_value(json!({"kind":"foreground","colour":colour})).unwrap(),
    }
}
fn glyph(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: serde_json::from_value(json!({"kind":"g0","code":65})).unwrap(),
    }
}
fn invoke(global: bool, column: u8) -> TeletextEnhancementCommand {
    serde_json::from_value(json!({"kind":"invoke_object","target":identity(!global,0),
        "object_type":"passive","offset":{"row":0,"column":column}}))
    .unwrap()
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn call_change(global: bool, subpage: u8) -> TeletextResourceObjectProgramOverlay {
    TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        identity(global, subpage),
        program(vec![foreground(1), glyph(1), invoke(global, 0)]),
        program(vec![foreground(1), glyph(1), invoke(global, 1)]),
        vec![Some(0), Some(1), Some(2)],
        budget().validation.service,
    )
    .unwrap()
}
fn original_for(change: &TeletextResourceObjectProgramOverlay) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original(false)).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    let id = change.identity();
    let scope = if id.resource().unwrap().kind() == ersatzrs_domain::TeletextResourceKind::Pop {
        "public"
    } else {
        "global"
    };
    let definition = source["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| {
            object["identity"]["scope"] == scope
                && object["identity"]["resource"]["subpage"]
                    == id.resource().unwrap().logical_subpage().get()
        })
        .unwrap();
    definition["object_type"] = json!("active");
    definition["program"] = serde_json::to_value(change.expected()).unwrap();
    for command in definition["program"]["commands"].as_array_mut().unwrap() {
        if let Some(resource) = command.pointer_mut("/target/resource") {
            let logical = resource
                .as_object_mut()
                .unwrap()
                .remove("logical_subpage")
                .unwrap();
            resource["subpage"] = logical;
        }
    }
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}
fn intent(programs: Vec<TeletextResourceObjectProgramOverlay>) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        (vec![], programs),
    )
    .unwrap()
}
fn concept() -> Value {
    let change = call_change(true, 15);
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(
            &original_for(&change),
            &intent(vec![change]),
            budget(),
        )
        .unwrap()
        .bytes(),
    )
    .unwrap()
}
fn rejects(value: &Value) {
    assert!(decode_teletext_design_documents(&bytes(value), budget()).is_err());
}

#[test]
fn v16_invocation_roundtrips_both_resource_scopes_and_s1_with_exact_originals() {
    for global in [false, true] {
        for subpage in [0, 15] {
            let change = call_change(global, subpage);
            let original = original_for(&change);
            let before = decode_teletext_design_documents(&original, budget()).unwrap();
            let overlay = intent(vec![change]);
            assert_eq!(overlay.schema_version(), 16);
            let encoded =
                append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
            let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
            assert_eq!(after.editorial_overlay(), Some(&overlay));
            assert!(after
                .editorial_overlay()
                .unwrap()
                .resource_object_program_overlays()[0]
                .has_invocation_changes());
            assert_eq!(after.selection(), before.selection());
            assert_eq!(
                after.resource_transmissions(),
                before.resource_transmissions()
            );
            assert_eq!(after.edit_policy(), before.edit_policy());
            for (saved, root) in after.sources().iter().zip(before.sources()) {
                assert_eq!(saved.bytes(), root.bytes());
            }
            let reencoded = append_teletext_design_editorial_overlay(
                &original,
                after.editorial_overlay().unwrap(),
                budget(),
            )
            .unwrap();
            assert_eq!(reencoded.bytes(), encoded.bytes());
        }
    }
}

#[test]
fn v16_mixed_call_and_legacy_structure_restores_each_policy_without_widening_v15() {
    let calls = call_change(true, 15);
    let glyphs = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        program(vec![glyph(1)]),
        program(vec![glyph(1), glyph(2)]),
        vec![Some(0), None],
        budget().validation.service,
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![calls.clone(), glyphs.clone()]),
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![glyphs.clone()]),
        )
        .is_err()
    );
    let original = original_for(&calls);
    let overlay = intent(vec![calls, glyphs]);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.editorial_overlay(), Some(&overlay));
    let programs = restored
        .editorial_overlay()
        .unwrap()
        .resource_object_program_overlays();
    assert!(programs[0].has_invocation_changes());
    assert!(!programs[1].has_invocation_changes());
    let mut wire: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    for version in 1..=15 {
        wire["editorial_overlay"]["schema_version"] = json!(version);
        rejects(&wire);
    }
    let old = TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        (vec![], vec![programs[1].clone()]),
    )
    .unwrap();
    let old_encoded = append_teletext_design_editorial_overlay(&original, &old, budget()).unwrap();
    let restored = decode_teletext_design_documents(old_encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.editorial_overlay().unwrap().schema_version(), 15);
    assert_eq!(
        append_teletext_design_editorial_overlay(
            &original,
            restored.editorial_overlay().unwrap(),
            budget()
        )
        .unwrap()
        .bytes(),
        old_encoded.bytes()
    );
}

#[test]
fn v16_requires_changed_calls_and_closed_nonnull_original_call_fields() {
    let baseline = concept();
    for replacement in [Value::Null, json!([]), json!({})] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["resource_object_program_overlays"] = replacement;
        rejects(&value);
    }
    let mut value = baseline.clone();
    value["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("resource_object_program_overlays");
    rejects(&value);
    for pointer in [
        "/editorial_overlay/resource_object_program_overlays/0",
        "/editorial_overlay/resource_object_program_overlays/0/replacement/commands/2",
        "/editorial_overlay/resource_object_program_overlays/0/replacement/commands/2/target",
        "/editorial_overlay/resource_object_program_overlays/0/replacement/commands/2/offset",
    ] {
        for replacement in [Value::Null, json!([])] {
            let mut value = baseline.clone();
            *value.pointer_mut(pointer).unwrap() = replacement;
            // Null offset is a supported explicit origin, not absent metadata.
            if pointer.ends_with("/offset") && value.pointer(pointer).unwrap().is_null() {
                assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_ok());
            } else {
                rejects(&value);
            }
        }
        let mut value = baseline.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(1));
        rejects(&value);
    }
    let mut value = baseline.clone();
    value["editorial_overlay"]["resource_object_program_overlays"][0]["replacement"]["commands"][2]
        .as_object_mut()
        .unwrap()
        .remove("offset");
    rejects(&value);
    let mut value = baseline.clone();
    let program = value["editorial_overlay"]["resource_object_program_overlays"][0].clone();
    value["editorial_overlay"]["resource_object_program_overlays"]
        .as_array_mut()
        .unwrap()
        .push(program);
    rejects(&value);
    let mut value = baseline;
    value["editorial_overlay"]["resource_object_program_overlays"][0]["replacement"] =
        value["editorial_overlay"]["resource_object_program_overlays"][0]["expected"].clone();
    rejects(&value);
}

#[test]
fn v16_original_call_and_scalar_predecessors_remain_authenticated() {
    let baseline = concept();
    for (pointer,replacement) in [
        ("/editorial_overlay/resource_object_program_overlays/0/expected/commands/2/offset/column",json!(2)),
        ("/editorial_overlay/resource_object_program_overlays/0/identity/resource/logical_subpage",json!(1)),
        ("/editorial_overlay/resource_object_program_overlays/0/retained_commands",json!([0,1,null])),
        ("/editorial_overlay/resource_object_program_overlays/0/retained_commands",json!([0,1,1])),
    ] { let mut value = baseline.clone(); *value.pointer_mut(pointer).unwrap()=replacement; rejects(&value); }
    let calls = call_change(true, 15);
    let original = original_for(&calls);
    let mut expected = calls.expected().commands().to_vec();
    expected[0] = foreground(4);
    let mut replacement = calls.replacement().commands().to_vec();
    replacement[0] = foreground(4);
    let patched = TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        calls.identity(),
        program(expected),
        program(replacement),
        vec![Some(0), Some(1), Some(2)],
        budget().validation.service,
    )
    .unwrap();
    let scalar_operation = |colour| -> TeletextColumnOperation {
        serde_json::from_value(json!({"kind":"foreground","colour":colour})).unwrap()
    };
    let scalar = TeletextResourceObjectOperandOverlay::new(
        calls.identity(),
        0,
        1,
        scalar_operation(1),
        scalar_operation(4),
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        (vec![scalar], vec![patched.clone()]),
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    assert!(
        append_teletext_design_editorial_overlay(&original, &intent(vec![patched]), budget())
            .is_err()
    );
}

#[test]
fn v16_raw_command_map_and_target_budgets_are_shared_with_legacy_programs() {
    let calls = call_change(true, 15);
    let original = original_for(&calls);
    let glyphs = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        program(vec![glyph(1)]),
        program(vec![glyph(1), glyph(2)]),
        vec![Some(0), None],
        budget().validation.service,
    )
    .unwrap();
    let encoded =
        append_teletext_design_editorial_overlay(&original, &intent(vec![calls, glyphs]), budget())
            .unwrap();
    for (commands, items, objects) in [(8, 2048, 64), (2048, 8, 64), (2048, 2048, 1)] {
        let mut limited = budget();
        limited.validation.service.max_commands = commands;
        limited.validation.service.max_structure_items = items;
        limited.validation.service.max_objects = objects;
        let error = super::super::preflight_json(encoded.bytes(), limited).unwrap_err();
        assert!(matches!(
            error.reason,
            TeletextDesignDocumentFailure::Budget { .. }
        ));
    }
    let mut limited = budget();
    limited.max_serialized_bytes = encoded.bytes().len() - 1;
    assert!(decode_teletext_design_documents(encoded.bytes(), limited).is_err());
}

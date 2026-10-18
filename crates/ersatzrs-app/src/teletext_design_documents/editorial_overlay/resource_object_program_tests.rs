//! V15 restores exact resource structure without changing earlier private roles.
use super::resource_object_operand_tests::{identity, local_program, original};
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

fn operation(value: Value) -> TeletextColumnOperation {
    serde_json::from_value(value).unwrap()
}
fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: operation(json!({"kind":"g0","code":code})),
    }
}
fn foreground(colour: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: operation(json!({"kind":"foreground","colour":colour})),
    }
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn change(global: bool, s1: u8, variant: usize) -> TeletextResourceObjectProgramOverlay {
    let (before, after, map) = match variant {
        0 => (
            vec![glyph(1, 65)],
            vec![glyph(1, 65), glyph(2, 66)],
            vec![Some(0), None],
        ),
        1 => (
            vec![glyph(1, 65), glyph(2, 66)],
            vec![glyph(1, 65)],
            vec![Some(0)],
        ),
        2 => (vec![glyph(1, 65)], vec![glyph(2, 65)], vec![Some(0)]),
        3 => (
            vec![glyph(1, 65)],
            vec![foreground(2), glyph(1, 65)],
            vec![None, Some(0)],
        ),
        4 => (
            vec![foreground(2), glyph(1, 65)],
            vec![glyph(1, 65)],
            vec![Some(1)],
        ),
        _ => unreachable!(),
    };
    TeletextResourceObjectProgramOverlay::new(
        identity(global, s1),
        program(before),
        program(after),
        map,
        budget().validation.service,
    )
    .unwrap()
}
fn original_for(change: &TeletextResourceObjectProgramOverlay, mixed: bool) -> Vec<u8> {
    original_with_program(change.identity(), change.expected(), mixed)
}
pub(super) fn original_with_program(
    id: ersatzrs_domain::TeletextObjectIdentity,
    program: &TeletextEnhancementProgram,
    mixed: bool,
) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original(mixed)).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    let resource = id.resource().unwrap();
    let scope = if resource.kind() == ersatzrs_domain::TeletextResourceKind::Pop {
        "public"
    } else {
        "global"
    };
    let definition = source["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|definition| {
            definition["identity"]["scope"] == scope
                && definition["identity"]["resource"]["subpage"] == resource.logical_subpage().get()
        })
        .unwrap();
    definition["program"] = serde_json::to_value(program).unwrap();
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}
fn intent(
    change: TeletextResourceObjectProgramOverlay,
    mixed: bool,
) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        if mixed { vec![local_program()] } else { vec![] },
        (vec![], vec![change]),
    )
    .unwrap()
}
fn concept() -> Value {
    let change = change(false, 15, 0);
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(
            &original_for(&change, false),
            &intent(change, false),
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
fn v15_resource_structure_roundtrips_both_scopes_s1_and_glyph_style_variants() {
    for global in [false, true] {
        for s1 in [0, 15] {
            for variant in 0..5 {
                let change = change(global, s1, variant);
                let original = original_for(&change, false);
                let before = decode_teletext_design_documents(&original, budget()).unwrap();
                let overlay = intent(change, false);
                let encoded =
                    append_teletext_design_editorial_overlay(&original, &overlay, budget())
                        .unwrap();
                let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
                assert_eq!(after.editorial_overlay(), Some(&overlay));
                assert_eq!(overlay.schema_version(), 15);
                assert_eq!(overlay.resource_object_program_overlays().len(), 1);
                assert_eq!(after.selection(), before.selection());
                assert_eq!(after.guide_source(), before.guide_source());
                assert_eq!(
                    after.resource_transmissions(),
                    before.resource_transmissions()
                );
                assert_eq!(after.edit_policy(), before.edit_policy());
                for (saved, root) in after.sources().iter().zip(before.sources()) {
                    assert_eq!(saved.bytes(), root.bytes());
                }
                let end = original
                    .iter()
                    .rposition(|byte| !byte.is_ascii_whitespace())
                    .unwrap();
                assert_eq!(&encoded.bytes()[..end], &original[..end]);
                assert!(encoded.bytes().ends_with(&original[end..]));
                for contract in [
                    ersatzrs_domain::TeletextPageContract::V1,
                    ersatzrs_domain::TeletextPageContract::V2,
                    ersatzrs_domain::TeletextPageContract::V3,
                ] {
                    assert_eq!(
                        after
                            .guide_inputs()
                            .and_then(|input| input.request(contract)),
                        before
                            .guide_inputs()
                            .and_then(|input| input.request(contract))
                    );
                }
                let debug = format!("{overlay:?}");
                assert!(!debug.contains(&"d".repeat(64)));
                assert!(!debug.contains("expected"));
            }
        }
    }
}
#[test]
fn v15_supports_combined_local_resource_programs_and_large_logical_programs_under_host_budgets() {
    let change = change(false, 15, 0);
    let original = original_for(&change, true);
    let overlay = intent(change, true);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    let expected = program(vec![glyph(1, 65)]);
    let replacement = program(vec![glyph(1, 65); 520]);
    let mut map = vec![None; 520];
    map[0] = Some(0);
    let large = TeletextResourceObjectProgramOverlay::new(
        identity(true, 0),
        expected,
        replacement,
        map,
        budget().validation.service,
    )
    .unwrap();
    let original = original_for(&large, false);
    let overlay = intent(large, false);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
}
#[test]
fn v15_requires_nonempty_structural_intent_and_preserves_old_v14_bytes_without_new_fields() {
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![])
        )
        .is_err()
    );
    let change = change(false, 15, 0);
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![change.clone(), change])
        )
        .is_err()
    );
    let scalar_only = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        program(vec![glyph(1, 65)]),
        program(vec![glyph(1, 66)]),
        vec![Some(0)],
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
            (vec![], vec![scalar_only])
        )
        .is_err(),
        "V15 requires resource structural intent rather than aliasing V14"
    );
    let baseline = concept();
    for version in 1..=14 {
        let mut value = baseline.clone();
        value["editorial_overlay"]["schema_version"] = json!(version);
        rejects(&value);
    }
    for replacement in [Value::Null, json!([]), json!({})] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["resource_object_program_overlays"] = replacement;
        rejects(&value);
    }
    let mut missing = baseline;
    missing["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("resource_object_program_overlays");
    rejects(&missing);
    let scalar = TeletextResourceObjectOperandOverlay::new(
        identity(false, 15),
        0,
        1,
        operation(json!({"kind":"g0","code":65})),
        operation(json!({"kind":"g0","code":66})),
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_operands(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![scalar],
    )
    .unwrap();
    let original = original(false);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    assert_eq!(value["editorial_overlay"]["schema_version"], json!(14));
    assert!(value["editorial_overlay"]
        .get("resource_object_program_overlays")
        .is_none());
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    let reencoded = append_teletext_design_editorial_overlay(
        &original,
        restored.editorial_overlay().unwrap(),
        budget(),
    )
    .unwrap();
    assert_eq!(reencoded.bytes(), encoded.bytes());
}
#[test]
fn v15_resource_program_maps_identities_and_programs_are_closed_and_nonnull() {
    let baseline = concept();
    for pointer in [
        "/editorial_overlay/resource_object_program_overlays/0",
        "/editorial_overlay/resource_object_program_overlays/0/identity",
        "/editorial_overlay/resource_object_program_overlays/0/identity/resource",
        "/editorial_overlay/resource_object_program_overlays/0/identity/resource/page",
        "/editorial_overlay/resource_object_program_overlays/0/expected",
        "/editorial_overlay/resource_object_program_overlays/0/replacement",
        "/editorial_overlay/resource_object_program_overlays/0/expected/commands/0",
    ] {
        for replacement in [Value::Null, json!([])] {
            let mut value = baseline.clone();
            *value.pointer_mut(pointer).unwrap() = replacement;
            rejects(&value);
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
    for (field, replacement) in [
        ("retained_commands", Value::Null),
        ("retained_commands", json!({})),
        ("retained_commands", json!([0, 0])),
        ("retained_commands", json!([1, null])),
        ("retained_commands", json!([0])),
        (
            "identity",
            json!({"scope":"local","owner":{"page":200,"subcode":16},"object_id":65520}),
        ),
    ] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["resource_object_program_overlays"][0][field] = replacement;
        rejects(&value);
    }
    // Editable originals may be deleted and replaced by newly inserted glyphs.
    let mut inserted = baseline.clone();
    inserted["editorial_overlay"]["resource_object_program_overlays"][0]["retained_commands"] =
        json!([null, null]);
    let restored = decode_teletext_design_documents(&bytes(&inserted), budget()).unwrap();
    let original = decode_teletext_design_documents(&bytes(&baseline), budget()).unwrap();
    for (saved, root) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(saved.bytes(), root.bytes());
    }
    assert_eq!(
        restored
            .editorial_overlay()
            .unwrap()
            .resource_object_program_overlays()[0]
            .retained_commands(),
        [None, None]
    );
}
#[test]
fn v15_program_expected_skeleton_authenticates_full_original_and_ambiguous_owners() {
    let baseline = concept();
    for (pointer,replacement) in [
        ("/editorial_overlay/resource_object_program_overlays/0/identity/resource/logical_subpage",json!(1)),
        ("/editorial_overlay/resource_object_program_overlays/0/identity/object_id",json!(65519)),
        ("/editorial_overlay/resource_object_program_overlays/0/expected/commands/0/operation/code",json!(67)),
        ("/editorial_overlay/resource_object_program_overlays/0/expected/commands/0/column",json!(2)),
    ] {let mut value=baseline.clone();*value.pointer_mut(pointer).unwrap()=replacement;rejects(&value);}
    let change = change(false, 15, 0);
    let original = original_for(&change, false);
    let restored = decode_teletext_design_documents(&original, budget()).unwrap();
    let super::TeletextProfileDocument::Authored(service) = &restored.documents()[0] else {
        panic!("authored fixture")
    };
    let documents = vec![
        super::TeletextProfileDocument::Authored(service.clone()),
        super::TeletextProfileDocument::Authored(service.clone()),
    ];
    assert!(super::resource_object_programs::validate_association(
        &intent(change, false),
        &documents,
        budget()
    )
    .is_err());
}
#[test]
fn v15_prior_resource_scalars_must_match_original_then_the_structural_expected_program() {
    let root = program(vec![foreground(1), glyph(1, 65)]);
    let original = original_with_program(identity(false, 15), &root, false);
    let scalar = TeletextResourceObjectOperandOverlay::new(
        identity(false, 15),
        0,
        1,
        operation(json!({"kind":"foreground","colour":1})),
        operation(json!({"kind":"foreground","colour":4})),
    )
    .unwrap();
    let change = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        program(vec![foreground(4), glyph(1, 65)]),
        program(vec![foreground(4), glyph(1, 65), glyph(2, 66)]),
        vec![Some(0), Some(1), None],
        budget().validation.service,
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        (vec![scalar], vec![change.clone()]),
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    let without = intent(change, false);
    assert!(append_teletext_design_editorial_overlay(&original, &without, budget()).is_err());
    let glyph_scalar = TeletextResourceObjectOperandOverlay::new(
        identity(false, 15),
        1,
        1,
        operation(json!({"kind":"g0","code":65})),
        operation(json!({"kind":"g0","code":67})),
    )
    .unwrap();
    let change = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        root.clone(),
        program(vec![foreground(1), glyph(1, 65), glyph(2, 66)]),
        vec![Some(0), Some(1), None],
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
            (vec![glyph_scalar], vec![change])
        )
        .is_err()
    );
}
#[test]
fn v15_aggregate_program_commands_maps_and_targets_are_preflighted_before_owned_decode() {
    let change = change(false, 15, 0);
    let original = original_for(&change, true);
    let overlay = intent(change, true);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    for (commands, items, objects) in [(4, 2048, 64), (2048, 8, 64), (2048, 2048, 1)] {
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
#[test]
fn v15_final_total_command_budget_includes_unchanged_objects_and_new_program_results() {
    let expected = program(vec![glyph(1, 65)]);
    let mut replacement = vec![glyph(1, 65)];
    replacement.extend([glyph(2, 66); 4]);
    let change = TeletextResourceObjectProgramOverlay::new(
        identity(false, 15),
        expected,
        program(replacement),
        vec![Some(0), None, None, None, None],
        budget().validation.service,
    )
    .unwrap();
    let original = original_for(&change, false);
    let overlay = intent(change, false);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let mut limited = budget();
    limited.validation.service.max_commands = 6;
    assert!(decode_teletext_design_documents(&original, limited).is_ok());
    assert!(
        super::super::preflight_json(encoded.bytes(), limited).is_ok(),
        "raw expected+replacement input fits before the complete result is counted"
    );
    let error = decode_teletext_design_documents(encoded.bytes(), limited).unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial result commands",
            ..
        }
    ));
    assert!(append_teletext_design_editorial_overlay(&original, &overlay, limited).is_err());
}

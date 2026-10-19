//! V20 retained Local calls and V21 call structure keep distinct storage roles.

use super::resource_object_addition_tests::addition;
use super::resource_object_operand_tests::{identity as resource_id, local_program, original};
use super::tests::{address, budget, bytes, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementPosition,
    TeletextEnhancementProgram as Program, TeletextLocalObjectGlyphProgramOverlay,
    TeletextLocalObjectOperandOverlay, TeletextObjectId, TeletextObjectIdentity,
    TeletextResourceObjectOperandOverlay, TeletextResourceObjectProgramOverlay,
};
use serde_json::{json, Value};

fn id() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65522))
}
fn position(row: u8) -> Command {
    Command::SetPosition {
        position: TeletextEnhancementPosition::new(row, 8).unwrap(),
    }
}
fn glyph(column: u8, code: u8) -> Command {
    serde_json::from_value(json!({"kind":"at_column","column":column,
        "operation":{"kind":"g0","code":code}}))
    .unwrap()
}
fn program(commands: Vec<Command>) -> Program {
    Program::new(commands).unwrap()
}
fn call(offset: u8) -> Command {
    serde_json::from_value(
        json!({"kind":"invoke_object","target":local_program().identity(),
        "object_type":"passive","offset":{"row":0,"column":offset}}),
    )
    .unwrap()
}
fn change() -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
        id(),
        program(vec![position(0), call(0)]),
        program(vec![position(0), call(1)]),
        vec![Some(0), Some(1)],
    )
    .unwrap()
}
fn original_for(resources: &[TeletextResourceObjectProgramOverlay]) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original(true)).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    source["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .push(json!({"identity":id(),"object_type":"active","program":change().expected()}));
    for change in resources {
        let resource = change.identity().resource().unwrap();
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
                    && definition["identity"]["resource"]["subpage"]
                        == resource.logical_subpage().get()
            })
            .unwrap();
        definition["program"] = serde_json::to_value(change.expected()).unwrap();
        if change
            .expected()
            .commands()
            .iter()
            .chain(change.replacement().commands())
            .any(|command| matches!(command, Command::InvokeObject { .. }))
        {
            definition["object_type"] = json!("active");
        }
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
    }
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}
fn pure() -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_local_object_invocation_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![change()],
    )
    .unwrap()
}
fn saved(
    overlay: &TeletextSavedEditorialOverlay,
    resources: &[TeletextResourceObjectProgramOverlay],
) -> Vec<u8> {
    append_teletext_design_editorial_overlay(&original_for(resources), overlay, budget())
        .unwrap()
        .bytes()
        .to_vec()
}

#[test]
fn v20_retained_local_offset_roundtrips_exact_original_and_minimal_fields() {
    let original = original_for(&[]);
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    let overlay = pure();
    assert_eq!(overlay.schema_version(), 20);
    let encoded = saved(&overlay, &[]);
    let after = decode_teletext_design_documents(&encoded, budget()).unwrap();
    assert_eq!(after.editorial_overlay(), Some(&overlay));
    assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
    assert_eq!(after.selection(), before.selection());
    assert_eq!(
        after.resource_transmissions(),
        before.resource_transmissions()
    );
    let wire: Value = serde_json::from_slice(&encoded).unwrap();
    assert!(wire["editorial_overlay"]
        .get("resource_object_program_overlays")
        .is_none());
    assert_eq!(saved(after.editorial_overlay().unwrap(), &[]), encoded);
}

#[test]
fn v20_local_calls_cannot_enter_any_older_constructor_or_version() {
    for constructor in [
        TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs,
        TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs,
        TeletextSavedEditorialOverlay::new_with_local_object_style_programs,
    ] {
        assert!(constructor(provenance(), vec![], vec![], vec![], vec![], vec![change()]).is_err());
    }
    let mut wire: Value = serde_json::from_slice(&saved(&pure(), &[])).unwrap();
    for version in 1..=19 {
        wire["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&wire), budget()).is_err());
    }
    let scalar = TeletextResourceObjectOperandOverlay::new(
        resource_id(false, 15),
        0,
        1,
        serde_json::from_value(json!({"kind":"g0","code":65})).unwrap(),
        serde_json::from_value(json!({"kind":"g0","code":66})).unwrap(),
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_operands(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![change()],
            vec![scalar]
        )
        .is_err()
    );
}

#[test]
fn v20_requires_call_delta_and_rejects_duplicate_overlap_and_stale_original() {
    let overlap = TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
        id(),
        program(vec![glyph(0, 65), call(0)]),
        program(vec![glyph(0, 65), call(1)]),
        vec![Some(0), Some(1)],
    )
    .unwrap();
    let scalar = TeletextLocalObjectOperandOverlay::new(
        id(),
        0,
        0,
        serde_json::from_value(json!({"kind":"g0","code":65})).unwrap(),
        serde_json::from_value(json!({"kind":"g0","code":66})).unwrap(),
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_invocation_programs(
            provenance(),
            vec![],
            vec![],
            vec![scalar],
            vec![],
            vec![overlap],
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_invocation_programs(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![local_program()]
        )
        .is_err()
    );
    assert!(pure()
        .with_local_object_invocation_programs(vec![change()])
        .is_err());
    let mut wire: Value = serde_json::from_slice(&saved(&pure(), &[])).unwrap();
    wire["editorial_overlay"]["local_object_glyph_program_overlays"][0]["expected"]["commands"]
        [1]["offset"]["column"] = json!(2);
    assert!(decode_teletext_design_documents(&bytes(&wire), budget()).is_err());
    let old = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![local_program()],
    )
    .unwrap();
    let encoded =
        append_teletext_design_editorial_overlay(&original_for(&[]), &old, budget()).unwrap();
    let mut alias: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    alias["editorial_overlay"]["schema_version"] = json!(20);
    assert!(decode_teletext_design_documents(&bytes(&alias), budget()).is_err());
    let mut empty: Value = serde_json::from_slice(&saved(&pure(), &[])).unwrap();
    empty["editorial_overlay"]["resource_object_operands"] = json!([]);
    assert!(decode_teletext_design_documents(&bytes(&empty), budget()).is_err());
}

fn resource_positions(transient: bool) -> TeletextResourceObjectProgramOverlay {
    let constructor = if transient {
        TeletextResourceObjectProgramOverlay::new_with_inserted_positions
    } else {
        TeletextResourceObjectProgramOverlay::new_with_original_positions
    };
    constructor(
        resource_id(false, 15),
        program(vec![position(1), glyph(8, 65)]),
        program(vec![position(1), glyph(8, 65), position(2), glyph(8, 66)]),
        vec![Some(0), Some(1), None, None],
        budget().validation.service,
    )
    .unwrap()
}

#[test]
fn v20_preserves_local_resource_positions_and_additions_in_both_attachment_orders() {
    let resource = resource_positions(false);
    let sibling = TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![local_program()],
        (vec![], vec![resource.clone()]),
    )
    .unwrap();
    let additions = vec![addition(true, 0)];
    let before = sibling
        .clone()
        .with_resource_object_definition_additions(additions.clone())
        .unwrap()
        .with_local_object_invocation_programs(vec![change()])
        .unwrap();
    let after = sibling
        .with_local_object_invocation_programs(vec![change()])
        .unwrap()
        .with_resource_object_definition_additions(additions.clone())
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(before.schema_version(), 20);
    assert_eq!(
        before.local_object_glyph_program_overlays(),
        &[local_program(), change()]
    );
    assert_eq!(
        before.resource_object_program_overlays(),
        std::slice::from_ref(&resource)
    );
    assert_eq!(before.resource_object_definition_additions(), additions);
    let encoded = saved(&before, &[resource]);
    assert_eq!(
        decode_teletext_design_documents(&encoded, budget())
            .unwrap()
            .editorial_overlay(),
        Some(&before)
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            (vec![], vec![resource_positions(true)]),
        )
        .is_err(),
        "transient Add authority cannot enter a mixed persisted original"
    );
}

#[test]
fn v20_retained_calls_keep_ordinary_style_program_siblings_closed_and_complete() {
    let expected = local_program().expected().clone();
    let foreground = serde_json::from_value(json!({"kind":"at_column","column":0,
        "operation":{"kind":"foreground","colour":2}}))
    .unwrap();
    let style = TeletextLocalObjectGlyphProgramOverlay::new_with_style_commands(
        local_program().identity(),
        expected.clone(),
        program(vec![foreground, expected.commands()[0]]),
        vec![None, Some(0)],
    )
    .unwrap();
    let old = TeletextSavedEditorialOverlay::new_with_local_object_style_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![style.clone()],
    )
    .unwrap();
    let overlay = old
        .with_local_object_invocation_programs(vec![change()])
        .unwrap();
    let encoded = saved(&overlay, &[]);
    assert_eq!(
        decode_teletext_design_documents(&encoded, budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        &[style, change()]
    );
}

#[test]
fn v20_preserves_ordinary_retained_and_structural_resource_call_siblings() {
    let resource_call = |offset| {
        serde_json::from_value(json!({"kind":"invoke_object",
        "target":resource_id(true,0),"object_type":"passive","offset":{"row":0,"column":offset}}))
        .unwrap()
    };
    for variant in 0..3 {
        let constructor = match variant {
            0 => TeletextResourceObjectProgramOverlay::new,
            1 => TeletextResourceObjectProgramOverlay::new_with_invocation_changes,
            _ => TeletextResourceObjectProgramOverlay::new_with_invocation_structure,
        };
        let (expected, replacement, map) = match variant {
            0 => (
                program(vec![glyph(1, 65)]),
                program(vec![glyph(1, 65), glyph(2, 66)]),
                vec![Some(0), None],
            ),
            1 => (
                program(vec![resource_call(0)]),
                program(vec![resource_call(1)]),
                vec![Some(0)],
            ),
            _ => (
                program(vec![glyph(1, 65)]),
                program(vec![glyph(1, 65), resource_call(0)]),
                vec![Some(0), None],
            ),
        };
        let resource = constructor(
            resource_id(false, 15),
            expected,
            replacement,
            map,
            budget().validation.service,
        )
        .unwrap();
        let constructor = match variant {
            0 => TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays,
            1 => TeletextSavedEditorialOverlay::new_with_resource_object_invocation_overlays,
            _ => TeletextSavedEditorialOverlay::new_with_resource_object_invocation_structure_overlays,
        };
        let sibling = constructor(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![local_program()],
            (vec![], vec![resource.clone()]),
        )
        .unwrap();
        let overlay = sibling
            .with_local_object_invocation_programs(vec![change()])
            .unwrap();
        let encoded = saved(&overlay, &[resource]);
        assert_eq!(
            decode_teletext_design_documents(&encoded, budget())
                .unwrap()
                .editorial_overlay(),
            Some(&overlay)
        );
    }
}

#[test]
fn v20_existing_serialized_and_aggregate_budgets_bound_new_local_call_intent() {
    let original = original_for(&[]);
    let overlay = pure();
    let encoded = saved(&overlay, &[]);
    let mut limited = budget();
    limited.max_serialized_bytes = encoded.len() - 1;
    assert!(append_teletext_design_editorial_overlay(&original, &overlay, limited).is_err());
    for commands in [true, false] {
        let mut limited = budget();
        if commands {
            limited.validation.service.max_commands = 3;
        } else {
            limited.validation.service.max_structure_items = 3;
        }
        assert!(append_teletext_design_editorial_overlay(&original, &overlay, limited).is_err());
        assert!(decode_teletext_design_documents(&encoded, limited).is_err());
    }
}

fn structure(delete: bool, retained_change: bool) -> TeletextLocalObjectGlyphProgramOverlay {
    let (replacement, map) = if delete {
        (program(vec![position(0)]), vec![Some(0)])
    } else {
        (
            program(vec![position(0), call(u8::from(retained_change)), call(2)]),
            vec![Some(0), Some(1), None],
        )
    };
    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_structure(
        id(),
        change().expected().clone(),
        replacement,
        map,
    )
    .unwrap()
}

fn structural(overlay: TeletextLocalObjectGlyphProgramOverlay) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_local_object_invocation_structure_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![overlay],
    )
    .unwrap()
}

#[test]
fn v21_local_call_insertions_and_deletions_roundtrip_without_changing_original_bytes() {
    let original = original_for(&[]);
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    for change in [
        structure(false, false),
        structure(false, true),
        structure(true, false),
    ] {
        let overlay = structural(change);
        assert_eq!(overlay.schema_version(), 21);
        let encoded = saved(&overlay, &[]);
        let restored = decode_teletext_design_documents(&encoded, budget()).unwrap();
        assert_eq!(restored.editorial_overlay(), Some(&overlay));
        assert_eq!(restored.sources()[0].bytes(), before.sources()[0].bytes());
        assert_eq!(restored.selection(), before.selection());
        assert_eq!(
            restored.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(saved(restored.editorial_overlay().unwrap(), &[]), encoded);
    }
}

#[test]
fn v21_local_call_structure_cannot_alias_retained_only_or_earlier_storage_roles() {
    let structural_change = structure(false, true);
    assert!(structural_change.has_invocation_changes());
    assert!(structural_change.has_invocation_structure_changes());
    for constructor in [
        TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs,
        TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs,
        TeletextSavedEditorialOverlay::new_with_local_object_style_programs,
        TeletextSavedEditorialOverlay::new_with_local_object_invocation_programs,
    ] {
        assert!(constructor(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![structural_change.clone()]
        )
        .is_err());
    }
    let mut wire: Value =
        serde_json::from_slice(&saved(&structural(structural_change.clone()), &[])).unwrap();
    for version in 1..=20 {
        wire["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&wire), budget()).is_err());
    }
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_invocation_structure_programs(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![change()]
        )
        .is_err()
    );
    let mut old: Value = serde_json::from_slice(&saved(&pure(), &[])).unwrap();
    old["editorial_overlay"]["schema_version"] = json!(21);
    assert!(decode_teletext_design_documents(&bytes(&old), budget()).is_err());
    assert!(structural(structural_change)
        .with_local_object_invocation_programs(vec![retained_sibling()])
        .is_err());
}

fn retained_sibling() -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
        TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65523)),
        change().expected().clone(),
        change().replacement().clone(),
        vec![Some(0), Some(1)],
    )
    .unwrap()
}

fn original_with_retained_sibling(resources: &[TeletextResourceObjectProgramOverlay]) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original_for(resources)).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    let sibling = retained_sibling();
    source["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "identity":sibling.identity(),"object_type":"active","program":sibling.expected()
        }));
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}

#[test]
fn v21_preserves_mixed_local_calls_resource_positions_and_additions_in_both_orders() {
    let resource = resource_positions(false);
    let sibling = TeletextSavedEditorialOverlay::new_with_resource_object_position_overlays(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![local_program()],
        (vec![], vec![resource.clone()]),
    )
    .unwrap();
    let calls = vec![retained_sibling(), structure(false, true)];
    let additions = vec![addition(true, 0)];
    let before = sibling
        .clone()
        .with_resource_object_definition_additions(additions.clone())
        .unwrap()
        .with_local_object_invocation_structure_programs(calls.clone())
        .unwrap();
    let after = sibling
        .with_local_object_invocation_structure_programs(calls.clone())
        .unwrap()
        .with_resource_object_definition_additions(additions.clone())
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(before.schema_version(), 21);
    assert_eq!(
        before.local_object_glyph_program_overlays(),
        &[local_program(), calls[0].clone(), calls[1].clone()]
    );
    assert_eq!(
        before.resource_object_program_overlays(),
        std::slice::from_ref(&resource)
    );
    assert_eq!(before.resource_object_definition_additions(), additions);
    let original = original_with_retained_sibling(&[resource]);
    let encoded = append_teletext_design_editorial_overlay(&original, &before, budget()).unwrap();
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.editorial_overlay(), Some(&before));
    assert_eq!(
        append_teletext_design_editorial_overlay(
            &original,
            restored.editorial_overlay().unwrap(),
            budget()
        )
        .unwrap()
        .bytes(),
        encoded.bytes()
    );
}

#[test]
fn v21_requires_distinct_originals_and_preserves_saved_payload_and_structure_budgets() {
    let conflicting_program =
        TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_structure(
            id(),
            program(vec![glyph(0, 65), call(0)]),
            program(vec![glyph(0, 65), call(0), call(1)]),
            vec![Some(0), Some(1), None],
        )
        .unwrap();
    let scalar = TeletextLocalObjectOperandOverlay::new(
        id(),
        0,
        0,
        serde_json::from_value(json!({"kind":"g0","code":65})).unwrap(),
        serde_json::from_value(json!({"kind":"g0","code":66})).unwrap(),
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_invocation_structure_programs(
            provenance(),
            vec![],
            vec![],
            vec![scalar],
            vec![],
            vec![conflicting_program],
        )
        .is_err()
    );
    let change = structure(false, false);
    assert!(structural(change.clone())
        .with_local_object_invocation_structure_programs(vec![change.clone()])
        .is_err());
    let overlay = structural(change);
    let original = original_for(&[]);
    let encoded = saved(&overlay, &[]);
    let mut stale: Value = serde_json::from_slice(&encoded).unwrap();
    stale["editorial_overlay"]["local_object_glyph_program_overlays"][0]["expected"]["commands"]
        [1]["offset"]["column"] = json!(2);
    assert!(decode_teletext_design_documents(&bytes(&stale), budget()).is_err());
    let mut empty: Value = serde_json::from_slice(&encoded).unwrap();
    empty["editorial_overlay"]["resource_object_operands"] = json!([]);
    assert!(decode_teletext_design_documents(&bytes(&empty), budget()).is_err());
    let mut limited = budget();
    limited.max_serialized_bytes = encoded.len() - 1;
    assert!(append_teletext_design_editorial_overlay(&original, &overlay, limited).is_err());
    for commands in [true, false] {
        let mut limited = budget();
        if commands {
            limited.validation.service.max_commands = 3;
        } else {
            limited.validation.service.max_structure_items = 3;
        }
        assert!(append_teletext_design_editorial_overlay(&original, &overlay, limited).is_err());
        assert!(decode_teletext_design_documents(&encoded, limited).is_err());
    }
}

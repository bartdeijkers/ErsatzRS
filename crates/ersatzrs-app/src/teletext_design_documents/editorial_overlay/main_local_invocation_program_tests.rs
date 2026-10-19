//! Private22 Main structure preserves sealed source bytes and sibling storage roles.
use super::resource_object_operand_tests::{local_program, original as resource_original};
use super::tests::{address, budget, bytes, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextEnhancementCommand as Command, TeletextEnhancementProgram as Program,
    TeletextLocalObjectGlyphProgramOverlay, TeletextLocalProgramOperandOverlay,
    TeletextMainLocalInvocationProgramOverlay as Overlay, TeletextObjectId, TeletextObjectIdentity,
};
use serde_json::{json, Value};
fn program(commands: Vec<Command>) -> Program {
    Program::new(commands).unwrap()
}
fn call() -> Command {
    serde_json::from_value(json!({"kind":"invoke_object","target":local_program().identity(),"object_type":"passive","offset":null})).unwrap()
}
fn colour(value: u8) -> Command {
    serde_json::from_value(
        json!({"kind":"at_column","column":0,"operation":{"kind":"foreground","colour":value}}),
    )
    .unwrap()
}
fn main(patched: bool, delete: bool) -> Overlay {
    let expected = vec![colour(if patched { 2 } else { 1 }), call()];
    let (replacement, map) = if delete {
        (vec![expected[0]], vec![Some(0)])
    } else {
        (
            vec![expected[0], call(), call()],
            vec![Some(0), Some(1), None],
        )
    };
    Overlay::new(address(16), program(expected), program(replacement), map).unwrap()
}
fn wrapper() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65522))
}
fn sibling() -> TeletextLocalObjectGlyphProgramOverlay {
    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_structure(
        wrapper(),
        program(vec![call()]),
        program(vec![call(), call()]),
        vec![Some(0), None],
    )
    .unwrap()
}
fn original() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&resource_original(true)).unwrap();
    let mut response: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    response["service"]["page_enhancements"][0]["local_program"] =
        json!(main(false, false).expected());
    response["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .push(json!({"identity":wrapper(),"object_type":"active","program":sibling().expected()}));
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    bytes(&value)
}
fn saved(value: &TeletextSavedEditorialOverlay) -> Vec<u8> {
    append_teletext_design_editorial_overlay(&original(), value, budget())
        .unwrap()
        .bytes()
        .to_vec()
}
fn overlay(patched: bool, delete: bool) -> TeletextSavedEditorialOverlay {
    let operands = if patched {
        vec![TeletextLocalProgramOperandOverlay::new(
            address(16),
            0,
            0,
            serde_json::from_value(json!({"kind":"foreground","colour":1})).unwrap(),
            serde_json::from_value(json!({"kind":"foreground","colour":2})).unwrap(),
        )
        .unwrap()]
    } else {
        vec![]
    };
    TeletextSavedEditorialOverlay::new_with_main_local_invocation_programs(
        provenance(),
        vec![],
        operands,
        vec![main(patched, delete)],
    )
    .unwrap()
}
#[test]
fn v22_main_insert_delete_roundtrip_authenticates_scalar_before_main_and_preserves_originals() {
    let original = original();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    for patched in [false, true] {
        for delete in [false, true] {
            let value = overlay(patched, delete);
            assert_eq!(value.schema_version(), 22);
            assert_eq!(
                value.main_local_invocation_program_overlays(),
                [main(patched, delete)]
            );
            let encoded = saved(&value);
            let restored = decode_teletext_design_documents(&encoded, budget()).unwrap();
            assert_eq!(restored.editorial_overlay(), Some(&value));
            assert_eq!(restored.selection(), before.selection());
            assert_eq!(restored.sources()[0].bytes(), before.sources()[0].bytes());
            assert_eq!(
                restored.resource_transmissions(),
                before.resource_transmissions()
            );
            assert_eq!(saved(restored.editorial_overlay().unwrap()), encoded);
        }
    }
}
#[test]
fn v22_main_composes_local21_glyph_siblings_and_resource_additions_without_downgrade() {
    let body = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![local_program()],
    )
    .unwrap();
    let first = body
        .clone()
        .with_local_object_invocation_structure_programs(vec![sibling()])
        .unwrap()
        .with_main_local_invocation_programs(vec![main(false, false)])
        .unwrap();
    let second = body
        .with_main_local_invocation_programs(vec![main(false, false)])
        .unwrap()
        .with_local_object_invocation_structure_programs(vec![sibling()])
        .unwrap();
    assert_eq!(first, second);
    let additions = vec![super::resource_object_addition_tests::addition(true, 0)];
    let before = first
        .clone()
        .with_resource_object_definition_additions(additions.clone())
        .unwrap();
    assert_eq!(before.schema_version(), 22);
    let reversed = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![local_program()],
    )
    .unwrap()
    .with_resource_object_definition_additions(additions)
    .unwrap()
    .with_local_object_invocation_structure_programs(vec![sibling()])
    .unwrap()
    .with_main_local_invocation_programs(vec![main(false, false)])
    .unwrap();
    assert_eq!(before, reversed);
    let encoded = saved(&before);
    assert_eq!(
        decode_teletext_design_documents(&encoded, budget())
            .unwrap()
            .editorial_overlay(),
        Some(&before)
    );
}
#[test]
fn v22_main_keeps_older_versions_closed_and_rejects_empty_null_unknown_or_false_maps() {
    let value: Value = serde_json::from_slice(&saved(&overlay(false, false))).unwrap();
    for version in 1..=21 {
        let mut changed = value.clone();
        changed["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    for bad in [json!([]), json!(null)] {
        let mut changed = value.clone();
        changed["editorial_overlay"]["main_local_invocation_program_overlays"] = bad;
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    for field in ["retained_commands", "unknown"] {
        let mut changed = value.clone();
        changed["editorial_overlay"]["main_local_invocation_program_overlays"][0][field] =
            json!([0, 0, null]);
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    let old = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![local_program()],
    )
    .unwrap();
    let mut changed: Value = serde_json::from_slice(&saved(&old)).unwrap();
    assert!(changed["editorial_overlay"]
        .get("main_local_invocation_program_overlays")
        .is_none());
    changed["editorial_overlay"]["schema_version"] = json!(22);
    assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
}
#[test]
fn v22_main_rejects_stale_original_unknown_target_and_duplicate_owner_at_attachment() {
    assert!(overlay(false, false)
        .with_main_local_invocation_programs(vec![main(false, true)])
        .is_err());
    let missing_scalar = TeletextSavedEditorialOverlay::new_with_main_local_invocation_programs(
        provenance(),
        vec![],
        vec![],
        vec![main(true, false)],
    )
    .unwrap();
    assert!(
        append_teletext_design_editorial_overlay(&original(), &missing_scalar, budget()).is_err()
    );
    let value: Value = serde_json::from_slice(&saved(&overlay(false, false))).unwrap();
    for path in ["expected", "replacement"] {
        let mut changed = value.clone();
        let ordinal = if path == "replacement" { 2 } else { 1 };
        changed["editorial_overlay"]["main_local_invocation_program_overlays"][0][path]
            ["commands"][ordinal]["target"]["object_id"] = json!(65519);
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    let mut changed = value;
    changed["editorial_overlay"]["main_local_invocation_program_overlays"][0]["replacement"]
        ["commands"][2]["object_type"] = json!("active");
    assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
}
#[test]
fn v22_main_streaming_preflight_and_aggregate_command_structure_budgets_stay_bounded() {
    let value = overlay(false, false);
    let encoded = saved(&value);
    for which in 0..4 {
        let mut limited = budget();
        match which {
            0 => limited.max_serialized_bytes = encoded.len() - 1,
            1 => limited.validation.service.max_records = 0,
            2 => limited.validation.service.max_commands = 4,
            _ => limited.validation.service.max_structure_items = 4,
        }
        assert!(decode_teletext_design_documents(&encoded, limited).is_err());
        assert!(append_teletext_design_editorial_overlay(&original(), &value, limited).is_err());
    }
}

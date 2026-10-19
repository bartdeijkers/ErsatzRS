use super::tests::{address, budget, bytes, original_value, provenance};
use super::*;
use ersatzrs_domain::{TeletextBaseTextEdit, TeletextEnhancementPosition};
use serde_json::{json, Value};

fn original(v3: bool) -> Vec<u8> {
    let mut root = original_value(v3);
    let mut response: Value =
        serde_json::from_str(root["documents"][0]["response"].as_str().unwrap()).unwrap();
    let base = if v3 {
        &mut response["service"]
    } else {
        &mut response
    };
    for page in base["pages"].as_array_mut().unwrap() {
        page["rows"] = json!([{"row":1,"runs":[{"column":4,
            "style":{"foreground":"white","background":"black","height":"normal","boxed":false,"flash":false,"conceal":false},
            "content":{"kind":"text","text":"£ŁD"}}]}]);
    }
    root["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    bytes(&root)
}
fn edit() -> TeletextBaseTextEdit {
    TeletextBaseTextEdit::new(
        address(16),
        TeletextEnhancementPosition::new(1, 4).unwrap(),
        '£',
        '¥',
    )
    .unwrap()
}
fn saved(v3: bool) -> Vec<u8> {
    let overlay =
        TeletextSavedEditorialOverlay::new_with_base_text_edits(provenance(), vec![edit()])
            .unwrap();
    append_teletext_design_editorial_overlay(&original(v3), &overlay, budget())
        .unwrap()
        .bytes()
        .to_vec()
}

#[test]
fn v23_base_text_roundtrip_preserves_exact_originals_metadata_and_base_only_contract() {
    for v3 in [false, true] {
        let original = original(v3);
        let before = decode_teletext_design_documents(&original, budget()).unwrap();
        let overlay =
            TeletextSavedEditorialOverlay::new_with_base_text_edits(provenance(), vec![edit()])
                .unwrap();
        let bytes =
            append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        let after = decode_teletext_design_documents(bytes.bytes(), budget()).unwrap();
        assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
        assert_eq!(after.selection(), before.selection());
        assert_eq!(
            after.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(after.edit_policy(), before.edit_policy());
        let stored = after.editorial_overlay().unwrap();
        assert_eq!(stored, &overlay);
        assert_eq!(stored.schema_version(), 23);
        assert_eq!(stored.base_text_edits(), [edit()]);
        assert!(stored.local_program_operands().is_empty());
        assert!(stored.main_local_invocation_program_overlays().is_empty());
        assert!(stored.resource_object_program_overlays().is_empty());
    }
}

#[test]
fn v23_base_text_roundtrip_retains_physical_rows_and_cannot_downgrade_to_legacy() {
    let physical = TeletextPhysicalBaseOverlay::new(
        address(17),
        vec![TeletextPhysicalBaseRow::new(2, [b'A'; 40]).unwrap()],
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new(provenance(), vec![physical.clone()])
        .unwrap()
        .with_base_text_edits(vec![edit()])
        .unwrap();
    let data =
        append_teletext_design_editorial_overlay(&original(true), &overlay, budget()).unwrap();
    let restored = decode_teletext_design_documents(data.bytes(), budget()).unwrap();
    assert_eq!(restored.editorial_overlay().unwrap(), &overlay);
    assert_eq!(restored.editorial_overlay().unwrap().pages(), [physical]);
    let value: Value = serde_json::from_slice(data.bytes()).unwrap();
    for version in [1, 2, 10, 14, 19, 20, 21, 22, 24] {
        let mut changed = value.clone();
        changed["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
}

#[test]
fn v23_base_text_storage_refuses_forged_originals_targets_shapes_and_oversized_vectors() {
    let original: Value = serde_json::from_slice(&saved(true)).unwrap();
    let target = json!({"owner":{"page":200,"subcode":16},"row":1,"column":4,"expected":"£","replacement":"¥"});
    for field in [
        Value::Null,
        json!([]),
        json!([target.clone(), target.clone()]),
        json!([{"owner":{"page":200,"subcode":16},"row":1,"column":4,"expected":"X","replacement":"¥"}]),
        json!([{"owner":{"page":200,"subcode":32},"row":1,"column":4,"expected":"£","replacement":"¥"}]),
        json!([{"owner":{"page":200,"subcode":16},"row":1,"column":12,"expected":"£","replacement":"¥"}]),
        json!([{"owner":{"page":200,"subcode":16},"row":0,"column":7,"expected":"£","replacement":"¥"}]),
        json!([{"owner":{"page":200,"subcode":16},"row":1,"column":4,"expected":"£","replacement":"\n"}]),
        json!([{"owner":{"page":200,"subcode":16},"row":1,"column":4,"expected":"£","replacement":"¥","local_command":0}]),
        json!([{"owner":{"page":200,"subcode":16},"row":1,"column":4,"expected":"£","replacement":"¥¥"}]),
    ] {
        let mut changed = original.clone();
        changed["editorial_overlay"]["base_text_edits"] = field;
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
    let mut changed = original.clone();
    changed["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("base_text_edits");
    assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    let mut changed = original;
    changed["editorial_overlay"]["base_text_edits"] = json!([target.clone(), target]);
    let mut limits = budget();
    limits.validation.service.max_commands = 1;
    let error = decode_teletext_design_documents(&bytes(&changed), limits).unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial operands",
            ..
        }
    ));
}

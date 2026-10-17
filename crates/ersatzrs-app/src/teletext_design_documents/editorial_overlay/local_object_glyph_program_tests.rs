//! V11 structural storage keeps exact-root association and older private formats closed.

use super::tests::{address, budget, bytes, original_with_context, overlay, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextDesignDocumentBudget, TeletextDesignDocumentError, TeletextDesignDocumentFailure,
    TeletextSavedEditorialOverlay,
};
use crate::teletext_design_documents::{
    append_teletext_effective_design_editorial_overlay, decode_teletext_effective_design_documents,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay, TeletextLocalObjectOperandOverlay,
    TeletextLocalProgramOperandOverlay, TeletextObjectId, TeletextObjectIdentity,
};
use serde_json::{json, Value};

fn operation(value: Value) -> TeletextColumnOperation {
    serde_json::from_value(value).unwrap()
}
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65520))
}
fn command(column: u8, operation: Value) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: self::operation(operation),
    }
}
fn original_program(patched: bool) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![
        command(
            0,
            json!({"kind":"foreground","colour":if patched {2} else {1}}),
        ),
        command(0, json!({"kind":"g0","code":65})),
        command(1, json!({"kind":"g1","mosaic":5})),
        command(2, json!({"kind":"background","colour":0})),
    ])
    .unwrap()
}
fn program(patched: bool, variant: usize) -> TeletextLocalObjectGlyphProgramOverlay {
    let expected = original_program(patched);
    let mut replacement = expected.commands().to_vec();
    let mut retained = vec![Some(0), Some(1), Some(2), Some(3)];
    match variant {
        0 => {
            replacement.insert(2, command(1, json!({"kind":"g0","code":66})));
            retained.insert(2, None);
        }
        1 => {
            replacement.remove(1);
            retained.remove(1);
        }
        2 => replacement[1] = command(3, json!({"kind":"g0","code":65})),
        3 => replacement[1] = command(0, json!({"kind":"g0","code":66})),
        _ => unreachable!(),
    }
    TeletextLocalObjectGlyphProgramOverlay::new(
        identity(),
        expected,
        TeletextEnhancementProgram::new(replacement).unwrap(),
        retained,
    )
    .unwrap()
}
fn colour_intent() -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(
        identity(),
        0,
        0,
        operation(json!({"kind":"foreground","colour":1})),
        operation(json!({"kind":"foreground","colour":2})),
    )
    .unwrap()
}
fn page_intent() -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(
        address(16),
        0,
        0,
        operation(json!({"kind":"foreground","colour":1})),
        operation(json!({"kind":"foreground","colour":3})),
    )
    .unwrap()
}
fn original() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original_with_context()).unwrap();
    let mut response: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    response["service"]["objects"] = json!([{
        "identity":identity(),"object_type":"passive","program":original_program(false)
    }]);
    response["service"]["page_enhancements"] = json!([{
        "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
        "local_program":{"commands":[{"kind":"at_column","column":0,"operation":{"kind":"foreground","colour":1}}]},
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}
    }]);
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    bytes(&value)
}
fn intent(patched: bool, variant: usize) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        if patched {
            vec![colour_intent()]
        } else {
            vec![]
        },
        vec![],
        vec![program(patched, variant)],
    )
    .unwrap()
}
fn concept() -> Value {
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original(), &intent(false, 0), budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}
fn rejected(value: &Value) {
    assert!(decode_teletext_design_documents(&bytes(value), budget()).is_err());
}

#[test]
fn v11_insert_delete_and_column_roundtrip_retain_immutable_originals() {
    let original = original();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    for variant in 0..3 {
        let overlay = intent(false, variant);
        assert_eq!(overlay.schema_version(), 11);
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
        assert_eq!(after.editorial_overlay(), Some(&overlay));
        assert_eq!(after.sources().len(), before.sources().len());
        for (a, b) in after.sources().iter().zip(before.sources()) {
            assert_eq!(a.bytes(), b.bytes());
            assert_eq!(a.contract(), b.contract());
            assert_eq!(a.generation(), b.generation());
        }
        assert_eq!(
            after.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(after.entry(), before.entry());
        assert_eq!(after.selection(), before.selection());
        let map = &after
            .editorial_overlay()
            .unwrap()
            .local_object_glyph_program_overlays()[0];
        assert_eq!(map.expected(), &original_program(false));
        assert_eq!(
            map.retained_commands(),
            program(false, variant).retained_commands()
        );
    }
}

#[test]
fn v11_mixed_rows_page_and_same_object_nonglyph_scalar_match_patched_root() {
    let value = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        overlay().pages().to_vec(),
        vec![page_intent()],
        vec![colour_intent()],
        vec![],
        vec![program(true, 0)],
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original(), &value, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&value)
    );
    let unpatched = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![colour_intent()],
        vec![],
        vec![program(false, 0)],
    )
    .unwrap();
    assert!(append_teletext_design_editorial_overlay(&original(), &unpatched, budget()).is_err());
    // A patched uploaded skeleton has no authority without its root-bound scalar intent.
    let detached = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![program(true, 0)],
    )
    .unwrap();
    assert!(append_teletext_design_editorial_overlay(&original(), &detached, budget()).is_err());
}

#[test]
fn v11_requires_structure_and_rejects_duplicate_or_scalar_glyph_overlap() {
    for programs in [vec![], vec![program(false, 3)], vec![program(false, 0); 2]] {
        assert!(
            TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
                provenance(),
                vec![],
                vec![],
                vec![],
                vec![],
                programs,
            )
            .is_err()
        );
    }
    let scalar = TeletextLocalObjectOperandOverlay::new(
        identity(),
        1,
        0,
        operation(json!({"kind":"g0","code":65})),
        operation(json!({"kind":"g0","code":66})),
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
            provenance(),
            vec![],
            vec![],
            vec![scalar],
            vec![],
            vec![program(false, 0)],
        )
        .is_err()
    );
}

#[test]
fn v11_closed_fields_versions_and_original_correspondence_reject_mutation() {
    let baseline = concept();
    for version in 1..=10 {
        let mut value = baseline.clone();
        value["editorial_overlay"]["schema_version"] = json!(version);
        rejected(&value);
    }
    for replacement in [Value::Null, json!([]), json!({}), json!([null])] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["local_object_glyph_program_overlays"] = replacement;
        rejected(&value);
    }
    let prefix = "/editorial_overlay/local_object_glyph_program_overlays/0";
    for (suffix, replacement) in [
        ("/identity/owner/subcode", json!(17)),
        ("/identity/object_id", json!(1)),
        ("/expected/commands/1/operation/code", json!(67)),
        ("/retained_commands/2", json!(1)),
        ("/retained_commands/0", Value::Null),
    ] {
        let mut value = baseline.clone();
        *value.pointer_mut(&format!("{prefix}{suffix}")).unwrap() = replacement;
        rejected(&value);
    }
    let mut unknown = baseline.clone();
    unknown.pointer_mut(prefix).unwrap()["unknown"] = json!(1);
    rejected(&unknown);
    let mut unknown_program = baseline.clone();
    unknown_program
        .pointer_mut(&format!("{prefix}/expected"))
        .unwrap()["unknown"] = json!(1);
    rejected(&unknown_program);
    let mut missing = baseline.clone();
    missing["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("local_object_glyph_program_overlays");
    rejected(&missing);
    let serialized = String::from_utf8(bytes(&baseline)).unwrap();
    let duplicate = serialized.replacen(
        "\"retained_commands\":",
        "\"retained_commands\":[],\"retained_commands\":",
        1,
    );
    assert!(decode_teletext_design_documents(duplicate.as_bytes(), budget()).is_err());
}

fn budget_failure(value: &Value, bounded: TeletextDesignDocumentBudget, dimension: &str) {
    let result = decode_teletext_design_documents(&bytes(value), bounded);
    assert!(
        matches!(&result,
            Err(TeletextDesignDocumentError { reason: TeletextDesignDocumentFailure::Budget { dimension: actual, .. }, .. }) if *actual == dimension
        ),
        "expected {dimension}, got {:?}",
        result.as_ref().err().map(|error| &error.reason)
    );
}

#[test]
fn v11_streaming_preflight_bounds_programs_commands_maps_and_combined_operands() {
    let baseline = concept();
    let prefix = "/editorial_overlay/local_object_glyph_program_overlays/0";
    for field in ["expected", "replacement"] {
        let mut value = baseline.clone();
        value
            .pointer_mut(&format!("{prefix}/{field}/commands"))
            .unwrap()
            .as_array_mut()
            .unwrap()
            .resize(
                209,
                json!({"kind":"at_column","column":0,"operation":{"kind":"g0","code":65}}),
            );
        budget_failure(&value, budget(), "editorial glyph program commands");
    }
    let mut long_map = baseline.clone();
    long_map
        .pointer_mut(&format!("{prefix}/retained_commands"))
        .unwrap()
        .as_array_mut()
        .unwrap()
        .resize(209, Value::Null);
    budget_failure(&long_map, budget(), "editorial retained commands");
    let mut many = baseline.clone();
    let record = many.pointer(prefix).unwrap().clone();
    many["editorial_overlay"]["local_object_glyph_program_overlays"] = json!(vec![record; 65]);
    // Admit this synthetic buffer so the independent 64-object gate is reached.
    let mut object_limit = budget();
    object_limit.max_serialized_bytes = bytes(&many).len();
    budget_failure(&many, object_limit, "editorial glyph programs");
    // Each array fits independently: expected4 + replacement5 + one page operand =10.
    let mut combined = baseline.clone();
    combined["editorial_overlay"]["local_program_operands"] = json!([{
        "owner":{"page":200,"subcode":16},"command":0,"column":0,
        "expected":{"kind":"foreground","colour":1},"replacement":{"kind":"foreground","colour":3}
    }]);
    let mut bounded = budget();
    bounded.validation.service.max_commands = 9;
    budget_failure(&combined, bounded, "editorial operands");
    let mut bounded = budget();
    // One program +4 expected+5 replacement+5 map entries =15 structure items.
    bounded.validation.service.max_structure_items = 14;
    budget_failure(&baseline, bounded, "editorial structure items");
}

#[test]
fn v11_effective_role_retains_original_selection_sources_and_role_separation() {
    let mut value: Value = serde_json::from_slice(&original()).unwrap();
    let selection = value.as_object_mut().unwrap().remove("selection").unwrap();
    value["format_version"] = json!(2);
    value["effective_selection"] = json!({"channel_id":9,"bindings":[{"origin":"base","base":selection["bindings"][0],"scripted":null}],"valid_until":null});
    value["producer_bindings"] = json!([0]);
    let original = bytes(&value);
    let before = decode_teletext_effective_design_documents(&original, budget()).unwrap();
    let overlay = intent(true, 0);
    let encoded =
        append_teletext_effective_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let after = decode_teletext_effective_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(after.editorial_overlay(), Some(&overlay));
    assert_eq!(after.selection(), before.selection());
    assert_eq!(after.producer_bindings(), before.producer_bindings());
    assert_eq!(after.guide_source(), before.guide_source());
    assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
    assert!(decode_teletext_design_documents(encoded.bytes(), budget()).is_err());
}

#[test]
fn v2_literal_bytes_stay_exact_and_do_not_acquire_v11_fields() {
    let original = original();
    let overlay = TeletextSavedEditorialOverlay::new_with_local_program_operands(
        provenance(),
        vec![],
        vec![page_intent()],
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let expected = format!(concat!("{{\"schema_version\":2,\"export_receipt_id\":3,\"export_receipt_digest\":\"{}\",",
        "\"original_design_id\":7,\"original_revision\":2,\"original_digest\":\"{}\",\"pages\":[],",
        "\"local_program_operands\":[{{\"owner\":{{\"page\":200,\"subcode\":16}},\"command\":0,\"column\":0,",
        "\"expected\":{{\"kind\":\"foreground\",\"colour\":1}},\"replacement\":{{\"kind\":\"foreground\",\"colour\":3}}}}]}}"),
        "d".repeat(64), "e".repeat(64));
    let end = original
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .unwrap();
    let mut golden = original[..end].to_vec();
    golden.extend_from_slice(b",\"editorial_overlay\":");
    golden.extend_from_slice(expected.as_bytes());
    golden.extend_from_slice(&original[end..]);
    assert_eq!(encoded.bytes(), golden);
}

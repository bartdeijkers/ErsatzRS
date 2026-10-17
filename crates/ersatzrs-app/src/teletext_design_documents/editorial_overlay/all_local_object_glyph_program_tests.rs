//! V12 widens structural glyph intent without changing V11 anchors or old bytes.

use super::tests::{address, budget, bytes, original_with_context, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextDesignDocumentBudget, TeletextDesignDocumentError, TeletextDesignDocumentFailure,
    TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextEnhancementCommand, TeletextEnhancementProgram,
    TeletextLocalObjectGlyphProgramOverlay, TeletextLocalObjectOperandOverlay, TeletextObjectId,
    TeletextObjectIdentity, TeletextRawDataPageNumber, TeletextResourceIdentity,
    TeletextResourceKind, TeletextResourceSubpageIndex,
};
use serde_json::{json, Value};

fn operation(value: Value) -> TeletextColumnOperation {
    serde_json::from_value(value).unwrap()
}
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65520))
}
fn command(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn families() -> Vec<TeletextColumnOperation> {
    [
        json!({"kind":"g2","code":35}),
        json!({"kind":"g0_diacritic","code":65,"diacritic":1}),
        json!({"kind":"g1_alpha","code":65}),
        json!({"kind":"g3","code":47,"compatibility":"level15"}),
        json!({"kind":"g3","code":47,"compatibility":"level25"}),
        json!({"kind":"drcs","reference":{"table":"normal","subtable":0,"slot":0}}),
        json!({"kind":"drcs","reference":{"table":"global","subtable":0,"slot":0}}),
    ]
    .into_iter()
    .map(operation)
    .collect()
}
fn wide(
    operation: TeletextColumnOperation,
    variant: usize,
) -> TeletextLocalObjectGlyphProgramOverlay {
    let expected = program(vec![
        command(0, self::operation(json!({"kind":"foreground","colour":1}))),
        command(1, operation),
        command(2, self::operation(json!({"kind":"g0","code":65}))),
        command(3, self::operation(json!({"kind":"background","colour":0}))),
    ]);
    let mut commands = expected.commands().to_vec();
    let mut retained = vec![Some(0), Some(1), Some(2), Some(3)];
    match variant {
        0 => {
            commands.insert(2, command(1, operation));
            retained.insert(2, None);
        }
        1 => {
            commands.remove(1);
            retained.remove(1);
        }
        2 => commands[1] = command(2, operation),
        _ => unreachable!(),
    }
    TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        identity(),
        expected,
        program(commands),
        retained,
    )
    .unwrap()
}
fn legacy(patched: bool) -> TeletextLocalObjectGlyphProgramOverlay {
    let extended = operation(json!({"kind":"g2","code":if patched {36} else {35}}));
    let g0 = operation(json!({"kind":"g0","code":65}));
    TeletextLocalObjectGlyphProgramOverlay::new(
        identity(),
        program(vec![command(0, extended), command(1, g0)]),
        program(vec![command(0, extended), command(2, g0)]),
        vec![Some(0), Some(1)],
    )
    .unwrap()
}
fn scalar() -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(
        identity(),
        0,
        0,
        operation(json!({"kind":"g2","code":35})),
        operation(json!({"kind":"g2","code":36})),
    )
    .unwrap()
}
fn resource(global: bool) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gdrcs
        } else {
            TeletextResourceKind::Drcs
        },
        TeletextRawDataPageNumber::new(2, if global { 0xc1 } else { 0xc0 }).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}
fn original(expected: &TeletextEnhancementProgram) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original_with_context()).unwrap();
    let mut response: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    response["service"]["objects"] = json!([{
        "identity":identity(),"object_type":"passive","program":expected
    }]);
    response["service"]["page_enhancements"] = json!([{
        "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
        "local_program":{"commands":[]},
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}
    }]);
    response["service"]["drcs"] = json!([
        {"identity":{"kind":"drcs","page":{"magazine":2,"page":192},"subpage":0},"glyphs":[{"slot":0,"bitmap":vec![1;10]}]},
        {"identity":{"kind":"gdrcs","page":{"magazine":2,"page":193},"subpage":0},"glyphs":[{"slot":0,"bitmap":vec![3;10]}]}
    ]);
    for global in [false, true] {
        value["resource_transmissions"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "resource":resource(global),"counter":5,"update":"changed","last_subpage":true
            }));
    }
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    bytes(&value)
}
fn intent(program: TeletextLocalObjectGlyphProgramOverlay) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![program],
    )
    .unwrap()
}
fn concept() -> Value {
    let program = wide(families()[0], 0);
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(
            &original(program.expected()),
            &intent(program),
            budget(),
        )
        .unwrap()
        .bytes(),
    )
    .unwrap()
}

#[test]
fn v12_each_additional_family_insert_delete_and_column_roundtrip_keep_originals() {
    for family in families() {
        for variant in 0..3 {
            let program = wide(family, variant);
            assert!(!program.is_legacy_compatible());
            let original = original(program.expected());
            let before = decode_teletext_design_documents(&original, budget()).unwrap();
            let overlay = intent(program);
            assert_eq!(overlay.schema_version(), 12);
            let encoded =
                append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
            let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
            assert_eq!(after.editorial_overlay(), Some(&overlay));
            assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
            assert_eq!(after.selection(), before.selection());
            assert_eq!(
                after.resource_transmissions(),
                before.resource_transmissions()
            );
        }
    }
}

#[test]
fn v11_rejects_prebuilt_wide_values_and_downgraded_v12_json() {
    for family in families() {
        let program = wide(family, 0);
        assert!(
            TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
                provenance(),
                vec![],
                vec![],
                vec![],
                vec![],
                vec![program],
            )
            .is_err()
        );
    }
    let baseline = concept();
    for version in 1..=11 {
        let mut value = baseline.clone();
        value["editorial_overlay"]["schema_version"] = json!(version);
        assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    }
}

#[test]
fn v11_exact_extended_anchor_and_prior_scalar_patch_remain_valid() {
    let root = legacy(false);
    let original = original(root.expected());
    for patched in [false, true] {
        let value = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
            provenance(),
            vec![],
            vec![],
            if patched { vec![scalar()] } else { vec![] },
            vec![],
            vec![legacy(patched)],
        )
        .unwrap();
        assert_eq!(value.schema_version(), 11);
        let encoded =
            append_teletext_design_editorial_overlay(&original, &value, budget()).unwrap();
        assert_eq!(
            decode_teletext_design_documents(encoded.bytes(), budget())
                .unwrap()
                .editorial_overlay(),
            Some(&value)
        );
    }
}

#[test]
fn v12_rejects_extended_anchor_alias_and_nonstructural_extended_code_only() {
    assert!(
        TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![legacy(false)],
        )
        .is_err()
    );
    let code_only = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        identity(),
        program(vec![command(0, families()[0])]),
        program(vec![command(0, operation(json!({"kind":"g2","code":36})))]),
        vec![Some(0)],
    )
    .unwrap();
    assert!(!code_only.is_legacy_compatible());
    assert!(
        TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![code_only],
        )
        .is_err()
    );
    let original = original(legacy(false).expected());
    let overlay = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![legacy(false)],
    )
    .unwrap();
    let mut value: Value = serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original, &overlay, budget())
            .unwrap()
            .bytes(),
    )
    .unwrap();
    value["editorial_overlay"]["schema_version"] = json!(12);
    assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
}

#[test]
fn v12_all_glyph_scalar_overlap_rejects_while_nonglyph_patch_binds_exact_root() {
    let family = operation(json!({"kind":"g2","code":36}));
    let glyph_scalar =
        TeletextLocalObjectOperandOverlay::new(identity(), 1, 1, families()[0], family).unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs(
            provenance(),
            vec![],
            vec![],
            vec![glyph_scalar],
            vec![],
            vec![wide(family, 0)],
        )
        .is_err()
    );
    let root = wide(families()[0], 0);
    let original = original(root.expected());
    let foreground = TeletextLocalObjectOperandOverlay::new(
        identity(),
        0,
        0,
        operation(json!({"kind":"foreground","colour":1})),
        operation(json!({"kind":"foreground","colour":2})),
    )
    .unwrap();
    let mut expected = root.expected().commands().to_vec();
    let mut replacement = root.replacement().commands().to_vec();
    expected[0] = command(0, foreground.replacement());
    replacement[0] = expected[0];
    let patched = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        identity(),
        program(expected),
        program(replacement),
        root.retained_commands().to_vec(),
    )
    .unwrap();
    let value = TeletextSavedEditorialOverlay::new_with_all_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![foreground],
        vec![],
        vec![patched.clone()],
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original, &value, budget()).unwrap();
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&value)
    );
    assert!(
        append_teletext_design_editorial_overlay(&original, &intent(patched), budget()).is_err()
    );
}

#[test]
fn v12_closed_field_shapes_and_unchanged_quota_refuse_before_typed_restore() {
    let baseline = concept();
    for invalid in [Value::Null, json!([]), json!({}), json!([null])] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["local_object_glyph_program_overlays"] = invalid;
        assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    }
    let prefix = "/editorial_overlay/local_object_glyph_program_overlays/0";
    let mut unknown = baseline.clone();
    unknown.pointer_mut(prefix).unwrap()["future"] = json!(1);
    assert!(decode_teletext_design_documents(&bytes(&unknown), budget()).is_err());
    let mut stale = baseline.clone();
    *stale
        .pointer_mut(&format!("{prefix}/expected/commands/1/operation/code"))
        .unwrap() = json!(36);
    assert!(decode_teletext_design_documents(&bytes(&stale), budget()).is_err());
    for field in ["expected", "replacement"] {
        let mut value = baseline.clone();
        value
            .pointer_mut(&format!("{prefix}/{field}/commands"))
            .unwrap()
            .as_array_mut()
            .unwrap()
            .resize(
                209,
                json!({"kind":"at_column","column":1,"operation":{"kind":"g2","code":35}}),
            );
        bounded(&value, budget(), "editorial glyph program commands");
    }
    let mut long_map = baseline.clone();
    long_map
        .pointer_mut(&format!("{prefix}/retained_commands"))
        .unwrap()
        .as_array_mut()
        .unwrap()
        .resize(209, Value::Null);
    bounded(&long_map, budget(), "editorial retained commands");
    let mut tight = budget();
    tight.validation.service.max_commands = 8; // expected4 + replacement5 exceeds8.
    bounded(&baseline, tight, "editorial operands");
}

fn bounded(value: &Value, limits: TeletextDesignDocumentBudget, dimension: &str) {
    assert!(
        matches!(decode_teletext_design_documents(&bytes(value), limits),
        Err(TeletextDesignDocumentError {reason:TeletextDesignDocumentFailure::Budget {dimension:actual,..},..}) if actual==dimension)
    );
}

#[test]
fn v11_literal_program_bytes_with_unchanged_g2_anchor_remain_exact_beside_v12() {
    let root = legacy(false);
    let original = original(root.expected());
    let overlay = TeletextSavedEditorialOverlay::new_with_local_object_glyph_programs(
        provenance(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![root],
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let expected = format!(concat!("{{\"schema_version\":11,\"export_receipt_id\":3,\"export_receipt_digest\":\"{}\",",
        "\"original_design_id\":7,\"original_revision\":2,\"original_digest\":\"{}\",\"pages\":[],",
        "\"local_object_glyph_program_overlays\":[{{\"identity\":{{\"scope\":\"local\",\"owner\":{{\"page\":200,\"subcode\":16}},\"object_id\":65520}},",
        "\"expected\":{{\"commands\":[{{\"kind\":\"at_column\",\"column\":0,\"operation\":{{\"kind\":\"g2\",\"code\":35}}}},{{\"kind\":\"at_column\",\"column\":1,\"operation\":{{\"kind\":\"g0\",\"code\":65}}}}]}},",
        "\"replacement\":{{\"commands\":[{{\"kind\":\"at_column\",\"column\":0,\"operation\":{{\"kind\":\"g2\",\"code\":35}}}},{{\"kind\":\"at_column\",\"column\":2,\"operation\":{{\"kind\":\"g0\",\"code\":65}}}}]}},\"retained_commands\":[0,1]}}]}}"),
        "d".repeat(64),"e".repeat(64));
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

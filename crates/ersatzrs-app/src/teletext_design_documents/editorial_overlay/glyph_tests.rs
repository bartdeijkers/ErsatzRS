//! V4 glyph concepts retain original scope and legacy byte contracts.
use super::tests::{address, budget, bytes, original_with_context, overlay, provenance};
use super::*;
use ersatzrs_domain::{
    TeletextObjectId, TeletextObjectIdentity, TeletextRawDataPageNumber, TeletextResourceKind,
    TeletextResourceSubpageIndex,
};
use serde_json::{json, Value};

fn operation(value: Value) -> TeletextColumnOperation {
    serde_json::from_value(value).unwrap()
}
fn g0(code: u8) -> TeletextColumnOperation {
    operation(json!({"kind":"g0","code":code}))
}
fn g1(mosaic: u8) -> TeletextColumnOperation {
    operation(json!({"kind":"g1","mosaic":mosaic}))
}
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65520))
}
fn page_intent(mosaic: bool) -> TeletextLocalProgramOperandOverlay {
    let (command, column, expected, replacement) = if mosaic {
        (1, 4, g1(5), g1(37))
    } else {
        (0, 3, g0(65), g0(81))
    };
    TeletextLocalProgramOperandOverlay::new(address(16), command, column, expected, replacement)
        .unwrap()
}
fn object_intent(mosaic: bool) -> TeletextLocalObjectOperandOverlay {
    let (command, column, expected, replacement) = if mosaic {
        (1, 1, g1(5), g1(37))
    } else {
        (0, 0, g0(65), g0(81))
    };
    TeletextLocalObjectOperandOverlay::new(identity(), command, column, expected, replacement)
        .unwrap()
}
fn original() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original_with_context()).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    source["service"]["page_enhancements"] = json!([{
        "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
        "local_program":{"commands":[
            {"kind":"at_column","column":3,"operation":{"kind":"g0","code":65}},
            {"kind":"at_column","column":4,"operation":{"kind":"g1","mosaic":5}}]},
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}]);
    source["service"]["objects"] = json!([{"identity":{"scope":"local","owner":{"page":200,"subcode":16},"object_id":65520},
        "object_type":"passive","program":{"commands":[
            {"kind":"at_column","column":0,"operation":{"kind":"g0","code":65}},
            {"kind":"at_column","column":1,"operation":{"kind":"g1","mosaic":5}}]}}]);
    source["service"]["drcs"] = json!([{"identity":{"kind":"drcs","page":{"magazine":2,"page":192},"subpage":1},
        "glyphs":[{"slot":0,"bitmap":vec![1;10]}]}]);
    value["resource_transmissions"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "resource":bitmap_intent().identity(),"counter":5,"update":"changed","last_subpage":true}));
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}
fn intent(page: bool, object: bool, mosaic: bool) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_local_object_operands(
        provenance(),
        vec![],
        if page {
            vec![page_intent(mosaic)]
        } else {
            vec![]
        },
        if object {
            vec![object_intent(mosaic)]
        } else {
            vec![]
        },
        vec![],
    )
    .unwrap()
}
fn concept() -> Value {
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original(), &intent(true, true, false), budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}
fn restore(
    value: &Value,
) -> Result<super::super::RestoredTeletextDesignDocuments, TeletextDesignDocumentError> {
    decode_teletext_design_documents(&bytes(value), budget())
}

#[test]
fn v4_page_and_local_object_g0_g1_round_trip_exact_original_context_and_prefix() {
    let original = original();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    for (page, object) in [(true, false), (false, true), (true, true)] {
        for mosaic in [false, true] {
            let overlay = intent(page, object, mosaic);
            let encoded =
                append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
            let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
            assert_eq!(after.editorial_overlay(), Some(&overlay));
            assert_eq!(overlay.schema_version(), 4);
            assert_eq!(after.selection(), before.selection());
            assert_eq!(after.guide_source(), before.guide_source());
            assert_eq!(
                after.resource_transmissions(),
                before.resource_transmissions()
            );
            let (
                TeletextProfileDocument::Authored(after_document),
                TeletextProfileDocument::Authored(before_document),
            ) = (&after.documents()[0], &before.documents()[0])
            else {
                panic!("authored original")
            };
            assert_eq!(after_document.semantic(), before_document.semantic());
            assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
            assert_eq!(
                after.sources()[0].authoring(),
                before.sources()[0].authoring()
            );
            let end = original
                .iter()
                .rposition(|byte| !byte.is_ascii_whitespace())
                .unwrap();
            assert_eq!(&encoded.bytes()[..end], &original[..end]);
            assert!(encoded.bytes().ends_with(&original[end..]));
            let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
            assert_eq!(
                value["editorial_overlay"]
                    .get("local_object_operands")
                    .is_some(),
                object
            );
            assert_eq!(
                value["editorial_overlay"]
                    .get("local_program_operands")
                    .is_some(),
                page
            );
            let debug = format!("{overlay:?}");
            assert!(!debug.contains("expected") && !debug.contains(&"d".repeat(64)));
        }
    }
}

#[test]
fn v4_requires_new_glyph_feature_and_legacy_constructors_keep_their_meaning() {
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_program_operands(
            provenance(),
            vec![],
            vec![page_intent(false)]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_operands(
            provenance(),
            overlay().pages().to_vec(),
            vec![],
            vec![],
            vec![]
        )
        .is_err()
    );
    let colour = TeletextLocalProgramOperandOverlay::new(
        address(16),
        0,
        0,
        operation(json!({"kind":"foreground","colour":1})),
        operation(json!({"kind":"foreground","colour":2})),
    )
    .unwrap();
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_operands(
            provenance(),
            vec![],
            vec![colour],
            vec![],
            vec![]
        )
        .is_err()
    );
    let baseline = concept();
    for version in [0, 1, 2, 3, 5] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["schema_version"] = json!(version);
        assert!(restore(&changed).is_err());
    }
    let mut page_only = baseline;
    page_only["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("local_object_operands");
    for version in [2, 3] {
        page_only["editorial_overlay"]["schema_version"] = json!(version);
        assert!(restore(&page_only).is_err());
    }
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_operands(
            provenance(),
            vec![],
            vec![],
            vec![object_intent(false); 2],
            vec![]
        )
        .is_err()
    );
}

#[test]
fn v4_object_shapes_are_closed_nonnull_nonempty_and_duplicate_fields_reject() {
    let baseline = concept();
    for replacement in [
        Value::Null,
        json!([]),
        json!({}),
        json!([[]]),
        json!([null]),
    ] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["local_object_operands"] = replacement;
        assert!(restore(&changed).is_err());
    }
    for path in [
        vec![],
        vec!["identity"],
        vec!["identity", "owner"],
        vec!["expected"],
        vec!["replacement"],
    ] {
        for replacement in [Value::Null, json!([]), json!([200, 16])] {
            let mut changed = baseline.clone();
            let mut target = &mut changed["editorial_overlay"]["local_object_operands"][0];
            for key in &path {
                target = &mut target[*key];
            }
            *target = replacement;
            assert!(restore(&changed).is_err());
        }
        let mut changed = baseline.clone();
        let mut target = &mut changed["editorial_overlay"]["local_object_operands"][0];
        for key in &path {
            target = &mut target[*key];
        }
        target["edit_permission"] = json!(true);
        assert!(restore(&changed).is_err());
    }
    let serialized = serde_json::to_string(&baseline).unwrap();
    let duplicated = serialized.replace(
        "\"object_id\":65520",
        "\"object_id\":65520,\"object_id\":65520",
    );
    assert_ne!(duplicated, serialized);
    assert!(decode_teletext_design_documents(duplicated.as_bytes(), budget()).is_err());
}

#[test]
fn v4_matches_full_original_identity_command_column_and_glyph_family() {
    let baseline = concept();
    for (field, replacement) in [
        ("command", json!(1)),
        ("column", json!(1)),
        ("column", json!(40)),
        ("expected", json!({"kind":"g0","code":66})),
        ("replacement", json!({"kind":"g1","mosaic":5})),
        ("replacement", json!({"kind":"g0","code":65})),
        ("replacement", json!({"kind":"g1_alpha","code":65})),
    ] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["local_object_operands"][0][field] = replacement;
        assert!(restore(&changed).is_err());
    }
    for (field, replacement) in [
        ("scope", json!("public")),
        ("object_id", json!(0)),
        ("owner", json!({"page":200,"subcode":17})),
    ] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["local_object_operands"][0]["identity"][field] = replacement;
        assert!(restore(&changed).is_err());
    }
    let original = original();
    let parsed = decode_teletext_design_documents(&original, budget()).unwrap();
    let mut documents = parsed.into_parts().documents;
    let duplicate = decode_teletext_design_documents(&original, budget())
        .unwrap()
        .into_parts()
        .documents;
    documents.extend(duplicate);
    assert!(validate_association(&intent(false, true, false), &documents, budget()).is_err());
}

#[test]
fn v4_combined_operand_preflight_and_structure_and_serialized_limits_hold() {
    let baseline = concept();
    let mut limited = budget();
    limited.validation.service.max_commands = 1;
    let error = decode_teletext_design_documents(&bytes(&baseline), limited).unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial operands",
            count: 2,
            maximum: 1
        }
    ));
    let original = original();
    let value = intent(true, true, false);
    let mut limited = budget();
    limited.validation.service.max_structure_items = 1;
    assert!(matches!(
        decode_teletext_design_documents(&bytes(&baseline), limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial structure items",
            count: 2,
            maximum: 1
        }
    ));
    let encoded = append_teletext_design_editorial_overlay(&original, &value, budget()).unwrap();
    let mut limited = budget();
    limited.max_serialized_bytes = encoded.bytes().len() - 1;
    assert!(matches!(
        append_teletext_design_editorial_overlay(&original, &value, limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "serialized",
            ..
        }
    ));
}

#[test]
fn v4_effective_role_retains_original_selection_context_and_role_separation() {
    use crate::teletext_design_documents::{
        append_teletext_effective_design_editorial_overlay,
        decode_teletext_effective_design_documents,
    };
    let mut value: Value = serde_json::from_slice(&original()).unwrap();
    let selection = value.as_object_mut().unwrap().remove("selection").unwrap();
    value["format_version"] = json!(2);
    value["effective_selection"] = json!({"channel_id":9,"bindings":[{"origin":"base","base":selection["bindings"][0],"scripted":null}],"valid_until":null});
    value["producer_bindings"] = json!([0]);
    let original = bytes(&value);
    let before = decode_teletext_effective_design_documents(&original, budget()).unwrap();
    let overlay = intent(true, true, false);
    let encoded =
        append_teletext_effective_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let after = decode_teletext_effective_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(after.editorial_overlay(), Some(&overlay));
    assert_eq!(after.selection(), before.selection());
    assert_eq!(after.producer_bindings(), before.producer_bindings());
    assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
    assert_eq!(after.guide_source(), before.guide_source());
    assert!(decode_teletext_design_documents(encoded.bytes(), budget()).is_err());
}

fn bitmap_intent() -> TeletextDrcsBitmapOverlay {
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
        TeletextResourceSubpageIndex::new(1).unwrap(),
    )
    .unwrap();
    TeletextDrcsBitmapOverlay::new(
        resource,
        TeletextDrcsGlyphSlot::new(0).unwrap(),
        TeletextDrcsBitmap::new([1; 10]).unwrap(),
        TeletextDrcsBitmap::new([3; 10]).unwrap(),
    )
    .unwrap()
}

#[test]
fn v4_mixed_rows_glyphs_and_bitmap_preserve_each_intent_without_legacy_aliases() {
    let mixed = TeletextSavedEditorialOverlay::new_with_local_object_operands(
        provenance(),
        overlay().pages().to_vec(),
        vec![page_intent(false)],
        vec![object_intent(true)],
        vec![bitmap_intent()],
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original(), &mixed, budget()).unwrap();
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.editorial_overlay(), Some(&mixed));
    assert_eq!(mixed.schema_version(), 4);
    assert_eq!(mixed.local_object_operands(), [object_intent(true)]);
    assert_eq!(mixed.drcs_bitmap_overlays(), [bitmap_intent()]);
    assert!(
        TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
            provenance(),
            vec![],
            vec![page_intent(false)],
            vec![bitmap_intent()]
        )
        .is_err()
    );
    let mut downgraded: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    downgraded["editorial_overlay"]["schema_version"] = json!(3);
    downgraded["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("local_object_operands");
    assert!(restore(&downgraded).is_err());
}

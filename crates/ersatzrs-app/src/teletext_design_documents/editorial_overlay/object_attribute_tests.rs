//! V5 object attributes retain exact original commands and older codec meanings.
use super::tests::{address, budget, bytes, original_with_context, overlay, provenance};
use super::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    TeletextDesignDocumentError, TeletextDesignDocumentFailure, TeletextSavedEditorialOverlay,
};
use crate::teletext_design_documents::{
    append_teletext_effective_design_editorial_overlay, decode_teletext_effective_design_documents,
    RestoredTeletextDesignDocuments,
};
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextDrcsBitmap, TeletextDrcsBitmapOverlay, TeletextDrcsGlyphSlot,
    TeletextLocalObjectOperandOverlay, TeletextLocalProgramOperandOverlay, TeletextObjectId,
    TeletextObjectIdentity, TeletextRawDataPageNumber, TeletextResourceIdentity,
    TeletextResourceKind, TeletextResourceSubpageIndex,
};
use serde_json::{json, Value};

fn operation(value: Value) -> TeletextColumnOperation {
    serde_json::from_value(value).unwrap()
}
fn foreground(colour: u8) -> TeletextColumnOperation {
    operation(json!({"kind":"foreground","colour":colour}))
}
fn background(colour: u8) -> TeletextColumnOperation {
    operation(json!({"kind":"background","colour":colour}))
}
fn attributes(invert: bool) -> TeletextColumnOperation {
    TeletextColumnOperation::Attributes {
        double_height: false,
        double_width: false,
        boxed: false,
        conceal: false,
        invert,
        underline_or_separated_mosaics: false,
    }
}
fn g0(code: u8) -> TeletextColumnOperation {
    operation(json!({"kind":"g0","code":code}))
}
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65520))
}
fn object_intent(
    command: usize,
    column: u8,
    expected: TeletextColumnOperation,
    replacement: TeletextColumnOperation,
) -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(identity(), command, column, expected, replacement)
        .unwrap()
}
fn attribute_intent(family: u8) -> TeletextLocalObjectOperandOverlay {
    match family {
        0 => object_intent(0, 0, foreground(1), foreground(4)),
        1 => object_intent(1, 0, background(0), background(16)),
        _ => object_intent(2, 0, attributes(false), attributes(true)),
    }
}
fn glyph_intent() -> TeletextLocalObjectOperandOverlay {
    object_intent(3, 0, g0(65), g0(66))
}
fn page_intent(glyph: bool) -> TeletextLocalProgramOperandOverlay {
    let (command, expected, replacement) = if glyph {
        (1, g0(65), g0(66))
    } else {
        (0, foreground(1), foreground(4))
    };
    TeletextLocalProgramOperandOverlay::new(address(16), command, 3, expected, replacement).unwrap()
}
fn bitmap_intent() -> TeletextDrcsBitmapOverlay {
    let identity = TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
        TeletextResourceSubpageIndex::new(1).unwrap(),
    )
    .unwrap();
    TeletextDrcsBitmapOverlay::new(
        identity,
        TeletextDrcsGlyphSlot::new(0).unwrap(),
        TeletextDrcsBitmap::new([1; 10]).unwrap(),
        TeletextDrcsBitmap::new([3; 10]).unwrap(),
    )
    .unwrap()
}
fn original() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original_with_context()).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    source["service"]["page_enhancements"] = json!([{
        "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
        "local_program":{"commands":[
            {"kind":"at_column","column":3,"operation":foreground(1)},
            {"kind":"at_column","column":3,"operation":g0(65)}]},
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}]);
    source["service"]["objects"] = json!([{"identity":identity(),"object_type":"passive","program":{"commands":[
        {"kind":"at_column","column":0,"operation":foreground(1)},
        {"kind":"at_column","column":0,"operation":background(0)},
        {"kind":"at_column","column":0,"operation":attributes(false)},
        {"kind":"at_column","column":0,"operation":g0(65)},
        {"kind":"at_column","column":1,"operation":{"kind":"g1","mosaic":1}}]}}]);
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
fn intent(family: u8, mixed: bool) -> TeletextSavedEditorialOverlay {
    let mut objects = vec![attribute_intent(family)];
    if mixed {
        objects.push(glyph_intent());
        objects.push(object_intent(
            4,
            1,
            operation(json!({"kind":"g1","mosaic":1})),
            operation(json!({"kind":"g1","mosaic":2})),
        ));
    }
    TeletextSavedEditorialOverlay::new_with_local_object_attributes(
        provenance(),
        if mixed {
            overlay().pages().to_vec()
        } else {
            vec![]
        },
        if mixed {
            vec![page_intent(false), page_intent(true)]
        } else {
            vec![]
        },
        objects,
        if mixed { vec![bitmap_intent()] } else { vec![] },
    )
    .unwrap()
}
fn concept() -> Value {
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original(), &intent(0, false), budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}
fn restore(value: &Value) -> Result<RestoredTeletextDesignDocuments, TeletextDesignDocumentError> {
    decode_teletext_design_documents(&bytes(value), budget())
}

#[test]
fn v5_each_attribute_and_mixed_glyph_rows_and_bitmap_round_trip_original_context() {
    let original = original();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    for family in 0..3 {
        for mixed in [false, true] {
            let overlay = intent(family, mixed);
            let encoded =
                append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
            let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
            assert_eq!(overlay.schema_version(), 5);
            assert_eq!(after.editorial_overlay(), Some(&overlay));
            assert_eq!(after.selection(), before.selection());
            assert_eq!(after.guide_source(), before.guide_source());
            assert_eq!(
                after.resource_transmissions(),
                before.resource_transmissions()
            );
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
            assert!(!format!("{overlay:?}").contains(&"d".repeat(64)));
        }
    }
}

#[test]
fn v5_requires_attribute_feature_and_v4_never_admits_object_attributes() {
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_attributes(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_attributes(
            provenance(),
            overlay().pages().to_vec(),
            vec![page_intent(true)],
            vec![glyph_intent()],
            vec![bitmap_intent()]
        )
        .is_err()
    );
    for family in 0..3 {
        assert!(
            TeletextSavedEditorialOverlay::new_with_local_object_operands(
                provenance(),
                vec![],
                vec![],
                vec![attribute_intent(family)],
                vec![]
            )
            .is_err()
        );
        assert!(
            TeletextSavedEditorialOverlay::new_with_local_object_operands(
                provenance(),
                vec![],
                vec![page_intent(true)],
                vec![glyph_intent(), attribute_intent(family)],
                vec![]
            )
            .is_err()
        );
    }
    let baseline = concept();
    for version in [0, 1, 2, 3, 4, 6] {
        let mut invalid = baseline.clone();
        invalid["editorial_overlay"]["schema_version"] = json!(version);
        assert!(restore(&invalid).is_err());
    }
    let mut alias = baseline;
    alias["editorial_overlay"]["local_object_operands"][0] = json!({"identity":identity(),"command":3,"column":0,"expected":g0(65),"replacement":g0(66)});
    assert!(restore(&alias).is_err());
}

#[test]
fn v5_requires_closed_nonempty_object_vector_and_exact_original_attribute_commands() {
    let baseline = concept();
    for replacement in [
        Value::Null,
        json!([]),
        json!({}),
        json!([[]]),
        json!([null]),
    ] {
        let mut invalid = baseline.clone();
        invalid["editorial_overlay"]["local_object_operands"] = replacement;
        assert!(restore(&invalid).is_err());
    }
    let mut missing = baseline.clone();
    missing["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("local_object_operands");
    assert!(restore(&missing).is_err());
    for (field, replacement) in [
        ("command", json!(1)),
        ("column", json!(1)),
        ("expected", json!({"kind":"foreground","colour":2})),
        ("replacement", json!({"kind":"background","colour":2})),
        ("replacement", json!({"kind":"foreground","colour":1})),
        ("permission", json!(true)),
    ] {
        let mut invalid = baseline.clone();
        invalid["editorial_overlay"]["local_object_operands"][0][field] = replacement;
        assert!(restore(&invalid).is_err());
    }
    let mut wrong_owner = baseline.clone();
    wrong_owner["editorial_overlay"]["local_object_operands"][0]["identity"]["owner"]["subcode"] =
        json!(17);
    assert!(restore(&wrong_owner).is_err());
    let mut duplicate = baseline.clone();
    duplicate["editorial_overlay"]["local_object_operands"]
        .as_array_mut()
        .unwrap()
        .push(baseline["editorial_overlay"]["local_object_operands"][0].clone());
    assert!(restore(&duplicate).is_err());
    let serialized = serde_json::to_string(&baseline).unwrap();
    let duplicate = serialized.replace(
        "\"object_id\":65520",
        "\"object_id\":65520,\"object_id\":65520",
    );
    assert_ne!(duplicate, serialized);
    assert!(decode_teletext_design_documents(duplicate.as_bytes(), budget()).is_err());
}

#[test]
fn v5_size_changes_and_other_nonspacing_families_remain_rejected() {
    let mut baseline = concept();
    baseline["editorial_overlay"]["local_object_operands"][0] = json!({"identity":identity(),"command":2,"column":0,
        "expected":attributes(false),"replacement":attributes(true)});
    assert!(restore(&baseline).is_ok());
    for field in ["double_width", "double_height"] {
        let mut invalid = baseline.clone();
        invalid["editorial_overlay"]["local_object_operands"][0]["replacement"][field] =
            json!(true);
        assert!(restore(&invalid).is_err());
    }
    for unsupported in [
        json!({"kind":"g1_alpha","code":65}),
        json!({"kind":"g2","code":65}),
        json!({"kind":"g3","code":65,"compatibility":"level25"}),
    ] {
        let _ = operation(unsupported.clone()); // Valid typed operation, unsupported overlay family.
        let mut invalid = baseline.clone();
        invalid["editorial_overlay"]["local_object_operands"][0]["replacement"] = unsupported;
        assert!(restore(&invalid).is_err());
    }
    let mut invalid = baseline;
    invalid["editorial_overlay"]["original_digest"] = json!("g".repeat(64));
    assert!(restore(&invalid).is_err());
}

#[test]
fn v5_combined_operand_structure_and_serialized_budgets_remain_strict() {
    let overlay = TeletextSavedEditorialOverlay::new_with_local_object_attributes(
        provenance(),
        vec![],
        vec![page_intent(false)],
        vec![attribute_intent(0)],
        vec![],
    )
    .unwrap();
    let original = original();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let mut limited = budget();
    limited.validation.service.max_commands = 1;
    assert!(matches!(
        decode_teletext_design_documents(encoded.bytes(), limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial operands",
            count: 2,
            maximum: 1
        }
    ));
    let mut limited = budget();
    limited.validation.service.max_structure_items = 1;
    assert!(matches!(
        decode_teletext_design_documents(encoded.bytes(), limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial structure items",
            count: 2,
            maximum: 1
        }
    ));
    let mut limited = budget();
    limited.max_serialized_bytes = encoded.bytes().len() - 1;
    assert!(matches!(
        append_teletext_design_editorial_overlay(&original, &overlay, limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "serialized",
            ..
        }
    ));
}

#[test]
fn v4_object_glyph_serialization_remains_exact_without_a_new_attribute_field() {
    let original = original();
    let overlay = TeletextSavedEditorialOverlay::new_with_local_object_operands(
        provenance(),
        vec![],
        vec![],
        vec![glyph_intent()],
        vec![],
    )
    .unwrap();
    assert_eq!(overlay.schema_version(), 4);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let expected=format!(concat!(
        "{{\"schema_version\":4,\"export_receipt_id\":3,\"export_receipt_digest\":\"{}\",",
        "\"original_design_id\":7,\"original_revision\":2,\"original_digest\":\"{}\",\"pages\":[],",
        "\"local_object_operands\":[{{\"identity\":{{\"scope\":\"local\",\"owner\":{{\"page\":200,\"subcode\":16}},\"object_id\":65520}},",
        "\"command\":3,\"column\":0,\"expected\":{{\"kind\":\"g0\",\"code\":65}},\"replacement\":{{\"kind\":\"g0\",\"code\":66}}}}]}}"),"d".repeat(64),"e".repeat(64));
    let end = original
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .unwrap();
    let mut golden = original[..end].to_vec();
    golden.extend_from_slice(b",\"editorial_overlay\":");
    golden.extend_from_slice(expected.as_bytes());
    golden.extend_from_slice(&original[end..]);
    assert_eq!(encoded.bytes(), golden);
    assert_eq!(
        decode_teletext_design_documents(&golden, budget())
            .unwrap()
            .editorial_overlay(),
        Some(&overlay)
    );
}

#[test]
fn v5_effective_role_retains_original_expiry_sources_and_role_separation() {
    let mut value: Value = serde_json::from_slice(&original()).unwrap();
    let selection = value.as_object_mut().unwrap().remove("selection").unwrap();
    value["format_version"] = json!(2);
    value["effective_selection"] = json!({"channel_id":9,"bindings":[{"origin":"base","base":selection["bindings"][0],"scripted":null}],"valid_until":null});
    value["producer_bindings"] = json!([0]);
    let original = bytes(&value);
    let before = decode_teletext_effective_design_documents(&original, budget()).unwrap();
    let overlay = intent(2, true);
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
fn v5_preserves_nonzero_original_character_size_for_attribute_changes() {
    let mut value: Value = serde_json::from_slice(&original()).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    let original_attributes =
        &mut source["service"]["objects"][0]["program"]["commands"][2]["operation"];
    original_attributes["double_height"] = json!(true);
    original_attributes["double_width"] = json!(true);
    let expected = operation(original_attributes.clone());
    let mut changed_attributes = original_attributes.clone();
    changed_attributes["invert"] = json!(true);
    let replacement = operation(changed_attributes);
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    let original = bytes(&value);
    let overlay = TeletextSavedEditorialOverlay::new_with_local_object_attributes(
        provenance(),
        vec![],
        vec![],
        vec![object_intent(2, 0, expected, replacement)],
        vec![],
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(after.editorial_overlay(), Some(&overlay));
    assert_eq!(overlay.schema_version(), 5);
}

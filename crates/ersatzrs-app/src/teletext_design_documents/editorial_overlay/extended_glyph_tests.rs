//! V6 extended glyphs remain exact-original intents, not rendering capabilities.
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
fn g2(code: u8) -> TeletextColumnOperation {
    operation(json!({"kind":"g2","code":code}))
}
fn diacritic(code: u8, index: u8) -> TeletextColumnOperation {
    operation(json!({"kind":"g0_diacritic","code":code,"diacritic":index}))
}
fn foreground(colour: u8) -> TeletextColumnOperation {
    operation(json!({"kind":"foreground","colour":colour}))
}
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65520))
}
fn original_operations() -> Vec<TeletextColumnOperation> {
    vec![
        g2(35),
        diacritic(65, 1),
        foreground(1),
        operation(json!({"kind":"g0","code":65})),
        operation(json!({"kind":"g1","mosaic":1})),
        operation(json!({"kind":"background","colour":0})),
        operation(
            json!({"kind":"attributes","double_height":false,"double_width":false,
            "boxed":false,"conceal":false,"invert":false,"underline_or_separated_mosaics":false}),
        ),
    ]
}
fn object_intent(
    command: usize,
    replacement: TeletextColumnOperation,
) -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(
        identity(),
        command,
        u8::try_from(command).unwrap(),
        original_operations()[command],
        replacement,
    )
    .unwrap()
}
fn extended(case: u8) -> (usize, TeletextColumnOperation, TeletextColumnOperation) {
    match case {
        0 => (0, g2(35), g2(36)),
        1 => (1, diacritic(65, 1), diacritic(66, 1)),
        2 => (1, diacritic(65, 1), diacritic(65, 9)),
        3 => (1, diacritic(65, 1), diacritic(66, 12)),
        4 => (1, diacritic(65, 1), diacritic(65, 0)),
        _ => (0, g2(35), g2(127)),
    }
}
fn page_intent(case: u8) -> TeletextLocalProgramOperandOverlay {
    let (command, expected, replacement) = extended(case);
    TeletextLocalProgramOperandOverlay::new(
        address(16),
        command,
        3 + u8::try_from(command).unwrap(),
        expected,
        replacement,
    )
    .unwrap()
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
    let operations = original_operations();
    let commands: Vec<_> = operations
        .iter()
        .enumerate()
        .map(
            |(column, operation)| json!({"kind":"at_column","column":column,"operation":operation}),
        )
        .collect();
    source["service"]["objects"] =
        json!([{"identity":identity(),"object_type":"passive","program":{"commands":commands}}]);
    source["service"]["page_enhancements"] = json!([{
        "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
        "local_program":{"commands":[
            {"kind":"at_column","column":3,"operation":g2(35)},
            {"kind":"at_column","column":4,"operation":diacritic(65,1)},
            {"kind":"at_column","column":5,"operation":foreground(1)}]},
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}]);
    source["service"]["drcs"] = json!([{"identity":{"kind":"drcs","page":{"magazine":2,"page":192},"subpage":1},
        "glyphs":[{"slot":0,"bitmap":vec![1;10]}]}]);
    value["resource_transmissions"].as_array_mut().unwrap().push(json!({"resource":bitmap_intent().identity(),"counter":5,"update":"changed","last_subpage":true}));
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}
fn intent(case: u8, page: bool, object: bool) -> TeletextSavedEditorialOverlay {
    let (command, _, replacement) = extended(case);
    TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
        provenance(),
        vec![],
        if page {
            vec![page_intent(case)]
        } else {
            vec![]
        },
        if object {
            vec![object_intent(command, replacement)]
        } else {
            vec![]
        },
        vec![],
    )
    .unwrap()
}
fn mixed() -> TeletextSavedEditorialOverlay {
    let mut attributes = serde_json::to_value(original_operations()[6]).unwrap();
    attributes["invert"] = json!(true);
    TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
        provenance(),
        overlay().pages().to_vec(),
        vec![
            page_intent(0),
            TeletextLocalProgramOperandOverlay::new(
                address(16),
                2,
                5,
                foreground(1),
                foreground(4),
            )
            .unwrap(),
        ],
        vec![
            object_intent(1, diacritic(66, 12)),
            object_intent(2, foreground(4)),
            object_intent(3, operation(json!({"kind":"g0","code":66}))),
            object_intent(4, operation(json!({"kind":"g1","mosaic":2}))),
            object_intent(5, operation(json!({"kind":"background","colour":16}))),
            object_intent(6, operation(attributes)),
        ],
        vec![bitmap_intent()],
    )
    .unwrap()
}
fn concept() -> Value {
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original(), &intent(3, true, true), budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}
fn restore(value: &Value) -> Result<RestoredTeletextDesignDocuments, TeletextDesignDocumentError> {
    decode_teletext_design_documents(&bytes(value), budget())
}

#[test]
fn v6_g2_and_diacritic_code_mark_scalars_round_trip_page_object_and_mixed_intent() {
    let original = original();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    let mut variants = Vec::new();
    for case in 0..6 {
        for (page, object) in [(true, false), (false, true), (true, true)] {
            variants.push(intent(case, page, object));
        }
    }
    variants.push(mixed());
    for overlay in variants {
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
        assert_eq!(overlay.schema_version(), 6);
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

#[test]
fn v6_requires_extended_intent_and_all_older_constructors_remain_closed() {
    let page = page_intent(0);
    let glyph = object_intent(3, operation(json!({"kind":"g0","code":66})));
    let attribute = object_intent(2, foreground(4));
    assert!(
        TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
            provenance(),
            overlay().pages().to_vec(),
            vec![],
            vec![glyph, attribute],
            vec![bitmap_intent()]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_program_operands(
            provenance(),
            vec![],
            vec![page]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
            provenance(),
            vec![],
            vec![page],
            vec![bitmap_intent()]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_operands(
            provenance(),
            vec![],
            vec![page],
            vec![glyph],
            vec![]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_attributes(
            provenance(),
            vec![],
            vec![page],
            vec![attribute],
            vec![]
        )
        .is_err()
    );
    let extended_object = object_intent(0, g2(36));
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_operands(
            provenance(),
            vec![],
            vec![],
            vec![extended_object],
            vec![]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_local_object_attributes(
            provenance(),
            vec![],
            vec![],
            vec![attribute, extended_object],
            vec![]
        )
        .is_err()
    );
}

#[test]
fn v6_json_downgrades_reject_even_when_old_version_shapes_are_satisfied() {
    let baseline = concept();
    for version in 1..=5 {
        let mut invalid = baseline.clone();
        invalid["editorial_overlay"]["schema_version"] = json!(version);
        assert!(restore(&invalid).is_err());
    }
    for version in [2, 3, 4, 5] {
        let mut invalid = baseline.clone();
        invalid["editorial_overlay"]["schema_version"] = json!(version);
        invalid["editorial_overlay"]
            .as_object_mut()
            .unwrap()
            .remove("local_object_operands");
        if version == 3 {
            invalid["editorial_overlay"]["drcs_bitmap_overlays"] = json!([{"identity":bitmap_intent().identity(),"slot":0,"expected":vec![1;10],"replacement":vec![3;10]}]);
        }
        if version == 4 {
            invalid["editorial_overlay"]["local_object_operands"] = json!([{"identity":identity(),"command":3,"column":3,"expected":{"kind":"g0","code":65},"replacement":{"kind":"g0","code":66}}]);
        }
        if version == 5 {
            invalid["editorial_overlay"]["local_object_operands"] = json!([{"identity":identity(),"command":2,"column":2,"expected":foreground(1),"replacement":foreground(4)}]);
        }
        assert!(restore(&invalid).is_err());
    }
}

#[test]
fn v6_optional_vectors_scalar_bounds_provenance_and_original_expectations_stay_strict() {
    let baseline = concept();
    for field in [
        "local_program_operands",
        "local_object_operands",
        "drcs_bitmap_overlays",
    ] {
        for invalid_field in [Value::Null, json!([])] {
            let mut invalid = baseline.clone();
            invalid["editorial_overlay"][field] = invalid_field;
            assert!(restore(&invalid).is_err());
        }
    }
    let mut missing = baseline.clone();
    for field in ["local_program_operands", "local_object_operands"] {
        missing["editorial_overlay"]
            .as_object_mut()
            .unwrap()
            .remove(field);
    }
    assert!(restore(&missing).is_err());
    for replacement in [
        json!({"kind":"g0_diacritic","code":31,"diacritic":2}),
        json!({"kind":"g0_diacritic","code":128,"diacritic":2}),
        json!({"kind":"g0_diacritic","code":66,"diacritic":16}),
        json!({"kind":"g0_diacritic","code":65,"diacritic":1}),
        json!({"kind":"g2","code":66}),
        json!({"kind":"g1_alpha","code":65}),
        json!({"kind":"g3","code":65,"compatibility":"level25"}),
    ] {
        let mut invalid = baseline.clone();
        invalid["editorial_overlay"]["local_object_operands"][0]["replacement"] = replacement;
        assert!(restore(&invalid).is_err());
    }
    let mut stale = baseline.clone();
    stale["editorial_overlay"]["local_object_operands"][0]["expected"]["diacritic"] = json!(2);
    assert!(restore(&stale).is_err());
    let mut wrong = baseline.clone();
    wrong["editorial_overlay"]["local_object_operands"][0]["identity"]["owner"]["subcode"] =
        json!(17);
    assert!(restore(&wrong).is_err());
    let mut duplicate = baseline.clone();
    duplicate["editorial_overlay"]["local_object_operands"]
        .as_array_mut()
        .unwrap()
        .push(baseline["editorial_overlay"]["local_object_operands"][0].clone());
    assert!(restore(&duplicate).is_err());
    let mut invalid = baseline;
    invalid["editorial_overlay"]["original_digest"] = json!("x".repeat(64));
    assert!(restore(&invalid).is_err());
}

#[test]
fn v6_combined_operand_structure_and_serialized_budgets_reject_before_widening() {
    let baseline = concept();
    for commands in [true, false] {
        let mut limited = budget();
        if commands {
            limited.validation.service.max_commands = 1;
        } else {
            limited.validation.service.max_structure_items = 1;
        }
        let error = decode_teletext_design_documents(&bytes(&baseline), limited).unwrap_err();
        let expected_dimension = if commands {
            "editorial operands"
        } else {
            "editorial structure items"
        };
        assert!(matches!(error.reason,
            TeletextDesignDocumentFailure::Budget { dimension, count: 2, maximum: 1 }
                if dimension == expected_dimension));
    }
    let original = original();
    let overlay = mixed();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
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
fn v5_literal_object_attribute_bytes_remain_unchanged_beside_v6() {
    let original = original();
    let attribute = object_intent(2, foreground(4));
    let overlay = TeletextSavedEditorialOverlay::new_with_local_object_attributes(
        provenance(),
        vec![],
        vec![],
        vec![attribute],
        vec![],
    )
    .unwrap();
    assert_eq!(overlay.schema_version(), 5);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let expected=format!(concat!("{{\"schema_version\":5,\"export_receipt_id\":3,\"export_receipt_digest\":\"{}\",",
        "\"original_design_id\":7,\"original_revision\":2,\"original_digest\":\"{}\",\"pages\":[],",
        "\"local_object_operands\":[{{\"identity\":{{\"scope\":\"local\",\"owner\":{{\"page\":200,\"subcode\":16}},\"object_id\":65520}},",
        "\"command\":2,\"column\":2,\"expected\":{{\"kind\":\"foreground\",\"colour\":1}},\"replacement\":{{\"kind\":\"foreground\",\"colour\":4}}}}]}}"),"d".repeat(64),"e".repeat(64));
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

#[test]
fn v6_effective_role_retains_original_selection_sources_and_role_separation() {
    let mut value: Value = serde_json::from_slice(&original()).unwrap();
    let selection = value.as_object_mut().unwrap().remove("selection").unwrap();
    value["format_version"] = json!(2);
    value["effective_selection"] = json!({"channel_id":9,"bindings":[{"origin":"base","base":selection["bindings"][0],"scripted":null}],"valid_until":null});
    value["producer_bindings"] = json!([0]);
    let original = bytes(&value);
    let before = decode_teletext_effective_design_documents(&original, budget()).unwrap();
    let overlay = mixed();
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

//! V7 G1-alphabetic/G3 intents preserve exact source mode, not rendering authority.
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
fn identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(address(16), TeletextObjectId::new(65520))
}
fn operations(edited: bool) -> Vec<TeletextColumnOperation> {
    let mut attributes = json!({"kind":"attributes","double_height":false,"double_width":false,
        "boxed":false,"conceal":false,"invert":false,"underline_or_separated_mosaics":false});
    attributes["invert"] = json!(edited);
    vec![
        operation(json!({"kind":"g1_alpha","code":if edited {95} else {64}})),
        operation(json!({"kind":"g3","code":if edited {33} else {32},"compatibility":"level15"})),
        operation(json!({"kind":"g3","code":if edited {33} else {32},"compatibility":"level25"})),
        operation(json!({"kind":"g2","code":if edited {36} else {35}})),
        operation(
            json!({"kind":"g0_diacritic","code":if edited {66} else {65},"diacritic":if edited {12} else {1}}),
        ),
        operation(json!({"kind":"foreground","colour":if edited {4} else {1}})),
        operation(json!({"kind":"g0","code":if edited {66} else {65}})),
        operation(json!({"kind":"g1","mosaic":if edited {2} else {1}})),
        operation(json!({"kind":"background","colour":if edited {16} else {0}})),
        operation(attributes),
    ]
}
fn page_intent(command: usize) -> TeletextLocalProgramOperandOverlay {
    TeletextLocalProgramOperandOverlay::new(
        address(16),
        command,
        u8::try_from(command).unwrap(),
        operations(false)[command],
        operations(true)[command],
    )
    .unwrap()
}
fn object_intent(command: usize) -> TeletextLocalObjectOperandOverlay {
    TeletextLocalObjectOperandOverlay::new(
        identity(),
        command,
        u8::try_from(command).unwrap(),
        operations(false)[command],
        operations(true)[command],
    )
    .unwrap()
}
fn bitmap_intent() -> TeletextDrcsBitmapOverlay {
    TeletextDrcsBitmapOverlay::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Drcs,
            TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
            TeletextResourceSubpageIndex::new(1).unwrap(),
        )
        .unwrap(),
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
    let commands: Vec<_> = operations(false)
        .into_iter()
        .enumerate()
        .map(
            |(column, operation)| json!({"kind":"at_column","column":column,"operation":operation}),
        )
        .collect();
    source["service"]["objects"] =
        json!([{"identity":identity(),"object_type":"passive","program":{"commands":commands}}]);
    source["service"]["page_enhancements"] = json!([{
        "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
        "local_program":{"commands":commands},
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}]);
    source["service"]["drcs"] = json!([{"identity":{"kind":"drcs","page":{"magazine":2,"page":192},"subpage":1},
        "glyphs":[{"slot":0,"bitmap":vec![1;10]}]}]);
    value["resource_transmissions"].as_array_mut().unwrap().push(json!({"resource":bitmap_intent().identity(),"counter":5,"update":"changed","last_subpage":true}));
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}
fn intent(command: usize, page: bool, object: bool) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
        provenance(),
        vec![],
        if page {
            vec![page_intent(command)]
        } else {
            vec![]
        },
        if object {
            vec![object_intent(command)]
        } else {
            vec![]
        },
        vec![],
    )
    .unwrap()
}
fn mixed() -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
        provenance(),
        overlay().pages().to_vec(),
        vec![page_intent(0), page_intent(3), page_intent(4)],
        (1..10).map(object_intent).collect(),
        vec![bitmap_intent()],
    )
    .unwrap()
}
fn concept(overlay: &TeletextSavedEditorialOverlay) -> Value {
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original(), overlay, budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}
fn restore(value: &Value) -> Result<RestoredTeletextDesignDocuments, TeletextDesignDocumentError> {
    decode_teletext_design_documents(&bytes(value), budget())
}

#[test]
fn v7_g1_alpha_and_both_g3_modes_round_trip_page_object_and_mixed_prior_intent() {
    let original = original();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    let mut variants = Vec::new();
    for command in 0..3 {
        for (page, object) in [(true, false), (false, true), (true, true)] {
            variants.push(intent(command, page, object));
        }
    }
    variants.push(mixed());
    for overlay in variants {
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
        assert_eq!(overlay.schema_version(), 7);
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
    }
}

#[test]
fn v7_requires_remaining_intent_and_v1_through_v6_constructors_stay_closed() {
    assert!(
        TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
            provenance(),
            overlay().pages().to_vec(),
            vec![page_intent(3)],
            vec![object_intent(4), object_intent(5)],
            vec![bitmap_intent()]
        )
        .is_err()
    );
    for command in 0..3 {
        let page = page_intent(command);
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
                vec![object_intent(6)],
                vec![]
            )
            .is_err()
        );
        assert!(
            TeletextSavedEditorialOverlay::new_with_local_object_attributes(
                provenance(),
                vec![],
                vec![page],
                vec![object_intent(5)],
                vec![]
            )
            .is_err()
        );
        assert!(
            TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
                provenance(),
                vec![],
                vec![page, page_intent(3)],
                vec![],
                vec![]
            )
            .is_err()
        );
        assert!(
            TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
                provenance(),
                vec![],
                vec![page_intent(3)],
                vec![object_intent(command)],
                vec![]
            )
            .is_err()
        );
    }
}

#[test]
fn v7_json_downgrades_reject_with_each_previous_versions_feature_shape_present() {
    for version in 1..=6 {
        let pages = match version {
            4 => vec![page_intent(0), page_intent(6)],
            6 => vec![page_intent(0), page_intent(3)],
            _ => vec![page_intent(0)],
        };
        let objects = if version == 5 {
            vec![object_intent(5)]
        } else {
            vec![]
        };
        let bitmaps = if version == 3 {
            vec![bitmap_intent()]
        } else {
            vec![]
        };
        let overlay = TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
            provenance(),
            vec![],
            pages,
            objects,
            bitmaps,
        )
        .unwrap();
        let mut value = concept(&overlay);
        value["editorial_overlay"]["schema_version"] = json!(version);
        assert!(
            restore(&value).is_err(),
            "version{version} cannot acquire remaining glyph semantics"
        );
    }
}

#[test]
fn v7_keeps_g3_compatibility_scalar_family_and_exact_original_association() {
    let baseline = concept(&intent(1, true, true));
    for replacement in [
        json!({"kind":"g3","code":33,"compatibility":"level25"}),
        json!({"kind":"g3","code":31,"compatibility":"level15"}),
        json!({"kind":"g3","code":128,"compatibility":"level15"}),
        json!({"kind":"g3","code":33,"compatibility":"future"}),
        json!({"kind":"g1_alpha","code":65}),
        json!({"kind":"g2","code":33}),
    ] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["local_object_operands"][0]["replacement"] = replacement;
        assert!(restore(&value).is_err());
    }
    let alpha = concept(&intent(0, true, true));
    for replacement in [
        json!({"kind":"g1_alpha","code":63}),
        json!({"kind":"g1_alpha","code":96}),
        json!({"kind":"g0","code":65}),
        json!({"kind":"g1","mosaic":1}),
    ] {
        let mut value = alpha.clone();
        value["editorial_overlay"]["local_program_operands"][0]["replacement"] = replacement;
        assert!(restore(&value).is_err());
    }
    for field in ["local_program_operands", "local_object_operands"] {
        for invalid in [Value::Null, json!([])] {
            let mut value = baseline.clone();
            value["editorial_overlay"][field] = invalid;
            assert!(restore(&value).is_err());
        }
    }
    let mut stale = baseline.clone();
    stale["editorial_overlay"]["local_object_operands"][0]["expected"]["code"] = json!(34);
    assert!(restore(&stale).is_err());
    let mut duplicate = baseline.clone();
    duplicate["editorial_overlay"]["local_object_operands"]
        .as_array_mut()
        .unwrap()
        .push(baseline["editorial_overlay"]["local_object_operands"][0].clone());
    assert!(restore(&duplicate).is_err());
    let mut wrong_owner = baseline;
    wrong_owner["editorial_overlay"]["local_object_operands"][0]["identity"]["owner"]["subcode"] =
        json!(17);
    assert!(restore(&wrong_owner).is_err());
}

#[test]
fn v7_combined_operand_and_structure_limits_and_bounded_serialization_remain() {
    let value = concept(&intent(2, true, true));
    for commands in [true, false] {
        let mut limited = budget();
        if commands {
            limited.validation.service.max_commands = 1;
        } else {
            limited.validation.service.max_structure_items = 1;
        }
        let error = decode_teletext_design_documents(&bytes(&value), limited).unwrap_err();
        let expected = if commands {
            "editorial operands"
        } else {
            "editorial structure items"
        };
        assert!(matches!(error.reason,
            TeletextDesignDocumentFailure::Budget { dimension, count: 2, maximum: 1 }
                if dimension == expected));
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
fn v6_literal_page_g2_serialization_stays_unchanged_beside_v7() {
    let original = original();
    let overlay = TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
        provenance(),
        vec![],
        vec![page_intent(3)],
        vec![],
        vec![],
    )
    .unwrap();
    assert_eq!(overlay.schema_version(), 6);
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let expected=format!(concat!("{{\"schema_version\":6,\"export_receipt_id\":3,\"export_receipt_digest\":\"{}\",",
        "\"original_design_id\":7,\"original_revision\":2,\"original_digest\":\"{}\",\"pages\":[],",
        "\"local_program_operands\":[{{\"owner\":{{\"page\":200,\"subcode\":16}},\"command\":3,\"column\":3,",
        "\"expected\":{{\"kind\":\"g2\",\"code\":35}},\"replacement\":{{\"kind\":\"g2\",\"code\":36}}}}]}}"),"d".repeat(64),"e".repeat(64));
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
fn v7_effective_role_retains_exact_sources_selection_and_static_separation() {
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

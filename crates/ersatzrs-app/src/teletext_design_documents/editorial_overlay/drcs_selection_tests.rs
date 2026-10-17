//! V10 selection storage is not target-usage qualification; native rebuild remains required.
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
        operation(
            json!({"kind":"drcs","reference":{"table":if edited {"global"} else {"normal"},"subtable":1,"slot":if edited {47} else {0}}}),
        ),
        operation(
            json!({"kind":"drcs","reference":{"table":"global","subtable":if edited {15} else {1},"slot":u8::from(edited)}}),
        ),
        operation(
            json!({"kind":"flash","mode":if edited {"inverted"} else {"normal"},"rate":if edited {"fast2_hz_phase3"} else {"slow1_hz"}}),
        ),
        operation(json!({"kind":"designate_charset","designation":if edited {55} else {0}})),
        operation(
            json!({"kind":"drcs","reference":{"table":"normal","subtable":1,"slot":if edited {47} else {0}}}),
        ),
        operation(
            json!({"kind":"drcs","reference":{"table":"global","subtable":1,"slot":u8::from(edited)}}),
        ),
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
        resource(false, 1),
        TeletextDrcsGlyphSlot::new(0).unwrap(),
        TeletextDrcsBitmap::new([1; 10]).unwrap(),
        TeletextDrcsBitmap::new([3; 10]).unwrap(),
    )
    .unwrap()
}
fn resource(global: bool, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gdrcs
        } else {
            TeletextResourceKind::Drcs
        },
        TeletextRawDataPageNumber::new(2, if global { 0xc1 } else { 0xc0 }).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}
fn links() -> Value {
    let unspecified = json!({"target":{"kind":"unspecified","magazine":2},"validity":"level25_only","required_subpages":65535});
    json!({"gpop":unspecified,"pop":unspecified,
        "gdrcs":{"target":{"kind":"page","page":{"magazine":2,"page":193}},"validity":"level25_only","required_subpages":32770},
        "drcs":{"target":{"kind":"page","page":{"magazine":2,"page":192}},"validity":"level25_only","required_subpages":32770}})
}
fn drcs_subtable(global: bool, subpage: u8) -> Value {
    let (kind, page) = if global {
        ("gdrcs", 193)
    } else {
        ("drcs", 192)
    };
    // Producer wire uses subpage, unlike outer domain lifecycle logical_subpage.
    json!({"identity":{"kind":kind,"page":{"magazine":2,"page":page},"subpage":subpage},
        "glyphs":[{"slot":0,"bitmap":vec![1;10]},
            {"slot":1,"bitmap":vec![7;10]},{"slot":47,"bitmap":vec![15;10]}]})
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
        "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":links(),
        "local_program":{"commands":commands},
        "resources":{"objects":{"public":null,"global":null},
        "drcs":{"normal":{"magazine":2,"page":192},"global":{"magazine":2,"page":193}},"defaults":[null,null,null,null]}}]);
    source["service"]["drcs"] = json!([
        drcs_subtable(false, 1),
        drcs_subtable(true, 1),
        drcs_subtable(false, 15),
        drcs_subtable(true, 15)
    ]);
    for global in [false, true] {
        for subpage in [1, 15] {
            value["resource_transmissions"].as_array_mut().unwrap().push(json!({"resource":resource(global,subpage),"counter":5,"update":"changed","last_subpage":true}));
        }
    }
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&source).unwrap());
    bytes(&value)
}
fn intent(command: usize, page: bool, object: bool) -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_drcs_reference_selections(
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
    TeletextSavedEditorialOverlay::new_with_drcs_reference_selections(
        provenance(),
        overlay().pages().to_vec(),
        (0..operations(false).len()).map(page_intent).collect(),
        (0..operations(false).len()).map(object_intent).collect(),
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
fn v10_table_subtable_page_object_and_mixed_intent_preserve_original_documents() {
    let original = original();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    let mut variants = Vec::new();
    for command in 0..2 {
        for (page, object) in [(true, false), (false, true), (true, true)] {
            variants.push(intent(command, page, object));
        }
    }
    variants.push(mixed());
    for overlay in variants {
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
        assert_eq!(overlay.schema_version(), 10);
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

fn previous_page_constructor(
    version: u16,
    command: usize,
) -> Result<TeletextSavedEditorialOverlay, TeletextDesignDocumentError> {
    let page = page_intent(command);
    match version {
        2 => TeletextSavedEditorialOverlay::new_with_local_program_operands(
            provenance(),
            vec![],
            vec![page],
        ),
        3 => TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
            provenance(),
            vec![],
            vec![page],
            vec![bitmap_intent()],
        ),
        4 => TeletextSavedEditorialOverlay::new_with_local_object_operands(
            provenance(),
            vec![],
            vec![page],
            vec![object_intent(12)],
            vec![],
        ),
        5 => TeletextSavedEditorialOverlay::new_with_local_object_attributes(
            provenance(),
            vec![],
            vec![page],
            vec![object_intent(11)],
            vec![],
        ),
        6 => TeletextSavedEditorialOverlay::new_with_extended_glyph_operands(
            provenance(),
            vec![],
            vec![page, page_intent(9)],
            vec![],
            vec![],
        ),
        7 => TeletextSavedEditorialOverlay::new_with_remaining_glyph_operands(
            provenance(),
            vec![],
            vec![page, page_intent(6)],
            vec![],
            vec![],
        ),
        8 => TeletextSavedEditorialOverlay::new_with_drcs_reference_slots(
            provenance(),
            vec![],
            vec![page, page_intent(4)],
            vec![],
            vec![],
        ),
        9 => TeletextSavedEditorialOverlay::new_with_flash_charset_operands(
            provenance(),
            vec![],
            vec![page, page_intent(2)],
            vec![],
            vec![],
        ),
        _ => unreachable!(),
    }
}
#[test]
fn v10_requires_actual_selection_change_and_earlier_constructors_stay_closed() {
    assert!(
        TeletextSavedEditorialOverlay::new_with_drcs_reference_selections(
            provenance(),
            vec![],
            vec![],
            vec![],
            vec![]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_drcs_reference_selections(
            provenance(),
            overlay().pages().to_vec(),
            vec![page_intent(2), page_intent(4), page_intent(9)],
            vec![object_intent(11)],
            vec![bitmap_intent()]
        )
        .is_err()
    );
    for version in [8, 9] {
        let mut inflated = concept(&golden_overlay(version));
        inflated["editorial_overlay"]["schema_version"] = json!(10);
        assert!(
            restore(&inflated).is_err(),
            "V10 must contain a real table/subtable change"
        );
    }
    for command in 0..2 {
        for version in 2..=9 {
            assert!(previous_page_constructor(version, command).is_err());
        }
        assert!(
            TeletextSavedEditorialOverlay::new_with_flash_charset_operands(
                provenance(),
                vec![],
                vec![page_intent(2)],
                vec![object_intent(command)],
                vec![]
            )
            .is_err()
        );
    }
}

#[test]
fn v10_downgrades_reject_even_with_prior_feature_requirements_present() {
    for version in 1..=9 {
        let mut pages = vec![page_intent(0)];
        match version {
            4 => pages.push(page_intent(12)),
            6 => pages.push(page_intent(9)),
            7 => pages.push(page_intent(6)),
            8 => pages.push(page_intent(4)),
            9 => pages.push(page_intent(2)),
            _ => {}
        }
        let objects = if version == 5 {
            vec![object_intent(11)]
        } else {
            vec![]
        };
        let bitmaps = if version == 3 {
            vec![bitmap_intent()]
        } else {
            vec![]
        };
        let overlay = TeletextSavedEditorialOverlay::new_with_drcs_reference_selections(
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
            "version{version} retains its original families"
        );
    }
}

#[test]
fn v10_checked_selectors_and_exact_original_association_stay_strict() {
    let baseline = concept(&intent(0, true, true));
    for reference in [
        json!({"table":"future","subtable":1,"slot":47}),
        json!({"table":"global","subtable":16,"slot":47}),
        json!({"table":"global","subtable":1,"slot":48}),
        json!({"table":"normal","subtable":1,"slot":0}),
        json!({"table":"global","slot":47}),
        json!({"table":null,"subtable":1,"slot":47}),
    ] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["local_object_operands"][0]["replacement"]["reference"] =
            reference;
        assert!(restore(&value).is_err());
    }
    let mut crossed = baseline.clone();
    crossed["editorial_overlay"]["local_program_operands"][0]["replacement"] =
        json!({"kind":"g0","code":65});
    assert!(restore(&crossed).is_err());
    let mut stale = baseline.clone();
    stale["editorial_overlay"]["local_object_operands"][0]["expected"]["reference"]["slot"] =
        json!(1);
    assert!(restore(&stale).is_err());
    let mut duplicate = baseline.clone();
    duplicate["editorial_overlay"]["local_object_operands"]
        .as_array_mut()
        .unwrap()
        .push(baseline["editorial_overlay"]["local_object_operands"][0].clone());
    assert!(restore(&duplicate).is_err());
    for field in ["local_program_operands", "local_object_operands"] {
        for invalid in [Value::Null, json!([])] {
            let mut value = baseline.clone();
            value["editorial_overlay"][field] = invalid;
            assert!(restore(&value).is_err());
        }
    }
    let mut wrong = baseline;
    wrong["editorial_overlay"]["local_object_operands"][0]["identity"]["owner"]["subcode"] =
        json!(17);
    assert!(restore(&wrong).is_err());
}

#[test]
fn v10_combined_operand_structure_and_serialized_caps_remain_bounded() {
    let value = concept(&intent(1, true, true));
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
        assert!(
            matches!(error.reason,TeletextDesignDocumentFailure::Budget {dimension,count:2,maximum:1} if dimension==expected)
        );
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

fn golden_overlay(version: u16) -> TeletextSavedEditorialOverlay {
    match version {
        8 => TeletextSavedEditorialOverlay::new_with_drcs_reference_slots(
            provenance(),
            vec![],
            vec![page_intent(4)],
            vec![],
            vec![],
        )
        .unwrap(),
        9 => TeletextSavedEditorialOverlay::new_with_flash_charset_operands(
            provenance(),
            vec![],
            vec![page_intent(2), page_intent(4)],
            vec![],
            vec![],
        )
        .unwrap(),
        _ => unreachable!(),
    }
}
#[test]
fn v8_slot_and_mixed_v9_literal_bytes_stay_unchanged_beside_v10() {
    const SLOT: &str=concat!("{\"owner\":{\"page\":200,\"subcode\":16},\"command\":4,\"column\":4,",
        "\"expected\":{\"kind\":\"drcs\",\"reference\":{\"table\":\"normal\",\"subtable\":1,\"slot\":0}},",
        "\"replacement\":{\"kind\":\"drcs\",\"reference\":{\"table\":\"normal\",\"subtable\":1,\"slot\":47}}}");
    const FLASH: &str =
        concat!("{\"owner\":{\"page\":200,\"subcode\":16},\"command\":2,\"column\":2,",
        "\"expected\":{\"kind\":\"flash\",\"mode\":\"normal\",\"rate\":\"slow1_hz\"},",
        "\"replacement\":{\"kind\":\"flash\",\"mode\":\"inverted\",\"rate\":\"fast2_hz_phase3\"}}");
    let original = original();
    for (version, operands) in [(8, SLOT.to_owned()), (9, format!("{FLASH},{SLOT}"))] {
        let overlay = golden_overlay(version);
        assert_eq!(overlay.schema_version(), version);
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        let expected=format!(concat!("{{\"schema_version\":{},\"export_receipt_id\":3,\"export_receipt_digest\":\"{}\",",
            "\"original_design_id\":7,\"original_revision\":2,\"original_digest\":\"{}\",\"pages\":[],",
            "\"local_program_operands\":[{}]}}"),version,"d".repeat(64),"e".repeat(64),operands);
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
}

#[test]
fn v10_effective_role_preserves_sources_selection_and_static_separation() {
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

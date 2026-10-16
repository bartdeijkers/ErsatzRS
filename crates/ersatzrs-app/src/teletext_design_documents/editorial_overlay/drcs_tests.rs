//! Private bitmap concepts authenticate originals without granting editor authority.
use super::tests::*;
use super::*;
use ersatzrs_domain::{
    TeletextRawDataPageNumber, TeletextResourceKind, TeletextResourceSubpageIndex,
};
use serde_json::{json, Value};

fn identity() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
        TeletextResourceSubpageIndex::new(1).unwrap(),
    )
    .unwrap()
}
fn bitmap(value: u16) -> TeletextDrcsBitmap {
    TeletextDrcsBitmap::new([value; 10]).unwrap()
}
fn intent() -> TeletextDrcsBitmapOverlay {
    TeletextDrcsBitmapOverlay::new(
        identity(),
        TeletextDrcsGlyphSlot::new(0).unwrap(),
        bitmap(1),
        bitmap(3),
    )
    .unwrap()
}
fn original() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&original_with_context()).unwrap();
    let mut source: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    source["service"]["drcs"] = json!([{"identity":{"kind":"drcs","page":{"magazine":2,"page":192},"subpage":1},
        "glyphs":[{"slot":0,"bitmap":(vec![1;10])},{"slot":1,"bitmap":(vec![7;10])}]}]);
    source["service"]["page_enhancements"] = json!([{
        "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
        "local_program":{"commands":[{"kind":"set_position","position":{"row":1,"column":0}},
            {"kind":"at_column","column":3,"operation":{"kind":"foreground","colour":1}}]},
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}
    }]);
    value["documents"][0]["response"] = json!(format!(
        "{}\n \t",
        serde_json::to_string_pretty(&source).unwrap()
    ));
    value["resource_transmissions"]
        .as_array_mut()
        .unwrap()
        .push(
            serde_json::to_value(
                crate::teletext_design_documents::TeletextSavedResourceTransmission {
                    resource: identity(),
                    counter: ersatzrs_domain::TeletextResourceUpdateCounter::new(5).unwrap(),
                    update: crate::teletext_design_documents::TeletextSavedResourceUpdate::Changed,
                    last_subpage: true,
                },
            )
            .unwrap(),
        );
    bytes(&value)
}
fn mixed_intent(mixed: bool) -> TeletextSavedEditorialOverlay {
    let operands = if mixed {
        vec![TeletextLocalProgramOperandOverlay::new(
            address(16),
            1,
            3,
            TeletextColumnOperation::Foreground {
                colour: ersatzrs_domain::TeletextColourReference::new(1).unwrap(),
            },
            TeletextColumnOperation::Foreground {
                colour: ersatzrs_domain::TeletextColourReference::new(4).unwrap(),
            },
        )
        .unwrap()]
    } else {
        vec![]
    };
    TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
        provenance(),
        if mixed {
            overlay().pages().to_vec()
        } else {
            vec![]
        },
        operands,
        vec![intent()],
    )
    .unwrap()
}
fn concept() -> Value {
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original(), &mixed_intent(false), budget())
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
fn bitmap_only_and_mixed_v3_preserve_exact_original_sources_context_and_prefix() {
    let original = original();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    for mixed in [false, true] {
        let overlay = mixed_intent(mixed);
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
        let end = original
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .unwrap();
        assert_eq!(&encoded.bytes()[..end], &original[..end]);
        assert!(encoded.bytes().ends_with(&original[end..]));
        let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
        assert_eq!(after.editorial_overlay(), Some(&overlay));
        assert_eq!(after.editorial_overlay().unwrap().schema_version(), 3);
        assert_eq!(
            after.editorial_overlay().unwrap().drcs_bitmap_overlays(),
            [intent()]
        );
        assert_eq!(after.selection(), before.selection());
        assert_eq!(after.guide_source(), before.guide_source());
        assert_eq!(
            after.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(after.edit_policy(), before.edit_policy());
        for (a, b) in after.sources().iter().zip(before.sources()) {
            assert_eq!(a.bytes(), b.bytes());
        }
        let printed = format!("{:?}", after.editorial_overlay().unwrap());
        assert!(!printed.contains(&"d".repeat(64)));
        assert!(!printed.contains("expected"));
        let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
        assert_eq!(
            value["editorial_overlay"]
                .get("local_program_operands")
                .is_some(),
            mixed
        );
    }
}

#[test]
fn v3_requires_nonempty_bitmap_intent_and_forbids_version_contamination() {
    let baseline = concept();
    for version in [0, 1, 2, 4] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["schema_version"] = json!(version);
        assert!(restore(&changed).is_err());
    }
    for field in [Value::Null, json!([]), json!({})] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["drcs_bitmap_overlays"] = field;
        assert!(restore(&changed).is_err());
    }
    let mut changed = baseline.clone();
    changed["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("drcs_bitmap_overlays");
    assert!(restore(&changed).is_err());
    for field in [Value::Null, json!([])] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["local_program_operands"] = field;
        assert!(restore(&changed).is_err());
    }
    for version in [1, 2] {
        for field in [Value::Null, json!([])] {
            let mut changed = baseline.clone();
            changed["editorial_overlay"]["schema_version"] = json!(version);
            changed["editorial_overlay"]["drcs_bitmap_overlays"] = field;
            assert!(restore(&changed).is_err());
        }
    }
    assert!(
        TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
            provenance(),
            vec![],
            vec![],
            vec![]
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_drcs_bitmap_overlays(
            provenance(),
            vec![],
            vec![],
            vec![intent(), intent()]
        )
        .is_err()
    );
}

#[test]
fn bitmap_restore_requires_exact_kind_s1_slot_and_expected_pixels() {
    let baseline = concept();
    for (path, value) in [
        ("kind", json!("gdrcs")),
        ("kind", json!("pop")),
        ("logical_subpage", json!(0)),
    ] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["drcs_bitmap_overlays"][0]["identity"][path] = value;
        assert!(restore(&changed).is_err());
    }
    for (field, value) in [
        ("slot", json!(47)),
        ("slot", json!(48)),
        ("expected", json!(vec![2; 10])),
        ("replacement", json!(vec![1; 10])),
    ] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["drcs_bitmap_overlays"][0][field] = value;
        assert!(restore(&changed).is_err());
    }
    let mut missing = baseline.clone();
    missing["editorial_overlay"]["drcs_bitmap_overlays"][0]["identity"]["page"]["page"] =
        json!(193);
    assert!(restore(&missing).is_err());
    let mut duplicate = baseline.clone();
    duplicate["editorial_overlay"]["drcs_bitmap_overlays"]
        .as_array_mut()
        .unwrap()
        .push(baseline["editorial_overlay"]["drcs_bitmap_overlays"][0].clone());
    assert!(restore(&duplicate).is_err());
}

#[test]
fn bitmap_dtos_require_closed_maps_and_fixed_ten_row_twelve_bit_arrays() {
    let baseline = concept();
    for path in [vec![], vec!["identity"], vec!["identity", "page"]] {
        for invalid in [Value::Null, json!([]), json!("invalid")] {
            let mut changed = baseline.clone();
            let mut target = &mut changed["editorial_overlay"]["drcs_bitmap_overlays"][0];
            for key in &path {
                target = &mut target[*key];
            }
            *target = invalid;
            assert!(restore(&changed).is_err());
        }
        let mut changed = baseline.clone();
        let mut target = &mut changed["editorial_overlay"]["drcs_bitmap_overlays"][0];
        for key in &path {
            target = &mut target[*key];
        }
        target["extra"] = json!(true);
        assert!(restore(&changed).is_err());
    }
    for field in ["expected", "replacement"] {
        for invalid in [
            json!(vec![1; 9]),
            json!(vec![1; 11]),
            json!(vec![4096; 10]),
            json!(vec![1.5; 10]),
            Value::Null,
            json!({}),
        ] {
            let mut changed = baseline.clone();
            changed["editorial_overlay"]["drcs_bitmap_overlays"][0][field] = invalid;
            assert!(restore(&changed).is_err());
        }
    }
    let duplicate = serde_json::to_string(&baseline)
        .unwrap()
        .replace("\"slot\":0", "\"slot\":0,\"slot\":0");
    assert!(decode_teletext_design_documents(duplicate.as_bytes(), budget()).is_err());
}

#[test]
fn bitmap_preflight_bounds_vectors_fixed_rows_and_total_serialized_storage() {
    let baseline = concept();
    let mut repeated = baseline.clone();
    repeated["editorial_overlay"]["drcs_bitmap_overlays"] =
        json!(vec![
            baseline["editorial_overlay"]["drcs_bitmap_overlays"][0]
                .clone();
            9
        ]);
    let mut limited = budget();
    limited.validation.service.max_structure_items = 8;
    assert!(matches!(
        decode_teletext_design_documents(&bytes(&repeated), limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial bitmaps" | "editorial structure items",
            ..
        }
    ));
    let mut long = baseline.clone();
    long["editorial_overlay"]["drcs_bitmap_overlays"][0]["replacement"] = json!(vec![1; 11]);
    assert!(matches!(
        restore(&long).unwrap_err().reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial bitmap rows",
            ..
        }
    ));
    let original = original();
    let encoded =
        append_teletext_design_editorial_overlay(&original, &mixed_intent(false), budget())
            .unwrap();
    let mut limited = budget();
    limited.max_serialized_bytes = encoded.bytes().len() - 1;
    assert!(matches!(
        append_teletext_design_editorial_overlay(&original, &mixed_intent(false), limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "serialized",
            ..
        }
    ));
}

#[test]
fn identical_shared_subtables_are_valid_but_any_conflicting_companion_is_rejected() {
    let original = original();
    let mut shared = decode_teletext_design_documents(&original, budget())
        .unwrap()
        .into_parts()
        .documents;
    shared.extend(
        decode_teletext_design_documents(&original, budget())
            .unwrap()
            .into_parts()
            .documents,
    );
    validate_association(&mixed_intent(false), &shared, budget()).unwrap();
    let mut changed: Value = serde_json::from_slice(&original).unwrap();
    let mut source: Value =
        serde_json::from_str(changed["documents"][0]["response"].as_str().unwrap()).unwrap();
    source["service"]["drcs"][0]["glyphs"][1]["bitmap"] = json!(vec![9; 10]);
    changed["documents"][0]["response"] = json!(serde_json::to_string(&source).unwrap());
    let conflict = decode_teletext_design_documents(&bytes(&changed), budget()).unwrap();
    shared.pop();
    shared.extend(conflict.into_parts().documents);
    assert!(validate_association(&mixed_intent(false), &shared, budget()).is_err());
}

#[test]
fn v2_serialization_retains_exact_legacy_bytes_without_bitmap_field() {
    let original = original();
    let operands = mixed_intent(true).local_program_operands().to_vec();
    let overlay = TeletextSavedEditorialOverlay::new_with_local_program_operands(
        provenance(),
        vec![],
        operands,
    )
    .unwrap();
    let encoded = append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
    let legacy = format!(
        concat!(
            "{{\"schema_version\":2,\"export_receipt_id\":3,\"export_receipt_digest\":\"{}\",",
            "\"original_design_id\":7,\"original_revision\":2,\"original_digest\":\"{}\",",
            "\"pages\":[],\"local_program_operands\":[{{\"owner\":{{\"page\":200,\"subcode\":16}},",
            "\"command\":1,\"column\":3,\"expected\":{{\"kind\":\"foreground\",\"colour\":1}},",
            "\"replacement\":{{\"kind\":\"foreground\",\"colour\":4}}}}]}}"
        ),
        "d".repeat(64),
        "e".repeat(64)
    );
    let end = original
        .iter()
        .rposition(|b| !b.is_ascii_whitespace())
        .unwrap();
    let mut expected = original[..end].to_vec();
    expected.extend_from_slice(b",\"editorial_overlay\":");
    expected.extend_from_slice(legacy.as_bytes());
    expected.extend_from_slice(&original[end..]);
    assert_eq!(encoded.bytes(), expected);
    assert!(decode_teletext_design_documents(&expected, budget())
        .unwrap()
        .editorial_overlay()
        .unwrap()
        .drcs_bitmap_overlays()
        .is_empty());
}

#[test]
fn v3_effective_role_preserves_selection_expiry_sources_context_and_role_separation() {
    use crate::teletext_design_documents::{
        append_teletext_effective_design_editorial_overlay,
        decode_teletext_effective_design_documents,
    };
    let mut original: Value = serde_json::from_slice(&original()).unwrap();
    let static_selection = original
        .as_object_mut()
        .unwrap()
        .remove("selection")
        .unwrap();
    original["format_version"] = json!(2);
    original["effective_selection"] = json!({"channel_id":9,"bindings":[{"origin":"base","base":static_selection["bindings"][0],"scripted":null}],"valid_until":null});
    original["producer_bindings"] = json!([0]);
    let original = bytes(&original);
    let before = decode_teletext_effective_design_documents(&original, budget()).unwrap();
    for mixed in [false, true] {
        let overlay = mixed_intent(mixed);
        let encoded =
            append_teletext_effective_design_editorial_overlay(&original, &overlay, budget())
                .unwrap();
        assert_eq!(encoded.version(), 2);
        let end = original
            .iter()
            .rposition(|b| !b.is_ascii_whitespace())
            .unwrap();
        assert_eq!(&encoded.bytes()[..end], &original[..end]);
        let after = decode_teletext_effective_design_documents(encoded.bytes(), budget()).unwrap();
        assert_eq!(after.editorial_overlay(), Some(&overlay));
        assert_eq!(after.selection(), before.selection());
        assert_eq!(after.producer_bindings(), before.producer_bindings());
        assert_eq!(after.guide_source(), before.guide_source());
        assert_eq!(
            after.resource_transmissions(),
            before.resource_transmissions()
        );
        assert_eq!(after.edit_policy(), before.edit_policy());
        for (a, b) in after.sources().iter().zip(before.sources()) {
            assert_eq!(a.bytes(), b.bytes());
        }
        assert!(decode_teletext_design_documents(encoded.bytes(), budget()).is_err());
        assert!(append_teletext_effective_design_editorial_overlay(
            encoded.bytes(),
            &overlay,
            budget()
        )
        .is_err());
    }
}

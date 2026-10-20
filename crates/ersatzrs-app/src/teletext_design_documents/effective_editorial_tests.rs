//! Structural role-2 concepts; receipt authentication and edit permission remain native duties.

use super::{budget, Fixture, GENERATION};
use crate::teletext_design_documents::{
    append_teletext_design_editorial_overlay, append_teletext_effective_design_editorial_overlay,
    decode_teletext_design_documents, decode_teletext_effective_design_documents,
    encode_teletext_design_documents, TeletextDesignDocumentFailure, TeletextDesignDocumentInput,
    TeletextSavedEditPolicy, TeletextSavedEditorialOverlay, TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::{
    TeletextChannelSelectionSnapshot, TeletextPageAddress, TeletextPageNumber,
    TeletextPhysicalBaseOverlay, TeletextPhysicalBaseRow, TeletextSubcode,
};
use serde_json::{json, Value};

pub(super) fn overlay() -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new(
        TeletextSavedEditorialProvenance {
            export_receipt_id: 3,
            export_receipt_digest: "d".repeat(64),
            original_design_id: 7,
            original_revision: 2,
            original_digest: "e".repeat(64),
        },
        [100, 200, 300]
            .into_iter()
            .map(|number| {
                let mut cells = [b' '; 40];
                cells[0] = 1;
                cells[1] = b'X';
                TeletextPhysicalBaseOverlay::new(
                    TeletextPageAddress {
                        page: TeletextPageNumber::new(number).unwrap(),
                        subcode: TeletextSubcode::new(16).unwrap(),
                    },
                    vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}

#[test]
fn v25_object_resources_effective_storage_keeps_source_ordinal_and_scripted_membership() {
    use crate::teletext_design_documents::{
        TeletextSavedObjectResourceAddition, TeletextSavedResourceTransmission,
        TeletextSavedResourceUpdate,
    };
    use ersatzrs_domain::{
        TeletextObjectResourceAddition, TeletextRawDataPageNumber, TeletextResourceIdentity,
        TeletextResourceKind, TeletextResourceSubpageIndex, TeletextResourceUpdateCounter,
    };
    let fixture = Fixture::mixed();
    let original = fixture.encode();
    let before = decode_teletext_effective_design_documents(original.bytes(), budget()).unwrap();
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Gpop,
        TeletextRawDataPageNumber::new(3, 0xa7).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap();
    let addition = TeletextSavedObjectResourceAddition::new(
        2,
        TeletextObjectResourceAddition::new(resource, vec![], budget().validation.service).unwrap(),
        TeletextSavedResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(6).unwrap(),
            update: TeletextSavedResourceUpdate::Changed,
            last_subpage: false,
        },
    )
    .unwrap();
    let edits = overlay()
        .with_object_resource_additions(vec![addition.clone()])
        .unwrap();
    let saved =
        append_teletext_effective_design_editorial_overlay(original.bytes(), &edits, budget())
            .unwrap();
    let after = decode_teletext_effective_design_documents(saved.bytes(), budget()).unwrap();
    assert_eq!(after.selection(), before.selection());
    assert_eq!(after.producer_bindings(), &[0, 2, 3]);
    assert_eq!(
        after
            .editorial_overlay()
            .unwrap()
            .object_resource_additions(),
        [addition]
    );
    assert_eq!(after.editorial_overlay(), Some(&edits));
    assert_eq!(
        after.resource_transmissions(),
        before.resource_transmissions()
    );
    for (a, b) in after.sources().iter().zip(before.sources()) {
        assert_eq!(a.bytes(), b.bytes());
        assert_eq!(a.authoring(), b.authoring());
    }
    // Document ordinal2 is producer binding3; it must never be inferred as binding2.
    assert_eq!(after.producer_bindings()[2], 3);
}

pub(super) fn assert_preserved_original(original: &[u8]) {
    let before = decode_teletext_effective_design_documents(original, budget()).unwrap();
    assert!(before.editorial_overlay().is_none());
    let concept =
        append_teletext_effective_design_editorial_overlay(original, &overlay(), budget()).unwrap();
    assert_eq!(concept.version(), 2);
    let end = original
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .unwrap();
    assert_eq!(&concept.bytes()[..end], &original[..end]);
    assert!(concept.bytes().ends_with(&original[end..]));
    let after = decode_teletext_effective_design_documents(concept.bytes(), budget()).unwrap();
    assert_eq!(after.selection(), before.selection());
    assert_eq!(after.producer_bindings(), &[0, 2, 3]);
    assert_eq!(after.producer_bindings(), before.producer_bindings());
    assert_eq!(after.generation(), before.generation());
    assert_eq!(after.entry_addon_id(), before.entry_addon_id());
    assert_eq!(after.entry(), before.entry());
    assert_eq!(after.v1_region(), before.v1_region());
    assert_eq!(
        after.resource_transmissions(),
        before.resource_transmissions()
    );
    assert_eq!(after.guide_source(), before.guide_source());
    assert_eq!(after.edit_policy(), TeletextSavedEditPolicy::OriginalOnly);
    assert_eq!(after.editorial_overlay(), Some(&overlay()));
    for (a, b) in before.sources().iter().zip(after.sources()) {
        assert_eq!(a.bytes(), b.bytes());
        assert_eq!(a.contract(), b.contract());
        assert_eq!(a.generation(), b.generation());
        assert_eq!(a.authoring(), b.authoring());
        assert_eq!(
            after.guide_inputs().unwrap().request(a.contract()),
            before.guide_inputs().unwrap().request(a.contract())
        );
    }
    let parts = after.into_parts();
    assert_eq!(parts.editorial_overlay, Some(overlay()));
    for private in [
        GENERATION,
        "night-value-sentinel",
        "UNRESOLVED_ENV_FIXTURE",
        "Café original fixture",
        "night-theme-fixture",
        &"d".repeat(64),
        &"e".repeat(64),
    ] {
        assert!(!format!("{parts:?}").contains(private));
    }
}

fn concept_value() -> Value {
    let encoded = Fixture::mixed().encode();
    serde_json::from_slice(
        append_teletext_effective_design_editorial_overlay(encoded.bytes(), &overlay(), budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}

#[test]
fn effective_editorial_retains_expired_mixed_origins_exact_sources_and_original_whitespace() {
    let mut original = Fixture::mixed().encode().into_bytes();
    original.extend_from_slice(b"\r\n \t");
    assert_preserved_original(&original);
    let restored = decode_teletext_effective_design_documents(&original, budget()).unwrap();
    assert!(restored.selection().valid_until.unwrap() < chrono::Utc::now());
}

#[test]
fn effective_editorial_rejects_cross_role_reoverlay_and_edit_policy_escalation() {
    let fixture = Fixture::mixed();
    let original = fixture.encode();
    let concept =
        append_teletext_effective_design_editorial_overlay(original.bytes(), &overlay(), budget())
            .unwrap();
    assert_eq!(
        append_teletext_effective_design_editorial_overlay(concept.bytes(), &overlay(), budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::EditorialOverlay
    );
    assert!(
        append_teletext_design_editorial_overlay(original.bytes(), &overlay(), budget()).is_err()
    );
    assert!(decode_teletext_design_documents(concept.bytes(), budget()).is_err());
    let selection = TeletextChannelSelectionSnapshot {
        channel_id: 9,
        bindings: vec![],
    };
    let legacy = encode_teletext_design_documents(
        TeletextDesignDocumentInput {
            selection: &selection,
            generation: GENERATION,
            sources: &[],
            entry_addon_id: None,
            entry: None,
            v1_region: None,
            resource_transmissions: &[],
        },
        budget(),
    )
    .unwrap();
    assert!(append_teletext_effective_design_editorial_overlay(
        legacy.bytes(),
        &overlay(),
        budget()
    )
    .is_err());
    let mut escalated: Value = serde_json::from_slice(original.bytes()).unwrap();
    escalated["edit_policy"]["kind"] = json!("editable");
    assert_eq!(
        append_teletext_effective_design_editorial_overlay(
            &serde_json::to_vec(&escalated).unwrap(),
            &overlay(),
            budget()
        )
        .unwrap_err()
        .reason,
        TeletextDesignDocumentFailure::EditPolicy
    );
}

#[test]
fn effective_editorial_rejects_invalid_shape_provenance_owner_and_producer_mapping() {
    let mutations: [fn(&mut Value); 12] = [
        |v| v["editorial_overlay"] = Value::Null,
        |v| v["editorial_overlay"] = json!([]),
        |v| v["editorial_overlay"]["edit_permission"] = json!(true),
        |v| v["editorial_overlay"]["schema_version"] = json!(2),
        |v| v["editorial_overlay"]["original_design_id"] = json!(0),
        |v| v["editorial_overlay"]["original_revision"] = json!(-1),
        |v| v["editorial_overlay"]["original_digest"] = json!("F".repeat(64)),
        |v| v["editorial_overlay"]["pages"][0]["owner"]["page"] = json!(222),
        |v| v["editorial_overlay"]["pages"][0]["owner"]["subcode"] = json!(17),
        |v| v["editorial_overlay"]["pages"][0]["rows"][0]["row"] = json!(0),
        |v| v["editorial_overlay"]["pages"][0]["rows"][0]["cells"][39] = json!(128),
        |v| v["producer_bindings"] = json!([0, 1, 3]),
    ];
    for mutation in mutations {
        let mut value = concept_value();
        mutation(&mut value);
        let error = decode_teletext_effective_design_documents(
            &serde_json::to_vec(&value).unwrap(),
            budget(),
        )
        .unwrap_err();
        assert!(!format!("{error:?}").contains("fixture"));
    }
    let duplicate = serde_json::to_string(&concept_value()).unwrap().replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    assert!(decode_teletext_effective_design_documents(duplicate.as_bytes(), budget()).is_err());
}

#[test]
fn effective_editorial_vector_and_cell_limits_precede_owned_overlay_decode() {
    let value = concept_value();
    let bytes = serde_json::to_vec(&value).unwrap();
    for (dimension, policy) in [
        ("editorial pages", {
            let mut b = budget();
            b.validation.service.max_records = 2;
            b
        }),
        ("editorial structure items", {
            let mut b = budget();
            b.validation.service.max_structure_items = 5;
            b
        }),
        ("editorial content bytes", {
            let mut b = budget();
            b.validation.service.max_base_content_bytes = 119;
            b
        }),
    ] {
        assert!(
            matches!(decode_teletext_effective_design_documents(&bytes, policy).unwrap_err().reason,
            TeletextDesignDocumentFailure::Budget { dimension: actual, .. } if actual == dimension)
        );
    }
    let mut row = value;
    row["editorial_overlay"]["pages"][0]["rows"][0]["cells"] = json!(vec![32; 41]);
    assert!(matches!(
        decode_teletext_effective_design_documents(&serde_json::to_vec(&row).unwrap(), budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial cells",
            ..
        }
    ));
    let original = Fixture::mixed().encode();
    let mut limited = budget();
    limited.max_serialized_bytes = original.bytes().len();
    assert!(matches!(
        append_teletext_effective_design_editorial_overlay(original.bytes(), &overlay(), limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "serialized",
            ..
        }
    ));
}

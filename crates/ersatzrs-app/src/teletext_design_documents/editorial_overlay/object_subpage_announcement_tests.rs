//! V26 structural dependency storage, not selection/effect/publication acceptance.
use super::base_text_tests::{edit, native_edit, original};
use super::object_resource_storage_tests::additions;
use super::tests::{address, budget, bytes, provenance};
use super::*;
use crate::teletext_design_documents::MAX_BINDINGS;
use ersatzrs_domain::*;
use serde_json::{json, Value};

fn announcement(resource: TeletextResourceIdentity) -> TeletextSavedObjectSubpageAnnouncement {
    TeletextSavedObjectSubpageAnnouncement::new(
        0,
        TeletextObjectSubpageAnnouncement::new(address(16), resource).unwrap(),
    )
    .unwrap()
}
fn overlay(banks: bool, native: bool) -> TeletextSavedEditorialOverlay {
    let value = TeletextSavedEditorialOverlay::new_with_base_text_edits(
        provenance(),
        vec![if native { native_edit() } else { edit() }],
    )
    .unwrap();
    if banks {
        value
            .with_object_resource_additions(additions())
            .unwrap()
            .with_object_subpage_announcements(vec![announcement(
                additions()[1].addition().resource(),
            )])
            .unwrap()
    } else {
        let resource = TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(15).unwrap(),
        )
        .unwrap();
        value
            .with_object_subpage_announcements(vec![announcement(resource)])
            .unwrap()
    }
}
fn saved(banks: bool, native: bool) -> Vec<u8> {
    append_teletext_design_editorial_overlay(&original(true), &overlay(banks, native), budget())
        .unwrap()
        .into_bytes()
}
#[test]
fn v26_object_subpage_announcements_roundtrip_with_banks_and_prior_roles_preserves_exact_sources() {
    let source = original(true);
    let before = decode_teletext_design_documents(&source, budget()).unwrap();
    // These wrap the original row-only and explicit-definition roles, rather
    // than laundering them through Base-text inference or a new-bank envelope.
    for previous in [
        super::tests::overlay(),
        TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
            provenance(),
            vec![super::resource_object_addition_tests::addition(false, 15)],
        )
        .unwrap(),
    ] {
        let prior = previous.schema_version();
        let dependency = overlay(false, false).object_subpage_announcements()[0];
        let combined = previous
            .with_object_subpage_announcements(vec![dependency])
            .unwrap();
        let data = append_teletext_design_editorial_overlay(&source, &combined, budget()).unwrap();
        let value: Value = serde_json::from_slice(data.bytes()).unwrap();
        assert_eq!(value["editorial_overlay"]["prior_schema_version"], prior);
        assert_eq!(
            decode_teletext_design_documents(data.bytes(), budget())
                .unwrap()
                .editorial_overlay(),
            Some(&combined)
        );
    }
    for banks in [false, true] {
        for native in [false, true] {
            let overlay = overlay(banks, native);
            assert_eq!(overlay.schema_version(), 26);
            let data =
                append_teletext_design_editorial_overlay(&source, &overlay, budget()).unwrap();
            let end = source
                .iter()
                .rposition(|b| !b.is_ascii_whitespace())
                .unwrap();
            assert_eq!(&data.bytes()[..end], &source[..end]);
            assert!(data.bytes().ends_with(&source[end..]));
            let value: Value = serde_json::from_slice(data.bytes()).unwrap();
            assert_eq!(
                value["editorial_overlay"]["prior_schema_version"],
                if native { 24 } else { 23 }
            );
            assert_eq!(
                value["editorial_overlay"]
                    .get("object_resource_additions")
                    .is_some(),
                banks
            );
            let after = decode_teletext_design_documents(data.bytes(), budget()).unwrap();
            assert_eq!(after.editorial_overlay(), Some(&overlay));
            assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
            assert_eq!(
                after.sources()[0].authoring(),
                before.sources()[0].authoring()
            );
            assert_eq!(after.selection(), before.selection());
            assert_eq!(
                after.resource_transmissions(),
                before.resource_transmissions()
            );
            assert_eq!(
                append_teletext_design_editorial_overlay(
                    &source,
                    after.editorial_overlay().unwrap(),
                    budget()
                )
                .unwrap()
                .bytes(),
                data.bytes()
            );
        }
    }
}
#[test]
fn v26_object_subpage_announcements_keep_legacy_roles_closed_and_refuse_invalid_shapes() {
    let source: Value = serde_json::from_slice(&saved(true, true)).unwrap();
    for version in 1..=25 {
        for role in [
            json!([]),
            Value::Null,
            source["editorial_overlay"]["object_subpage_announcements"].clone(),
        ] {
            let mut value = source.clone();
            value["editorial_overlay"]["schema_version"] = json!(version);
            value["editorial_overlay"]["object_subpage_announcements"] = role;
            assert!(
                decode_teletext_design_documents(&bytes(&value), budget()).is_err(),
                "version {version}"
            );
        }
    }
    for role in [json!([]), Value::Null, json!({}), json!([[]])] {
        let mut value = source.clone();
        value["editorial_overlay"]["object_subpage_announcements"] = role;
        assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    }
    for (pointer, replacement) in [
        ("/editorial_overlay/prior_schema_version", json!(25)),
        (
            "/editorial_overlay/object_subpage_announcements/0/document_index",
            json!(1),
        ),
        (
            "/editorial_overlay/object_subpage_announcements/0/display",
            json!([200, 16]),
        ),
        (
            "/editorial_overlay/object_subpage_announcements/0/display/subcode",
            json!(32),
        ),
        (
            "/editorial_overlay/object_subpage_announcements/0/resource/logical_subpage",
            json!(14),
        ),
        (
            "/editorial_overlay/object_subpage_announcements/0/resource/kind",
            json!("drcs"),
        ),
    ] {
        let mut value = source.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(
            decode_teletext_design_documents(&bytes(&value), budget()).is_err(),
            "{pointer}"
        );
    }
    let mut value = source.clone();
    value["editorial_overlay"]["object_subpage_announcements"][0]["extra"] = json!(true);
    assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    let mut value = source.clone();
    let duplicate = value["editorial_overlay"]["object_subpage_announcements"][0].clone();
    value["editorial_overlay"]["object_subpage_announcements"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    let compact = String::from_utf8(saved(false, false)).unwrap();
    let duplicate = compact.replacen(
        "\"document_index\":0",
        "\"document_index\":0,\"document_index\":0",
        1,
    );
    assert_ne!(duplicate, compact);
    assert!(decode_teletext_design_documents(duplicate.as_bytes(), budget()).is_err());
    let mut value = source;
    value["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("object_subpage_announcements");
    assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
}
#[test]
fn v26_object_subpage_announcements_preflight_zero_and_combined_structure_before_dto_decode() {
    let data = saved(false, false);
    for maximum in [0, 1] {
        let mut limit = budget();
        limit.validation.service.max_structure_items = maximum;
        let error = decode_teletext_design_documents(&data, limit).unwrap_err();
        assert!(
            matches!(
                error.reason,
                TeletextDesignDocumentFailure::Budget {
                    dimension: "editorial structure items"
                        | "editorial object subpage announcements",
                    ..
                }
            ),
            "{:?}",
            error.reason
        );
    }
    let mut value: Value = serde_json::from_slice(&data).unwrap();
    value["editorial_overlay"]["object_subpage_announcements"] = json!([null, null]);
    let mut limit = budget();
    limit.validation.service.max_structure_items = 1;
    let error = decode_teletext_design_documents(&bytes(&value), limit).unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDesignDocumentFailure::Budget { .. }
    ));
    assert!(append_teletext_design_editorial_overlay(
        &original(false),
        &overlay(false, false),
        budget()
    )
    .is_err());
    let one = announcement(additions()[0].addition().resource());
    assert!(overlay(false, false)
        .with_object_subpage_announcements(vec![one])
        .is_err());
    assert!(TeletextSavedObjectSubpageAnnouncement::new(MAX_BINDINGS, one.announcement()).is_err());
}

#[test]
fn v26_object_subpage_announcements_dependency_only_keeps_v1_empty_rule_enclosed() {
    let resource = overlay(false, false).object_subpage_announcements()[0];
    let overlay = TeletextSavedEditorialOverlay::new_with_object_subpage_announcements(
        provenance(),
        vec![resource],
    )
    .unwrap();
    let data =
        append_teletext_design_editorial_overlay(&original(true), &overlay, budget()).unwrap();
    let decoded = decode_teletext_design_documents(data.bytes(), budget()).unwrap();
    assert_eq!(decoded.editorial_overlay(), Some(&overlay));
    let mut value: Value = serde_json::from_slice(data.bytes()).unwrap();
    assert_eq!(value["editorial_overlay"]["prior_schema_version"], 1);
    let role = value["editorial_overlay"].as_object_mut().unwrap();
    role.remove("object_subpage_announcements");
    role.remove("prior_schema_version");
    role.insert("schema_version".into(), json!(1));
    assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
}

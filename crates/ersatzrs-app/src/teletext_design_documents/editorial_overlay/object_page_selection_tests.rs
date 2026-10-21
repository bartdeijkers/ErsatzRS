//! V27 structural storage preserves full selected provenance, not replay authority.
use super::base_text_tests::original;
use super::object_resource_storage_tests::additions;
use super::tests::{address, budget, bytes, provenance};
use super::*;
use crate::teletext_design_documents::MAX_BINDINGS;
use ersatzrs_domain::*;
use serde_json::{json, Value};

fn selection(
    provenance: TeletextMotLinkProvenance,
    requirements: TeletextMotResolutionRequirements,
    original_mot: Option<TeletextMotResolutionCandidate>,
) -> TeletextSavedObjectPageSelectionChange {
    let old = TeletextRawDataPageNumber::new(2, 0xa0).unwrap();
    let target = additions()[1].addition().resource().page();
    let change = TeletextObjectPageSelectionChange::new(
        address(16),
        TeletextX27LinkRole::Pop,
        None,
        TeletextMotRoleResolution::Selected {
            target: old,
            requirements,
            provenance,
            original_mot,
        },
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page { page: target },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(0x8001),
        )
        .unwrap(),
    )
    .unwrap();
    TeletextSavedObjectPageSelectionChange::new(0, change).unwrap()
}
fn selected() -> TeletextSavedObjectPageSelectionChange {
    selection(
        TeletextMotLinkProvenance::X27 {
            display: address(16),
            role: TeletextX27LinkRole::Pop,
        },
        TeletextMotResolutionRequirements::X27Mask(TeletextSubpageRequirementMask::new(0x8001)),
        None,
    )
}
fn overlay() -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_object_page_selections(provenance(), vec![selected()])
        .unwrap()
        .with_object_resource_additions(additions())
        .unwrap()
}
pub(super) fn saved() -> Vec<u8> {
    append_teletext_design_editorial_overlay(&original(true), &overlay(), budget())
        .unwrap()
        .into_bytes()
}
#[test]
fn v27_object_page_selections_roundtrip_full_provenance_and_prior_roles_without_source_changes() {
    let identity = TeletextResourceIdentity::new(
        TeletextResourceKind::Mot,
        TeletextRawDataPageNumber::new(1, 0xfe).unwrap(),
        TeletextResourceSubpageIndex::new(7).unwrap(),
    )
    .unwrap();
    let role = TeletextMotLinkRole::Pop {
        index: TeletextMotPublicLinkIndex::new(7).unwrap(),
    };
    let mot = TeletextMotLinkProvenance::Mot { identity, role };
    let candidate = TeletextMotResolutionCandidate {
        target: TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
        extent: TeletextMotSubpageExtent::new(15).unwrap(),
        supersedable: true,
        provenance: mot,
    };
    let source = original(true);
    let before = decode_teletext_design_documents(&source, budget()).unwrap();
    for selected in [
        selected(),
        selection(
            mot,
            TeletextMotResolutionRequirements::MotExtent(candidate.extent),
            Some(candidate),
        ),
        selection(
            TeletextMotLinkProvenance::MotPageWithX27Mask {
                identity,
                role,
                display: address(16),
            },
            TeletextMotResolutionRequirements::X27Mask(TeletextSubpageRequirementMask::new(0xffff)),
            Some(TeletextMotResolutionCandidate {
                supersedable: false,
                ..candidate
            }),
        ),
    ] {
        for previous in [
            None,
            Some(super::tests::overlay()),
            Some(
                TeletextSavedEditorialOverlay::new_with_base_text_edits(
                    provenance(),
                    vec![super::base_text_tests::native_edit()],
                )
                .unwrap(),
            ),
            Some(
                TeletextSavedEditorialOverlay::new_with_resource_object_definition_additions(
                    provenance(),
                    vec![super::resource_object_addition_tests::addition(false, 15)],
                )
                .unwrap(),
            ),
        ] {
            let combined = match previous {
                Some(prior) => prior.with_object_page_selections(vec![selected]).unwrap(),
                None => TeletextSavedEditorialOverlay::new_with_object_page_selections(
                    provenance(),
                    vec![selected],
                )
                .unwrap(),
            }
            .with_object_resource_additions(additions())
            .unwrap();
            let data =
                append_teletext_design_editorial_overlay(&source, &combined, budget()).unwrap();
            let decoded = decode_teletext_design_documents(data.bytes(), budget()).unwrap();
            assert_eq!(combined.schema_version(), 27);
            assert_eq!(decoded.editorial_overlay(), Some(&combined));
            assert_eq!(decoded.sources()[0].bytes(), before.sources()[0].bytes());
            assert_eq!(
                decoded.sources()[0].authoring(),
                before.sources()[0].authoring()
            );
            assert_eq!(decoded.selection(), before.selection());
            assert_eq!(
                decoded.resource_transmissions(),
                before.resource_transmissions()
            );
            assert_eq!(
                append_teletext_design_editorial_overlay(
                    &source,
                    decoded.editorial_overlay().unwrap(),
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
fn v27_object_page_selections_keep_all_old_schemas_closed_and_reject_null_unknown_duplicate_fields()
{
    let source: Value = serde_json::from_slice(&saved()).unwrap();
    for version in 1..=26 {
        let mut value = source.clone();
        value["editorial_overlay"]["schema_version"] = json!(version);
        assert!(
            decode_teletext_design_documents(&bytes(&value), budget()).is_err(),
            "version {version}"
        );
    }
    for path in [
        "/editorial_overlay/object_page_selections",
        "/editorial_overlay/object_page_selections/0",
        "/editorial_overlay/object_page_selections/0/expected_links",
        "/editorial_overlay/object_page_selections/0/expected_role",
        "/editorial_overlay/object_page_selections/0/expected_role/original_mot",
        "/editorial_overlay/object_page_selections/0/expected_role/provenance",
        "/editorial_overlay/object_page_selections/0/expected_role/requirements",
    ] {
        let mut value = source.clone();
        *value.pointer_mut(path).unwrap() = Value::Null;
        assert!(
            decode_teletext_design_documents(&bytes(&value), budget()).is_err(),
            "{path}"
        );
    }
    for path in [
        "/editorial_overlay/object_page_selections/0",
        "/editorial_overlay/object_page_selections/0/expected_role",
        "/editorial_overlay/object_page_selections/0/expected_role/provenance",
        "/editorial_overlay/object_page_selections/0/expected_role/requirements",
    ] {
        let mut value = source.clone();
        value
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("uploaded_authority".into(), json!(true));
        assert!(
            decode_teletext_design_documents(&bytes(&value), budget()).is_err(),
            "{path}"
        );
    }
    for (old, new) in [
        (
            "\"document_index\":0",
            "\"document_index\":0,\"document_index\":0",
        ),
        ("\"target\":", "\"target\":null,\"target\":"),
        ("\"kind\":\"x27\"", "\"kind\":\"x27\",\"kind\":\"x27\""),
    ] {
        let text = String::from_utf8(saved()).unwrap();
        assert!(text.contains(old));
        assert!(
            decode_teletext_design_documents(text.replacen(old, new, 1).as_bytes(), budget())
                .is_err()
        );
    }
    let mut value = source.clone();
    value["editorial_overlay"]["object_page_selections"] = json!([]);
    assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    let mut value = source.clone();
    value["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("object_page_selections");
    assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
    let mut value = source;
    let duplicate = value["editorial_overlay"]["object_page_selections"][0].clone();
    value["editorial_overlay"]["object_page_selections"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(decode_teletext_design_documents(&bytes(&value), budget()).is_err());
}
#[test]
fn v27_object_page_selections_check_bounds_membership_and_lexical_preflight_before_dto() {
    let source: Value = serde_json::from_slice(&saved()).unwrap();
    for (path, bad) in [
        (
            "/editorial_overlay/object_page_selections/0/document_index",
            json!(MAX_BINDINGS),
        ),
        (
            "/editorial_overlay/object_page_selections/0/role",
            json!("drcs"),
        ),
        (
            "/editorial_overlay/object_page_selections/0/display/page",
            json!(999),
        ),
        (
            "/editorial_overlay/object_page_selections/0/expected_role/requirements/value",
            json!(65536),
        ),
    ] {
        let mut value = source.clone();
        *value.pointer_mut(path).unwrap() = bad;
        assert!(
            decode_teletext_design_documents(&bytes(&value), budget()).is_err(),
            "{path}"
        );
    }
    let mut value = source.clone();
    value["editorial_overlay"]["object_page_selections"][0]["expected_links"] = json!([null, null]);
    let error = decode_teletext_design_documents(&bytes(&value), budget()).unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDesignDocumentFailure::Budget { .. }
    ));
    let mut value = source;
    value["editorial_overlay"]["object_page_selections"] = json!([null]);
    let mut zero = budget();
    zero.validation.service.max_structure_items = 0;
    let error = decode_teletext_design_documents(&bytes(&value), zero).unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDesignDocumentFailure::Budget { .. }
    ));
    assert!(
        TeletextSavedObjectPageSelectionChange::new(MAX_BINDINGS, selected().change()).is_err()
    );
    assert!(overlay()
        .with_object_page_selections(vec![selected()])
        .is_err());
    assert!(
        TeletextSavedEditorialOverlay::new_with_object_page_selections(
            provenance(),
            vec![selected(), selected()]
        )
        .is_err()
    );
}

#[test]
fn v27_object_page_selections_preserve_present_links_and_announcements_and_require_every_owned_s1()
{
    let original_change = selected().change();
    let unspecified = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let old = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0x8000),
    )
    .unwrap();
    let links = TeletextX27Level25Links::new(unspecified, old, unspecified, unspecified).unwrap();
    let change = TeletextObjectPageSelectionChange::new(
        original_change.display(),
        original_change.role(),
        Some(links),
        original_change.expected_role(),
        original_change.replacement(),
    )
    .unwrap();
    let dependency = TeletextSavedObjectSubpageAnnouncement::new(
        0,
        TeletextObjectSubpageAnnouncement::new(address(16), additions()[2].addition().resource())
            .unwrap(),
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_object_page_selections(
        provenance(),
        vec![TeletextSavedObjectPageSelectionChange::new(0, change).unwrap()],
    )
    .unwrap()
    .with_object_resource_additions(additions())
    .unwrap()
    .with_object_subpage_announcements(vec![dependency])
    .unwrap();
    let data =
        append_teletext_design_editorial_overlay(&original(true), &overlay, budget()).unwrap();
    let decoded = decode_teletext_design_documents(data.bytes(), budget()).unwrap();
    assert_eq!(decoded.editorial_overlay(), Some(&overlay));
    assert_eq!(
        decoded
            .editorial_overlay()
            .unwrap()
            .object_page_selections()[0]
            .change()
            .expected_links(),
        Some(links)
    );

    let mut missing_s1 = additions();
    missing_s1.pop();
    let missing = TeletextSavedEditorialOverlay::new_with_object_page_selections(
        provenance(),
        vec![selected()],
    )
    .unwrap()
    .with_object_resource_additions(missing_s1)
    .unwrap();
    assert!(append_teletext_design_editorial_overlay(&original(true), &missing, budget()).is_err());
    let original_target = TeletextObjectPageSelectionChange::new(
        address(16),
        TeletextX27LinkRole::Pop,
        None,
        TeletextMotRoleResolution::Selected {
            target: additions()[1].addition().resource().page(),
            requirements: TeletextMotResolutionRequirements::X27Mask(
                TeletextSubpageRequirementMask::new(1),
            ),
            provenance: TeletextMotLinkProvenance::X27 {
                display: address(16),
                role: TeletextX27LinkRole::Pop,
            },
            original_mot: None,
        },
        old,
    )
    .unwrap();
    let existing_only = TeletextSavedEditorialOverlay::new_with_object_page_selections(
        provenance(),
        vec![TeletextSavedObjectPageSelectionChange::new(0, original_target).unwrap()],
    )
    .unwrap();
    let data = append_teletext_design_editorial_overlay(&original(true), &existing_only, budget())
        .unwrap();
    assert_eq!(
        decode_teletext_design_documents(data.bytes(), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&existing_only)
    );
}
#[test]
fn v27_object_page_selections_nested_provenance_bounds_and_optional_roles_remain_closed() {
    let source: Value = serde_json::from_slice(&saved()).unwrap();
    for role in ["object_resource_additions", "object_subpage_announcements"] {
        for value in [Value::Null, json!([])] {
            let mut changed = source.clone();
            changed["editorial_overlay"][role] = value;
            assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
        }
    }
    for (path, value) in [
        (
            "/editorial_overlay/object_page_selections/0/expected_role/requirements",
            json!({"kind":"mot_extent","value":16}),
        ),
        (
            "/editorial_overlay/object_page_selections/0/expected_role/provenance",
            json!({"kind":"mot","identity": additions()[0].addition().resource(),"role":{"kind":"pop","index":0}}),
        ),
        (
            "/editorial_overlay/object_page_selections/0/expected_role/original_mot",
            json!([{"target":{"magazine":2,"page":160},"extent":16,"supersedable":true,"provenance":{"kind":"x27","display":{"page":200,"subcode":16},"role":"pop"}}]),
        ),
    ] {
        let mut changed = source.clone();
        *changed.pointer_mut(path).unwrap() = value;
        assert!(decode_teletext_design_documents(&bytes(&changed), budget()).is_err());
    }
}

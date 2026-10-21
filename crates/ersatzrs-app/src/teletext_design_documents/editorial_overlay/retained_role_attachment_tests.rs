//! V31 retains explicit first-attachment intent without importing replay authority.
use super::main_resource_invocation_tests as main_calls;
use super::tests::{address, budget, bytes, provenance};
use super::*;
use ersatzrs_domain::*;
use serde_json::{json, Value};

fn page(role: TeletextX27LinkRole) -> TeletextRawDataPageNumber {
    if matches!(role, TeletextX27LinkRole::Pop | TeletextX27LinkRole::Gpop) {
        super::resource_object_operand_tests::identity(role == TeletextX27LinkRole::Gpop, 15)
            .resource()
            .unwrap()
            .page()
    } else {
        TeletextRawDataPageNumber::new(
            2,
            if role == TeletextX27LinkRole::Drcs {
                0xd0
            } else {
                0xd1
            },
        )
        .unwrap()
    }
}
fn source() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&main_calls::original()).unwrap();
    let mut response: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    response["service"]["page_enhancements"].as_array_mut().unwrap().push(json!({
        "address":address(17),"presentation":null,"local_program":null,"compositional_links":null,
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}
    }));
    for role in [TeletextX27LinkRole::Drcs, TeletextX27LinkRole::Gdrcs] {
        let identity = TeletextResourceIdentity::new(
            role.kind(),
            page(role),
            TeletextResourceSubpageIndex::new(1).unwrap(),
        )
        .unwrap();
        response["service"]["drcs"]
            .as_array_mut()
            .unwrap()
            .push(json!({
                "identity":{"kind":role.kind(),"page":page(role),"subpage":1},
                "glyphs":[{"slot":0,"bitmap":vec![3;10]}]
            }));
        value["resource_transmissions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"resource":identity,"counter":5,"update":"changed","last_subpage":true}));
    }
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    let data = bytes(&value);
    decode_teletext_design_documents(&data, budget())
        .expect("retained banks with no selected display roles must parse");
    data
}
fn attachment(role: TeletextX27LinkRole) -> TeletextSavedObjectPageSelectionChange {
    TeletextSavedObjectPageSelectionChange::new(
        0,
        TeletextObjectPageSelectionChange::new_attachment(
            address(17),
            role,
            None,
            TeletextX27LinkDeclaration::new(
                TeletextX27LinkTarget::Page { page: page(role) },
                TeletextX27LinkValidity::Level25Only,
                TeletextSubpageRequirementMask::new(
                    if matches!(role, TeletextX27LinkRole::Drcs | TeletextX27LinkRole::Gdrcs) {
                        2
                    } else {
                        0x8001
                    },
                ),
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn selected(role: TeletextX27LinkRole) -> TeletextSavedObjectPageSelectionChange {
    let change = attachment(role).change();
    TeletextSavedObjectPageSelectionChange::new(
        0,
        TeletextObjectPageSelectionChange::new(
            address(17),
            role,
            None,
            TeletextMotRoleResolution::Selected {
                target: TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
                requirements: TeletextMotResolutionRequirements::X27Mask(
                    change.replacement().required_subpages(),
                ),
                provenance: TeletextMotLinkProvenance::X27 {
                    display: address(17),
                    role,
                },
                original_mot: None,
            },
            change.replacement(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn pure() -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_object_page_selections(
        provenance(),
        [
            TeletextX27LinkRole::Pop,
            TeletextX27LinkRole::Gpop,
            TeletextX27LinkRole::Drcs,
            TeletextX27LinkRole::Gdrcs,
        ]
        .into_iter()
        .map(attachment)
        .collect(),
    )
    .unwrap()
}
fn saved(overlay: &TeletextSavedEditorialOverlay) -> Vec<u8> {
    append_teletext_design_editorial_overlay(&source(), overlay, budget())
        .unwrap()
        .into_bytes()
}
#[test]
fn v31_retained_role_attachments_roundtrip_all_four_roles_without_changing_sources() {
    let original = source();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    for overlay in [
        pure(),
        super::tests::overlay()
            .with_object_page_selections(pure().object_page_selections().to_vec())
            .unwrap(),
        pure()
            .with_main_program_presence_overlays(vec![TeletextMainProgramPresenceOverlay::new(
                address(17),
                None,
                Some(TeletextEnhancementProgram::new(vec![]).unwrap()),
            )
            .unwrap()])
            .unwrap(),
    ] {
        let data = saved(&overlay);
        let after = decode_teletext_design_documents(&data, budget()).unwrap();
        assert_eq!(overlay.schema_version(), 31);
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
                &original,
                after.editorial_overlay().unwrap(),
                budget()
            )
            .unwrap()
            .bytes(),
            data
        );
        let json: Value = serde_json::from_slice(&data).unwrap();
        for change in json["editorial_overlay"]["object_page_selections"]
            .as_array()
            .unwrap()
        {
            assert_eq!(change["expected_role"], json!({"unselected":true}));
        }
    }
}
#[test]
fn v31_retained_role_attachments_seal_marker_versions_and_envelope_presence() {
    let original: Value = serde_json::from_slice(&saved(&pure())).unwrap();
    for version in 1..=30 {
        let mut bad = original.clone();
        bad["editorial_overlay"]["schema_version"] = json!(version);
        assert!(
            decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
            "V{version}"
        );
    }
    for marker in [
        Value::Null,
        json!({}),
        json!({"unselected":false}),
        json!({"unselected":null}),
        json!({"unselected":1}),
        json!({"unselected":true,"target":page(TeletextX27LinkRole::Pop)}),
        json!({"kind":"unselected"}),
        json!([true]),
    ] {
        let mut bad = original.clone();
        bad["editorial_overlay"]["object_page_selections"][0]["expected_role"] = marker.clone();
        assert!(
            decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
            "{marker}"
        );
    }
    let text = String::from_utf8(saved(&pure())).unwrap();
    assert!(text.contains("\"unselected\":true"));
    assert!(decode_teletext_design_documents(
        text.replacen(
            "\"unselected\":true",
            "\"unselected\":true,\"unselected\":true",
            1
        )
        .as_bytes(),
        budget()
    )
    .is_err());
    for value in [Value::Null, json!([])] {
        let mut bad = original.clone();
        bad["editorial_overlay"]["object_page_selections"] = value;
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    let mut bad = original.clone();
    bad["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("object_page_selections");
    assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    for prior in [0, 25, 26, 27, 28, 29, 30, 31, 65535] {
        let mut bad = original.clone();
        bad["editorial_overlay"]["prior_schema_version"] = json!(prior);
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    for historical in [
        super::object_page_selection_tests::saved(),
        saved(
            &TeletextSavedEditorialOverlay::new_with_object_page_selections(
                provenance(),
                vec![selected(TeletextX27LinkRole::Drcs)],
            )
            .unwrap(),
        ),
    ] {
        let mut bad: Value = serde_json::from_slice(&historical).unwrap();
        bad["editorial_overlay"]["schema_version"] = json!(31);
        assert!(
            decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
            "V31 must contain a genuine attachment"
        );
    }
}
#[test]
fn v31_retained_role_attachments_mix_selected_roles_without_changing_their_wire_shape() {
    let old = TeletextSavedEditorialOverlay::new_with_object_page_selections(
        provenance(),
        vec![selected(TeletextX27LinkRole::Pop)],
    )
    .unwrap();
    let mixed = TeletextSavedEditorialOverlay::new_with_object_page_selections(
        provenance(),
        vec![
            selected(TeletextX27LinkRole::Pop),
            attachment(TeletextX27LinkRole::Gpop),
            attachment(TeletextX27LinkRole::Drcs),
            attachment(TeletextX27LinkRole::Gdrcs),
        ],
    )
    .unwrap()
    .with_main_resource_invocations(vec![main_calls::change()])
    .unwrap();
    let old_json: Value = serde_json::from_slice(&saved(&old)).unwrap();
    let new_json: Value = serde_json::from_slice(&saved(&mixed)).unwrap();
    assert_eq!(old.schema_version(), 27);
    assert_eq!(mixed.schema_version(), 31);
    assert_eq!(
        new_json["editorial_overlay"]["object_page_selections"][0],
        old_json["editorial_overlay"]["object_page_selections"][0]
    );
    assert_eq!(
        decode_teletext_design_documents(&saved(&mixed), budget())
            .unwrap()
            .editorial_overlay(),
        Some(&mixed)
    );
}
#[test]
fn v31_retained_role_attachments_check_retained_bank_kind_masks_owners_and_budgets() {
    let original: Value = serde_json::from_slice(&saved(&pure())).unwrap();
    for (path, value) in [
        (
            "/editorial_overlay/object_page_selections/0/document_index",
            json!(1),
        ),
        (
            "/editorial_overlay/object_page_selections/0/display/page",
            json!(999),
        ),
        (
            "/editorial_overlay/object_page_selections/0/replacement/target/page/page",
            json!(0xd0),
        ),
        (
            "/editorial_overlay/object_page_selections/2/replacement/target/page/page",
            json!(0xd1),
        ),
        (
            "/editorial_overlay/object_page_selections/2/replacement/required_subpages",
            json!(0x8002),
        ),
        (
            "/editorial_overlay/object_page_selections/0/replacement/validity",
            json!("level35_only"),
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(
            decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
            "{path}"
        );
    }
    for name in [
        "object_resource_additions",
        "object_subpage_announcements",
        "main_resource_invocations",
        "main_program_presence_overlays",
    ] {
        for value in [Value::Null, json!([])] {
            let mut bad = original.clone();
            bad["editorial_overlay"][name] = value;
            assert!(
                decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
                "{name}"
            );
        }
    }
    let mut limits = budget();
    limits.validation.service.max_structure_items = 1;
    assert!(append_teletext_design_editorial_overlay(&source(), &pure(), limits).is_err());
    let mut duplicate = original.clone();
    let first = duplicate["editorial_overlay"]["object_page_selections"][0].clone();
    duplicate["editorial_overlay"]["object_page_selections"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert!(decode_teletext_design_documents(&bytes(&duplicate), budget()).is_err());
    let mut huge = original;
    huge["editorial_overlay"]["object_page_selections"][0]["expected_links"] = json!([null, null]);
    assert!(matches!(
        decode_teletext_design_documents(&bytes(&huge), budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget { .. }
    ));
}
#[test]
fn v31_retained_role_attachments_never_persist_transient_unselected_restoration() {
    let original = attachment(TeletextX27LinkRole::Pop).change();
    let unspecified = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let links = TeletextX27Level25Links::new(
        unspecified,
        original.replacement(),
        unspecified,
        unspecified,
    )
    .unwrap();
    let selected = TeletextMotRoleResolution::Selected {
        target: page(TeletextX27LinkRole::Pop),
        requirements: TeletextMotResolutionRequirements::X27Mask(
            original.replacement().required_subpages(),
        ),
        provenance: TeletextMotLinkProvenance::X27 {
            display: address(17),
            role: TeletextX27LinkRole::Pop,
        },
        original_mot: None,
    };
    let undo = TeletextObjectPageSelectionChange::new_unselected_restoration(
        original,
        Some(links),
        selected,
    )
    .unwrap();
    assert!(TeletextSavedObjectPageSelectionChange::new(0, undo).is_err());
    let transient =
        TeletextSavedObjectPageSelectionChange::for_unselected_restoration(0, undo).unwrap();
    assert!(
        TeletextSavedObjectPageSelectionChange::for_unselected_restoration(0, original).is_err()
    );
    assert!(
        TeletextSavedObjectPageSelectionChange::for_unselected_restoration(
            crate::teletext_design_documents::MAX_BINDINGS,
            undo
        )
        .is_err()
    );
    assert!(
        TeletextSavedEditorialOverlay::new_with_object_page_selections(
            provenance(),
            vec![transient]
        )
        .is_err()
    );
    assert!(super::tests::overlay()
        .with_object_page_selections(vec![attachment(TeletextX27LinkRole::Drcs), transient])
        .is_err());
}
#[test]
fn v31_retained_role_attachments_keep_all_existing_envelope_records_and_prior_roles() {
    let additions = super::object_resource_storage_tests::additions();
    let announcement = TeletextSavedObjectSubpageAnnouncement::new(
        0,
        TeletextObjectSubpageAnnouncement::new(address(17), additions[2].addition().resource())
            .unwrap(),
    )
    .unwrap();
    let overlay = super::tests::overlay()
        .with_object_resource_additions(additions)
        .unwrap()
        .with_object_subpage_announcements(vec![announcement])
        .unwrap()
        .with_object_page_selections(pure().object_page_selections().to_vec())
        .unwrap()
        .with_main_resource_invocations(vec![main_calls::change()])
        .unwrap()
        .with_main_program_presence_overlays(vec![TeletextMainProgramPresenceOverlay::new(
            address(17),
            None,
            Some(TeletextEnhancementProgram::new(vec![]).unwrap()),
        )
        .unwrap()])
        .unwrap();
    let mut limits = budget();
    limits.max_resource_transmissions = 11;
    let original = source();
    let data = append_teletext_design_editorial_overlay(&original, &overlay, limits).unwrap();
    let decoded = decode_teletext_design_documents(data.bytes(), limits).unwrap();
    assert_eq!(decoded.editorial_overlay(), Some(&overlay));
    assert_eq!(overlay.schema_version(), 31);
    assert_eq!(
        append_teletext_design_editorial_overlay(
            &original,
            decoded.editorial_overlay().unwrap(),
            limits
        )
        .unwrap()
        .bytes(),
        data.bytes()
    );
}

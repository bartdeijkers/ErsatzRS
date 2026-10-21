//! V30 retained DRCS bank selection storage, not complete native replay authority.
use super::main_resource_invocation_tests as main_calls;
use super::object_resource_storage_tests::additions;
use super::tests::{address, budget, bytes, provenance};
use super::*;
use ersatzrs_domain::*;
use serde_json::{json, Value};

fn page(role: TeletextX27LinkRole, retained: bool) -> TeletextRawDataPageNumber {
    let global = role == TeletextX27LinkRole::Gdrcs;
    TeletextRawDataPageNumber::new(2, if retained { 0xd0 } else { 0xc0 } + u8::from(global))
        .unwrap()
}
fn link(role: TeletextX27LinkRole, retained: bool) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: page(role, retained),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(2),
    )
    .unwrap()
}
fn source() -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(&main_calls::original()).unwrap();
    let mut response: Value =
        serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
    for role in [TeletextX27LinkRole::Drcs, TeletextX27LinkRole::Gdrcs] {
        let field = if role == TeletextX27LinkRole::Drcs {
            "drcs"
        } else {
            "gdrcs"
        };
        response["service"]["page_enhancements"][0]["compositional_links"][field] =
            json!(link(role, false));
        let selector = if role == TeletextX27LinkRole::Drcs {
            "normal"
        } else {
            "global"
        };
        response["service"]["page_enhancements"][0]["resources"]["drcs"][selector] =
            json!(page(role, false));
        for retained in [false, true] {
            let identity = TeletextResourceIdentity::new(
                role.kind(),
                page(role, retained),
                TeletextResourceSubpageIndex::new(1).unwrap(),
            )
            .unwrap();
            response["service"]["drcs"]
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "identity":{"kind":role.kind(),"page":page(role, retained),"subpage":1},
                    "glyphs":[{"slot":0,"bitmap":vec![if retained {3} else {1};10]}]
                }));
            value["resource_transmissions"]
                .as_array_mut()
                .unwrap()
                .push(json!({
                    "resource":identity,"counter":5,"update":"changed","last_subpage":true
                }));
        }
    }
    response["service"]["page_enhancements"].as_array_mut().unwrap().push(json!({
        "address":address(17),"presentation":null,"compositional_links":null,"local_program":null,
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}
    }));
    value["documents"][0]["response"] = json!(serde_json::to_string_pretty(&response).unwrap());
    let data = bytes(&value);
    decode_teletext_design_documents(&data, budget())
        .expect("independent retained DRCS selection source must qualify");
    data
}
fn selection(role: TeletextX27LinkRole) -> TeletextSavedObjectPageSelectionChange {
    let original = decode_teletext_design_documents(&source(), budget()).unwrap();
    let TeletextProfileDocument::Authored(service) = &original.documents()[0] else {
        panic!()
    };
    let links = service.semantic().page_enhancements()[&address(16)].compositional_links;
    let change = TeletextObjectPageSelectionChange::new(
        address(16),
        role,
        links,
        TeletextMotRoleResolution::Selected {
            target: page(role, false),
            requirements: TeletextMotResolutionRequirements::X27Mask(
                TeletextSubpageRequirementMask::new(2),
            ),
            provenance: TeletextMotLinkProvenance::X27 {
                display: address(16),
                role,
            },
            original_mot: None,
        },
        link(role, true),
    )
    .unwrap();
    TeletextSavedObjectPageSelectionChange::new(0, change).unwrap()
}
fn pure() -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new_with_object_page_selections(
        provenance(),
        vec![
            selection(TeletextX27LinkRole::Drcs),
            selection(TeletextX27LinkRole::Gdrcs),
        ],
    )
    .unwrap()
}
fn presence() -> TeletextMainProgramPresenceOverlay {
    TeletextMainProgramPresenceOverlay::new(
        address(17),
        None,
        Some(TeletextEnhancementProgram::new(vec![]).unwrap()),
    )
    .unwrap()
}
fn with_additions_budget() -> TeletextDesignDocumentBudget {
    let mut limits = budget();
    // Eight retained source banks plus the three complete canonical additions.
    limits.max_resource_transmissions = 11;
    limits
}
fn saved_with_budget(
    overlay: &TeletextSavedEditorialOverlay,
    limits: TeletextDesignDocumentBudget,
) -> Vec<u8> {
    append_teletext_design_editorial_overlay(&source(), overlay, limits)
        .unwrap()
        .into_bytes()
}
fn saved(overlay: &TeletextSavedEditorialOverlay) -> Vec<u8> {
    saved_with_budget(overlay, budget())
}
fn assert_roundtrip(overlay: &TeletextSavedEditorialOverlay) {
    assert_roundtrip_with_budget(overlay, budget());
}
fn assert_roundtrip_with_budget(
    overlay: &TeletextSavedEditorialOverlay,
    limits: TeletextDesignDocumentBudget,
) {
    let original = source();
    let before = decode_teletext_design_documents(&original, limits).unwrap();
    let data = append_teletext_design_editorial_overlay(&original, overlay, limits).unwrap();
    let after = decode_teletext_design_documents(data.bytes(), limits).unwrap();
    assert_eq!(overlay.schema_version(), 30);
    assert_eq!(after.editorial_overlay(), Some(overlay));
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
    assert_eq!(after.guide_source(), before.guide_source());
    assert_eq!(
        append_teletext_design_editorial_overlay(
            &original,
            after.editorial_overlay().unwrap(),
            limits
        )
        .unwrap()
        .bytes(),
        data.bytes(),
    );
}

#[test]
fn v30_drcs_page_selections_roundtrip_both_scopes_without_changing_originals() {
    assert_roundtrip(&pure());
    for role in [TeletextX27LinkRole::Drcs, TeletextX27LinkRole::Gdrcs] {
        let single = TeletextSavedEditorialOverlay::new_with_object_page_selections(
            provenance(),
            vec![selection(role)],
        )
        .unwrap();
        assert_roundtrip(&single);
    }
}

#[test]
fn v30_drcs_page_selections_preserve_prior_roles_and_presence_in_either_attachment_order() {
    let first = pure()
        .with_main_resource_invocations(vec![main_calls::change()])
        .unwrap()
        .with_main_program_presence_overlays(vec![presence()])
        .unwrap()
        .with_object_resource_additions(additions())
        .unwrap();
    let last = TeletextSavedEditorialOverlay::new_with_object_resource_additions(
        provenance(),
        additions(),
    )
    .unwrap()
    .with_main_program_presence_overlays(vec![presence()])
    .unwrap()
    .with_main_resource_invocations(vec![main_calls::change()])
    .unwrap()
    .with_object_page_selections(pure().object_page_selections().to_vec())
    .unwrap();
    assert_eq!(first, last);
    let quota_error =
        append_teletext_design_editorial_overlay(&source(), &first, budget()).unwrap_err();
    assert!(matches!(
        quota_error.reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "resource transmissions",
            maximum: 8,
            ..
        }
    ));
    assert_roundtrip_with_budget(&first, with_additions_budget());
    // V30 is an envelope; all original role families remain in the closed prior role.
    let rows = super::tests::overlay()
        .with_object_page_selections(pure().object_page_selections().to_vec())
        .unwrap();
    assert_roundtrip(&rows);
    let value: Value =
        serde_json::from_slice(&saved_with_budget(&first, with_additions_budget())).unwrap();
    assert_eq!(value["editorial_overlay"]["prior_schema_version"], 1);
    assert_eq!(
        value["editorial_overlay"]["main_program_presence_overlays"][0]["expected"],
        Value::Null
    );
}

#[test]
fn v30_drcs_page_selections_keep_all_historical_versions_and_empty_envelopes_closed() {
    let original: Value = serde_json::from_slice(&saved(&pure())).unwrap();
    for version in 1..=29 {
        let mut bad = original.clone();
        bad["editorial_overlay"]["schema_version"] = json!(version);
        assert!(
            decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
            "closed V{version}"
        );
    }
    // V28/V29 must still reject DRCS if their own mandatory field is genuine.
    let combined = pure()
        .with_main_resource_invocations(vec![main_calls::change()])
        .unwrap()
        .with_main_program_presence_overlays(vec![presence()])
        .unwrap();
    let combined: Value = serde_json::from_slice(&saved(&combined)).unwrap();
    for version in [28, 29] {
        let mut bad = combined.clone();
        bad["editorial_overlay"]["schema_version"] = json!(version);
        if version == 28 {
            bad["editorial_overlay"]
                .as_object_mut()
                .unwrap()
                .remove("main_program_presence_overlays");
        }
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    for empty in [Value::Null, json!([])] {
        let mut bad = original.clone();
        bad["editorial_overlay"]["object_page_selections"] = empty;
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
    let mut missing = original.clone();
    missing["editorial_overlay"]
        .as_object_mut()
        .unwrap()
        .remove("object_page_selections");
    assert!(decode_teletext_design_documents(&bytes(&missing), budget()).is_err());
    let mut old =
        serde_json::from_slice::<Value>(&super::object_page_selection_tests::saved()).unwrap();
    old["editorial_overlay"]["schema_version"] = json!(30);
    assert!(
        decode_teletext_design_documents(&bytes(&old), budget()).is_err(),
        "V30 requires a DRCS selection"
    );
    for prior in [0, 25, 26, 27, 28, 29, 30, 65535] {
        let mut bad = original.clone();
        bad["editorial_overlay"]["prior_schema_version"] = json!(prior);
        assert!(decode_teletext_design_documents(&bytes(&bad), budget()).is_err());
    }
}

#[test]
fn v30_drcs_page_selections_reject_wrong_scope_missing_s1_and_foreign_source_authority() {
    let original: Value = serde_json::from_slice(&saved(&pure())).unwrap();
    for (path, bad_value) in [
        (
            "/editorial_overlay/object_page_selections/0/replacement/target/page/page",
            json!(0xd1),
        ),
        (
            "/editorial_overlay/object_page_selections/0/replacement/required_subpages",
            json!(0x8002),
        ),
        (
            "/editorial_overlay/object_page_selections/0/document_index",
            json!(1),
        ),
        (
            "/editorial_overlay/object_page_selections/0/replacement/target/page/page",
            json!(0xa2),
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(path).unwrap() = bad_value;
        assert!(
            decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
            "{path}"
        );
    }
    let with_additions = saved_with_budget(
        &pure().with_object_resource_additions(additions()).unwrap(),
        with_additions_budget(),
    );
    decode_teletext_design_documents(&with_additions, with_additions_budget())
        .expect("authority negative must start with an accepted full-size fixture");
    let mut addition_target: Value = serde_json::from_slice(&with_additions).unwrap();
    addition_target["editorial_overlay"]["object_page_selections"][0]["replacement"]["target"]
        ["page"]["page"] = json!(0xa2);
    let error = decode_teletext_design_documents(&bytes(&addition_target), with_additions_budget())
        .unwrap_err();
    assert!(
        matches!(
            error.reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        ),
        "a POP addition is never a DRCS bank; no quota false positive: {error:?}"
    );
    let before = decode_teletext_design_documents(&source(), budget()).unwrap();
    let TeletextProfileDocument::Authored(service) = &before.documents()[0] else {
        panic!()
    };
    let duplicate = vec![
        TeletextProfileDocument::Authored(service.clone()),
        TeletextProfileDocument::Authored(service.clone()),
    ];
    assert!(object_page_selections::validate_association(&pure(), &duplicate, budget()).is_err());
    assert!(pure()
        .with_object_page_selections(vec![selection(TeletextX27LinkRole::Drcs)])
        .is_err());
    assert!(
        TeletextSavedEditorialOverlay::new_with_object_page_selections(
            provenance(),
            vec![
                selection(TeletextX27LinkRole::Drcs),
                selection(TeletextX27LinkRole::Drcs)
            ],
        )
        .is_err()
    );
}

#[test]
fn v30_drcs_page_selections_keep_optional_role_shapes_and_structure_budgets_closed() {
    let original: Value = serde_json::from_slice(&saved(&pure())).unwrap();
    for field in [
        "main_program_presence_overlays",
        "main_resource_invocations",
        "object_subpage_announcements",
        "object_resource_additions",
    ] {
        for bad_value in [Value::Null, json!([])] {
            let mut bad = original.clone();
            bad["editorial_overlay"][field] = bad_value;
            assert!(
                decode_teletext_design_documents(&bytes(&bad), budget()).is_err(),
                "{field}"
            );
        }
    }
    let mut limits = budget();
    limits.validation.service.max_structure_items = 1;
    assert!(append_teletext_design_editorial_overlay(&source(), &pure(), limits).is_err());
    let mut duplicate = original;
    let first = duplicate["editorial_overlay"]["object_page_selections"][0].clone();
    duplicate["editorial_overlay"]["object_page_selections"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert!(decode_teletext_design_documents(&bytes(&duplicate), budget()).is_err());
}

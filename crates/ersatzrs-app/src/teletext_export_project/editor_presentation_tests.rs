//! Editor descriptors are bounded claims; native originals remain authoritative.

use super::*;

const V5: &str = "qteletextmaker-1.0-carrier-v5";

fn presentation(national_option: u8, pop_flags: u8) -> TeletextExportEditorPresentation {
    TeletextExportEditorPresentation::Qtm10OriginalPopFallbackV1 {
        national_option,
        pop_flags,
    }
}
fn v5_project() -> TeletextExportProject {
    let mut project = valid_project();
    project.editor_profile = V5.into();
    project.pages[0].editor_presentation = Some(presentation(1, 14));
    project
}

#[test]
fn pop_editor_projection_round_trips_static_and_effective_original_roles() {
    for effective in [false, true] {
        let mut project = v5_project();
        if effective {
            project.schema_version = 2;
            project.original.document_version = 2;
            project.effective_scope = Some(TeletextExportEffectiveScope {
                binding_count: 1,
                valid_until: None,
            });
            project.producers[0].effective_origin =
                Some(TeletextExportEffectiveOrigin::Base { binding_index: 0 });
        }
        let encoded = write_teletext_export_project(&project, budget()).unwrap();
        let value: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            value["pages"][0]["editor_presentation"],
            json!({
            "kind":"qtm10_original_pop_fallback_v1","national_option":1,"pop_flags":14})
        );
        assert_eq!(
            parse_teletext_export_project(&encoded, budget()).unwrap(),
            project
        );
        assert_eq!(project.producers[0].binding_revision, Some(2));
    }
}

#[test]
fn pop_editor_projection_preserves_all_original_flags_and_valid_national_options() {
    for national_option in 0..=6 {
        for pop_flags in 0..=15 {
            let mut project = v5_project();
            project.pages[0].editor_presentation = Some(presentation(national_option, pop_flags));
            let bytes = write_teletext_export_project(&project, budget()).unwrap();
            assert_eq!(
                parse_teletext_export_project(&bytes, budget()).unwrap(),
                project
            );
        }
    }
    for (national_option, pop_flags) in [(7, 14), (255, 14), (1, 16), (1, 255)] {
        let mut project = v5_project();
        project.pages[0].editor_presentation = Some(presentation(national_option, pop_flags));
        let error = project.validate(budget()).unwrap_err();
        assert_eq!(error.kind, TeletextExportProjectFailure::Mapping);
        assert_eq!(error.item, Some(0));
    }
}

#[test]
fn pop_editor_projection_is_v5_only_and_selected_subsets_need_no_projection() {
    for profile in [
        "qteletextmaker-1.0",
        "qteletextmaker-1.0-carrier-v2",
        "qteletextmaker-1.0-carrier-v3",
        "qteletextmaker-1.0-carrier-v4",
    ] {
        let mut project = v5_project();
        project.editor_profile = profile.into();
        assert_eq!(
            project.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    let mut project = v5_project();
    project.pages[0].editor_presentation = None;
    project.pages[0].page.page = 201;
    project.pages[0].editor_page = 0x201;
    let encoded = write_teletext_export_project(&project, budget()).unwrap();
    let value: Value = serde_json::from_slice(&encoded).unwrap();
    assert!(value["pages"][0].get("editor_presentation").is_none());
    assert_eq!(
        parse_teletext_export_project(&encoded, budget()).unwrap(),
        project
    );
}

#[test]
fn pop_editor_projection_rejects_null_sequences_unknown_tags_properties_and_numbers() {
    let source = serde_json::to_value(v5_project()).unwrap();
    for projection in [
        Value::Null,
        json!([]),
        json!(["qtm10_original_pop_fallback_v1", 1, 14]),
        json!({"kind":"unknown","national_option":1,"pop_flags":14}),
        json!({"kind":"qtm10_original_pop_fallback_v2","national_option":1,"pop_flags":14}),
        json!({"kind":"qtm10_original_pop_fallback_v1","national_option":1,"pop_flags":14,"authority":true}),
        json!({"kind":"qtm10_original_pop_fallback_v1","national_option":1}),
        json!({"kind":"qtm10_original_pop_fallback_v1","pop_flags":14}),
        json!({"kind":"qtm10_original_pop_fallback_v1","national_option":7,"pop_flags":14}),
        json!({"kind":"qtm10_original_pop_fallback_v1","national_option":1,"pop_flags":16}),
        json!({"kind":"qtm10_original_pop_fallback_v1","national_option":-1,"pop_flags":14}),
        json!({"kind":"qtm10_original_pop_fallback_v1","national_option":1,"pop_flags":256}),
        json!({"kind":"qtm10_original_pop_fallback_v1","national_option":1.5,"pop_flags":14}),
    ] {
        let mut invalid = source.clone();
        invalid["pages"][0]["editor_presentation"] = projection;
        assert!(parse_teletext_export_project(&as_bytes(&invalid), budget()).is_err());
    }
    // The enum itself also refuses Serde's positional sequence representation.
    assert!(
        serde_json::from_value::<TeletextExportEditorPresentation>(json!([
            "qtm10_original_pop_fallback_v1",
            1,
            14
        ]))
        .is_err()
    );
}

#[test]
fn pop_editor_projection_duplicate_fields_fail_before_typed_allocation() {
    let encoded =
        String::from_utf8(write_teletext_export_project(&v5_project(), budget()).unwrap()).unwrap();
    for (needle, duplicate) in [
        ("\"national_option\":1", "\"national_option\":1,\"national_option\":2"),
        ("\"pop_flags\":14", "\"pop_flags\":14,\"pop_flags\":15"),
        ("\"kind\":\"qtm10_original_pop_fallback_v1\"",
         "\"kind\":\"qtm10_original_pop_fallback_v1\",\"kind\":\"qtm10_original_pop_fallback_v1\"")
    ] {
        assert_eq!(encoded.matches(needle).count(), 1);
        let duplicate = encoded.replacen(needle, duplicate, 1);
        assert_eq!(parse_teletext_export_project(duplicate.as_bytes(), budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Duplicate);
    }
}

#[test]
fn v5_retains_v4_dense_and_padded_bank_rules_without_allowing_v3_companions() {
    for subpages in [&[0][..], &[1][..], &[0, 2][..]] {
        let mut body = drcs_set_receipt(subpages);
        body.project.editor_profile = V5.into();
        body.project.pages[0].editor_presentation = Some(presentation(1, 14));
        let bytes = write_teletext_export_receipt_body(&body, budget()).unwrap();
        assert_eq!(
            parse_teletext_export_receipt_body(&bytes, budget()).unwrap(),
            body
        );
        let mut invalid = body.clone();
        invalid.project.assets.last_mut().unwrap().unit_count = 1;
        assert_eq!(
            invalid.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
        invalid.project.assets.last_mut().unwrap().scope = TeletextExportAssetScope::DrcsTti {
            resource_name: "resource-0000.dvb".into(),
            editor_ordinal: 0,
        };
        assert_eq!(
            invalid.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
}

#[test]
fn absent_editor_projection_preserves_pinned_legacy_schema_one_bytes() {
    let golden = include_bytes!("fixtures/schema1-project.json");
    let project = parse_teletext_export_project(golden, budget()).unwrap();
    assert!(project.pages[0].editor_presentation.is_none());
    assert_eq!(
        write_teletext_export_project(&project, budget()).unwrap(),
        golden.as_slice()
    );
    assert_eq!(project, valid_project());
    assert_eq!(
        format!("{:?}", presentation(1, 14)),
        "TeletextExportEditorPresentation(<redacted>)"
    );
}

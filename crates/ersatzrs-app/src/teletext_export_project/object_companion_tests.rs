//! V6 object companions describe editor syntax without native definition authority.
use super::*;

const V6: &str = "qteletextmaker-1.0-carrier-v6";
fn object_receipt(
    kind: TeletextResourceKindWire,
    subpage: u8,
    object_id: u16,
) -> TeletextExportReceiptBody {
    let mut body = receipt();
    body.project.editor_profile = V6.into();
    let bytes =
        b"PN,10000\r\nSC,0000\r\nPS,0000\r\nOL,1,                                        \r\n"
            .to_vec();
    body.project.assets.extend([
        TeletextExportAssetMapping {
            name: "resource.bin".into(),
            scope: TeletextExportAssetScope::Resource {
                identity: TeletextResourceIdentityWire {
                    kind,
                    page: ersatzrs_addon_contract::TeletextRawDataPageNumberWire {
                        magazine: 2,
                        page: 0xa0,
                    },
                    subpage,
                },
                physical_subcode: 0x100 | u16::from(subpage),
            },
            unit_count: 1,
            bytes: 46,
            sha256: "c".repeat(64),
        },
        TeletextExportAssetMapping {
            name: "object.tti".into(),
            scope: TeletextExportAssetScope::ObjectTtiV1 {
                resource_name: "resource.bin".into(),
                object_id,
                editor_page: 0x100,
                editor_ordinal: 0,
            },
            unit_count: 0,
            bytes: bytes.len(),
            sha256: "d".repeat(64),
        },
    ]);
    body.files.extend([
        TeletextExportFile {
            name: "resource.bin".into(),
            bytes: vec![0x55; 46],
        },
        TeletextExportFile {
            name: "object.tti".into(),
            bytes,
        },
    ]);
    body
}
fn valid_object_receipt() -> TeletextExportReceiptBody {
    object_receipt(TeletextResourceKindWire::Pop, 15, 65535)
}
fn rejects_scope(scope: Value) {
    let mut project = serde_json::to_value(valid_object_receipt().project).unwrap();
    project["assets"][1]["scope"] = scope;
    assert!(parse_teletext_export_project(&as_bytes(&project), budget()).is_err());
}

#[test]
fn object_companion_v6_roundtrips_both_original_roles_full_resource_s1_and_object_ids() {
    for kind in [
        TeletextResourceKindWire::Pop,
        TeletextResourceKindWire::Gpop,
    ] {
        for subpage in [0, 15] {
            for object_id in [0, 65535] {
                for effective in [false, true] {
                    let mut body = object_receipt(kind, subpage, object_id);
                    if effective {
                        body.project.schema_version = 2;
                        body.project.original.document_version = 2;
                        body.project.effective_scope = Some(TeletextExportEffectiveScope {
                            binding_count: 1,
                            valid_until: None,
                        });
                        body.project.producers[0].effective_origin =
                            Some(TeletextExportEffectiveOrigin::Base { binding_index: 0 });
                    }
                    let bytes = write_teletext_export_receipt_body(&body, budget()).unwrap();
                    assert_eq!(
                        parse_teletext_export_receipt_body(&bytes, budget()).unwrap(),
                        body
                    );
                    let value: Value = serde_json::from_slice(&bytes).unwrap();
                    assert_eq!(
                        value["project"]["assets"][1]["scope"],
                        json!({"kind":"object_tti_v1","resource_name":"resource.bin","object_id":object_id,"editor_page":256,"editor_ordinal":0})
                    );
                    let sidecar = TeletextExportSidecar {
                        schema_version: 1,
                        receipt_id: 9,
                        receipt_digest: "a".repeat(64),
                        project: body.project,
                    };
                    let bytes = write_teletext_export_sidecar(&sidecar, budget()).unwrap();
                    assert_eq!(
                        parse_teletext_export_sidecar(&bytes, budget()).unwrap(),
                        sidecar
                    );
                    assert!(serde_json::from_slice::<Value>(&bytes)
                        .unwrap()
                        .get("files")
                        .is_none());
                }
            }
        }
    }
}

#[test]
fn object_companion_requires_v6_without_widening_older_carriers() {
    for profile in [
        "qteletextmaker-1.0",
        "qteletextmaker-1.0-carrier-v2",
        "qteletextmaker-1.0-carrier-v3",
        "qteletextmaker-1.0-carrier-v4",
        "qteletextmaker-1.0-carrier-v5",
    ] {
        let mut body = valid_object_receipt();
        body.project.editor_profile = profile.into();
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    let mut project = valid_project();
    project.editor_profile = V6.into();
    assert!(
        project.validate(budget()).is_ok(),
        "a selected subset need not contain resource objects"
    );
}

#[test]
fn object_companion_v6_retains_drcs_banks_and_v5_editor_presentation_rules() {
    for subpages in [&[0][..], &[1][..], &[0, 2][..]] {
        let mut body = drcs_set_receipt(subpages);
        let object = valid_object_receipt();
        body.project.editor_profile = V6.into();
        body.project.pages[0].editor_presentation = Some(
            TeletextExportEditorPresentation::Qtm10OriginalPopFallbackV1 {
                national_option: 1,
                pop_flags: 14,
            },
        );
        body.project.assets.extend(object.project.assets);
        body.files.extend(object.files.into_iter().skip(1));
        let bytes = write_teletext_export_receipt_body(&body, budget()).unwrap();
        assert_eq!(
            parse_teletext_export_receipt_body(&bytes, budget()).unwrap(),
            body
        );
        let mut invalid = body.clone();
        invalid.project.pages[0].editor_presentation = Some(
            TeletextExportEditorPresentation::Qtm10OriginalPopFallbackV1 {
                national_option: 7,
                pop_flags: 14,
            },
        );
        assert_eq!(
            invalid.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
        let mut invalid = body;
        let TeletextExportAssetScope::DrcsTtiSet { resource_names } = &mut invalid
            .project
            .assets
            .iter_mut()
            .find(|asset| matches!(asset.scope, TeletextExportAssetScope::DrcsTtiSet { .. }))
            .unwrap()
            .scope
        else {
            panic!("bank fixture")
        };
        resource_names.push(None);
        assert_eq!(
            invalid.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
}

#[test]
fn object_companion_scope_is_closed_nonnull_and_has_no_native_identity_or_authority() {
    for scope in [
        Value::Null,
        json!([]),
        json!(["object_tti_v1", "resource.bin", 65535, 256, 0]),
        json!({"kind":"object_tti_v2","resource_name":"resource.bin","object_id":65535,"editor_page":256,"editor_ordinal":0}),
        json!({"kind":"object_tti_v1","resource_name":"resource.bin","object_id":65535,"editor_page":256}),
        json!({"kind":"object_tti_v1","resource_name":"resource.bin","object_id":65535,"editor_page":256,"editor_ordinal":0,"identity":{"scope":"public"}}),
        json!({"kind":"object_tti_v1","resource_name":"resource.bin","object_id":65535,"editor_page":256,"editor_ordinal":0,"owner":{"page":200,"subcode":0}}),
        json!({"kind":"object_tti_v1","resource_name":null,"object_id":65535,"editor_page":256,"editor_ordinal":0}),
        json!({"kind":"object_tti_v1","resource_name":[],"object_id":65535,"editor_page":256,"editor_ordinal":0}),
        json!({"kind":"object_tti_v1","resource_name":"resource.bin","object_id":65536,"editor_page":256,"editor_ordinal":0}),
        json!({"kind":"object_tti_v1","resource_name":"resource.bin","object_id":-1,"editor_page":256,"editor_ordinal":0}),
    ] {
        rejects_scope(scope);
    }
}

#[test]
fn object_companion_rejects_wrong_references_resource_kind_identity_and_duplicate_targets() {
    for name in [
        "missing.bin",
        "object.tti",
        "200-0123.tti",
        "../resource.bin",
        "",
    ] {
        let mut body = valid_object_receipt();
        let TeletextExportAssetScope::ObjectTtiV1 { resource_name, .. } =
            &mut body.project.assets[1].scope
        else {
            unreachable!()
        };
        *resource_name = name.into();
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    for kind in [
        TeletextResourceKindWire::Drcs,
        TeletextResourceKindWire::Gdrcs,
        TeletextResourceKindWire::Mot,
        TeletextResourceKindWire::Mip,
    ] {
        let body = object_receipt(kind, 0, 0);
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    for (magazine, page, subpage) in [
        (0, 0xa0, 0),
        (9, 0xa0, 0),
        (2, 0xfe, 0),
        (2, 0xfd, 0),
        (1, 0xbe, 0),
        (2, 0xa0, 16),
    ] {
        let mut body = valid_object_receipt();
        let TeletextExportAssetScope::Resource { identity, .. } = &mut body.project.assets[0].scope
        else {
            unreachable!()
        };
        identity.page.magazine = magazine;
        identity.page.page = page;
        identity.subpage = subpage;
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    let mut body = valid_object_receipt();
    let mut companion = body.project.assets[1].clone();
    companion.name = "duplicate.tti".into();
    body.project.assets.push(companion);
    assert_eq!(
        body.project.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Mapping
    );
    let mut body = valid_object_receipt();
    let duplicate = body.project.assets[0].clone();
    body.project.assets.push(duplicate);
    assert_eq!(
        body.project.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Mapping
    );
}

#[test]
fn object_companion_page_ordinal_unit_and_file_budgets_are_exact() {
    for (page, ordinal, units) in [
        (0x200, 0, 0),
        (0x100, 1, 0),
        (0x100, u64::MAX, 0),
        (0x100, 0, 1),
    ] {
        let mut body = valid_object_receipt();
        let TeletextExportAssetScope::ObjectTtiV1 {
            editor_page,
            editor_ordinal,
            ..
        } = &mut body.project.assets[1].scope
        else {
            unreachable!()
        };
        *editor_page = page;
        *editor_ordinal = ordinal;
        body.project.assets[1].unit_count = units;
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    let body = valid_object_receipt();
    for limit in ["files", "bytes", "units"] {
        let mut limited = budget();
        match limit {
            "files" => limited.max_files = 2,
            "bytes" => limited.max_file_bytes = body.project.assets[1].bytes - 1,
            _ => limited.max_units = 1,
        }
        if limit == "units" {
            assert!(
                body.project.validate(limited).is_ok(),
                "editor companions consume no protected raw unit quota"
            );
        } else {
            assert!(body.validate(limited).is_err());
        }
    }
}

#[test]
fn object_companion_receipt_tti_identity_is_parsed_instead_of_trusting_the_sidecar() {
    let baseline = valid_object_receipt();
    for text in [
        "PN,20000\r\nSC,0000\r\n",
        "PN,10001\r\nSC,0001\r\n",
        "PN,10000\r\nSC,0001\r\n",
        "PN,10000\r\n",
        "SC,0000\r\n",
        "PN,10000\r\nSC,0000\r\nPN,10000\r\nSC,0000\r\n",
        "PN,10000\r\nSC,0000\r\nSC,0000\r\n",
    ] {
        let mut body = baseline.clone();
        body.files[2].bytes = text.as_bytes().to_vec();
        body.project.assets[1].bytes = text.len();
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Tti
        );
    }
    let mut missing = baseline.clone();
    missing.files.pop();
    assert_eq!(
        missing.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Files
    );
    let mut truncated = baseline;
    truncated.files[2].bytes.pop();
    assert_eq!(
        truncated.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Files
    );
}

#[test]
fn object_companion_duplicate_fields_and_existing_total_byte_limit_fail_closed() {
    let body = valid_object_receipt();
    let encoded =
        String::from_utf8(write_teletext_export_project(&body.project, budget()).unwrap()).unwrap();
    let invalid = encoded.replacen(
        "\"object_id\":65535",
        "\"object_id\":65535,\"object_id\":0",
        1,
    );
    assert_eq!(
        parse_teletext_export_project(invalid.as_bytes(), budget())
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Duplicate
    );
    let mut limited = budget();
    limited.max_bytes = encoded.len() - 1;
    assert_eq!(
        parse_teletext_export_project(encoded.as_bytes(), limited)
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Bytes
    );
    for profile in [
        "qteletextmaker-1.0",
        "qteletextmaker-1.0-carrier-v2",
        "qteletextmaker-1.0-carrier-v3",
        "qteletextmaker-1.0-carrier-v4",
        "qteletextmaker-1.0-carrier-v5",
    ] {
        let mut project = valid_project();
        project.editor_profile = profile.into();
        let before = serde_json::to_vec(&project).unwrap();
        assert_eq!(
            write_teletext_export_project(&project, budget()).unwrap(),
            before
        );
        assert_eq!(
            parse_teletext_export_project(&before, budget()).unwrap(),
            project
        );
    }
}

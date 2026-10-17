//! V8 context claims remain closed syntax, never uploaded dependency authority.
use super::*;

const V8: &str = "qteletextmaker-1.0-carrier-v8";

fn context_receipt(
    kind: TeletextResourceKindWire,
    subpage: u8,
    id: u16,
) -> TeletextExportReceiptBody {
    let mut body = receipt();
    body.project.editor_profile = V8.into();
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
            name: "context.tti".into(),
            scope: TeletextExportAssetScope::ObjectContextTtiV1 {
                resource_name: "resource.bin".into(),
                object_id: id,
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
            name: "context.tti".into(),
            bytes,
        },
    ]);
    body
}

fn baseline() -> TeletextExportReceiptBody {
    context_receipt(TeletextResourceKindWire::Pop, 15, 65535)
}

#[test]
fn object_context_v8_roundtrips_full_original_identity_in_base_and_effective_projects() {
    for kind in [
        TeletextResourceKindWire::Pop,
        TeletextResourceKindWire::Gpop,
    ] {
        for subpage in [0, 15] {
            for id in [0, 65535] {
                for effective in [false, true] {
                    let mut body = context_receipt(kind, subpage, id);
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
                        json!({
                            "kind":"object_context_tti_v1","resource_name":"resource.bin",
                            "object_id":id,"editor_page":256,"editor_ordinal":0,
                        })
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
                    assert_eq!(
                        format!("{:?}", sidecar.project.assets[1].scope),
                        "TeletextExportAssetScope(<redacted>)"
                    );
                }
            }
        }
    }
}

#[test]
fn object_context_requires_v8_and_does_not_reinterpret_v6_or_v7_scopes() {
    for profile in [
        "qteletextmaker-1.0",
        "qteletextmaker-1.0-carrier-v2",
        "qteletextmaker-1.0-carrier-v3",
        "qteletextmaker-1.0-carrier-v4",
        "qteletextmaker-1.0-carrier-v5",
        "qteletextmaker-1.0-carrier-v6",
        "qteletextmaker-1.0-carrier-v7",
    ] {
        let mut body = baseline();
        profile.clone_into(&mut body.project.editor_profile);
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    for scope in [
        TeletextExportAssetScope::ObjectTtiV1 {
            resource_name: "resource.bin".into(),
            object_id: 65535,
            editor_page: 256,
            editor_ordinal: 0,
        },
        TeletextExportAssetScope::ObjectTtiFragmentV1 {
            resource_name: "resource.bin".into(),
            object_id: 65535,
            fragment_index: 0,
            fragment_count: 1,
            command_start: 0,
            command_end: 1,
            native_word_start: 1,
            native_word_end: 2,
            editor_page: 256,
            editor_ordinal: 0,
        },
    ] {
        let mut body = baseline();
        body.project.assets[1].scope = scope;
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    let mut subset = valid_project();
    subset.editor_profile = V8.into();
    assert!(
        subset.validate(budget()).is_ok(),
        "a subset without any object root needs no context companion"
    );
}

#[test]
fn object_context_claims_are_map_only_closed_required_and_duplicate_checked() {
    let baseline = serde_json::to_value(baseline().project).unwrap();
    let scope = baseline["assets"][1]["scope"].clone();
    for key in [
        "kind",
        "resource_name",
        "object_id",
        "editor_page",
        "editor_ordinal",
    ] {
        let mut missing = scope.clone();
        missing.as_object_mut().unwrap().remove(key);
        let mut project = baseline.clone();
        project["assets"][1]["scope"] = missing;
        assert!(parse_teletext_export_project(&as_bytes(&project), budget()).is_err());
    }
    for bad in [
        Value::Null,
        json!([]),
        json!(["object_context_tti_v1", "resource.bin", 65535, 256, 0]),
    ] {
        let mut project = baseline.clone();
        project["assets"][1]["scope"] = bad;
        assert!(parse_teletext_export_project(&as_bytes(&project), budget()).is_err());
    }
    for (key, value) in [
        ("aliases", json!([])),
        ("editable", json!(true)),
        ("owner", json!({"page":124})),
        ("kind", json!("object_context_tti_v2")),
        ("object_id", json!(65536)),
        ("object_id", json!(-1)),
        ("resource_name", Value::Null),
    ] {
        let mut project = baseline.clone();
        project["assets"][1]["scope"][key] = value;
        assert!(parse_teletext_export_project(&as_bytes(&project), budget()).is_err());
    }
    let bytes = String::from_utf8(as_bytes(&baseline)).unwrap();
    let duplicate = bytes.replacen(
        "\"object_id\":65535",
        "\"object_id\":65535,\"object_id\":0",
        1,
    );
    assert_eq!(
        parse_teletext_export_project(duplicate.as_bytes(), budget())
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Duplicate
    );
}

#[test]
fn object_context_requires_exact_unique_parent_and_fixed_editor_mapping() {
    let project = serde_json::to_value(baseline().project).unwrap();
    for (key, value) in [
        ("resource_name", json!("missing.bin")),
        ("resource_name", json!("context.tti")),
        ("resource_name", json!("../resource.bin")),
        ("editor_page", json!(0x200)),
        ("editor_ordinal", json!(1)),
        ("editor_ordinal", json!(u64::MAX)),
    ] {
        let mut invalid = project.clone();
        invalid["assets"][1]["scope"][key] = value;
        assert_eq!(
            parse_teletext_export_project(&as_bytes(&invalid), budget())
                .unwrap_err()
                .kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    for kind in [
        TeletextResourceKindWire::Drcs,
        TeletextResourceKindWire::Gdrcs,
        TeletextResourceKindWire::Mot,
        TeletextResourceKindWire::Mip,
    ] {
        assert_eq!(
            context_receipt(kind, 0, 0)
                .validate(budget())
                .unwrap_err()
                .kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    for (key, value) in [("magazine", json!(0)), ("page", json!(0xfe))] {
        let mut invalid = project.clone();
        invalid["assets"][0]["scope"]["identity"]["page"][key] = value;
        assert!(parse_teletext_export_project(&as_bytes(&invalid), budget()).is_err());
    }
    let mut duplicate = baseline();
    let mut same = duplicate.project.assets[1].clone();
    same.name = "other-context.tti".into();
    duplicate.project.assets.push(same);
    assert_eq!(
        duplicate.project.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Mapping
    );
    let mut duplicate = baseline();
    duplicate
        .project
        .assets
        .push(duplicate.project.assets[0].clone());
    assert!(duplicate.project.validate(budget()).is_err());
}

#[test]
fn object_context_object_ids_are_scoped_to_the_original_resource_link() {
    let mut body = baseline();
    let mut resource = body.project.assets[0].clone();
    resource.name = "other-resource.bin".into();
    let TeletextExportAssetScope::Resource { identity, .. } = &mut resource.scope else {
        unreachable!()
    };
    identity.page.magazine = 3;
    let mut companion = body.project.assets[1].clone();
    companion.name = "other-context.tti".into();
    let TeletextExportAssetScope::ObjectContextTtiV1 {
        resource_name,
        object_id,
        ..
    } = &mut companion.scope
    else {
        unreachable!()
    };
    "other-resource.bin".clone_into(resource_name);
    assert_eq!(*object_id, 65535);
    body.project.assets.extend([resource, companion]);
    let mut resource = body.files[1].clone();
    resource.name = "other-resource.bin".into();
    let mut companion = body.files[2].clone();
    companion.name = "other-context.tti".into();
    body.files.extend([resource, companion]);
    let bytes = write_teletext_export_receipt_body(&body, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_receipt_body(&bytes, budget()).unwrap(),
        body
    );
}

#[test]
fn object_context_validates_actual_tti_identity_files_and_existing_bounds() {
    for text in [
        "PN,20000\r\nSC,0000\r\n",
        "PN,10001\r\nSC,0001\r\n",
        "PN,10000\r\n",
        "PN,10000\r\nSC,0000\r\nPN,10000\r\nSC,0000\r\n",
    ] {
        let mut body = baseline();
        body.files[2].bytes = text.as_bytes().to_vec();
        body.project.assets[1].bytes = text.len();
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Tti
        );
    }
    let mut missing = baseline();
    missing.files.pop();
    assert_eq!(
        missing.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Files
    );
    let mut units = baseline();
    units.project.assets[1].unit_count = 1;
    assert_eq!(
        units.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Mapping
    );
    let body = baseline();
    let mut limited = budget();
    limited.max_units = 1;
    assert!(
        body.project.validate(limited).is_ok(),
        "context files are not raw DVB units"
    );
    limited = budget();
    limited.max_files = 2;
    assert_eq!(
        body.validate(limited).unwrap_err().kind,
        TeletextExportProjectFailure::Limit
    );
    limited = budget();
    limited.max_file_bytes = body.project.assets[1].bytes - 1;
    assert!(body.validate(limited).is_err());
    let bytes = write_teletext_export_project(&body.project, budget()).unwrap();
    limited = budget();
    limited.max_bytes = bytes.len() - 1;
    assert_eq!(
        parse_teletext_export_project(&bytes, limited)
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Bytes
    );
}

#[test]
fn object_context_v8_retains_drcs_sets_and_editor_presentation_without_changing_legacy_bytes() {
    let mut body = drcs_set_receipt(&[0, 2]);
    let context = baseline();
    body.project.editor_profile = V8.into();
    body.project.pages[0].editor_presentation = Some(
        TeletextExportEditorPresentation::Qtm10OriginalPopFallbackV1 {
            national_option: 1,
            pop_flags: 14,
        },
    );
    body.project.assets.extend(context.project.assets);
    body.files.extend(context.files.into_iter().skip(1));
    let bytes = write_teletext_export_receipt_body(&body, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_receipt_body(&bytes, budget()).unwrap(),
        body
    );
    for profile in [
        "qteletextmaker-1.0",
        "qteletextmaker-1.0-carrier-v2",
        "qteletextmaker-1.0-carrier-v3",
        "qteletextmaker-1.0-carrier-v4",
        "qteletextmaker-1.0-carrier-v5",
        "qteletextmaker-1.0-carrier-v6",
        "qteletextmaker-1.0-carrier-v7",
    ] {
        let mut project = valid_project();
        profile.clone_into(&mut project.editor_profile);
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

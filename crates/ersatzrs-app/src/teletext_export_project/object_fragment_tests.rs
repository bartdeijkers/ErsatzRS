//! V7 fragment syntax is descriptive; native command provenance remains host-owned.
use super::*;
const V7: &str = "qteletextmaker-1.0-carrier-v7";

fn fragment_receipt(count: u16) -> TeletextExportReceiptBody {
    let mut body = receipt();
    body.project.editor_profile = V7.into();
    body.project.assets.push(TeletextExportAssetMapping {
        name: "resource.bin".into(),
        scope: TeletextExportAssetScope::Resource {
            identity: TeletextResourceIdentityWire {
                kind: TeletextResourceKindWire::Pop,
                page: ersatzrs_addon_contract::TeletextRawDataPageNumberWire {
                    magazine: 8,
                    page: 0xa0,
                },
                subpage: 15,
            },
            physical_subcode: 0x10f,
        },
        unit_count: 1,
        bytes: 46,
        sha256: "a".repeat(64),
    });
    body.files.push(TeletextExportFile {
        name: "resource.bin".into(),
        bytes: vec![0x55; 46],
    });
    for index in 0..count {
        let bytes=b"PN,10000\r\nSC,0000\r\nPS,C000\r\nRE,0\r\nCT,20,T\r\nOL,0,                                        \r\n".to_vec();
        let name = format!("fragment-{index}.tti");
        body.project.assets.push(TeletextExportAssetMapping {
            name: name.clone(),
            scope: TeletextExportAssetScope::ObjectTtiFragmentV1 {
                resource_name: "resource.bin".into(),
                object_id: 65535,
                fragment_index: index,
                fragment_count: count,
                command_start: u64::from(index),
                command_end: u64::from(index) + 1,
                native_word_start: index + 1,
                native_word_end: index + 2,
                editor_page: 0x100,
                editor_ordinal: 0,
            },
            unit_count: 0,
            bytes: bytes.len(),
            sha256: "b".repeat(64),
        });
        body.files.push(TeletextExportFile { name, bytes });
    }
    body
}
fn project_json(count: u16) -> Value {
    serde_json::to_value(fragment_receipt(count).project).unwrap()
}
fn set(value: &mut Value, fragment: usize, field: &str, replacement: Value) {
    value["assets"][fragment + 1]["scope"][field] = replacement;
}
fn reject(value: &Value) {
    assert!(parse_teletext_export_project(&as_bytes(value), budget()).is_err());
}

#[test]
fn object_fragments_v7_roundtrip_both_roles_and_resource_kinds_without_three_fragment_cap() {
    for count in [1, 2, 4] {
        for global in [false, true] {
            for effective in [false, true] {
                let mut body = fragment_receipt(count);
                if global {
                    let TeletextExportAssetScope::Resource { identity, .. } =
                        &mut body.project.assets[0].scope
                    else {
                        unreachable!()
                    };
                    identity.kind = TeletextResourceKindWire::Gpop;
                }
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
                let project = serde_json::to_value(&body.project).unwrap();
                assert_eq!(
                    project["assets"][1]["scope"],
                    json!({"kind":"object_tti_fragment_v1","resource_name":"resource.bin","object_id":65535,"fragment_index":0,"fragment_count":count,"command_start":0,"command_end":1,"native_word_start":1,"native_word_end":2,"editor_page":256,"editor_ordinal":0})
                );
            }
        }
    }
    let body = fragment_receipt(506);
    let mut expanded = budget();
    expanded.max_files = 508;
    expanded.max_bytes = 1_000_000;
    let bytes = write_teletext_export_receipt_body(&body, expanded).unwrap();
    assert_eq!(
        parse_teletext_export_receipt_body(&bytes, expanded).unwrap(),
        body
    );
}

#[test]
fn object_fragments_v7_singleton_empty_and_hidden_only_bodies_retain_explicit_ranges() {
    for command_end in [0, 1, 8192, u64::MAX] {
        let mut value = project_json(1);
        set(&mut value, 0, "command_end", json!(command_end));
        set(&mut value, 0, "native_word_end", json!(1));
        let project = parse_teletext_export_project(&as_bytes(&value), budget()).unwrap();
        assert_eq!(
            serde_json::to_value(project).unwrap()["assets"][1]["scope"]["command_end"],
            json!(command_end)
        );
    }
    for field in ["command_end", "native_word_end"] {
        let mut value = project_json(2);
        let replacement = i32::from(field != "command_end");
        set(&mut value, 0, field, json!(replacement));
        reject(&value);
    }
}

#[test]
fn object_fragments_v7_require_complete_unique_ordered_contiguous_group_ranges() {
    for missing in [0, 1] {
        let mut value = project_json(2);
        value["assets"].as_array_mut().unwrap().remove(missing + 1);
        reject(&value);
    }
    for (fragment, field, replacement) in [
        (0, "fragment_count", json!(0)),
        (0, "fragment_count", json!(508)),
        (0, "fragment_count", json!(1)),
        (1, "fragment_count", json!(3)),
        (1, "fragment_index", json!(0)),
        (1, "fragment_index", json!(2)),
        (0, "command_start", json!(1)),
        (1, "command_start", json!(0)),
        (1, "command_start", json!(2)),
        (1, "command_end", json!(0)),
        (0, "native_word_start", json!(0)),
        (0, "native_word_end", json!(508)),
        (1, "native_word_start", json!(1)),
        (1, "native_word_start", json!(3)),
        (1, "native_word_end", json!(1)),
    ] {
        let mut value = project_json(2);
        set(&mut value, fragment, field, replacement);
        reject(&value);
    }
    let mut duplicate = project_json(2);
    let mut asset = duplicate["assets"][1].clone();
    asset["name"] = json!("duplicate.tti");
    duplicate["assets"].as_array_mut().unwrap().push(asset);
    reject(&duplicate);
    // Asset order is not fragment order; the explicit exact index establishes it.
    let mut reordered = project_json(2);
    reordered["assets"].as_array_mut().unwrap().swap(1, 2);
    assert!(parse_teletext_export_project(&as_bytes(&reordered), budget()).is_ok());
}

#[test]
fn object_fragments_v7_reject_old_profiles_mixed_scopes_and_preserve_v6_codec() {
    for profile in [
        "qteletextmaker-1.0",
        "qteletextmaker-1.0-carrier-v2",
        "qteletextmaker-1.0-carrier-v3",
        "qteletextmaker-1.0-carrier-v4",
        "qteletextmaker-1.0-carrier-v5",
        "qteletextmaker-1.0-carrier-v6",
    ] {
        let mut value = project_json(1);
        value["editor_profile"] = json!(profile);
        reject(&value);
        let mut project = valid_project();
        project.editor_profile = profile.into();
        let literal = serde_json::to_vec(&project).unwrap();
        assert_eq!(
            write_teletext_export_project(&project, budget()).unwrap(),
            literal
        );
        assert_eq!(
            parse_teletext_export_project(&literal, budget()).unwrap(),
            project
        );
    }
    let mut v6 = fragment_receipt(1);
    v6.project.editor_profile = "qteletextmaker-1.0-carrier-v6".into();
    v6.project.assets[1].scope = TeletextExportAssetScope::ObjectTtiV1 {
        resource_name: "resource.bin".into(),
        object_id: 65535,
        editor_page: 0x100,
        editor_ordinal: 0,
    };
    let bytes = write_teletext_export_receipt_body(&v6, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_receipt_body(&bytes, budget()).unwrap(),
        v6
    );
    v6.project.editor_profile = V7.into();
    assert!(v6.validate(budget()).is_err());
    let mut mixed = fragment_receipt(1).project;
    let mut asset = v6.project.assets[1].clone();
    asset.name = "legacy-object.tti".into();
    mixed.assets.push(asset);
    assert!(mixed.validate(budget()).is_err());
}

#[test]
fn object_fragments_v7_scope_is_closed_nonnull_and_numeric_fields_are_typed() {
    let original = project_json(1);
    for scope in [
        Value::Null,
        json!([]),
        json!([
            "object_tti_fragment_v1",
            "resource.bin",
            65535,
            0,
            1,
            0,
            1,
            1,
            2,
            256,
            0
        ]),
        json!({"kind":"object_tti_fragment_v2"}),
    ] {
        let mut value = original.clone();
        value["assets"][1]["scope"] = scope;
        reject(&value);
    }
    for field in [
        "resource_name",
        "object_id",
        "fragment_index",
        "fragment_count",
        "command_start",
        "command_end",
        "native_word_start",
        "native_word_end",
        "editor_page",
        "editor_ordinal",
    ] {
        let mut missing = original.clone();
        missing["assets"][1]["scope"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        reject(&missing);
        for replacement in [Value::Null, json!([]), json!({})] {
            let mut value = original.clone();
            set(&mut value, 0, field, replacement);
            reject(&value);
        }
    }
    for field in ["unknown", "identity", "owner"] {
        let mut value = original.clone();
        set(&mut value, 0, field, json!({}));
        reject(&value);
    }
    for (field, replacement) in [
        ("object_id", json!(65536)),
        ("fragment_index", json!(-1)),
        ("fragment_count", json!(65536)),
        ("command_start", json!(-1)),
        ("command_end", json!(1.5)),
        ("native_word_start", json!(65536)),
        (
            "editor_ordinal",
            serde_json::from_str::<Value>("18446744073709551616").unwrap(),
        ),
    ] {
        let mut value = original.clone();
        set(&mut value, 0, field, replacement);
        reject(&value);
    }
    let encoded = String::from_utf8(as_bytes(&original)).unwrap();
    let duplicate = encoded.replacen(
        "\"fragment_index\":0",
        "\"fragment_index\":0,\"fragment_index\":0",
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
fn object_fragments_v7_raw_resource_and_fixed_editor_identity_are_exact() {
    for (field, replacement) in [
        ("resource_name", json!("missing.bin")),
        ("resource_name", json!("fragment-0.tti")),
        ("resource_name", json!("200-0123.tti")),
        ("resource_name", json!("../resource.bin")),
        ("editor_page", json!(0x200)),
        ("editor_ordinal", json!(1)),
    ] {
        let mut value = project_json(1);
        set(&mut value, 0, field, replacement);
        reject(&value);
    }
    for kind in ["drcs", "gdrcs", "mot", "mip"] {
        let mut value = project_json(1);
        value["assets"][0]["scope"]["identity"]["kind"] = json!(kind);
        reject(&value);
    }
    for (field, replacement) in [
        ("magazine", 0),
        ("magazine", 9),
        ("page", 0xfe),
        ("page", 0xfd),
    ] {
        let mut value = project_json(1);
        value["assets"][0]["scope"]["identity"]["page"][field] = json!(replacement);
        reject(&value);
    }
    let mut value = project_json(1);
    value["assets"][0]["scope"]["identity"]["subpage"] = json!(16);
    reject(&value);
    let mut value = project_json(1);
    value["assets"][1]["unit_count"] = json!(1);
    reject(&value);
    let mut duplicate = project_json(1);
    let raw = duplicate["assets"][0].clone();
    duplicate["assets"].as_array_mut().unwrap().push(raw);
    reject(&duplicate);
}

#[test]
fn object_fragments_v7_receipts_parse_each_file_identity_and_retain_existing_budgets() {
    let original = fragment_receipt(2);
    for text in [
        "PN,20000\r\nSC,0000\r\n",
        "PN,10001\r\nSC,0001\r\n",
        "PN,10000\r\nSC,0001\r\n",
        "PN,10000\r\n",
    ] {
        let mut changed = original.clone();
        changed.files[3].bytes = text.as_bytes().to_vec();
        changed.project.assets[2].bytes = text.len();
        assert_eq!(
            changed.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Tti
        );
    }
    let mut missing = original.clone();
    missing.files.pop();
    assert_eq!(
        missing.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Files
    );
    let encoded = write_teletext_export_receipt_body(&original, budget()).unwrap();
    let mut limited = budget();
    limited.max_bytes = encoded.len() - 1;
    assert_eq!(
        parse_teletext_export_receipt_body(&encoded, limited)
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Bytes
    );
    limited = budget();
    limited.max_files = 3;
    assert!(original.validate(limited).is_err());
    limited = budget();
    limited.max_file_bytes = original.project.assets[1].bytes - 1;
    assert!(original.validate(limited).is_err());
    limited = budget();
    limited.max_units = 1;
    assert!(
        original.project.validate(limited).is_ok(),
        "fragments do not consume protected raw unit quota"
    );
}

#[test]
fn object_fragments_v7_inherit_dense_and_padded_banks_and_editor_presentation() {
    for subpages in [&[0][..], &[1][..], &[0, 2][..]] {
        let mut body = drcs_set_receipt(subpages);
        let object = fragment_receipt(1);
        body.project.editor_profile = V7.into();
        body.project.assets.extend(object.project.assets);
        body.files.extend(object.files.into_iter().skip(1));
        body.project.pages[0].editor_presentation = Some(
            TeletextExportEditorPresentation::Qtm10OriginalPopFallbackV1 {
                national_option: 1,
                pop_flags: 14,
            },
        );
        let bytes = write_teletext_export_receipt_body(&body, budget()).unwrap();
        assert_eq!(
            parse_teletext_export_receipt_body(&bytes, budget()).unwrap(),
            body
        );
        body.project.pages[0].editor_presentation = Some(
            TeletextExportEditorPresentation::Qtm10OriginalPopFallbackV1 {
                national_option: 7,
                pop_flags: 14,
            },
        );
        assert!(body.validate(budget()).is_err());
    }
}

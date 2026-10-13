use super::*;
use serde_json::{json, Value};

fn budget() -> TeletextExportProjectBudget {
    TeletextExportProjectBudget {
        max_bytes: 65536,
        max_files: 8,
        max_producers: 4,
        max_file_bytes: 4096,
        max_units: 1024,
    }
}
fn project() -> TeletextExportProject {
    let digest = "a".repeat(64);
    TeletextExportProject {
        schema_version: 1,
        editor_profile: "qteletextmaker-1.0".into(),
        original: TeletextExportOriginal {
            design_id: 4,
            revision: 2,
            digest: digest.clone(),
            expected_head: 3,
            document_version: 1,
        },
        channel_id: 7,
        entry: TeletextPageAddressWire {
            page: 200,
            subcode: 0x0123,
        },
        language: "eng".into(),
        v1_region: None,
        producers: vec![TeletextExportProducer {
            ordinal: 0,
            addon_id: "org.ersatzrs.synthetic-guide".into(),
            contract: 2,
            binding_revision: 2,
            profile_id: 3,
            profile_revision: 4,
            installation: TeletextExportInstallation {
                state_revision: "1".into(),
                version: "1.0.0".into(),
                package_sha256: digest.clone(),
                content_sha256: digest.clone(),
                manifest_sha256: digest.clone(),
            },
            authoring: None,
        }],
        pages: vec![TeletextExportPageMapping {
            name: "200-0123.tti".into(),
            page: TeletextPageAddressWire {
                page: 200,
                subcode: 0x0123,
            },
            producer_ordinal: 0,
            editor_page: 0x200,
            editor_ordinal: 0,
            language: "eng".into(),
            kind: TeletextPageKindWire::FullPage,
            header_controls: 0,
            navigation: Vec::new(),
            materialized_magazine: None,
            tti_bytes: 0,
            tti_sha256: digest,
        }],
        assets: Vec::new(),
    }
}
fn receipt() -> TeletextExportReceiptBody {
    let bytes =
        b"PN,20000\r\nSC,0000\r\nPS,0000\r\nOL,1,                                        \r\n"
            .to_vec();
    let mut project = project();
    project.pages[0].tti_bytes = bytes.len();
    TeletextExportReceiptBody {
        schema_version: 1,
        project,
        files: vec![TeletextExportFile {
            name: "200-0123.tti".into(),
            bytes,
        }],
    }
}
fn valid_project() -> TeletextExportProject {
    receipt().project
}
fn as_bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}

#[test]
fn project_receipt_and_sidecar_round_trip_without_file_authority() {
    let body = receipt();
    let encoded = write_teletext_export_receipt_body(&body, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_receipt_body(&encoded, budget()).unwrap(),
        body
    );
    let project = write_teletext_export_project(&body.project, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_project(&project, budget()).unwrap(),
        body.project
    );
    let sidecar = TeletextExportSidecar {
        schema_version: 1,
        receipt_id: 9,
        receipt_digest: "b".repeat(64),
        project: body.project,
    };
    let encoded = write_teletext_export_sidecar(&sidecar, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_sidecar(&encoded, budget()).unwrap(),
        sidecar
    );
    let object: Value = serde_json::from_slice(&encoded).unwrap();
    assert!(object.get("files").is_none());
    assert!(object.get("bytes").is_none());
    assert!(object.get("permissions").is_none());
}
#[test]
fn duplicate_keys_at_each_level_are_not_silently_merged() {
    let project =
        String::from_utf8(write_teletext_export_project(&valid_project(), budget()).unwrap())
            .unwrap();
    for duplicate in [
        project.replacen(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
            1,
        ),
        project.replacen("\"page\":200", "\"page\":200,\"page\":200", 1),
        project.replacen("\"contract\":2", "\"contract\":2,\"contract\":2", 1),
    ] {
        assert_eq!(
            parse_teletext_export_project(duplicate.as_bytes(), budget())
                .unwrap_err()
                .kind,
            TeletextExportProjectFailure::Duplicate
        );
    }
}
#[test]
fn closed_objects_reject_settings_authority_unknown_fields_and_sequences() {
    let source = serde_json::to_value(valid_project()).unwrap();
    for (location, key, value) in [
        ("root", "permissions", json!("edit")),
        ("producer", "effective_settings", json!({"token":"private"})),
        ("producer", "secret_reference", json!("private")),
        ("producer", "activate", json!(true)),
        ("installation", "extra", json!(1)),
    ] {
        let mut p = source.clone();
        let target = match location {
            "root" => &mut p,
            "producer" => &mut p["producers"][0],
            _ => &mut p["producers"][0]["installation"],
        };
        target.as_object_mut().unwrap().insert(key.into(), value);
        assert!(parse_teletext_export_project(&as_bytes(&p), budget()).is_err());
    }
    for field in ["original", "entry"] {
        let mut p = source.clone();
        p[field] = json!([4, 2, "a", 3, 1]);
        assert!(parse_teletext_export_project(&as_bytes(&p), budget()).is_err());
    }
    let mut p = source;
    p["producers"][0] = json!([0, "org.ersatzrs.synthetic-guide", 2, 2, 3, 4, null, null]);
    assert!(parse_teletext_export_project(&as_bytes(&p), budget()).is_err());
}
#[test]
fn nullable_fields_are_required_even_when_their_value_is_null() {
    let source = serde_json::to_value(valid_project()).unwrap();
    for field in ["v1_region", "authoring", "materialized_magazine"] {
        let mut p = source.clone();
        match field {
            "v1_region" => {
                p.as_object_mut().unwrap().remove(field);
            }
            "authoring" => {
                p["producers"][0].as_object_mut().unwrap().remove(field);
            }
            _ => {
                p["pages"][0].as_object_mut().unwrap().remove(field);
            }
        }
        assert!(
            parse_teletext_export_project(&as_bytes(&p), budget()).is_err(),
            "{field}"
        );
    }
}
#[test]
fn paths_and_duplicate_mapping_identities_are_rejected() {
    for name in [
        "",
        ".",
        "..",
        "../page.tti",
        "sub/page.tti",
        "sub\\page.tti",
        "C:page.tti",
        "ページ.tti",
        "page\n.tti",
    ] {
        let mut p = valid_project();
        p.pages[0].name = name.into();
        assert!(p.validate(budget()).is_err(), "{name}");
    }
    let mut p = valid_project();
    p.pages.push(p.pages[0].clone());
    p.pages[1].name = "another.tti".into();
    assert!(p.validate(budget()).is_err());
    let mut p = valid_project();
    p.producers.push(p.producers[0].clone());
    p.producers[1].ordinal = 1;
    assert!(p.validate(budget()).is_err());
}
#[test]
fn indices_hashes_versions_editor_ordinals_and_header_ranges_are_checked() {
    let source = serde_json::to_value(valid_project()).unwrap();
    for (field, value) in [
        ("producer_ordinal", json!(1)),
        ("editor_ordinal", json!(1)),
        ("editor_page", json!(513)),
        ("header_controls", json!(2048)),
        ("tti_sha256", json!("A".repeat(64))),
        ("materialized_magazine", json!(3)),
    ] {
        let mut p = source.clone();
        p["pages"][0][field] = value;
        assert!(
            parse_teletext_export_project(&as_bytes(&p), budget()).is_err(),
            "{field}"
        );
    }
    let mut p = valid_project();
    p.original.expected_head = 1;
    assert!(p.validate(budget()).is_err());
    p = valid_project();
    p.original.document_version = 2;
    assert!(p.validate(budget()).is_err());
    p = valid_project();
    p.producers[0].contract = 1;
    assert!(p.validate(budget()).is_err());
    p.v1_region = Some(TeletextSavedV1Region::WesternLatin);
    assert!(p.validate(budget()).is_ok());
}
#[test]
fn caller_limits_and_bounded_writer_are_enforced() {
    let body = receipt();
    let encoded = write_teletext_export_receipt_body(&body, budget()).unwrap();
    for which in 0..5 {
        let mut b = budget();
        match which {
            0 => b.max_bytes = 0,
            1 => b.max_files = 0,
            2 => b.max_producers = 0,
            3 => b.max_file_bytes = 0,
            _ => b.max_units = 0,
        }
        assert_eq!(
            parse_teletext_export_receipt_body(&encoded, b)
                .unwrap_err()
                .kind,
            TeletextExportProjectFailure::Budget
        );
    }
    let mut b = budget();
    b.max_bytes = encoded.len() - 1;
    assert!(parse_teletext_export_receipt_body(&encoded, b).is_err());
    assert!(write_teletext_export_receipt_body(&body, b).is_err());
    b = budget();
    b.max_file_bytes = body.files[0].bytes.len() - 1;
    assert!(parse_teletext_export_receipt_body(&encoded, b).is_err());
    b = budget();
    b.max_units = 1;
    assert!(parse_teletext_export_receipt_body(&encoded, b).is_err());
}
#[test]
fn excessive_nested_input_is_rejected_before_typed_allocation() {
    let nested = format!("{}0{}", "[".repeat(40), "]".repeat(40));
    assert_eq!(
        parse_teletext_export_project(nested.as_bytes(), budget())
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Depth
    );
}

#[test]
fn file_page_count_is_preflighted_without_restricting_original_metadata_pages() {
    let mut bounds = budget();
    bounds.max_files = 1;
    let input = serde_json::to_value(valid_project()).unwrap();
    let mut too_many = input.clone();
    too_many["pages"]
        .as_array_mut()
        .unwrap()
        .push(input["pages"][0].clone());
    assert_eq!(
        parse_teletext_export_project(&as_bytes(&too_many), bounds)
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Limit
    );
    // This nonallocating scan checks collection admission only; the closed typed
    // parser separately validates authoring fields and actual native ownership.
    let captured = json!({"project":{"pages":[{}],"producers":[{"authoring":{"pages":[{},{}]}}]}});
    scan_json(&as_bytes(&captured), bounds).unwrap();
    let too_many_files = json!({"project":{"pages":[{},{}]}});
    assert_eq!(
        scan_json(&as_bytes(&too_many_files), bounds)
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Limit
    );
}
#[test]
fn body_requires_exact_files_and_exact_single_editor_page_zero() {
    let body = receipt();
    let mut changed = body.clone();
    changed.files[0].name = "wrong.tti".into();
    assert!(changed.validate(budget()).is_err());
    changed = body.clone();
    changed.files.clear();
    assert!(changed.validate(budget()).is_err());
    changed = body.clone();
    changed.files.push(changed.files[0].clone());
    assert!(changed.validate(budget()).is_err());
    for bytes in [
        b"PN,20100\r\nSC,0000\r\n".as_slice(),
        b"PN,20000\r\nSC,0001\r\n",
        b"PN,20000\r\n",
        b"PN,20000\r\nSC,0000\r\nPN,20001\r\nSC,0001\r\n",
        b"PN,20000\r\nSC,0000\r\nZZ,loss\r\n",
    ] {
        let mut changed = body.clone();
        changed.files[0].bytes = bytes.to_vec();
        changed.project.pages[0].tti_bytes = bytes.len();
        assert_eq!(
            changed.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Tti
        );
    }
}
#[test]
fn assets_have_exact_46_byte_unit_counts_and_private_byte_coverage() {
    let mut body = receipt();
    body.project.assets.push(TeletextExportAssetMapping {
        name: "magazine-2.units".into(),
        scope: TeletextExportAssetScope::Magazine { magazine: 2 },
        unit_count: 1,
        bytes: 46,
        sha256: "b".repeat(64),
    });
    body.files.push(TeletextExportFile {
        name: "magazine-2.units".into(),
        bytes: vec![0; 46],
    });
    assert!(body.validate(budget()).is_ok());
    body.project.assets[0].bytes = 45;
    assert!(body.validate(budget()).is_err());
    body.project.assets[0].bytes = 46;
    body.files[1].bytes.pop();
    assert!(body.validate(budget()).is_err());
}
#[test]
fn hashes_are_only_structural_claims_until_native_confirmation() {
    let mut body = receipt();
    body.project.pages[0].tti_sha256 = "0".repeat(64);
    // Deliberately false but well-shaped digest: this codec cannot attest content.
    assert!(body.validate(budget()).is_ok());
    let text = format!("{body:?} {:?}", body.files[0]);
    assert!(!text.contains("200-0123"));
    assert!(!text.contains("PN,"));
    assert!(!text.contains(&"a".repeat(64)));
    let error = TeletextExportProjectError {
        kind: TeletextExportProjectFailure::Tti,
        item: Some(0),
    };
    assert!(!error.to_string().contains("PN,"));
}
#[test]
fn v3_metadata_requires_selected_page_coverage_and_preserves_other_original_pages() {
    let metadata = json!({"schema_version":1,"template":{"id":"guide","revision":"1"},"theme":{"id":"day","overrides":[]},"pages":[{"page":{"page":200,"subcode":291},"regions":[],"dynamic_fields":[]},{"page":{"page":201,"subcode":0},"regions":[],"dynamic_fields":[]}]});
    let mut source = serde_json::to_value(valid_project()).unwrap();
    source["producers"][0]["contract"] = json!(3);
    source["producers"][0]["authoring"] = metadata;
    assert!(parse_teletext_export_project(&as_bytes(&source), budget()).is_ok());
    source["producers"][0]["authoring"]["pages"][0]["page"]["subcode"] = json!(292);
    assert!(parse_teletext_export_project(&as_bytes(&source), budget()).is_err());
}

#[test]
fn decimal_addresses_map_to_raw_bcd_editor_pages() {
    for (decimal, bcd) in [(123, 0x123), (899, 0x899)] {
        let mut project = valid_project();
        project.entry.page = decimal;
        project.pages[0].page.page = decimal;
        project.pages[0].editor_page = bcd;
        project.pages[0].materialized_magazine = Some(u8::try_from(decimal / 100).unwrap());
        assert!(project.validate(budget()).is_ok());
        project.pages[0].editor_page = decimal;
        assert!(project.validate(budget()).is_err());
    }
    for invalid in [99, 900] {
        let mut project = valid_project();
        project.pages[0].page.page = invalid;
        assert!(project.validate(budget()).is_err());
        let mut project = valid_project();
        project.entry.page = invalid;
        assert!(project.validate(budget()).is_err());
    }
}
#[test]
fn selected_subset_may_exclude_the_original_service_entry() {
    let mut project = valid_project();
    project.pages[0].page.page = 201;
    project.pages[0].editor_page = 0x201;
    assert_eq!(project.entry.page, 200);
    assert!(project.validate(budget()).is_ok());
    let encoded = write_teletext_export_project(&project, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_project(&encoded, budget()).unwrap(),
        project
    );
}

#[test]
fn navigation_uses_decimal_display_numbers_and_magazine_uses_hundreds() {
    let mut project = valid_project();
    project.pages[0]
        .navigation
        .push(TeletextNavigationLinkWire {
            role: ersatzrs_addon_contract::TeletextNavigationRoleWire::Red,
            target: ersatzrs_addon_contract::TeletextNavigationTargetWire {
                page: 899,
                subcode: None,
            },
        });
    project.pages[0].materialized_magazine = Some(2);
    assert!(project.validate(budget()).is_ok());
    project.pages[0].navigation[0].target.page = 0x899;
    assert!(project.validate(budget()).is_err());
    project.pages[0].navigation[0].target.page = 123;
    assert!(project.validate(budget()).is_ok());
    project.pages[0].materialized_magazine = Some(3);
    assert!(project.validate(budget()).is_err());
}

#[test]
fn per_page_language_and_kind_are_required_and_preserved_independently() {
    let mut project = valid_project();
    project.pages[0].language = "nld".into();
    project.pages[0].kind = TeletextPageKindWire::Subtitle;
    project.pages[0].header_controls = 4;
    let encoded = write_teletext_export_project(&project, budget()).unwrap();
    let decoded = parse_teletext_export_project(&encoded, budget()).unwrap();
    assert_eq!(decoded.language, "eng");
    assert_eq!(decoded.pages[0].language, "nld");
    assert_eq!(decoded.pages[0].kind, TeletextPageKindWire::Subtitle);
    let source = serde_json::to_value(project).unwrap();
    for key in ["language", "kind"] {
        let mut value = source.clone();
        value["pages"][0].as_object_mut().unwrap().remove(key);
        assert!(parse_teletext_export_project(&as_bytes(&value), budget()).is_err());
    }
    for language in ["en", "en-GB", "ENG", ""] {
        let mut value = source.clone();
        value["pages"][0]["language"] = json!(language);
        assert!(parse_teletext_export_project(&as_bytes(&value), budget()).is_err());
    }
    let mut value = source;
    value["pages"][0]["kind"] = json!({"subtitle":null});
    assert!(parse_teletext_export_project(&as_bytes(&value), budget()).is_err());
}
#[test]
fn page_kind_must_match_only_the_corresponding_c5_c6_control() {
    for (kind, expected) in [
        (TeletextPageKindWire::FullPage, 0),
        (TeletextPageKindWire::Newsflash, 2),
        (TeletextPageKindWire::Subtitle, 4),
    ] {
        for flags in [0, 2, 4, 6] {
            let mut project = valid_project();
            project.pages[0].kind = kind;
            project.pages[0].header_controls = flags | 1;
            assert_eq!(project.validate(budget()).is_ok(), flags == expected);
        }
    }
}

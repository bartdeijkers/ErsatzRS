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
fn v2_editor_profile_round_trips_without_changing_receipt_schema_or_raw_controls() {
    let mut body = receipt();
    body.project.editor_profile = "qteletextmaker-1.0-carrier-v2".into();
    body.project.pages[0].header_controls = 0x0481;
    let encoded = write_teletext_export_receipt_body(&body, budget()).unwrap();
    assert_eq!(
        parse_teletext_export_receipt_body(&encoded, budget()).unwrap(),
        body
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
}
#[test]
fn unknown_editor_carrier_profiles_are_rejected() {
    for profile in ["", "qteletextmaker-1.1", "qteletextmaker-1.0-carrier-v5"] {
        let mut value = valid_project();
        value.editor_profile = profile.into();
        assert_eq!(
            value.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Schema
        );
    }
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

fn drcs_companion_receipt() -> TeletextExportReceiptBody {
    let mut body = receipt();
    body.project.editor_profile = "qteletextmaker-1.0-carrier-v3".into();
    let companion = b"PN,2C000\r\nSC,0000\r\nPS,8000\r\n".to_vec();
    body.project.assets.extend([
        TeletextExportAssetMapping {
            name: "resource-0000.dvb".into(),
            scope: TeletextExportAssetScope::Resource {
                identity: TeletextResourceIdentityWire {
                    kind: TeletextResourceKindWire::Drcs,
                    page: ersatzrs_addon_contract::TeletextRawDataPageNumberWire {
                        magazine: 2,
                        page: 0xc0,
                    },
                    subpage: 0,
                },
                physical_subcode: 0x0110,
            },
            unit_count: 1,
            bytes: 46,
            sha256: "b".repeat(64),
        },
        TeletextExportAssetMapping {
            name: "resource-0000.tti".into(),
            scope: TeletextExportAssetScope::DrcsTti {
                resource_name: "resource-0000.dvb".into(),
                editor_ordinal: 0,
            },
            unit_count: 0,
            bytes: companion.len(),
            sha256: "c".repeat(64),
        },
    ]);
    body.files.extend([
        TeletextExportFile {
            name: "resource-0000.dvb".into(),
            bytes: vec![0x55; 46],
        },
        TeletextExportFile {
            name: "resource-0000.tti".into(),
            bytes: companion,
        },
    ]);
    body
}

#[test]
fn drcs_companion_v3_round_trips_without_changing_raw_asset_or_charging_units() {
    for kind in [
        TeletextResourceKindWire::Drcs,
        TeletextResourceKindWire::Gdrcs,
    ] {
        let mut body = drcs_companion_receipt();
        if let TeletextExportAssetScope::Resource { identity, .. } =
            &mut body.project.assets[0].scope
        {
            identity.kind = kind;
        }
        let mut bounds = budget();
        bounds.max_units = 1;
        assert!(body.project.validate(bounds).is_ok());
        // Receipt TTI parsing retains its existing independent record cap.
        bounds.max_units = 4;
        let encoded = write_teletext_export_receipt_body(&body, bounds).unwrap();
        let restored = parse_teletext_export_receipt_body(&encoded, bounds).unwrap();
        assert_eq!(restored, body);
        assert_eq!(restored.files[1].bytes, vec![0x55; 46]);
        assert_eq!(restored.project.assets[0].unit_count, 1);
        assert_eq!(restored.project.assets[1].unit_count, 0);
    }
}

#[test]
fn drcs_companion_rejects_legacy_v2_ordinals_and_duplicate_resource_companions() {
    for profile in ["qteletextmaker-1.0", "qteletextmaker-1.0-carrier-v2"] {
        let mut body = drcs_companion_receipt();
        body.project.editor_profile = profile.into();
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Mapping
        );
    }
    let mut body = drcs_companion_receipt();
    if let TeletextExportAssetScope::DrcsTti { editor_ordinal, .. } =
        &mut body.project.assets[1].scope
    {
        *editor_ordinal = 1;
    }
    assert!(body.validate(budget()).is_err());
    let mut body = drcs_companion_receipt();
    let mut duplicate = body.project.assets[1].clone();
    duplicate.name = "duplicate.tti".into();
    body.project.assets.push(duplicate);
    assert_eq!(
        body.project.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Mapping
    );
}

#[test]
fn drcs_companion_reference_must_resolve_to_original_drcs_dvb_resource() {
    for resource_name in [
        "missing.dvb",
        "resource-0000.tti",
        "200-0123.tti",
        "../resource-0000.dvb",
    ] {
        let mut body = drcs_companion_receipt();
        if let TeletextExportAssetScope::DrcsTti {
            resource_name: target,
            ..
        } = &mut body.project.assets[1].scope
        {
            *target = resource_name.into();
        }
        assert!(body.project.validate(budget()).is_err());
    }
    let mut body = drcs_companion_receipt();
    body.project.assets[0].scope = TeletextExportAssetScope::Magazine { magazine: 2 };
    assert!(body.project.validate(budget()).is_err());
    for field in ["kind", "subpage", "page"] {
        let mut body = drcs_companion_receipt();
        if let TeletextExportAssetScope::Resource { identity, .. } =
            &mut body.project.assets[0].scope
        {
            match field {
                "kind" => identity.kind = TeletextResourceKindWire::Pop,
                "subpage" => identity.subpage = 1,
                _ => identity.page.page = 0xff,
            }
        }
        assert!(body.project.validate(budget()).is_err());
    }
}

#[test]
fn drcs_companion_requires_positive_bounded_bytes_hash_and_zero_units() {
    for field in ["units", "empty", "bytes", "hash"] {
        let mut body = drcs_companion_receipt();
        let companion = &mut body.project.assets[1];
        match field {
            "units" => companion.unit_count = 1,
            "empty" => companion.bytes = 0,
            "bytes" => companion.bytes = budget().max_file_bytes + 1,
            _ => companion.sha256 = "C".repeat(64),
        }
        assert!(body.project.validate(budget()).is_err());
    }
    let mut bounds = budget();
    bounds.max_files = 2;
    assert!(drcs_companion_receipt().project.validate(bounds).is_err());
}

#[test]
fn drcs_companion_receipt_checks_raw_bank_and_both_ordinal_identities() {
    for text in [
        "PN,2C100\r\nSC,0000\r\n",
        "PN,2C001\r\nSC,0000\r\n",
        "PN,2C000\r\nSC,0001\r\n",
        "PN,2C000\r\n",
        "PN,2C000\r\nSC,0000\r\nPN,2C000\r\nSC,0000\r\n",
        "PN,2C000\r\nSC,0000\r\nPF,3,2\r\n",
    ] {
        let mut body = drcs_companion_receipt();
        body.files[2].bytes = text.as_bytes().to_vec();
        body.project.assets[1].bytes = text.len();
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Tti
        );
    }
    let mut body = drcs_companion_receipt();
    body.files[2].bytes.pop();
    assert_eq!(
        body.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Files
    );
    let mut body = drcs_companion_receipt();
    body.files.remove(1);
    assert_eq!(
        body.validate(budget()).unwrap_err().kind,
        TeletextExportProjectFailure::Files
    );
}

#[test]
fn drcs_companion_scope_remains_closed_nonnullable_and_redacted() {
    let body = drcs_companion_receipt();
    for scope in [
        json!({"kind":"drcs_tti","resource_name":"resource-0000.dvb","editor_ordinal":0,"extra":true}),
        json!({"kind":"drcs_tti","resource_name":null,"editor_ordinal":0}),
        json!({"kind":"drcs_tti","resource_name":"resource-0000.dvb","editor_ordinal":null}),
        json!({"kind":"drcs_tti","resource_name":"resource-0000.dvb"}),
        json!(["drcs_tti", "resource-0000.dvb", 0]),
    ] {
        let mut value = serde_json::to_value(&body.project).unwrap();
        value["assets"][1]["scope"] = scope;
        assert!(parse_teletext_export_project(&as_bytes(&value), budget()).is_err());
    }
    let debug = format!(
        "{:?} {:?}",
        body.project.assets[1], body.project.assets[1].scope
    );
    assert!(!debug.contains("resource-0000"));
    assert!(!debug.contains(&"c".repeat(64)));
}

fn drcs_set_receipt(subpages: &[u8]) -> TeletextExportReceiptBody {
    use crate::teletext_tti::{write_teletext_tti, TeletextTtiDocument};
    let mut body = receipt();
    body.project.editor_profile = "qteletextmaker-1.0-carrier-v4".into();
    let highest = *subpages.iter().max().unwrap();
    let mut resource_names = vec![None; usize::from(highest) + 1];
    for &subpage in subpages {
        let name = format!("resource-{subpage:04}.dvb");
        body.project.assets.push(TeletextExportAssetMapping {
            name: name.clone(),
            scope: TeletextExportAssetScope::Resource {
                identity: TeletextResourceIdentityWire {
                    kind: TeletextResourceKindWire::Drcs,
                    page: ersatzrs_addon_contract::TeletextRawDataPageNumberWire {
                        magazine: 2,
                        page: 0xc0,
                    },
                    subpage,
                },
                physical_subcode: 0x0110 | u16::from(subpage),
            },
            unit_count: 1,
            bytes: 46,
            sha256: "b".repeat(64),
        });
        body.files.push(TeletextExportFile {
            name: name.clone(),
            bytes: vec![0x55 + subpage; 46],
        });
        resource_names[usize::from(subpage)] = Some(name);
    }
    let mut records = Vec::new();
    for (subpage, resource) in resource_names.iter().enumerate() {
        records.extend([
            TeletextTtiRecord::PageNumber {
                page: 0x2c0,
                editor_ordinal: u64::try_from(subpage).unwrap(),
            },
            TeletextTtiRecord::SubpageOrdinal(u64::try_from(subpage).unwrap()),
        ]);
        if resource.is_some() {
            records.push(TeletextTtiRecord::PageStatus(0x8000));
            records.push(TeletextTtiRecord::BaseRow {
                row: 1,
                cells: [b'A'; 40],
            });
        } else {
            records.push(TeletextTtiRecord::Packet {
                row: 28,
                designation: 3,
                words: [
                    5, 0x3ffff, 0x3ffff, 0x3ffff, 0x3ffff, 0x3ffff, 0x3ffff, 0x3ffff, 0x3ffff,
                    0x3ffff, 0x3ffff, 0xfff, 0,
                ],
            });
        }
    }
    let tti_budget = TeletextTtiBudget {
        max_bytes: budget().max_file_bytes,
        max_records: budget().max_units,
    };
    let document = TeletextTtiDocument::checked_from_records(records, tti_budget).unwrap();
    let bytes = write_teletext_tti(&document, tti_budget).unwrap();
    body.project.assets.push(TeletextExportAssetMapping {
        name: "drcs-set-normal-2c0.tti".into(),
        scope: TeletextExportAssetScope::DrcsTtiSet { resource_names },
        unit_count: 0,
        bytes: bytes.len(),
        sha256: "c".repeat(64),
    });
    body.files.push(TeletextExportFile {
        name: "drcs-set-normal-2c0.tti".into(),
        bytes,
    });
    body
}

fn replace_set_tti(body: &mut TeletextExportReceiptBody, bytes: Vec<u8>) {
    body.project.assets.last_mut().unwrap().bytes = bytes.len();
    body.files.last_mut().unwrap().bytes = bytes;
}

#[test]
fn drcs_set_v4_dense_leading_and_interior_holes_round_trip_exact_native_assets() {
    for subpages in [&[0, 1][..], &[1][..], &[0, 2][..], &[15][..]] {
        let body = drcs_set_receipt(subpages);
        let mut bounds = budget();
        bounds.max_files = body.files.len();
        // Sixteen editor positions can coexist with only three actual files.
        let encoded = write_teletext_export_receipt_body(&body, bounds).unwrap();
        let restored = parse_teletext_export_receipt_body(&encoded, bounds).unwrap();
        assert_eq!(restored, body);
        for (index, &subpage) in subpages.iter().enumerate() {
            assert_eq!(restored.files[index + 1].bytes, vec![0x55 + subpage; 46]);
        }
        let mut unit_bounds = bounds;
        unit_bounds.max_units = subpages.len();
        assert!(body.project.validate(unit_bounds).is_ok());
        let project_bytes = write_teletext_export_project(&body.project, unit_bounds).unwrap();
        assert_eq!(
            parse_teletext_export_project(&project_bytes, unit_bounds).unwrap(),
            body.project
        );
    }
}

#[test]
fn drcs_set_rejects_incomplete_hidden_reordered_and_cross_bank_resources() {
    for case in [
        "empty",
        "all-null",
        "trailing",
        "hidden",
        "truncated",
        "swapped",
        "duplicate",
        "missing",
    ] {
        let mut body = drcs_set_receipt(&[0, 2]);
        let TeletextExportAssetScope::DrcsTtiSet { resource_names } =
            &mut body.project.assets.last_mut().unwrap().scope
        else {
            panic!("set fixture")
        };
        match case {
            "empty" => resource_names.clear(),
            "all-null" => resource_names.fill(None),
            "trailing" => resource_names.push(None),
            "hidden" => resource_names[0] = None,
            "truncated" => resource_names.truncate(1),
            "swapped" => resource_names.swap(0, 2),
            "duplicate" => resource_names[2] = resource_names[0].clone(),
            _ => resource_names[2] = Some("missing.dvb".into()),
        }
        assert!(body.project.validate(budget()).is_err());
    }
    for field in ["bank", "kind", "logical"] {
        let mut body = drcs_set_receipt(&[0, 1]);
        if let TeletextExportAssetScope::Resource { identity, .. } =
            &mut body.project.assets[1].scope
        {
            match field {
                "bank" => identity.page.page = 0xc1,
                "kind" => identity.kind = TeletextResourceKindWire::Gdrcs,
                _ => identity.subpage = 0,
            }
        }
        assert!(body.project.validate(budget()).is_err());
    }
    let mut body = drcs_set_receipt(&[0, 1]);
    let mut duplicate = body.project.assets[0].clone();
    duplicate.name = "another-counter.dvb".into();
    if let TeletextExportAssetScope::Resource {
        physical_subcode, ..
    } = &mut duplicate.scope
    {
        *physical_subcode = 0x0120;
    }
    body.project.assets.push(duplicate);
    assert!(body.project.validate(budget()).is_err());
}

#[test]
fn drcs_set_scope_is_v4_only_closed_and_preallocation_bounded_at_sixteen() {
    for profile in [
        "qteletextmaker-1.0",
        "qteletextmaker-1.0-carrier-v2",
        "qteletextmaker-1.0-carrier-v3",
    ] {
        let mut body = drcs_set_receipt(&[0, 1]);
        body.project.editor_profile = profile.into();
        assert!(body.project.validate(budget()).is_err());
    }
    let body = drcs_set_receipt(&[1]);
    for scope in [
        json!({"kind":"drcs_tti_set","resource_names":null}),
        json!({"kind":"drcs_tti_set","resource_names":[false,"resource-0001.dvb"]}),
        json!({"kind":"drcs_tti_set","resource_names":[null,"resource-0001.dvb"],"extra":true}),
        json!({"kind":"drcs_tti_set"}),
    ] {
        let mut value = serde_json::to_value(&body.project).unwrap();
        value["assets"][1]["scope"] = scope;
        assert!(parse_teletext_export_project(&as_bytes(&value), budget()).is_err());
    }
    let mut value = serde_json::to_value(&body.project).unwrap();
    value["assets"][1]["scope"]["resource_names"] = json!(vec![None::<String>; 17]);
    assert_eq!(
        parse_teletext_export_project(&as_bytes(&value), budget())
            .unwrap_err()
            .kind,
        TeletextExportProjectFailure::Limit
    );
}

#[test]
fn drcs_set_receipt_rejects_wrong_truncated_duplicate_and_missing_group_identities() {
    let original = drcs_set_receipt(&[0, 1]);
    let tti = String::from_utf8(original.files.last().unwrap().bytes.clone()).unwrap();
    for changed in [
        tti.replace("PN,2C001", "PN,2C101"),
        tti.replace("PN,2C001", "PN,2C002"),
        tti.replace("SC,0001", "SC,0000"),
        tti.replace("SC,0001\r\n", ""),
        tti.split("PN,2C001").next().unwrap().to_owned(),
        format!("{tti}PN,2C002\r\nSC,0002\r\n"),
    ] {
        let mut body = original.clone();
        replace_set_tti(&mut body, changed.into_bytes());
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Tti
        );
    }
}

#[test]
fn drcs_set_padding_requires_all_undefined_modes_and_no_ptu_or_extra_packets() {
    use crate::teletext_tti::{write_teletext_tti, TeletextTtiDocument};
    let original = drcs_set_receipt(&[1]);
    let tti_budget = TeletextTtiBudget {
        max_bytes: budget().max_file_bytes,
        max_records: budget().max_units,
    };
    let parsed = parse_teletext_tti(&original.files.last().unwrap().bytes, tti_budget).unwrap();
    for change in ["defined", "function", "missing", "row", "packet", "status"] {
        let mut records = parsed.records().to_vec();
        match change {
            "defined" | "function" => {
                if let TeletextTtiRecord::Packet { words, .. } = &mut records[2] {
                    if change == "defined" {
                        words[1] &= !15;
                    } else {
                        words[0] = 4;
                    }
                }
            }
            "missing" => {
                records.remove(2);
            }
            "row" => records.insert(
                2,
                TeletextTtiRecord::BaseRow {
                    row: 1,
                    cells: [b' '; 40],
                },
            ),
            "status" => records.insert(2, TeletextTtiRecord::PageStatus(0x8000)),
            _ => records.insert(
                2,
                TeletextTtiRecord::Packet {
                    row: 26,
                    designation: 0,
                    words: [0; 13],
                },
            ),
        }
        let document = TeletextTtiDocument::checked_from_records(records, tti_budget).unwrap();
        let bytes = write_teletext_tti(&document, tti_budget).unwrap();
        let mut body = original.clone();
        replace_set_tti(&mut body, bytes);
        assert_eq!(
            body.validate(budget()).unwrap_err().kind,
            TeletextExportProjectFailure::Tti
        );
    }
}

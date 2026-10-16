use super::*;
use crate::{
    teletext_channel_documents::preflight_requests,
    teletext_design_documents::{
        TeletextSavedEditPolicy, TeletextSavedResourceTransmission, TeletextSavedResourceUpdate,
        TeletextSavedV1Region,
    },
    teletext_guide::{SuppliedTeletextGuideContext, TeletextGuideChannel},
};
use ersatzrs_core::repositories::{
    TeletextGuideSourceFence, TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
    TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
};
use ersatzrs_domain::{
    TeletextDeclaredServiceBudget, TeletextPageContract, TeletextPageNumber,
    TeletextRawDataPageNumber, TeletextResourceIdentity, TeletextResourceKind,
    TeletextResourceSubpageIndex, TeletextResourceUpdateCounter, TeletextServiceBudget,
    TeletextSubcode,
};
use serde_json::{json, Value};

const GENERATION: &str = "original-correlation-sentinel";
const ADDON: &str = "org.example.synthetic";

pub(super) fn budget() -> TeletextDesignDocumentBudget {
    TeletextDesignDocumentBudget {
        max_serialized_bytes: 128 * 1024,
        max_documents: 8,
        max_response_bytes: 64 * 1024,
        max_context_bytes: 16 * 1024,
        max_context_field_bytes: 2048,
        max_resource_transmissions: 8,
        validation: crate::teletext_profile_documents::TeletextProfileDocumentBudget {
            service: TeletextServiceBudget {
                max_records: 64,
                max_objects: 64,
                max_structure_items: 2048,
                max_base_content_bytes: 2048,
                max_commands: 2048,
                max_collected_commands: 2048,
                max_collected_drcs_references: 64,
            },
            declarations: TeletextDeclaredServiceBudget {
                max_mots: 8,
                max_displays: 64,
                max_resources: 64,
                max_defaults: 64,
                max_objects: 64,
            },
        },
    }
}
pub(super) fn address(subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn page(subcode: u16) -> Value {
    json!({"address":{"page":200,"subcode":subcode},"kind":"full_page","language":"eng",
        "national_option":0,"rows":[],"navigation":[]})
}
fn response(v3: bool) -> String {
    let base = json!({"schema_version":1,"generation":GENERATION,"language":"eng",
        "initial_page":200,"pages":[page(16),page(17)]});
    let value = if v3 {
        let mut service = base;
        service["schema_version"] = json!(2);
        for key in [
            "page_enhancements",
            "magazines",
            "objects",
            "drcs",
            "mots",
            "empty_object_pages",
        ] {
            service[key] = json!([]);
        }
        // The producer wire uses `subpage`; private lifecycle/domain storage
        // uses `logical_subpage`. Preserve each contract's exact field name.
        service["empty_object_pages"] =
            json!([ersatzrs_addon_contract::TeletextResourceIdentityWire {
                kind: ersatzrs_addon_contract::TeletextResourceKindWire::Pop,
                page: ersatzrs_addon_contract::TeletextRawDataPageNumberWire {
                    magazine: 2,
                    page: 0xa0
                },
                subpage: 15,
            }]);
        service["mot_selections"] = json!([
            {"display":{"page":200,"subcode":16},"mot":null},
            {"display":{"page":200,"subcode":17},"mot":null}]);
        let metadata_pages = [16,17].map(|subcode| json!({"page":{"page":200,"subcode":subcode},
            "regions":[{"id":"editable","kind":"free","area":"main","row":1,"column":0,"width":40,"height":1}],"dynamic_fields":[]}));
        json!({"schema_version":3,"service":service,"authoring":{"schema_version":1,
            "template":{"id":"original-template-sentinel","revision":"1"},
            "theme":{"id":"original-theme-sentinel","overrides":[]},"pages":metadata_pages}})
    } else {
        base
    };
    format!("{}\n \t", serde_json::to_string_pretty(&value).unwrap())
}
fn resource() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap()
}
fn original_value(v3: bool) -> Value {
    let contract = if v3 { 3 } else { 1 };
    let transmissions = if v3 {
        serde_json::to_value([TeletextSavedResourceTransmission {
            resource: resource(),
            counter: TeletextResourceUpdateCounter::new(5).unwrap(),
            update: TeletextSavedResourceUpdate::Changed,
            last_subpage: true,
        }])
        .unwrap()
    } else {
        json!([])
    };
    json!({"format_version":1,"selection":{"channel_id":9,"bindings":[{
        "contract":contract,"channel_id":9,"addon_id":ADDON,"binding_revision":3,
        "profile_id":11,"profile_revision":4,
        "installation":{"addon_id":ADDON,"state_revision":"original-install-sentinel","version":"1.2.3",
            "package_sha256":"a".repeat(64),"content_sha256":"b".repeat(64),"manifest_sha256":"c".repeat(64)},
        "effective_settings":[{"key":"THEME","value":{"kind":"literal","value":"private-setting-sentinel"}}]}]},
        "generation":GENERATION,"entry_addon_id":ADDON,"entry":{"page":200,"subcode":16},
        "v1_region":if v3 {Value::Null} else {json!("western_latin")},
        "edit_policy":{"policy_version":1,"kind":"original_only"},
        "resource_transmissions":transmissions,"documents":[{"contract":contract,"generation":GENERATION,"response":response(v3)}]})
}
pub(super) fn bytes(value: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.extend_from_slice(b"\n \t");
    bytes
}
pub(super) fn original_with_context() -> Vec<u8> {
    let mut value = original_value(true);
    let original = decode_teletext_design_documents(&bytes(&value), budget()).unwrap();
    let channels = [TeletextGuideChannel {
        id: 17,
        number: "17.1",
        name: "guide-name-é-sentinel",
        programmes: &[],
    }];
    let context = SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl"],
        timezone: "Europe/Amsterdam",
        captured_at: "2026-10-13T12:00:00.125Z".parse().unwrap(),
        channels: &channels,
    };
    let requests = preflight_requests(original.selection(), &context).unwrap();
    let request = requests.request(TeletextPageContract::V3).unwrap();
    value["guide_inputs"] =
        json!({"input_version":1,"v1":null,"v2":null,"v3":std::str::from_utf8(request).unwrap()});
    let parsed =
        ersatzrs_addon_contract::TeletextPagesRequest::from_json_for_schema(request, 3).unwrap();
    value["guide_source"] = serde_json::to_value(TeletextGuideSourceFence {
        version: TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
        context_digest_version: TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
        context_digest: [0x5a; 32],
        channel_ids: vec![17],
        captured_at: parsed.captured_at,
        start: parsed.window.start,
        finish: parsed.window.finish,
        locale: parsed.locale,
        timezone: parsed.timezone,
    })
    .unwrap();
    bytes(&value)
}
pub(super) fn provenance() -> TeletextSavedEditorialProvenance {
    TeletextSavedEditorialProvenance {
        export_receipt_id: 3,
        export_receipt_digest: "d".repeat(64),
        original_design_id: 7,
        original_revision: 2,
        original_digest: "e".repeat(64),
    }
}
pub(super) fn overlay() -> TeletextSavedEditorialOverlay {
    TeletextSavedEditorialOverlay::new(
        provenance(),
        [16, 17]
            .into_iter()
            .map(|subcode| {
                let mut cells = [b' '; 40];
                cells[0] = 1;
                cells[1] = if subcode == 16 { b'A' } else { b'B' };
                TeletextPhysicalBaseOverlay::new(
                    address(subcode),
                    vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap()
}
fn concept_value() -> Value {
    let original = bytes(&original_value(true));
    serde_json::from_slice(
        append_teletext_design_editorial_overlay(&original, &overlay(), budget())
            .unwrap()
            .bytes(),
    )
    .unwrap()
}
fn decode(
    value: &Value,
) -> Result<super::super::RestoredTeletextDesignDocuments, TeletextDesignDocumentError> {
    decode_teletext_design_documents(&serde_json::to_vec(value).unwrap(), budget())
}

#[test]
fn editorial_legacy_absence_and_exact_original_prefix_are_preserved() {
    for v3 in [false, true] {
        let original = bytes(&original_value(v3));
        let restored = decode_teletext_design_documents(&original, budget()).unwrap();
        assert!(restored.editorial_overlay().is_none());
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay(), budget()).unwrap();
        assert_eq!(encoded.version(), 1);
        let end = original
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .unwrap();
        assert_eq!(&encoded.bytes()[..end], &original[..end]);
        assert!(encoded.bytes().ends_with(&original[end..]));
        let changed = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
        assert_eq!(changed.selection(), restored.selection());
        assert_eq!(changed.generation(), restored.generation());
        assert_eq!(changed.sources()[0].bytes(), restored.sources()[0].bytes());
        assert_eq!(
            changed.sources()[0].generation(),
            restored.sources()[0].generation()
        );
        assert_eq!(
            changed.sources()[0].authoring(),
            restored.sources()[0].authoring()
        );
        assert_eq!(changed.entry_addon_id(), restored.entry_addon_id());
        assert_eq!(changed.entry(), restored.entry());
        assert_eq!(
            changed.v1_region(),
            if v3 {
                None
            } else {
                Some(TeletextSavedV1Region::WesternLatin)
            }
        );
        assert_eq!(changed.edit_policy(), TeletextSavedEditPolicy::OriginalOnly);
        assert_eq!(changed.editorial_overlay(), Some(&overlay()));
    }
}

#[test]
fn editorial_full_guide_source_requests_context_and_authoring_survive_unchanged() {
    let original = original_with_context();
    let before = decode_teletext_design_documents(&original, budget()).unwrap();
    let encoded =
        append_teletext_design_editorial_overlay(&original, &overlay(), budget()).unwrap();
    let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(after.selection(), before.selection());
    assert_eq!(after.guide_source(), before.guide_source());
    assert_eq!(
        after
            .guide_inputs()
            .unwrap()
            .request(TeletextPageContract::V3),
        before
            .guide_inputs()
            .unwrap()
            .request(TeletextPageContract::V3)
    );
    assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
    assert_eq!(
        after.sources()[0].authoring(),
        before.sources()[0].authoring()
    );
    assert_eq!(
        after.resource_transmissions(),
        before.resource_transmissions()
    );
    let parts = after.into_parts();
    assert_eq!(parts.editorial_overlay.as_ref(), Some(&overlay()));
    for debug in [
        format!("{:?}", parts.editorial_overlay),
        format!("{parts:?}"),
    ] {
        for secret in [
            GENERATION,
            "private-setting-sentinel",
            "guide-name-é-sentinel",
            "original-template-sentinel",
            "original-theme-sentinel",
            &"d".repeat(64),
            &"e".repeat(64),
        ] {
            assert!(!debug.contains(secret));
        }
    }
}

#[test]
fn editorial_subdocument_is_map_only_closed_nonnull_and_independently_versioned() {
    let baseline = concept_value();
    for replacement in [Value::Null, json!([]), json!(true), json!("claims")] {
        let mut value = baseline.clone();
        value["editorial_overlay"] = replacement;
        assert_eq!(
            decode(&value).unwrap_err().reason,
            TeletextDesignDocumentFailure::Json
        );
    }
    for version in [0, 2, 65535] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["schema_version"] = json!(version);
        assert_eq!(
            decode(&value).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
    }
    for path in ["root", "page", "owner", "row"] {
        let mut value = baseline.clone();
        let target = match path {
            "root" => &mut value["editorial_overlay"],
            "page" => &mut value["editorial_overlay"]["pages"][0],
            "owner" => &mut value["editorial_overlay"]["pages"][0]["owner"],
            _ => &mut value["editorial_overlay"]["pages"][0]["rows"][0],
        };
        target["edit_permission"] = json!(true);
        assert_eq!(
            decode(&value).unwrap_err().reason,
            TeletextDesignDocumentFailure::Json
        );
    }
}

#[test]
fn editorial_reference_ids_revision_and_lowercase_digests_are_structural_only() {
    let baseline = concept_value();
    for field in [
        "export_receipt_id",
        "original_design_id",
        "original_revision",
    ] {
        for invalid in [0, -1] {
            let mut value = baseline.clone();
            value["editorial_overlay"][field] = json!(invalid);
            assert_eq!(
                decode(&value).unwrap_err().reason,
                TeletextDesignDocumentFailure::EditorialOverlay
            );
        }
    }
    for field in ["export_receipt_digest", "original_digest"] {
        for invalid in ["D".repeat(64), "a".repeat(63), "g".repeat(64)] {
            let mut value = baseline.clone();
            value["editorial_overlay"][field] = json!(invalid);
            assert_eq!(
                decode(&value).unwrap_err().reason,
                TeletextDesignDocumentFailure::EditorialOverlay
            );
        }
    }
    let mut unverified = baseline;
    unverified["editorial_overlay"]["original_digest"] = json!("0".repeat(64));
    // A syntactically valid invented digest remains untrusted, not authenticated.
    assert!(decode(&unverified).is_ok());
}

#[test]
fn editorial_fixed_rows_reject_bad_cells_shape_header_and_row_positions() {
    let baseline = concept_value();
    for cell in [json!(128), json!(-1), json!("A")] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["pages"][0]["rows"][0]["cells"][39] = cell;
        assert!(decode(&value).is_err());
    }
    for length in [0, 39, 41] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["pages"][0]["rows"][0]["cells"] = json!(vec![32; length]);
        assert!(decode(&value).is_err());
    }
    for row in [25, 255] {
        let mut value = baseline.clone();
        value["editorial_overlay"]["pages"][0]["rows"][0]["row"] = json!(row);
        assert_eq!(
            decode(&value).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
    }
    let mut header = baseline.clone();
    header["editorial_overlay"]["pages"][0]["rows"][0]["row"] = json!(0);
    assert_eq!(
        decode(&header).unwrap_err().reason,
        TeletextDesignDocumentFailure::EditorialOverlay
    );
    let mut seven_bit = baseline;
    seven_bit["editorial_overlay"]["pages"][0]["rows"][0]["cells"][39] = json!(0x1c);
    assert!(decode(&seven_bit).is_ok()); // Native control semantics remain mandatory.
}

#[test]
fn editorial_exact_subcodes_unique_rows_and_original_ownership_are_required() {
    let baseline = concept_value();
    for value in [
        json!([]),
        json!(vec![baseline["editorial_overlay"]["pages"][0].clone(); 2]),
    ] {
        let mut changed = baseline.clone();
        changed["editorial_overlay"]["pages"] = value;
        assert_eq!(
            decode(&changed).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
    }
    let mut missing = baseline.clone();
    missing["editorial_overlay"]["pages"][0]["owner"]["subcode"] = json!(18);
    assert_eq!(
        decode(&missing).unwrap_err().reason,
        TeletextDesignDocumentFailure::EditorialOverlay
    );
    let mut rows = baseline.clone();
    let row = rows["editorial_overlay"]["pages"][0]["rows"][0].clone();
    rows["editorial_overlay"]["pages"][0]["rows"] = json!([row.clone(), row]);
    assert_eq!(
        decode(&rows).unwrap_err().reason,
        TeletextDesignDocumentFailure::EditorialOverlay
    );
    let mut ambiguous = baseline;
    let mut binding = ambiguous["selection"]["bindings"][0].clone();
    binding["addon_id"] = json!("org.zz.synthetic");
    binding["installation"]["addon_id"] = json!("org.zz.synthetic");
    binding["profile_id"] = json!(12);
    ambiguous["selection"]["bindings"]
        .as_array_mut()
        .unwrap()
        .push(binding);
    let document = ambiguous["documents"][0].clone();
    ambiguous["documents"]
        .as_array_mut()
        .unwrap()
        .push(document);
    assert_eq!(
        decode(&ambiguous).unwrap_err().reason,
        TeletextDesignDocumentFailure::EditorialOverlay
    );
}

#[test]
fn editorial_owner_row_cell_and_aggregate_limits_precede_owned_decode() {
    let baseline = concept_value();
    for (dimension, policy) in [
        ("editorial pages", {
            let mut b = budget();
            b.validation.service.max_records = 1;
            b
        }),
        ("editorial structure items", {
            let mut b = budget();
            b.validation.service.max_structure_items = 1;
            b
        }),
        ("editorial content bytes", {
            let mut b = budget();
            b.validation.service.max_base_content_bytes = 39;
            b
        }),
    ] {
        let error = decode_teletext_design_documents(&bytes(&baseline), policy).unwrap_err();
        assert!(
            matches!(error.reason,TeletextDesignDocumentFailure::Budget{dimension:actual,..} if actual==dimension)
        );
    }
    let mut too_many = baseline.clone();
    let row = too_many["editorial_overlay"]["pages"][0]["rows"][0].clone();
    too_many["editorial_overlay"]["pages"][0]["rows"] = json!(vec![row; 26]);
    assert!(matches!(
        decode(&too_many).unwrap_err().reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial rows",
            ..
        }
    ));
    let mut long_row = baseline;
    long_row["editorial_overlay"]["pages"][0]["rows"][0]["cells"] = json!(vec![32; 41]);
    assert!(matches!(
        decode(&long_row).unwrap_err().reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "editorial cells",
            ..
        }
    ));
}

#[test]
fn editorial_append_rejects_second_overlay_policy_change_and_serialized_overflow() {
    let original = bytes(&original_value(true));
    let concept =
        append_teletext_design_editorial_overlay(&original, &overlay(), budget()).unwrap();
    assert_eq!(
        append_teletext_design_editorial_overlay(concept.bytes(), &overlay(), budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::EditorialOverlay
    );
    let mut limited = budget();
    limited.max_serialized_bytes = original.len();
    assert!(matches!(
        append_teletext_design_editorial_overlay(&original, &overlay(), limited)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "serialized",
            ..
        }
    ));
    let mut unauthorized = original_value(true);
    unauthorized["edit_policy"]["kind"] = json!("editable");
    assert_eq!(
        append_teletext_design_editorial_overlay(&bytes(&unauthorized), &overlay(), budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::EditPolicy
    );
}

mod local_operands {
    use super::*;
    use ersatzrs_domain::TeletextColourReference;

    fn foreground(colour: u8) -> TeletextColumnOperation {
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(colour).unwrap(),
        }
    }

    fn attributes(invert: bool) -> TeletextColumnOperation {
        TeletextColumnOperation::Attributes {
            double_height: false,
            double_width: false,
            boxed: false,
            conceal: false,
            invert,
            underline_or_separated_mosaics: false,
        }
    }

    fn operands() -> Vec<TeletextLocalProgramOperandOverlay> {
        [
            (1, foreground(1), foreground(31)),
            (
                2,
                TeletextColumnOperation::Background {
                    colour: TeletextColourReference::new(0).unwrap(),
                },
                TeletextColumnOperation::Background {
                    colour: TeletextColourReference::new(16).unwrap(),
                },
            ),
            (3, attributes(false), attributes(true)),
        ]
        .into_iter()
        .map(|(command, expected, replacement)| {
            TeletextLocalProgramOperandOverlay::new(address(16), command, 3, expected, replacement)
                .unwrap()
        })
        .collect()
    }

    fn original() -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(&original_with_context()).unwrap();
        let mut response: Value =
            serde_json::from_str(value["documents"][0]["response"].as_str().unwrap()).unwrap();
        response["service"]["page_enhancements"] = json!([{
            "address":{"page":200,"subcode":16},"presentation":null,"compositional_links":null,
            "local_program":{"commands":[
                {"kind":"set_position","position":{"row":1,"column":0}},
                {"kind":"at_column","column":3,"operation":{"kind":"foreground","colour":1}},
                {"kind":"at_column","column":3,"operation":{"kind":"background","colour":0}},
                {"kind":"at_column","column":3,"operation":attributes(false)},
                {"kind":"at_column","column":3,"operation":{"kind":"g0","code":65}}
            ]},
            "resources":{"objects":{"public":null,"global":null},
                "drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}
        }]);
        value["documents"][0]["response"] = json!(format!(
            "{}\n \t",
            serde_json::to_string_pretty(&response).unwrap()
        ));
        bytes(&value)
    }

    fn overlay_with_rows(rows: bool) -> TeletextSavedEditorialOverlay {
        TeletextSavedEditorialOverlay::new_with_local_program_operands(
            provenance(),
            if rows {
                overlay().pages().to_vec()
            } else {
                vec![]
            },
            operands(),
        )
        .unwrap()
    }

    fn concept() -> Value {
        serde_json::from_slice(
            append_teletext_design_editorial_overlay(
                &original(),
                &overlay_with_rows(false),
                budget(),
            )
            .unwrap()
            .bytes(),
        )
        .unwrap()
    }

    #[test]
    fn v1_saved_output_remains_byte_identical_without_new_fields() {
        let original = bytes(&original_value(true));
        let encoded =
            append_teletext_design_editorial_overlay(&original, &overlay(), budget()).unwrap();
        let end = original
            .iter()
            .rposition(|byte| !byte.is_ascii_whitespace())
            .unwrap();
        let cells = |letter| {
            let mut cells = vec![32; 40];
            cells[0] = 1;
            cells[1] = letter;
            serde_json::to_string(&cells).unwrap()
        };
        let expected_overlay = format!(
            concat!(
                "{{\"schema_version\":1,\"export_receipt_id\":3,\"export_receipt_digest\":\"{}\",",
                "\"original_design_id\":7,\"original_revision\":2,\"original_digest\":\"{}\",",
                "\"pages\":[{{\"owner\":{{\"page\":200,\"subcode\":16}},\"rows\":[{{\"row\":1,\"cells\":{}}}]}},",
                "{{\"owner\":{{\"page\":200,\"subcode\":17}},\"rows\":[{{\"row\":1,\"cells\":{}}}]}}]}}"
            ),
            "d".repeat(64), "e".repeat(64), cells(u32::from(b'A')), cells(u32::from(b'B'))
        );
        let mut expected = original[..end].to_vec();
        expected.extend_from_slice(b",\"editorial_overlay\":");
        expected.extend_from_slice(expected_overlay.as_bytes());
        expected.extend_from_slice(&original[end..]);
        assert_eq!(encoded.bytes(), expected);
        let restored = decode_teletext_design_documents(&expected, budget()).unwrap();
        assert_eq!(restored.editorial_overlay().unwrap().schema_version(), 1);
        assert!(restored
            .editorial_overlay()
            .unwrap()
            .local_program_operands()
            .is_empty());
    }

    #[test]
    fn v2_operand_only_and_mixed_concepts_preserve_exact_original_context() {
        let original = original();
        let before = decode_teletext_design_documents(&original, budget()).unwrap();
        for rows in [false, true] {
            let overlay = overlay_with_rows(rows);
            let encoded =
                append_teletext_design_editorial_overlay(&original, &overlay, budget()).unwrap();
            let after = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
            let saved = after.editorial_overlay().unwrap();
            assert_eq!(saved.schema_version(), 2);
            assert_eq!(saved, &overlay);
            assert_eq!(saved.local_program_operands(), operands());
            assert_eq!(after.selection(), before.selection());
            assert_eq!(after.sources()[0].bytes(), before.sources()[0].bytes());
            assert_eq!(
                after.sources()[0].authoring(),
                before.sources()[0].authoring()
            );
            assert_eq!(after.guide_source(), before.guide_source());
            assert_eq!(
                after
                    .guide_inputs()
                    .unwrap()
                    .request(TeletextPageContract::V3),
                before
                    .guide_inputs()
                    .unwrap()
                    .request(TeletextPageContract::V3)
            );
            assert_eq!(
                after.resource_transmissions(),
                before.resource_transmissions()
            );
            assert_eq!(after.edit_policy(), TeletextSavedEditPolicy::OriginalOnly);
            assert_eq!(after.documents().len(), before.documents().len());
            let (
                TeletextProfileDocument::Authored(after_document),
                TeletextProfileDocument::Authored(before_document),
            ) = (&after.documents()[0], &before.documents()[0])
            else {
                panic!("authored original");
            };
            assert_eq!(after_document.semantic(), before_document.semantic());
            let end = original
                .iter()
                .rposition(|byte| !byte.is_ascii_whitespace())
                .unwrap();
            assert_eq!(&encoded.bytes()[..end], &original[..end]);
            assert!(encoded.bytes().ends_with(&original[end..]));
            let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
            assert_eq!(
                value["editorial_overlay"]["local_program_operands"]
                    .as_array()
                    .unwrap()
                    .len(),
                3
            );
            assert_eq!(saved.pages().is_empty(), !rows);
        }
    }

    #[test]
    fn operand_version_boundary_requires_explicit_nonempty_v2_and_omitted_v1_field() {
        let baseline = concept();
        for field in [Value::Null, json!([])] {
            let mut changed = baseline.clone();
            changed["editorial_overlay"]["local_program_operands"] = field;
            assert!(decode(&changed).is_err());
        }
        let mut absent = baseline.clone();
        absent["editorial_overlay"]
            .as_object_mut()
            .unwrap()
            .remove("local_program_operands");
        assert_eq!(
            decode(&absent).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
        let mut downgrade = baseline.clone();
        downgrade["editorial_overlay"]["schema_version"] = json!(1);
        assert_eq!(
            decode(&downgrade).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
        let mut contaminated = concept_value();
        contaminated["editorial_overlay"]["local_program_operands"] = json!([]);
        assert_eq!(
            decode(&contaminated).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
        assert!(
            TeletextSavedEditorialOverlay::new_with_local_program_operands(
                provenance(),
                vec![],
                vec![]
            )
            .is_err()
        );
    }

    #[test]
    fn operand_dtos_are_closed_map_only_and_reject_duplicate_keys() {
        let baseline = concept();
        for field in ["operand", "owner", "expected", "replacement"] {
            for invalid in [Value::Null, json!([]), json!("operand")] {
                let mut changed = baseline.clone();
                let operand = &mut changed["editorial_overlay"]["local_program_operands"][0];
                let target = if field == "operand" {
                    operand
                } else {
                    &mut operand[field]
                };
                *target = invalid;
                assert_eq!(
                    decode(&changed).unwrap_err().reason,
                    TeletextDesignDocumentFailure::Json,
                    "{field}"
                );
            }
            let mut changed = baseline.clone();
            let operand = &mut changed["editorial_overlay"]["local_program_operands"][0];
            let target = if field == "operand" {
                operand
            } else {
                &mut operand[field]
            };
            target["permission"] = json!(true);
            assert_eq!(
                decode(&changed).unwrap_err().reason,
                TeletextDesignDocumentFailure::Json,
                "{field}"
            );
        }
        let serialized = serde_json::to_string(&baseline).unwrap();
        let duplicate = serialized.replacen("\"command\":1", "\"command\":1,\"command\":2", 1);
        assert_ne!(serialized, duplicate);
        assert_eq!(
            decode_teletext_design_documents(duplicate.as_bytes(), budget())
                .unwrap_err()
                .reason,
            TeletextDesignDocumentFailure::Json
        );
    }

    #[test]
    fn wrong_family_noop_unsupported_and_geometry_operands_cannot_restore() {
        let baseline = concept();
        for replacement in [
            json!({"kind":"foreground","colour":1}),
            json!({"kind":"background","colour":2}),
            json!({"kind":"g0","code":65}),
            json!({"kind":"designate_charset","designation":0}),
        ] {
            let mut changed = baseline.clone();
            changed["editorial_overlay"]["local_program_operands"][0]["replacement"] = replacement;
            assert_eq!(
                decode(&changed).unwrap_err().reason,
                TeletextDesignDocumentFailure::EditorialOverlay
            );
        }
        let mut glyph = baseline.clone();
        glyph["editorial_overlay"]["local_program_operands"][0]["command"] = json!(4);
        glyph["editorial_overlay"]["local_program_operands"][0]["expected"] =
            json!({"kind":"g0","code":65});
        glyph["editorial_overlay"]["local_program_operands"][0]["replacement"] =
            json!({"kind":"g0","code":66});
        assert_eq!(
            decode(&glyph).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
        for field in ["double_height", "double_width"] {
            let mut changed = baseline.clone();
            changed["editorial_overlay"]["local_program_operands"][2]["replacement"][field] =
                json!(true);
            assert_eq!(
                decode(&changed).unwrap_err().reason,
                TeletextDesignDocumentFailure::EditorialOverlay
            );
        }
        let mut invalid_colour = baseline;
        invalid_colour["editorial_overlay"]["local_program_operands"][0]["replacement"]["colour"] =
            json!(32);
        assert!(decode(&invalid_colour).is_err());
    }

    #[test]
    fn exact_unique_authored_original_and_original_command_match_are_required() {
        let baseline = concept();
        for (field, invalid) in [
            ("command", json!(0)),
            ("command", json!(999)),
            ("column", json!(4)),
            ("column", json!(40)),
        ] {
            let mut changed = baseline.clone();
            changed["editorial_overlay"]["local_program_operands"][0][field] = invalid;
            assert_eq!(
                decode(&changed).unwrap_err().reason,
                TeletextDesignDocumentFailure::EditorialOverlay
            );
        }
        for subcode in [17, 18] {
            let mut changed = baseline.clone();
            changed["editorial_overlay"]["local_program_operands"][0]["owner"]["subcode"] =
                json!(subcode);
            assert_eq!(
                decode(&changed).unwrap_err().reason,
                TeletextDesignDocumentFailure::EditorialOverlay
            );
        }
        let mut stale = baseline.clone();
        stale["editorial_overlay"]["local_program_operands"][0]["expected"]["colour"] = json!(2);
        assert_eq!(
            decode(&stale).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
        let mut duplicate = baseline.clone();
        let operand = duplicate["editorial_overlay"]["local_program_operands"][0].clone();
        duplicate["editorial_overlay"]["local_program_operands"]
            .as_array_mut()
            .unwrap()
            .push(operand);
        assert_eq!(
            decode(&duplicate).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
        let mut ambiguous = baseline;
        let mut binding = ambiguous["selection"]["bindings"][0].clone();
        binding["addon_id"] = json!("org.zz.synthetic");
        binding["installation"]["addon_id"] = json!("org.zz.synthetic");
        binding["profile_id"] = json!(12);
        ambiguous["selection"]["bindings"]
            .as_array_mut()
            .unwrap()
            .push(binding);
        let document = ambiguous["documents"][0].clone();
        ambiguous["documents"]
            .as_array_mut()
            .unwrap()
            .push(document);
        assert_eq!(
            decode(&ambiguous).unwrap_err().reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
        let overlay = overlay_with_rows(false);
        assert_eq!(
            append_teletext_design_editorial_overlay(
                &bytes(&original_value(false)),
                &overlay,
                budget()
            )
            .unwrap_err()
            .reason,
            TeletextDesignDocumentFailure::EditorialOverlay
        );
    }

    #[test]
    fn operand_count_shared_structure_and_serialized_limits_remain_bounded() {
        let baseline = concept();
        let mut repeated = baseline.clone();
        repeated["editorial_overlay"]["local_program_operands"] = json!(vec![
                baseline["editorial_overlay"]["local_program_operands"]
                    [0]
                .clone();
                6
            ]);
        let mut command_limit = budget();
        command_limit.validation.service.max_commands = 5;
        let error = decode_teletext_design_documents(&bytes(&repeated), command_limit).unwrap_err();
        assert!(matches!(
            error.reason,
            TeletextDesignDocumentFailure::Budget {
                dimension: "editorial operands",
                ..
            }
        ));
        let original = original();
        let mixed = overlay_with_rows(true);
        let mut structure_limit = budget();
        structure_limit.validation.service.max_structure_items = 6;
        let error = append_teletext_design_editorial_overlay(&original, &mixed, structure_limit)
            .unwrap_err();
        assert!(matches!(
            error.reason,
            TeletextDesignDocumentFailure::Budget {
                dimension: "editorial structure items",
                ..
            }
        ));
        let encoded = append_teletext_design_editorial_overlay(
            &original,
            &overlay_with_rows(false),
            budget(),
        )
        .unwrap();
        let mut serialized_limit = budget();
        serialized_limit.max_serialized_bytes = encoded.bytes().len() - 1;
        assert!(matches!(
            append_teletext_design_editorial_overlay(
                &original,
                &overlay_with_rows(false),
                serialized_limit
            )
            .unwrap_err()
            .reason,
            TeletextDesignDocumentFailure::Budget {
                dimension: "serialized",
                ..
            }
        ));
        let mut no_row_content = budget();
        no_row_content.validation.service.max_base_content_bytes = 0;
        assert!(append_teletext_design_editorial_overlay(
            &original,
            &overlay_with_rows(false),
            no_row_content
        )
        .is_ok());
        assert!(
            TeletextSavedEditorialOverlay::new_with_local_program_operands(
                provenance(),
                vec![],
                vec![operands()[0]; 2]
            )
            .is_err()
        );
    }
}

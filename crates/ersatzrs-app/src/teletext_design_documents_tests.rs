//! Private original storage tests; no persistence, editing or publication proof.
#[path = "teletext_design_documents/guide_inputs_tests.rs"]
mod guide_inputs_tests;
#[path = "teletext_design_documents/secondary_defaults_tests.rs"]
mod secondary_defaults_tests;
#[path = "teletext_design_documents/source_fence_tests.rs"]
mod source_fence_tests;
use super::*;
use crate::{
    teletext_profile_documents::BoundTeletextDocumentInput,
    teletext_profiles::TeletextProfileRuntimeReceipt,
};
use ersatzrs_domain::{
    TeletextDeclaredServiceBudget, TeletextRawDataPageNumber, TeletextResourceKind,
    TeletextResourceSubpageIndex, TeletextServiceBudget,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const GENERATION: &str = "private-correlation-sentinel";
const A: &str = "org.example.day";
const B: &str = "org.example.night";

fn budget() -> TeletextDesignDocumentBudget {
    TeletextDesignDocumentBudget {
        max_serialized_bytes: 128 * 1024,
        max_documents: 8,
        max_response_bytes: 64 * 1024,
        max_context_bytes: 16 * 1024,
        max_context_field_bytes: 16 * 1024,
        max_resource_transmissions: 8,
        validation: TeletextProfileDocumentBudget {
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
fn binding(addon: &str, contract: TeletextPageContract) -> TeletextSelectionSnapshot {
    TeletextSelectionSnapshot {
        contract,
        channel_id: 9,
        addon_id: addon.to_owned(),
        binding_revision: 3,
        profile_id: if addon == A { 11 } else { 12 },
        profile_revision: 4,
        installation: TeletextInstallationSnapshot {
            addon_id: addon.to_owned(),
            state_revision: "state-revision-private".to_owned(),
            version: "1.2.3".to_owned(),
            package_sha256: "a".repeat(64),
            content_sha256: "B".repeat(64),
            manifest_sha256: "c".repeat(64),
        },
        effective_settings: vec![
            TeletextProfileSetting {
                key: "THEME".to_owned(),
                value: TeletextProfileValue::Literal("private-setting-sentinel".to_owned()),
            },
            TeletextProfileSetting {
                key: "ENV".to_owned(),
                value: TeletextProfileValue::SecretReference {
                    kind: TeletextSecretReferenceKind::Environment,
                    reference: "UNRESOLVED_ENV_SENTINEL".to_owned(),
                },
            },
            TeletextProfileSetting {
                key: "FILE".to_owned(),
                value: TeletextProfileValue::SecretReference {
                    kind: TeletextSecretReferenceKind::File,
                    reference: "synthetic/unresolved-reference-sentinel".to_owned(),
                },
            },
        ],
    }
}
fn page(number: u16, subcode: u16) -> Value {
    json!({"address":{"page":number,"subcode":subcode},"kind":"full_page","language":"nld",
        "national_option":5,"rows":[{"row":1,"runs":[{"column":8,
        "style":{"foreground":"white","background":"black","height":"normal","boxed":false,"flash":false,"conceal":false},
        "content":{"kind":"text","text":"Café original-response-sentinel"}}]}],"navigation":[]})
}
fn base() -> Value {
    json!({"schema_version":1,"generation":GENERATION,"language":"nld","initial_page":100,"pages":[page(100,16)]})
}
fn presentation() -> Value {
    json!({"primary":1,"secondary":8,"palette":{"clut2":vec![[1,2,3];8],"clut3":vec![[4,5,6];8]},
        "screen_colour":16,"row_colour":23,"black_background_substitution":true,"remapping":"fg_clut2_bg_clut3",
        "panels":{"left":true,"right":true,"status":"level25_and35","width":4}})
}
fn authored() -> Value {
    let enhancement = |subcode, local| {
        json!({"address":{"page":200,"subcode":subcode},"presentation":null,
        "compositional_links":null,"local_program":local,
        "resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}})
    };
    json!({"schema_version":2,"generation":GENERATION,"language":"nld","initial_page":200,
        "pages":[page(200,16),page(200,17)],
        "page_enhancements":[enhancement(16,Value::Null),enhancement(17,json!({"commands":[]}))],
        "magazines":[{"magazine":2,"presentation":presentation()}],
        "objects":[],"drcs":[],"mots":[],
        "mot_selections":[{"display":{"page":200,"subcode":16},"mot":null},{"display":{"page":200,"subcode":17},"mot":null}],
        "empty_object_pages":[]})
}
fn authored_metadata() -> Value {
    let service = authored();
    let pages = service["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|page| {
            json!({
                "page":page["address"],
                "regions":[{"id":"guide-output","kind":"generated","area":"main",
                    "row":1,"column":0,"width":40,"height":1},
                    {"id":"private-custom-zone","kind":"zone","area":"left_panel",
                    "row":3,"column":0,"width":4,"height":2}],
                "dynamic_fields":[{"id":"programme-title","region":"guide-output",
                    "binding":"guide.programme.title"}]
            })
        })
        .collect::<Vec<_>>();
    json!({"schema_version":3,"service":service,"authoring":{
        "schema_version":1,"template":{"id":"private-guide-template","revision":"1"},
        "theme":{"id":"private-night-theme","overrides":[{"key":"accent",
            "value":{"kind":"colour","red":1,"green":2,"blue":3}}]},
        "pages":pages}})
}
fn exact_bytes(value: &Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.extend_from_slice(b"\n \t");
    bytes
}
fn source(selected: &TeletextSelectionSnapshot, bytes: Vec<u8>) -> TeletextValidatedDocumentSource {
    BoundTeletextDocumentInput::from_channel_receipt(
        TeletextProfileRuntimeReceipt {
            stdout: bytes,
            selection: selected.clone(),
            generation: GENERATION.to_owned(),
        },
        Instant::now().checked_add(Duration::from_secs(30)).unwrap(),
    )
    .validate(budget().validation)
    .unwrap()
    .into_parts_with_source()
    .3
}
struct Fixture {
    selection: TeletextChannelSelectionSnapshot,
    sources: Vec<TeletextValidatedDocumentSource>,
    lifecycle: Vec<TeletextSavedResourceTransmission>,
}
impl Fixture {
    fn mixed() -> Self {
        let bindings = vec![
            binding(A, TeletextPageContract::V1),
            binding(B, TeletextPageContract::V2),
        ];
        let sources = vec![
            source(&bindings[0], exact_bytes(&base())),
            source(&bindings[1], exact_bytes(&authored())),
        ];
        Self {
            selection: TeletextChannelSelectionSnapshot {
                channel_id: 9,
                bindings,
            },
            sources,
            lifecycle: Vec::new(),
        }
    }
    fn input(&self) -> TeletextDesignDocumentInput<'_> {
        TeletextDesignDocumentInput {
            selection: &self.selection,
            generation: GENERATION,
            sources: &self.sources,
            entry_addon_id: Some(A),
            entry: Some(TeletextPageAddress {
                page: TeletextPageNumber::new(100).unwrap(),
                subcode: TeletextSubcode::new(16).unwrap(),
            }),
            v1_region: Some(TeletextSavedV1Region::WesternLatin),
            resource_transmissions: &self.lifecycle,
        }
    }
    fn with_authoring() -> Self {
        let mut fixture = Self::mixed();
        fixture.selection.bindings[1].contract = TeletextPageContract::V3;
        fixture.sources[1] = source(
            &fixture.selection.bindings[1],
            exact_bytes(&authored_metadata()),
        );
        fixture
    }
    fn encoded(&self) -> EncodedTeletextDesignDocuments {
        encode_teletext_design_documents(self.input(), budget()).unwrap()
    }
    fn value(&self) -> Value {
        serde_json::from_slice(self.encoded().bytes()).unwrap()
    }
}

#[test]
fn saved_original_bytes_retain_noncanonical_spelling_and_decoder_limits() {
    let value = Fixture::with_authoring().value();
    let mut bytes = serde_json::to_vec_pretty(&value).unwrap();
    bytes.extend_from_slice(b"\n\n");
    let restored =
        EncodedTeletextDesignDocuments::from_saved_bytes(bytes.clone(), budget()).unwrap();
    assert_eq!(restored.bytes(), bytes);
    let tight = TeletextDesignDocumentBudget {
        max_serialized_bytes: bytes.len() - 1,
        ..budget()
    };
    assert!(EncodedTeletextDesignDocuments::from_saved_bytes(bytes.clone(), tight).is_err());
    bytes.extend_from_slice(b"{}");
    assert!(EncodedTeletextDesignDocuments::from_saved_bytes(bytes, budget()).is_err());
}

#[test]
fn original_v3_storage_preserves_exact_sources_metadata_and_no_edit_authority() {
    let fixture = Fixture::with_authoring();
    let encoded = fixture.encoded();
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(
        restored.edit_policy(),
        TeletextSavedEditPolicy::OriginalOnly
    );
    assert_eq!(restored.selection(), &fixture.selection);
    for (actual, original) in restored.sources().iter().zip(&fixture.sources) {
        assert_eq!(actual.bytes(), original.bytes());
        assert_eq!(actual.authoring(), original.authoring());
        assert_eq!(actual.contract(), original.contract());
    }
    assert!(restored.sources()[0].authoring().is_none());
    let metadata = restored.sources()[1].authoring().unwrap();
    assert_eq!(metadata.pages.len(), 2);
    assert_eq!(metadata.theme.id, "private-night-theme");
    assert_eq!(
        metadata.pages[0].dynamic_fields[0].binding,
        "guide.programme.title"
    );
    let TeletextProfileDocument::Authored(service) = &restored.documents()[1] else {
        panic!("saved V3 service downgraded")
    };
    assert_eq!(service.semantic().base().pages.len(), 2);
    for debug in [
        format!("{restored:?}"),
        format!("{:?}", restored.sources()[1]),
        format!("{metadata:?}"),
    ] {
        for sentinel in [
            "private-night-theme",
            "private-custom-zone",
            "private-guide-template",
            "original-response-sentinel",
            GENERATION,
        ] {
            assert!(!debug.contains(sentinel));
        }
    }
    let mut attempted_permission = fixture.value();
    attempted_permission["edit_policy"]["kind"] = json!("authoring_regions");
    assert_eq!(
        decode(&attempted_permission).unwrap_err().reason,
        TeletextDesignDocumentFailure::EditPolicy
    );
}

#[test]
fn saved_v3_metadata_is_revalidated_against_its_original_service() {
    let fixture = Fixture::with_authoring();
    let baseline = fixture.value();
    for invalid in 0..5 {
        let mut changed = baseline.clone();
        let mut response: Value =
            serde_json::from_str(changed["documents"][1]["response"].as_str().unwrap()).unwrap();
        match invalid {
            0 => response["schema_version"] = json!(2),
            1 => response["service"]["generation"] = json!("foreign-correlation"),
            2 => response["authoring"]["pages"][0]["page"]["subcode"] = json!(18),
            3 => response["authoring"]["pages"][0]["regions"][1]["width"] = json!(5),
            _ => {
                response.as_object_mut().unwrap().remove("authoring");
            }
        }
        changed["documents"][1]["response"] = json!(serde_json::to_string(&response).unwrap());
        let error = decode(&changed).unwrap_err();
        assert_eq!(error.binding, Some(1));
        assert_eq!(error.reason, TeletextDesignDocumentFailure::Document);
    }
}
fn decode(value: &Value) -> Result<RestoredTeletextDesignDocuments, TeletextDesignDocumentError> {
    decode_teletext_design_documents(&serde_json::to_vec(value).unwrap(), budget())
}
fn reject(value: &Value) {
    assert!(decode(value).is_err());
}

#[test]
fn mixed_originals_round_trip_exact_bytes_aliases_presence_and_private_context() {
    let fixture = Fixture::mixed();
    let before = fixture
        .sources
        .iter()
        .map(|v| v.bytes().to_vec())
        .collect::<Vec<_>>();
    let encoded = fixture.encoded();
    assert_eq!(encoded.version(), TELETEXT_DESIGN_DOCUMENT_VERSION);
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.selection(), &fixture.selection);
    assert_eq!(restored.generation(), GENERATION);
    assert_eq!(restored.entry_addon_id(), Some(A));
    assert_eq!(restored.entry().unwrap().subcode.get(), 16);
    assert_eq!(
        restored.v1_region(),
        Some(TeletextSavedV1Region::WesternLatin)
    );
    assert_eq!(
        restored.edit_policy(),
        TeletextSavedEditPolicy::OriginalOnly
    );
    for (index, saved) in restored.sources().iter().enumerate() {
        assert_eq!(saved.bytes(), before[index]);
        assert_eq!(saved.generation(), GENERATION);
        assert_eq!(saved.contract(), fixture.selection.bindings[index].contract);
        assert_eq!(fixture.sources[index].bytes(), before[index]);
    }
    let TeletextProfileDocument::Base(original) = &restored.documents()[0] else {
        panic!("V1 lost")
    };
    assert_eq!(original.pages[0].address.subcode.get(), 16);
    let TeletextProfileDocument::Authored(original) = &restored.documents()[1] else {
        panic!("V2 lost")
    };
    assert_eq!(
        original.semantic().base().pages[1].address.subcode.get(),
        17
    );
    let pages = original
        .semantic()
        .page_enhancements()
        .values()
        .collect::<Vec<_>>();
    assert!(pages[0].local_program.is_none());
    assert!(pages[1]
        .local_program
        .as_ref()
        .unwrap()
        .commands()
        .is_empty());
    assert_eq!(
        original.semantic().magazine_presentations()[&2]
            .primary
            .get(),
        1
    );
    assert_eq!(
        original.semantic().magazine_presentations()[&2]
            .secondary
            .unwrap()
            .get(),
        8
    );
    assert_eq!(
        original.semantic().magazine_presentations()[&2]
            .palette
            .clut2[0]
            .get(),
        [1, 2, 3]
    );
    let parts = restored.into_parts();
    assert_eq!(parts.sources.len(), 2);
    assert_eq!(parts.documents.len(), 2);
    assert_eq!(encoded.into_bytes(), fixture.encoded().into_bytes());
}

#[test]
fn required_closed_maps_and_original_only_policy_cannot_be_omitted_or_broadened() {
    let original = Fixture::mixed().value();
    for key in [
        "format_version",
        "selection",
        "generation",
        "entry_addon_id",
        "entry",
        "v1_region",
        "edit_policy",
        "resource_transmissions",
        "documents",
    ] {
        let mut value = original.clone();
        value.as_object_mut().unwrap().remove(key);
        reject(&value);
    }
    for pointer in [
        "",
        "/selection",
        "/selection/bindings/0",
        "/selection/bindings/0/installation",
        "/selection/bindings/0/effective_settings/0",
        "/selection/bindings/0/effective_settings/0/value",
        "/entry",
        "/edit_policy",
        "/documents/0",
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = json!([]);
        reject(&value);
        let mut value = original.clone();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("private-unknown-sentinel".to_owned(), json!(true));
        reject(&value);
    }
    for (field, bad) in [
        ("policy_version", json!(0)),
        ("policy_version", json!(2)),
        ("kind", json!("free")),
        ("kind", json!("editable_zones")),
        ("kind", json!({"original_only":null})),
    ] {
        let mut value = original.clone();
        value["edit_policy"][field] = bad;
        reject(&value);
    }
    let mut value = original.clone();
    value["format_version"] = json!(2);
    assert_eq!(
        decode(&value).unwrap_err().reason,
        TeletextDesignDocumentFailure::Version
    );
    let mut value = original.clone();
    value["selection"]["bindings"][0]["effective_settings"][0]["value"]["kind"] =
        json!("resolved_secret");
    reject(&value);
    let mut value = original;
    value["v1_region"] = json!({"western_latin":null});
    reject(&value);
}

#[test]
fn duplicate_fields_trailing_corruption_and_depth_fail_without_partial_restore() {
    let bytes = Fixture::mixed().encoded().into_bytes();
    let text = String::from_utf8(bytes.clone()).unwrap();
    for malformed in [
        text.replacen(
            "\"format_version\":1",
            "\"format_version\":1,\"format_version\":1",
            1,
        ),
        text.replacen("\"channel_id\":9", "\"channel_id\":9,\"channel_id\":9", 1),
        format!("{text} null"),
        text[..text.len() - 1].to_owned(),
    ] {
        assert!(decode_teletext_design_documents(malformed.as_bytes(), budget()).is_err());
    }
    let deep = format!(
        "{{\"unknown\":{}0{}}}",
        "[".repeat(MAX_DEPTH + 1),
        "]".repeat(MAX_DEPTH + 1)
    );
    assert_eq!(
        decode_teletext_design_documents(deep.as_bytes(), budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Depth
    );
    let mut corrupt = bytes;
    corrupt[0] = b'!';
    assert_eq!(
        decode_teletext_design_documents(&corrupt, budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Json
    );
}

#[test]
fn historical_association_is_exact_and_not_a_manifest_lookup_or_fallback() {
    let original = Fixture::mixed().value();
    for (pointer, bad) in [
        ("/selection/channel_id", json!(0)),
        ("/selection/bindings/1/channel_id", json!(10)),
        ("/selection/bindings/0/profile_revision", json!(0)),
        ("/selection/bindings/0/installation/addon_id", json!(B)),
        (
            "/selection/bindings/0/installation/package_sha256",
            json!("not-a-hash"),
        ),
        ("/documents/0/contract", json!(2)),
        ("/documents/0/generation", json!("another-correlation")),
        ("/entry/subcode", json!(0)),
        ("/entry_addon_id", json!("org.example.missing")),
        ("/v1_region", Value::Null),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = bad;
        reject(&value);
    }
    let mut value = original.clone();
    value["selection"]["bindings"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reject(&value);
    let mut value = original.clone();
    value["documents"].as_array_mut().unwrap().pop();
    reject(&value);
    let mut value = original.clone();
    let repeated = value["selection"]["bindings"][0]["effective_settings"][0].clone();
    value["selection"]["bindings"][0]["effective_settings"]
        .as_array_mut()
        .unwrap()
        .push(repeated);
    reject(&value);
    let mut value = original;
    value["documents"][1]["response"] = json!("{\"schema_version\":1}");
    let error = decode(&value).unwrap_err();
    assert_eq!(error.binding, Some(1));
    assert_eq!(error.reason, TeletextDesignDocumentFailure::Document);
}

#[test]
fn output_and_decode_byte_count_storage_and_native_model_budgets_are_independent() {
    let fixture = Fixture::mixed();
    let encoded = fixture.encoded();
    let response_bytes = fixture
        .sources
        .iter()
        .map(|v| v.bytes().len())
        .sum::<usize>();
    let mut exact = budget();
    exact.max_serialized_bytes = encoded.bytes().len();
    exact.max_response_bytes = response_bytes;
    exact.max_documents = 2;
    assert!(encode_teletext_design_documents(fixture.input(), exact).is_ok());
    assert!(decode_teletext_design_documents(encoded.bytes(), exact).is_ok());
    for dimension in 0..4 {
        let mut small = exact;
        match dimension {
            0 => small.max_serialized_bytes -= 1,
            1 => small.max_response_bytes -= 1,
            2 => small.max_documents -= 1,
            _ => small.max_context_field_bytes = 63,
        }
        assert!(encode_teletext_design_documents(fixture.input(), small).is_err());
        assert!(decode_teletext_design_documents(encoded.bytes(), small).is_err());
    }
    let mut no_context = budget();
    no_context.max_context_bytes = 0;
    assert!(encode_teletext_design_documents(fixture.input(), no_context).is_err());
    assert!(decode_teletext_design_documents(encoded.bytes(), no_context).is_err());
    let mut arithmetic = budget();
    arithmetic.max_resource_transmissions = usize::MAX;
    assert_eq!(
        decode_teletext_design_documents(encoded.bytes(), arithmetic)
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Storage
    );
    let mut models = budget();
    models.validation.service.max_records = 0;
    let error = decode_teletext_design_documents(encoded.bytes(), models).unwrap_err();
    assert_eq!(error.binding, Some(0));
    assert_eq!(error.reason, TeletextDesignDocumentFailure::Document);
    assert_eq!(fixture.encoded().bytes(), encoded.bytes());
}

#[test]
fn original_only_capture_privacy_and_utf8_boundaries_do_not_resolve_references() {
    let fixture = Fixture::mixed();
    let encoded = fixture.encoded();
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    let mut malicious = fixture.value();
    malicious["secret-unknown-key-sentinel"] = json!("secret-unknown-value-sentinel");
    let error = decode(&malicious).unwrap_err();
    for output in [
        format!("{:?}", fixture.input()),
        format!("{encoded:?}"),
        format!("{restored:?}"),
        format!("{:?}", restored.sources()),
        format!("{:?}", restored.into_parts()),
        format!("{error:?}"),
        error.to_string(),
    ] {
        for secret in [
            GENERATION,
            "private-setting-sentinel",
            "UNRESOLVED_ENV_SENTINEL",
            "unresolved-reference-sentinel",
            "original-response-sentinel",
            "secret-unknown-key-sentinel",
            "secret-unknown-value-sentinel",
        ] {
            assert!(!output.contains(secret), "private debug content leaked");
        }
    }
    assert!(String::from_utf8(encoded.into_bytes())
        .unwrap()
        .contains("UNRESOLVED_ENV_SENTINEL"));
    let off = TeletextChannelSelectionSnapshot {
        channel_id: 9,
        bindings: Vec::new(),
    };
    let utf8_at_limit = "é".repeat(512);
    let utf8_above_limit = "é".repeat(513);
    let off_input = |generation| TeletextDesignDocumentInput {
        selection: &off,
        generation,
        sources: &[],
        entry_addon_id: None,
        entry: None,
        v1_region: None,
        resource_transmissions: &[],
    };
    assert!(encode_teletext_design_documents(off_input(&utf8_at_limit), budget()).is_ok());
    assert!(encode_teletext_design_documents(off_input(&utf8_above_limit), budget()).is_err());
    assert!(encode_teletext_design_documents(off_input(""), budget()).is_err());
}

#[test]
fn off_and_v2_only_presence_are_explicit_while_sources_remain_readable_without_packages() {
    let off = TeletextChannelSelectionSnapshot {
        channel_id: 9,
        bindings: Vec::new(),
    };
    let input = TeletextDesignDocumentInput {
        selection: &off,
        generation: GENERATION,
        sources: &[],
        entry_addon_id: None,
        entry: None,
        v1_region: None,
        resource_transmissions: &[],
    };
    let saved = encode_teletext_design_documents(input, budget()).unwrap();
    let restored = decode_teletext_design_documents(saved.bytes(), budget()).unwrap();
    assert!(restored.sources().is_empty());
    assert!(restored.documents().is_empty());
    assert!(restored.selection().bindings.is_empty());
    let binding = binding(B, TeletextPageContract::V2);
    let sources = vec![source(&binding, exact_bytes(&authored()))];
    let selected = TeletextChannelSelectionSnapshot {
        channel_id: 9,
        bindings: vec![binding],
    };
    let input = TeletextDesignDocumentInput {
        selection: &selected,
        generation: GENERATION,
        sources: &sources,
        entry_addon_id: Some(B),
        entry: Some(TeletextPageAddress {
            page: TeletextPageNumber::new(200).unwrap(),
            subcode: TeletextSubcode::new(17).unwrap(),
        }),
        v1_region: None,
        resource_transmissions: &[],
    };
    let saved = encode_teletext_design_documents(input, budget()).unwrap();
    let mut bad: Value = serde_json::from_slice(saved.bytes()).unwrap();
    bad["v1_region"] = json!("western_latin");
    reject(&bad);
    drop(sources);
    drop(selected);
    let restored = decode_teletext_design_documents(saved.bytes(), budget()).unwrap();
    assert_eq!(restored.sources()[0].contract(), TeletextPageContract::V2);
    assert_eq!(restored.entry().unwrap().subcode.get(), 17);
}

#[test]
fn explicit_lifecycle_round_trips_and_rejects_tags_scalars_duplicates_and_count_before_body() {
    let mut fixture = Fixture::mixed();
    fixture.lifecycle = vec![TeletextSavedResourceTransmission {
        resource: TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(15).unwrap(),
        )
        .unwrap(),
        counter: TeletextResourceUpdateCounter::new(7).unwrap(),
        update: TeletextSavedResourceUpdate::Changed,
        last_subpage: true,
    }];
    let saved = fixture.encoded();
    let restored = decode_teletext_design_documents(saved.bytes(), budget()).unwrap();
    assert_eq!(restored.resource_transmissions(), fixture.lifecycle);
    for (pointer, bad) in [
        ("/resource_transmissions/0/update", json!("partial")),
        ("/resource_transmissions/0/update", json!({"changed":null})),
        ("/resource_transmissions/0/counter", json!(8)),
        (
            "/resource_transmissions/0/resource/logical_subpage",
            json!(16),
        ),
        ("/resource_transmissions/0/resource", json!([])),
        ("/resource_transmissions/0", json!([])),
    ] {
        let mut value = fixture.value();
        *value.pointer_mut(pointer).unwrap() = bad;
        reject(&value);
    }
    let mut value = fixture.value();
    let repeated = value["resource_transmissions"][0].clone();
    value["resource_transmissions"]
        .as_array_mut()
        .unwrap()
        .push(repeated);
    reject(&value);
    let mut none = budget();
    none.max_resource_transmissions = 0;
    assert!(encode_teletext_design_documents(fixture.input(), none).is_err());
    let malformed = br#"{"resource_transmissions":[{"very-deep":[[[[[["private"]]]]]]}]}"#;
    let error = decode_teletext_design_documents(malformed, none).unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "resource transmissions",
            count: 1,
            maximum: 0
        }
    ));
}

fn resource_authored() -> Value {
    let identity = |kind, page, subpage| json!({"kind":kind,"page":{"magazine":2,"page":page},"subpage":subpage});
    let target = json!({"scope":"public","resource":identity("pop",0xa0,1),"object_id":65535});
    let unspecified = json!({"kind":"unspecified","magazine":2});
    let object_link = json!({"target":unspecified,"extent":0,"supersedable":true,"raw_fallback":0,
        "defaults":[{"kind":"no_object","raw_number":213},{"kind":"dormant","object_type":"passive","raw_number":234}]});
    let drcs_link = json!({"target":unspecified,"extent":0,"supersedable":true});
    let mut public = vec![object_link.clone(); 7];
    public[0] = json!({"target":{"kind":"page","page":{"magazine":2,"page":160}},"extent":1,"supersedable":true,"raw_fallback":9,
        "defaults":[{"kind":"object","target":target,"object_type":"passive"},{"kind":"no_object","raw_number":77}]});
    let mut normal = vec![drcs_link.clone(); 7];
    normal[0] = json!({"target":{"kind":"page","page":{"magazine":2,"page":192}},"extent":0,"supersedable":true});
    let mut value = authored();
    for field in ["pages", "page_enhancements", "mot_selections"] {
        value[field].as_array_mut().unwrap().truncate(1);
    }
    value["mots"] = json!([{"identity":identity("mot",0xfe,7),
        "associations":[{"page":0,"objects":{"global":false,"public":1},"drcs":{"global":false,"public":1}}],
        "links":{"global_object":object_link,"public_objects":public,"global_drcs":drcs_link,"normal_drcs":normal,
            "counts":{"magazine_mask":2,"object_pages":1,"drcs_pages":1}}}]);
    value["mot_selections"][0]["mot"] = identity("mot", 0xfe, 7);
    value["objects"] = json!([{"identity":target,"object_type":"passive","program":{"commands":[
        {"kind":"set_position","position":{"row":1,"column":8}},
        {"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":18}},
        {"kind":"at_column","column":8,"operation":{"kind":"drcs","reference":{"table":"normal","subtable":0,"slot":47}}}
    ]}}]);
    value["drcs"] = json!([{"identity":identity("drcs",0xc0,0),"glyphs":[{"slot":47,"bitmap":[4095,1,2,4,8,16,32,64,128,2048]}]}]);
    value["empty_object_pages"] = json!([identity("pop", 0xa0, 0)]);
    value["page_enhancements"][0]["resources"] = json!({"objects":{"public":{"magazine":2,"page":160},"global":null},
        "drcs":{"normal":{"magazine":2,"page":192},"global":null},
        "defaults":[null,null,{"target":target,"object_type":"passive"},null]});
    value
}

#[test]
fn mot_sparse_defaults_full_resources_and_explicit_lifecycle_remain_original() {
    let selected = binding(B, TeletextPageContract::V2);
    let bytes = exact_bytes(&resource_authored());
    let sources = vec![source(&selected, bytes.clone())];
    let selection = TeletextChannelSelectionSnapshot {
        channel_id: 9,
        bindings: vec![selected],
    };
    let transmissions = [
        (TeletextResourceKind::Mot, 0xfe, 7),
        (TeletextResourceKind::Pop, 0xa0, 1),
        (TeletextResourceKind::Pop, 0xa0, 0),
        (TeletextResourceKind::Drcs, 0xc0, 0),
    ]
    .into_iter()
    .map(|(kind, page, subpage)| TeletextSavedResourceTransmission {
        resource: TeletextResourceIdentity::new(
            kind,
            TeletextRawDataPageNumber::new(2, page).unwrap(),
            TeletextResourceSubpageIndex::new(subpage).unwrap(),
        )
        .unwrap(),
        counter: TeletextResourceUpdateCounter::new(3).unwrap(),
        update: TeletextSavedResourceUpdate::Unchanged,
        last_subpage: true,
    })
    .collect::<Vec<_>>();
    let encoded = encode_teletext_design_documents(
        TeletextDesignDocumentInput {
            selection: &selection,
            generation: GENERATION,
            sources: &sources,
            entry_addon_id: Some(B),
            entry: Some(TeletextPageAddress {
                page: TeletextPageNumber::new(200).unwrap(),
                subcode: TeletextSubcode::new(16).unwrap(),
            }),
            v1_region: None,
            resource_transmissions: &transmissions,
        },
        budget(),
    )
    .unwrap();
    let restored = decode_teletext_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.sources()[0].bytes(), bytes);
    assert_eq!(restored.resource_transmissions(), transmissions);
    let expected = parse_declared_service(
        &bytes,
        GENERATION,
        budget().validation.service,
        budget().validation.declarations,
    )
    .unwrap();
    let TeletextProfileDocument::Authored(original) = &restored.documents()[0] else {
        panic!("authored resource source lost")
    };
    assert_eq!(original.as_ref(), &expected);
    assert_eq!(original.mots()[0].identity().logical_subpage().get(), 7);
    let page = original
        .semantic()
        .page_enhancements()
        .values()
        .next()
        .unwrap();
    assert!(page.resources.defaults[0].is_none());
    assert_eq!(
        page.resources.defaults[2]
            .unwrap()
            .target
            .resource()
            .unwrap()
            .logical_subpage()
            .get(),
        1
    );
}

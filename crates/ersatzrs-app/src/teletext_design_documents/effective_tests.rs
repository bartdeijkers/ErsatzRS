//! Structural original storage only; no current invocation/publication authority.

use std::time::{Duration as StdDuration, Instant};

use chrono::{Duration, TimeZone as _};
use ersatzrs_domain::{
    TeletextChannelSelectionSnapshot, TeletextDeclaredServiceBudget, TeletextPageNumber,
    TeletextProfileValue, TeletextResourceIdentity, TeletextResourceUpdateCounter,
    TeletextSecretReferenceKind, TeletextSelectionSnapshot, TeletextServiceBudget, TeletextSubcode,
};
use serde_json::{json, Value};

use super::super::{TeletextDesignDocumentInput, TeletextSavedResourceUpdate};
use super::*;
use crate::{
    teletext_channel_documents::preflight_contracts,
    teletext_guide::SuppliedTeletextGuideContext,
    teletext_profile_documents::{validate_document_response, TeletextProfileDocumentBudget},
};

const GENERATION: &str = "effective-original-fixture";
const A: &str = "org.example.a";
const B: &str = "org.example.b";
const C: &str = "org.example.c";
const D: &str = "org.example.d";

fn budget() -> TeletextDesignDocumentBudget {
    TeletextDesignDocumentBudget {
        max_serialized_bytes: 128 * 1024,
        max_documents: 8,
        max_response_bytes: 64 * 1024,
        max_context_bytes: 32 * 1024,
        max_context_field_bytes: 1024,
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

fn settings() -> Vec<TeletextProfileSetting> {
    vec![
        TeletextProfileSetting {
            key: "THEME".to_owned(),
            value: TeletextProfileValue::Literal("night-value-sentinel".to_owned()),
        },
        TeletextProfileSetting {
            key: "KEY".to_owned(),
            value: TeletextProfileValue::SecretReference {
                kind: TeletextSecretReferenceKind::Environment,
                reference: "UNRESOLVED_ENV_FIXTURE".to_owned(),
            },
        },
    ]
}
fn installation(addon: &str) -> TeletextInstallationSnapshot {
    TeletextInstallationSnapshot {
        addon_id: addon.to_owned(),
        state_revision: "state-fixture".to_owned(),
        version: "1.0.0".to_owned(),
        package_sha256: "a".repeat(64),
        content_sha256: "b".repeat(64),
        manifest_sha256: "c".repeat(64),
    }
}
fn base(addon: &str, contract: TeletextPageContract) -> TeletextSelectionSnapshot {
    TeletextSelectionSnapshot {
        contract,
        channel_id: 9,
        addon_id: addon.to_owned(),
        binding_revision: 3,
        profile_id: 11,
        profile_revision: 4,
        installation: installation(addon),
        effective_settings: settings(),
    }
}
fn scripted(addon: &str, contract: Option<TeletextPageContract>) -> Binding {
    let start = Utc.with_ymd_and_hms(2020, 1, 1, 12, 0, 0).unwrap();
    let state = contract.map_or_else(
        || TeletextOverrideState::Off {
            channel_id: 9,
            addon_id: addon.to_owned(),
        },
        |contract| {
            TeletextOverrideState::Enabled(TeletextProfileCandidateSnapshot {
                contract,
                channel_id: 9,
                addon_id: addon.to_owned(),
                profile_id: 12,
                profile_revision: 5,
                installation: installation(addon),
                effective_settings: settings(),
            })
        },
    );
    Binding::Scripted(TeletextPublishedOverride {
        playout_id: 42,
        interval: TeletextOverrideInterval {
            start,
            finish: start + Duration::minutes(10),
            state,
        },
    })
}

fn response(contract: TeletextPageContract, number: u16) -> Vec<u8> {
    let page = json!({"address":{"page":number,"subcode":16},"kind":"full_page","language":"nld","national_option":5,
        "rows":[{"row":1,"runs":[{"column":1,"style":{"foreground":"white","background":"black","height":"normal","boxed":false,"flash":false,"conceal":false},"content":{"kind":"text","text":"Café original fixture"}}]}],"navigation":[]});
    let mut document = json!({"schema_version":contract.schema_version(),"generation":GENERATION,"language":"nld","initial_page":number,"pages":[page]});
    if contract != TeletextPageContract::V1 {
        document["schema_version"] = json!(2);
        document["page_enhancements"] = json!([{"address":{"page":number,"subcode":16},"presentation":null,
            "compositional_links":null,"local_program":null,"resources":{"objects":{"public":null,"global":null},"drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}}]);
        for key in ["magazines", "objects", "drcs", "mots", "empty_object_pages"] {
            document[key] = json!([]);
        }
        document["mot_selections"] = json!([{"display":{"page":number,"subcode":16},"mot":null}]);
    }
    if contract == TeletextPageContract::V3 {
        document = json!({"schema_version":3,"service":document,"authoring":{"schema_version":1,
            "template":{"id":"template-fixture","revision":"7"},"theme":{"id":"night-theme-fixture","overrides":[]},
            "pages":[{"page":{"page":number,"subcode":16},"regions":[{"id":"guide-zone","kind":"generated","area":"main","row":1,"column":0,"width":40,"height":1}],
                "dynamic_fields":[{"id":"title","region":"guide-zone","binding":"guide.programme.title"}]}]}});
    }
    let mut bytes = serde_json::to_vec_pretty(&document).unwrap();
    bytes.extend_from_slice(b"\n\t ");
    bytes
}

fn requests(
    contracts: impl IntoIterator<Item = TeletextPageContract>,
) -> TeletextChannelRequestTemplates {
    let context = SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl"],
        timezone: "Europe/Amsterdam",
        captured_at: Utc.with_ymd_and_hms(2020, 1, 1, 12, 0, 0).unwrap(),
        channels: &[],
    };
    preflight_contracts(contracts, &context).unwrap()
}

struct Fixture {
    selection: Selection,
    producers: Vec<usize>,
    sources: Vec<TeletextValidatedDocumentSource>,
    requests: TeletextChannelRequestTemplates,
}
impl Fixture {
    fn mixed() -> Self {
        let selection = Selection {
            channel_id: 9,
            bindings: vec![
                Binding::Base(base(A, TeletextPageContract::V1)),
                scripted(B, None),
                scripted(C, Some(TeletextPageContract::V2)),
                scripted(D, Some(TeletextPageContract::V3)),
            ],
            valid_until: Some(Utc.with_ymd_and_hms(2020, 1, 1, 12, 5, 0).unwrap()),
        };
        let sources = [
            (TeletextPageContract::V1, 100),
            (TeletextPageContract::V2, 200),
            (TeletextPageContract::V3, 300),
        ]
        .into_iter()
        .map(|(contract, page)| {
            validate_document_response(
                contract,
                GENERATION,
                response(contract, page),
                budget().validation,
                Instant::now() + StdDuration::from_secs(30),
            )
            .unwrap()
            .1
        })
        .collect();
        Self {
            selection,
            producers: vec![0, 2, 3],
            sources,
            requests: requests([
                TeletextPageContract::V1,
                TeletextPageContract::V2,
                TeletextPageContract::V3,
            ]),
        }
    }
    fn input(&self) -> TeletextEffectiveDesignDocumentInput<'_> {
        TeletextEffectiveDesignDocumentInput {
            selection: &self.selection,
            producer_bindings: &self.producers,
            generation: GENERATION,
            sources: &self.sources,
            entry_addon_id: Some(A),
            entry: Some(TeletextPageAddress {
                page: TeletextPageNumber::new(100).unwrap(),
                subcode: TeletextSubcode::new(16).unwrap(),
            }),
            v1_region: Some(TeletextSavedV1Region::WesternLatin),
            resource_transmissions: &[],
        }
    }
    fn encode(&self) -> EncodedEffectiveTeletextDesignDocuments {
        encode_teletext_effective_design_documents_with_requests(
            self.input(),
            &self.requests,
            budget(),
        )
        .unwrap()
    }
}

#[test]
fn saved_effective_bytes_retain_noncanonical_spelling_and_decoder_limits() {
    let encoded = Fixture::mixed().encode();
    let value: Value = serde_json::from_slice(encoded.bytes()).unwrap();
    let mut bytes = serde_json::to_vec_pretty(&value).unwrap();
    bytes.extend_from_slice(b"\n\n");
    let restored =
        EncodedEffectiveTeletextDesignDocuments::from_saved_bytes(bytes.clone(), budget()).unwrap();
    assert_eq!(restored.bytes(), bytes);
    let tight = TeletextDesignDocumentBudget {
        max_serialized_bytes: bytes.len() - 1,
        ..budget()
    };
    assert!(
        EncodedEffectiveTeletextDesignDocuments::from_saved_bytes(bytes.clone(), tight).is_err()
    );
    bytes.extend_from_slice(b"{}");
    assert!(EncodedEffectiveTeletextDesignDocuments::from_saved_bytes(bytes, budget()).is_err());
}

#[test]
fn effective_mixed_contracts_preserve_expired_origins_settings_and_exact_responses() {
    let fixture = Fixture::mixed();
    let encoded = fixture.encode();
    assert_eq!(encoded.version(), 2);
    let restored = decode_teletext_effective_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(*restored.selection(), fixture.selection);
    assert_eq!(restored.producer_bindings(), [0, 2, 3]);
    assert_eq!(
        restored.edit_policy(),
        TeletextSavedEditPolicy::OriginalOnly
    );
    for (original, saved) in fixture.sources.iter().zip(restored.sources()) {
        assert_eq!(original.bytes(), saved.bytes());
        assert_eq!(original.contract(), saved.contract());
        assert_eq!(original.authoring(), saved.authoring());
        assert_eq!(
            fixture.requests.request(original.contract()),
            restored
                .guide_inputs()
                .unwrap()
                .request(original.contract())
        );
    }
    assert!(restored.sources()[2].authoring().is_some());
    for debug in [
        format!("{encoded:?}"),
        format!("{restored:?}"),
        format!("{:?}", fixture.input()),
    ] {
        for private in [
            "night-value-sentinel",
            "UNRESOLVED_ENV_FIXTURE",
            "Café original fixture",
            "night-theme-fixture",
        ] {
            assert!(!debug.contains(private));
        }
    }
}

#[test]
fn legacy_empty_original_writer_remains_exactly_version_one() {
    let selection = TeletextChannelSelectionSnapshot {
        channel_id: 9,
        bindings: vec![],
    };
    let encoded = super::super::encode_teletext_design_documents(
        TeletextDesignDocumentInput {
            selection: &selection,
            generation: GENERATION,
            sources: &[],
            entry_addon_id: None,
            entry: None,
            v1_region: None,
            resource_transmissions: &[],
        },
        budget(),
    )
    .unwrap();
    assert_eq!(encoded.bytes(), br#"{"format_version":1,"selection":{"channel_id":9,"bindings":[]},"generation":"effective-original-fixture","entry_addon_id":null,"entry":null,"v1_region":null,"edit_policy":{"policy_version":1,"kind":"original_only"},"resource_transmissions":[],"documents":[]}"#);
    assert_eq!(encoded.version(), 1);
    assert!(decode_teletext_effective_design_documents(encoded.bytes(), budget()).is_err());
}

#[test]
fn all_off_scope_preserves_members_without_sources_or_unused_entry() {
    let selection = Selection {
        channel_id: 9,
        bindings: vec![scripted(B, None)],
        valid_until: Some(Utc.with_ymd_and_hms(2020, 1, 1, 12, 10, 0).unwrap()),
    };
    let encoded = encode_teletext_effective_design_documents_with_requests(
        TeletextEffectiveDesignDocumentInput {
            selection: &selection,
            producer_bindings: &[],
            generation: GENERATION,
            sources: &[],
            entry_addon_id: None,
            entry: None,
            v1_region: None,
            resource_transmissions: &[],
        },
        &requests([]),
        budget(),
    )
    .unwrap();
    assert_eq!(encoded.bytes(), br#"{"format_version":2,"effective_selection":{"channel_id":9,"bindings":[{"origin":"scripted","base":null,"scripted":{"playout_id":42,"start":"2020-01-01T12:00:00Z","finish":"2020-01-01T12:10:00Z","state":{"kind":"off","channel_id":9,"addon_id":"org.example.b","candidate":null}}}],"valid_until":"2020-01-01T12:10:00Z"},"producer_bindings":[],"generation":"effective-original-fixture","entry_addon_id":null,"entry":null,"v1_region":null,"edit_policy":{"policy_version":1,"kind":"original_only"},"resource_transmissions":[],"documents":[],"guide_inputs":{"input_version":1,"v1":null,"v2":null}}"#);
    let restored = decode_teletext_effective_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(*restored.selection(), selection);
    assert!(restored.sources().is_empty());
    assert!(!restored.guide_inputs().unwrap().has_guide_channels());
    assert!(restored.editorial_overlay().is_none());
    assert_eq!(
        append_teletext_effective_design_editorial_overlay(
            encoded.bytes(),
            &editorial_tests::overlay(),
            budget()
        )
        .unwrap_err()
        .reason,
        TeletextDesignDocumentFailure::EditorialOverlay
    );
}

#[test]
fn effective_mapping_rejects_missing_duplicate_off_and_reordered_indices() {
    for producers in [
        vec![],
        vec![0, 2],
        vec![0, 1, 3],
        vec![0, 2, 2],
        vec![3, 2, 0],
        vec![0, 2, usize::MAX],
    ] {
        let mut fixture = Fixture::mixed();
        fixture.producers = producers;
        assert!(encode_teletext_effective_design_documents_with_requests(
            fixture.input(),
            &fixture.requests,
            budget()
        )
        .is_err());
    }
}

fn mutate_decode(mutator: impl FnOnce(&mut Value)) -> TeletextDesignDocumentError {
    let mut value: Value = serde_json::from_slice(Fixture::mixed().encode().bytes()).unwrap();
    mutator(&mut value);
    decode_teletext_effective_design_documents(&serde_json::to_vec(&value).unwrap(), budget())
        .unwrap_err()
}

#[test]
fn effective_closed_origin_time_identity_and_edit_policy_shapes_are_enforced() {
    let mutations: [fn(&mut Value); 13] = [
        |v| v["unexpected"] = json!(true),
        |v| v["guide_inputs"] = Value::Null,
        |v| {
            v.as_object_mut().unwrap().remove("guide_inputs");
        },
        |v| v["effective_selection"]["valid_until"] = json!("2020-01-01T12:00:00Z"),
        |v| v["effective_selection"]["valid_until"] = Value::Null,
        |v| v["effective_selection"]["bindings"][1]["scripted"]["playout_id"] = json!(0),
        |v| {
            v["effective_selection"]["bindings"][1]["scripted"]["finish"] =
                json!("2020-01-01T11:00:00Z");
        },
        |v| v["effective_selection"]["bindings"][2]["scripted"]["state"]["channel_id"] = json!(777),
        |v| {
            v["effective_selection"]["bindings"][2]["scripted"]["state"]["candidate"]
                ["binding_revision"] = json!(3);
        },
        |v| {
            v["effective_selection"]["bindings"][2]["scripted"]["state"]["candidate"]
                ["installation"]["addon_id"] = json!(A);
        },
        |v| v["effective_selection"]["bindings"][1]["origin"] = json!("base"),
        |v| v["producer_bindings"] = json!([0, 1, 3]),
        |v| v["edit_policy"]["kind"] = json!("editable"),
    ];
    for mutation in mutations {
        let error = mutate_decode(mutation);
        assert!(!format!("{error:?}").contains("fixture"));
    }
}

#[test]
fn effective_sources_requests_entry_and_settings_reject_silent_loss() {
    for mutation in [
        (|v: &mut Value| v["documents"][2]["contract"] = json!(2)) as fn(&mut Value),
        |v| v["documents"][0]["generation"] = json!("wrong"),
        |v| v["guide_inputs"]["v1"] = Value::Null,
        |v| v["entry_addon_id"] = json!(B),
        |v| v["entry"]["subcode"] = json!(17),
        |v| v["v1_region"] = Value::Null,
        |v| {
            v["effective_selection"]["bindings"][2]["scripted"]["state"]["candidate"]
                ["effective_settings"][1]["value"]["kind"] = json!("secret_contents");
        },
    ] {
        mutate_decode(mutation);
    }
}

#[test]
fn effective_preallocation_budgets_cover_nested_candidates_sources_and_maps() {
    let bytes = Fixture::mixed().encode().into_bytes();
    for limited in [
        TeletextDesignDocumentBudget {
            max_documents: 2,
            ..budget()
        },
        TeletextDesignDocumentBudget {
            max_context_bytes: 64,
            ..budget()
        },
        TeletextDesignDocumentBudget {
            max_context_field_bytes: 8,
            ..budget()
        },
        TeletextDesignDocumentBudget {
            max_response_bytes: 1,
            ..budget()
        },
        TeletextDesignDocumentBudget {
            max_serialized_bytes: bytes.len() - 1,
            ..budget()
        },
    ] {
        assert!(matches!(
            decode_teletext_effective_design_documents(&bytes, limited)
                .unwrap_err()
                .reason,
            TeletextDesignDocumentFailure::Budget { .. }
        ));
    }
    let mut value: Value = serde_json::from_slice(&bytes).unwrap();
    let settings = value["effective_selection"]["bindings"][2]["scripted"]["state"]["candidate"]
        ["effective_settings"][0]
        .clone();
    value["effective_selection"]["bindings"][2]["scripted"]["state"]["candidate"]
        ["effective_settings"] = Value::Array(vec![settings; MAX_TELETEXT_PROFILE_SETTINGS + 1]);
    assert!(matches!(
        decode_teletext_effective_design_documents(&serde_json::to_vec(&value).unwrap(), budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Budget {
            dimension: "settings",
            ..
        }
    ));
}

#[test]
fn effective_duplicate_fields_are_rejected_without_debug_payloads() {
    let bytes = Fixture::mixed().encode().into_bytes();
    let text = String::from_utf8(bytes).unwrap();
    let duplicate = text.replacen(
        "\"format_version\":2",
        "\"format_version\":2,\"format_version\":2",
        1,
    );
    assert_eq!(
        decode_teletext_effective_design_documents(duplicate.as_bytes(), budget())
            .unwrap_err()
            .reason,
        TeletextDesignDocumentFailure::Json
    );
}

#[test]
fn effective_guide_fence_requests_and_lifecycle_survive_without_live_authority() {
    use crate::teletext_guide::TeletextGuideChannel;
    use ersatzrs_core::repositories::{
        TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION, TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
    };
    use ersatzrs_domain::{
        TeletextRawDataPageNumber, TeletextResourceKind, TeletextResourceSubpageIndex,
    };

    let fixture = Fixture::mixed();
    let channels = [TeletextGuideChannel {
        id: 17,
        number: "17",
        name: "Guide fixture",
        programmes: &[],
    }];
    let context = SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl"],
        timezone: "Europe/Amsterdam",
        captured_at: Utc.with_ymd_and_hms(2020, 1, 1, 12, 0, 0).unwrap(),
        channels: &channels,
    };
    let requests = preflight_contracts(
        [
            TeletextPageContract::V1,
            TeletextPageContract::V2,
            TeletextPageContract::V3,
        ],
        &context,
    )
    .unwrap();
    let parsed = ersatzrs_addon_contract::TeletextPagesRequest::from_json(
        requests.request(TeletextPageContract::V1).unwrap(),
    )
    .unwrap();
    let fence = TeletextGuideSourceFence {
        version: TELETEXT_GUIDE_SOURCE_FENCE_VERSION,
        context_digest_version: TELETEXT_GUIDE_CONTEXT_DIGEST_VERSION,
        context_digest: [0x5a; 32],
        channel_ids: vec![17],
        captured_at: parsed.captured_at,
        start: parsed.window.start,
        finish: parsed.window.finish,
        locale: parsed.locale,
        timezone: parsed.timezone,
    };
    let lifecycle = [TeletextSavedResourceTransmission {
        resource: TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        counter: TeletextResourceUpdateCounter::new(3).unwrap(),
        update: TeletextSavedResourceUpdate::Unchanged,
        last_subpage: true,
    }];
    let input = TeletextEffectiveDesignDocumentInput {
        resource_transmissions: &lifecycle,
        ..fixture.input()
    };
    let encoded =
        encode_teletext_effective_design_documents_with_source(input, &requests, &fence, budget())
            .unwrap();
    let restored = decode_teletext_effective_design_documents(encoded.bytes(), budget()).unwrap();
    assert_eq!(restored.guide_source(), Some(&fence));
    assert!(restored.guide_inputs().unwrap().has_guide_channels());
    assert_eq!(restored.resource_transmissions(), lifecycle);
    for contract in [
        TeletextPageContract::V1,
        TeletextPageContract::V2,
        TeletextPageContract::V3,
    ] {
        assert_eq!(
            restored.guide_inputs().unwrap().request(contract),
            requests.request(contract)
        );
    }
    assert_invalid_guide_and_lifecycle(&encoded);
    editorial_tests::assert_preserved_original(encoded.bytes());
}

#[path = "effective_editorial_tests.rs"]
mod editorial_tests;

fn assert_invalid_guide_and_lifecycle(encoded: &EncodedEffectiveTeletextDesignDocuments) {
    let mut invalid = serde_json::from_slice::<Value>(encoded.bytes()).unwrap();
    invalid["guide_source"]["channel_ids"] = json!([18]);
    assert_eq!(
        decode_teletext_effective_design_documents(
            &serde_json::to_vec(&invalid).unwrap(),
            budget()
        )
        .unwrap_err()
        .reason,
        TeletextDesignDocumentFailure::GuideSource
    );
    let mut invalid = serde_json::from_slice::<Value>(encoded.bytes()).unwrap();
    let duplicate = invalid["resource_transmissions"][0].clone();
    invalid["resource_transmissions"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert_eq!(
        decode_teletext_effective_design_documents(
            &serde_json::to_vec(&invalid).unwrap(),
            budget()
        )
        .unwrap_err()
        .reason,
        TeletextDesignDocumentFailure::Lifecycle
    );
}

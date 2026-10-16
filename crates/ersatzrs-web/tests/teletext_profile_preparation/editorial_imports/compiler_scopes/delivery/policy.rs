//! Keep the located original quota failure and qualify one fixture-only change.

use super::*;
use ersatzrs_app::teletext_design_documents::TeletextDesignDocumentBudget;
use ersatzrs_web::host::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_generations::reload_accepted_teletext_generation,
};
use std::time::{Duration, Instant};

const RUNTIME: &str = include_str!("../../../../fixtures/teletext-output-policy-v1.json");

fn number(value: &Value, field: &str) -> usize {
    usize::try_from(
        value[field]
            .as_u64()
            .expect("explicit unsigned runtime limit"),
    )
    .expect("runtime limit must fit this supported target")
}

macro_rules! same_limits {
    ($json:expr, $budget:expr; $($field:ident),+ $(,)?) => {
        $(assert_eq!(number($json, stringify!($field)), ($budget).$field,
            "the preflight must retain the exact runtime {}", stringify!($field));)+
    };
}

fn assert_validation(json: &Value, policy: &TeletextChannelDocumentEncodingPolicy) {
    same_limits!(&json["validation"]["service"],policy.validation.service;
        max_records,max_objects,max_structure_items,max_base_content_bytes,max_commands,max_collected_commands,max_collected_drcs_references);
    same_limits!(&json["validation"]["declarations"],policy.validation.declarations;
        max_mots,max_displays,max_resources,max_defaults,max_objects);
    same_limits!(&json["composition"],policy.composition;
        max_producers,max_input_items,max_input_content_bytes,max_input_commands,max_storage_bytes,max_work);
    same_limits!(&json["composition"]["service"],policy.composition.service;
        max_records,max_objects,max_structure_items,max_base_content_bytes,max_commands,max_collected_commands,max_collected_drcs_references);
    same_limits!(&json["composition"]["declarations"],policy.composition.declarations;
        max_mots,max_displays,max_resources,max_defaults,max_objects);
}

fn mirrored_policy(
    json: &Value,
) -> (
    TeletextChannelDocumentEncodingPolicy,
    TeletextDesignDocumentBudget,
) {
    let mut policy = scope_fixture::encoding_policy();
    let design = designs::budget();
    assert_eq!(json["version"], 1);
    same_limits!(&json["design"],design;max_serialized_bytes,max_documents,max_response_bytes,max_context_bytes,max_context_field_bytes,max_resource_transmissions);
    same_limits!(&json["input"],policy.input;max_producers,max_response_bytes);
    assert_validation(json, &policy);
    let limits = &json["encoding"];
    assert_eq!(limits.as_object().unwrap().len(), 17);
    same_limits!(limits,policy.encoding;max_pages,max_units,max_commands);
    let encoding = &mut policy.encoding;
    encoding.max_display_updates = Some(number(limits, "max_display_updates"));
    encoding.max_glyph_comparison_work = Some(number(limits, "max_glyph_comparison_work"));
    encoding.max_display_comparison_updates =
        Some(number(limits, "max_display_comparison_updates"));
    encoding.max_realization_updates = Some(number(limits, "max_realization_updates"));
    encoding.max_placement_work = Some(number(limits, "max_placement_work"));
    encoding.max_assembly_work = Some(number(limits, "max_assembly_work"));
    encoding.max_ancestry_work = Some(number(limits, "max_ancestry_work"));
    encoding.max_collected_commands = Some(number(limits, "max_collected_commands"));
    encoding.max_collected_drcs_references = Some(number(limits, "max_collected_drcs_references"));
    encoding.max_spatial_event_visits = Some(number(limits, "max_spatial_event_visits"));
    encoding.max_spatial_invocation_visits = Some(number(limits, "max_spatial_invocation_visits"));
    encoding.max_identity_checks = Some(number(limits, "max_identity_checks"));
    encoding.max_attribute_cell_updates = Some(number(limits, "max_attribute_cell_updates"));
    // Production runtime recovery has placeholders here. The immutable saved
    // entry, region and lifecycle replace them before actual compilation.
    policy.transmissions.clear();
    policy.entry = None;
    (policy, design)
}

fn configured() -> (Value, Value) {
    let original: Value = serde_json::from_str(RUNTIME).unwrap();
    assert_eq!(original["encoding"]["max_assembly_work"], 8192);
    let mut configured = original.clone();
    // One-off exact accepted-service probe measured Day18840 and Night12792.
    configured["encoding"]["max_assembly_work"] = json!(18840);
    let mut restored = configured.clone();
    restored["encoding"]["max_assembly_work"] = original["encoding"]["max_assembly_work"].clone();
    assert_eq!(
        restored, original,
        "exactly one fixture policy field may change"
    );
    (original, configured)
}

pub(super) async fn configure(fixture: &Fixture) {
    let (original, configured) = configured();
    let actual = fixture
        .state
        .application
        .get_optional_setting("teletext.output.policy")
        .await
        .unwrap()
        .unwrap();
    assert!(actual.len() <= 65_536);
    assert_eq!(serde_json::from_str::<Value>(&actual).unwrap(), original);
    ordinary_stream_flow::setting(
        fixture,
        "teletext.output.policy",
        &serde_json::to_string(&configured).unwrap(),
    )
    .await;
}

async fn recover(
    fixture: &Fixture,
    json: &Value,
) -> (Vec<Value>, Vec<Option<AcceptedTeletextOutputCandidate>>) {
    let (policy, design) = mirrored_policy(json);
    let timeout =
        Duration::from_millis(u64::try_from(number(json, "recovery_timeout_ms")).unwrap());
    assert!(!timeout.is_zero());
    let mut records = Vec::new();
    let mut recovered = Vec::new();
    for channel in [1, 2] {
        let deadline = Instant::now().checked_add(timeout).unwrap();
        let result = tokio::time::timeout(
            timeout,
            reload_accepted_teletext_generation(
                &fixture.state.application,
                channel,
                design,
                policy.clone(),
                deadline,
            ),
        )
        .await;
        match result {
            Ok(Ok(Some(candidate))) => {
                records.push(json!({"channel_id":channel,"outcome":"ready","generation_id":candidate.generation().id}));
                recovered.push(Some(candidate));
            }
            Ok(Ok(None)) => {
                records.push(json!({"channel_id":channel,"outcome":"missing"}));
                recovered.push(None);
            }
            Ok(Err(error)) => {
                records.push(json!({"channel_id":channel,"outcome":"typed_error","error_debug":format!("{error:?}"),"error_display":error.to_string()}));
                recovered.push(None);
            }
            Err(_) => {
                records.push(json!({"channel_id":channel,"outcome":"outer_timeout"}));
                recovered.push(None);
            }
        }
    }
    (records, recovered)
}

fn assert_original_rejection(records: &[Value]) {
    let located = "reason: Assembly(TeletextAugmentedOriginProgramError { owner: TeletextPageAddress { page: TeletextPageNumber(129), subcode: TeletextSubcode(32) }, origin: Some(994), location: None, root_source: None, invocation: None, reason: WorkBudget { maximum: 197 } })";
    assert_eq!(records.len(), 2);
    for (record, channel) in records.iter().zip([1, 2]) {
        assert_eq!(record["channel_id"], channel);
        assert_eq!(record["outcome"], "typed_error");
        let debug = record["error_debug"].as_str().unwrap();
        assert!(debug.len() <= 4096);
        assert!(
            debug.contains(located),
            "original cap must retain its exact located assembly failure: {debug}"
        );
    }
}

pub(super) async fn preflight(
    fixture: &Fixture,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    directory: &Path,
) {
    let (original, configured) = configured();
    let actual = fixture
        .state
        .application
        .get_optional_setting("teletext.output.policy")
        .await
        .unwrap()
        .unwrap();
    assert!(actual.len() <= 65_536);
    assert_eq!(serde_json::from_str::<Value>(&actual).unwrap(), configured);
    polish_delivery::write_json(
        directory,
        "runtime-preflight-policy.json",
        &json!({"probe_version":1,
        "fixture":"crates/ersatzrs-web/tests/fixtures/teletext-output-policy-v1.json",
        "fixture_sha256":live_client::hash(RUNTIME.as_bytes()),"stored_policy_sha256":live_client::hash(actual.as_bytes()),
        "original_policy":original,"configured_policy":configured,
        "only_difference":{"path":"encoding.max_assembly_work","original":8192,"configured":18840},
        "one_field_difference_verified":true,"shared_fixture_and_production_defaults_unchanged":true,
        "service_wide_optional_caps_mirrored":13,"design_input_validation_composition_and_base_encoding_limits_equal":true,
        "nested_normalization":"inherited unchanged from native fixture; not a full private runtime DTO parser mirror",
        "scope":"public accepted-generation reload with original negative and one-field fixture policy; no source/PCR/HTTP/concurrency qualification"}),
    );
    let (original_records, original_recovered) = recover(fixture, &original).await;
    let (configured_records, recovered) = recover(fixture, &configured).await;
    polish_delivery::write_json(
        directory,
        "runtime-preflight-results.json",
        &json!({"probe_version":1,
        "before_any_http_request":true,"runtime_policy_file":"runtime-preflight-policy.json",
        "original_8192":original_records,"configured_18840":configured_records,
        "only_limit_changed":"encoding.max_assembly_work","capture_threshold_changed":false}),
    );
    assert_original_rejection(&original_records);
    assert!(original_recovered.iter().all(Option::is_none));
    assert!(
        recovered.iter().all(Option::is_some),
        "one-field ordinary runtime recovery rejected; see runtime-preflight-results.json"
    );
    assert_eq!(recovered.len(), accepted.len());
    for (candidate, expected) in recovered.into_iter().zip(accepted) {
        generations::assert_same_output(&candidate.unwrap(), expected);
    }
}

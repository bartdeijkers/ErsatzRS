use serde_json::{json, Value};

use super::*;

fn qualified_guide_limits() -> Value {
    // The guide-source limits of the production TV-guide integration tests
    // (tests/teletext_profile_preparation/tvguide_candidate/admin_preparation.rs).
    json!({
        "max_records":1000,"max_source_bytes":1_048_576,
        "max_model_nodes":10_000,"max_model_bytes":16_777_216,
        "max_digest_fields":100_000,"max_digest_bytes":16_777_216
    })
}

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap()
}

#[test]
fn tvguide_profile_is_exactly_the_policy_pair_the_production_guide_is_qualified_with() {
    let mut output = fixture();
    output["version"] = json!(4);
    output["guide_limits"] = qualified_guide_limits();
    output["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    assert_eq!(
        serde_json::from_str::<Value>(TVGUIDE_16.output).unwrap(),
        output
    );

    let mut authoring = fixture();
    let object = authoring.as_object_mut().unwrap();
    object.remove("source");
    object.remove("recovery_timeout_ms");
    authoring["version"] = json!(2);
    authoring["operation_timeout_ms"] = json!(30_000);
    authoring["preview"] = json!({"max_pages":16,"max_model_records":32_768,
        "max_pixels":168_000,"max_diagnostics":1400,"max_work":50_000_000});
    authoring["guide_limits"] = qualified_guide_limits();
    assert_eq!(
        serde_json::from_str::<Value>(TVGUIDE_16.authoring).unwrap(),
        authoring
    );
}

#[test]
fn every_profile_passes_the_runtime_validators_and_supports_guide_refresh() {
    for profile in PROFILES {
        let output = parse_native_teletext_runtime_policy(profile.output).unwrap();
        assert!(output.guide_refresh && output.guide_limits.is_some());
        assert!(output.live_updates.is_some());
        let authoring = parse_native_teletext_authoring_policy(profile.authoring).unwrap();
        assert!(authoring.guide_limits.is_some());
        assert_eq!(profile.max_pages(), Some(16));
    }
}

#[test]
fn classification_distinguishes_absent_profile_custom_and_invalid_without_raw_values() {
    assert_eq!(classify(Kind::Output, None), TeletextPolicyState::Absent);
    assert_eq!(
        classify(Kind::Output, Some(TVGUIDE_16.output)),
        TeletextPolicyState::Profile("tvguide-16")
    );
    // Whitespace and key order do not matter; values do.
    let compact = serde_json::from_str::<Value>(TVGUIDE_16.output)
        .unwrap()
        .to_string();
    assert_eq!(
        classify(Kind::Output, Some(&compact)),
        TeletextPolicyState::Profile("tvguide-16")
    );
    let mut tuned: Value = serde_json::from_str(TVGUIDE_16.output).unwrap();
    tuned["recovery_timeout_ms"] = json!(4000);
    assert_eq!(
        classify(Kind::Output, Some(&tuned.to_string())),
        TeletextPolicyState::Custom
    );
    assert_eq!(
        classify(Kind::Output, Some("{}")),
        TeletextPolicyState::Invalid
    );
    // An output policy is not a valid authoring policy and vice versa.
    assert_eq!(
        classify(Kind::Authoring, Some(TVGUIDE_16.output)),
        TeletextPolicyState::Invalid
    );
}

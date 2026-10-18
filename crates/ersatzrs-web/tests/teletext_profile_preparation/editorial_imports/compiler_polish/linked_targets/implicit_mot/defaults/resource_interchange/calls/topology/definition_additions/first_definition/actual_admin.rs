//! Owned headed Add synchronization; native assertions do not fabricate GUI proof.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path, time::Duration};

const CASE: &str = "teletext-definition-add";

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct BrowserComplete {
    probe_version: u8,
    case: String,
    design_id: i64,
    revision: i64,
    digest: String,
    new_ids: [u16; 4],
}
fn ready(origin: &live_client::LiveOrigin, flow: &Flow, marker: &Path) -> Value {
    json!({"probe_version":1,"case":CASE,"base_url":origin.base_url(),"channel_id":1,
        "design_id":flow.saved[0].head().id,"revision":1,"digest":flow.saved[0].digest(),
        "resources":flow.accepted[0].service().declared().resource_inventory(),
        "new_ids":NEW_IDS,"object_types":TYPES,"resource_indices":RESOURCE_INDICES,
        "page":124,"subcode":"0020","marker_file":marker,"timeout_seconds":600})
}
fn validate(complete: &BrowserComplete, flow: &Flow) {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, flow.saved[0].head().id);
    assert_eq!(complete.revision, 5);
    assert_eq!(complete.new_ids, NEW_IDS);
}
async fn verify(
    fixture: &Fixture,
    flow: &Flow,
    before: &Retained,
    complete: &BrowserComplete,
    expected: &EmptyExpected,
) {
    validate(complete, flow);
    let stored = Box::pin(assert_concept(fixture, flow, expected)).await;
    assert_eq!(stored.digest, complete.digest);
    Box::pin(assert_exported(fixture, &stored, expected)).await;
    unchanged(fixture, flow, before).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires actual headed localized Add forms and a fresh explicit ready path"]
async fn headed_admin_first_resource_definition_add_localized_concepts_and_cold_reopen() {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(
        directory.is_dir(),
        "caller supplies a fresh proof directory"
    );
    let expected = Box::pin(qualify_empty(4)).await;
    let fixture = Fixture::new_persistent().await;
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_empty(fixture)).await;
    assert_native_empty_definitions(flow.accepted[0].service());
    for page in flow.accepted[0].service().pages() {
        if let EncodedTeletextAuthoredPage::Object(page) = page {
            assert!(page.definition_positions().is_empty());
        }
    }
    let before = retained(&fixture).await;
    let marker = directory.join("browser-complete.json");
    assert!(!marker.exists(), "completion must be a fresh marker");
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        live_client::write_new(
            &ready_path,
            &serde_json::to_vec_pretty(&ready(&origin, &flow, &marker)).unwrap(),
        )
        .unwrap();
        let complete: BrowserComplete = tokio::time::timeout(
            Duration::from_mins(10),
            resource_bank_admin::marker(directory, "browser-complete.json"),
        )
        .await
        .expect("headed Add proof must finish within600seconds")
        .expect("closed valid browser completion marker");
        Box::pin(verify(&fixture, &flow, &before, &complete, &expected)).await;
        complete
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let complete = match outcome {
        Ok(complete) => complete,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    closed.expect("owned headed Add router must close");
    fixture = fixture.reopen().await;
    Box::pin(verify(&fixture, &flow, &before, &complete, &expected)).await;
    live_client::write_new(
        &directory.join("native-add-success.json"),
        &serde_json::to_vec_pretty(&json!({"probe_version":1,"case":CASE,"browser":complete,
            "schema_version":18,"concept_revision":5,"added_ids":NEW_IDS,
            "empty_typed_programs_without_noop":true,"native_pointer_and_marker_checks":true,
            "complete_native_units_and_preview_exact":true,"fresh_export_all_four_identities":true,
            "cold_reopen":true,"original_publications_night_profiles_and_package_unchanged":true,
            "ordinary_stream_or_client_acceptance_claim":false,"actual_qtm_claim":false,
            "full_engine_acceptance_claim":false}))
        .unwrap(),
    )
    .unwrap();
    fixture.state.pool.close().await;
}

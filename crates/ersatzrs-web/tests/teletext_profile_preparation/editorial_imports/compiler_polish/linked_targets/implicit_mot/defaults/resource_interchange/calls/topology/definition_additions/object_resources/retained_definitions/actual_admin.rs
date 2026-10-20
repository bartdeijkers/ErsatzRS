//! Reuse the existing owned headed form/marker boundary for retained banks.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path, time::Duration};

const CASE: &str = "teletext-retained-bank-definitions";
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
fn browser_response() -> Value {
    let mut value = filled_response(true);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    for (from, to) in [(32, 34), (33, 35)] {
        let index = objects
            .iter()
            .position(|object| object["identity"]["object_id"] == from)
            .unwrap();
        let mut added = objects[index].clone();
        added["identity"]["object_id"] = json!(to);
        objects.insert(index + 1, added);
    }
    value
}
fn ready(
    origin: &live_client::LiveOrigin,
    concept: &SavedEditorialTeletextDesign,
    marker: &Path,
) -> Value {
    json!({"probe_version":1,"case":CASE,"base_url":origin.base_url(),"channel_id":1,
        "design_id":concept.head().id,"revision":3,"digest":concept.digest(),
        "empty_resource":bank_resources()[2],"source_object":new_target(1),
        "new_ids":[32,33,34,35],"marker_file":marker,"timeout_seconds":600})
}
async fn verify(
    fixture: &Fixture,
    flow: &Flow,
    complete: &BrowserComplete,
    expected: &BankExpected,
) -> TeletextDesignRevision {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, flow.saved[0].head().id);
    assert_eq!(complete.revision, 7);
    assert_eq!(complete.new_ids, [32, 33, 34, 35]);
    let head = fixture
        .state
        .application
        .teletext_design_head(complete.design_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, 7);
    assert!(!head.deleted);
    let stored = fixture
        .state
        .application
        .teletext_design_revision(head.id, 7, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.digest, complete.digest);
    assert!(fixture
        .state
        .application
        .teletext_design_revision(head.id, 8, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    Box::pin(assert_bank_revision(fixture, flow, &stored, expected)).await;
    let exported = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: stored.design_id,
            revision: stored.revision,
            design_digest: stored.digest.clone(),
            expected_design_head: 7,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(matches!(exported, TeletextDesignMutation::Saved(_)));
    assert_original_pointers(fixture, flow).await;
    stored
}
async fn publish(
    fixture: &Fixture,
    flow: &Flow,
    stored: &TeletextDesignRevision,
) -> AcceptedTeletextOutputCandidate {
    use ersatzrs_core::repositories::TeletextPublicationMutation;
    let pending =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            stored.design_id,
            7,
            stored.digest.clone(),
            7,
            Some(flow.pending[0].id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!("checked browser concept must admit explicit publication")
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(flow.accepted[0].generation().clone())
    );
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: 7,
                expected_generation_id: Some(flow.accepted[0].generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires actual headed browsers, matched assets and a fresh explicit ready path"]
async fn retained_bank_add_copy_headed_localized_forms_preview_publication_and_cold_reopen() {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(directory.is_dir());
    let first = Box::pin(qualify_banks(false, false)).await;
    let expected = Box::pin(qualify_response(browser_response(), false)).await;
    let fixture = Fixture::new_persistent().await;
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    let (mut fixture, flow, concept) = Box::pin(setup_retained_on(&first, fixture)).await;
    let before = resource_bank_admin::Retained::capture(&fixture).await;
    let marker = directory.join("browser-complete.json");
    assert!(!marker.exists());
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        live_client::write_new(
            &ready_path,
            &serde_json::to_vec_pretty(&ready(&origin, &concept, &marker)).unwrap(),
        )
        .unwrap();
        let complete: BrowserComplete = tokio::time::timeout(
            Duration::from_mins(10),
            resource_bank_admin::marker(directory, "browser-complete.json"),
        )
        .await
        .expect("owned browser proof must finish within600seconds")
        .expect("closed browser completion marker");
        let stored = Box::pin(verify(&fixture, &flow, &complete, &expected)).await;
        (complete, stored)
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let (complete, stored) = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    closed.expect("owned headed router must close");
    fixture = fixture.reopen().await;
    let reopened = Box::pin(verify(&fixture, &flow, &complete, &expected)).await;
    assert_eq!(stored, reopened);
    let accepted = Box::pin(publish(&fixture, &flow, &stored)).await;
    assert_bank_output(accepted.service(), &expected);
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_revision(&fixture, &flow, &stored, &expected)).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    before.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    live_client::write_new(&directory.join("native-retained-bank-success.json"),&serde_json::to_vec_pretty(&json!({
        "probe_version":1,"case":CASE,"browser":complete,"complete_native_model_units_raster_exact":true,
        "original_lifecycle_metadata_sources_night_and_package_preserved":true,
        "separate_explicit_publication_and_cold_reopen":true,"actual_qtm_or_client_or_full9A_claim":false,
    })).unwrap()).unwrap();
    fixture.state.pool.close().await;
}

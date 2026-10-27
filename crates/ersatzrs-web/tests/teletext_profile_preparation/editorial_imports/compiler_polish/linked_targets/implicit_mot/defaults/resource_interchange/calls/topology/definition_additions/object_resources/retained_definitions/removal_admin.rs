//! Actual localized native forms, reusing the owned router/marker fixture.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, time::Duration};

const CASE: &str = "teletext-unused-copy-removal";
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

async fn verify(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    complete: &BrowserComplete,
    expected: &BankExpected,
) -> TeletextDesignRevision {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, concept.head().id);
    assert_eq!(complete.revision, 11);
    assert_eq!(complete.digest, concept.digest());
    assert_eq!(complete.new_ids, [33, 34, 35, 36]);
    let head = fixture
        .state
        .application
        .teletext_design_head(complete.design_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, 11);
    assert!(!head.deleted);
    let mut final_revision = None;
    for revision in [5, 7, 9, 11] {
        let stored = fixture
            .state
            .application
            .teletext_design_revision(head.id, revision, designs::budget().max_serialized_bytes)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.digest, concept.digest());
        Box::pin(assert_bank_revision(fixture, flow, &stored, expected)).await;
        final_revision = Some(stored);
    }
    assert!(fixture
        .state
        .application
        .teletext_design_revision(head.id, 12, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    final_revision.unwrap()
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
            stored.revision,
            stored.digest.clone(),
            stored.revision,
            Some(flow.pending[0].id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!("verified browser concept must admit explicit publication")
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
                expected_design_head: stored.revision,
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
async fn unused_copy_removal_headed_localized_forms_exact_undo_publication_and_cold_reopen() {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(directory.is_dir());
    let expected = Box::pin(qualify_banks(false, false)).await;
    let fixture = Fixture::new_persistent().await;
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    let (mut fixture, flow, concept) = Box::pin(setup_retained_on(&expected, fixture)).await;
    assert_eq!(concept.head().revision, 3);
    let before = resource_bank_admin::Retained::capture(&fixture).await;
    let marker = directory.join("browser-complete.json");
    assert!(!marker.exists());
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let ready = json!({"probe_version":1,"case":CASE,"base_url":origin.base_url(),
            "channel_id":1,"design_id":concept.head().id,"revision":3,"digest":concept.digest(),
            "source_object":new_target(1),"new_ids":[33,34,35,36],
            "marker_file":marker,"timeout_seconds":600});
        live_client::write_new(&ready_path, &serde_json::to_vec_pretty(&ready).unwrap()).unwrap();
        let complete: BrowserComplete = tokio::time::timeout(
            Duration::from_mins(10),
            resource_bank_admin::marker(directory, "browser-complete.json"),
        )
        .await
        .expect("owned browser proof must finish within600seconds")
        .expect("closed browser completion marker");
        let stored = Box::pin(verify(&fixture, &flow, &concept, &complete, &expected)).await;
        assert_original_pointers(&fixture, &flow).await;
        before.assert_unchanged(&fixture, &flow).await;
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
    Box::pin(assert_bank_revision(&fixture, &flow, &stored, &expected)).await;
    let accepted = Box::pin(publish(&fixture, &flow, &stored)).await;
    assert_bank_output(accepted.service(), &expected);
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_revision(&fixture, &flow, &stored, &expected)).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    before.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    live_client::write_new(&directory.join("native-removal-success.json"), &serde_json::to_vec_pretty(&json!({
        "probe_version":1,"case":CASE,"browser":complete,"four_exact_undo_native_model_units_preview":true,
        "original_lifecycle_metadata_sources_night_and_package_preserved":true,
        "separate_explicit_publication_and_cold_reopen":true,"actual_qtm_or_client_or_full9A_claim":false,
    })).unwrap()).unwrap();
    fixture.state.pool.close().await;
}

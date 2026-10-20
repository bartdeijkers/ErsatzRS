//! The existing owned `LiveOrigin` proves explicit current-draft choices in both browsers.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path, time::Duration};
const CASE: &str = "teletext-explicit-object-selection";
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct BrowserComplete {
    probe_version: u8,
    case: String,
    design_id: i64,
    revision: i64,
    digest: String,
}
fn ready(
    origin: &live_client::LiveOrigin,
    concept: &SavedEditorialTeletextDesign,
    marker: &Path,
) -> Value {
    json!({"probe_version":1,"case":CASE,"base_url":origin.base_url(),"channel_id":1,
        "design_id":concept.head().id,"revision":4,"digest":concept.digest(),
        "original_page":seed::target(20).resource().unwrap().page(),"selected_page":resources()[0].page(),
        "caller":seed::target(20),"original_target":seed::target(21),"selected_target":oracle::target(),
        "marker_file":marker,"timeout_seconds":600})
}
async fn verify(
    fixture: &Fixture,
    flow: &Flow,
    complete: &BrowserComplete,
    announced: &BankExpected,
    selected: &BankExpected,
) -> TeletextDesignRevision {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, flow.saved[0].head().id);
    assert_eq!(complete.revision, 7);
    let head = fixture
        .state
        .application
        .teletext_design_head(complete.design_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, 7);
    let mut last = None;
    for (revision, expected, schema) in [
        (4, announced, 26),
        (5, selected, 27),
        (6, announced, 26),
        (7, selected, 27),
    ] {
        let stored = fixture
            .state
            .application
            .teletext_design_revision(
                complete.design_id,
                revision,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .unwrap();
        Box::pin(assert_bank_revision_with_schema(
            fixture, flow, &stored, expected, schema,
        ))
        .await;
        last = Some(stored);
    }
    let stored = last.unwrap();
    assert_eq!(stored.digest, complete.digest);
    assert_original_pointers(fixture, flow).await;
    stored
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires actual headed browsers, matched assets and a fresh selection ready path"]
async fn selected_pages_and_explicit_calls_headed_localized_forms_preview_publication_and_cold_reopen(
) {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(directory.is_dir());
    let [before, announced, selected, _] = Box::pin(oracle::all()).await;
    let fixture = Fixture::new_persistent().await;
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    let (mut fixture, flow, concept) = Box::pin(setup_on(&before, oracle::original, fixture)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = Box::pin(announce(&fixture, &concept)).await;
    assert_eq!(concept.head().revision, 4);
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
        .expect("owned browser proof completes within600seconds")
        .expect("closed browser marker");
        let stored = Box::pin(verify(&fixture, &flow, &complete, &announced, &selected)).await;
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
    closed.expect("owned selection router closes");
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(verify(&fixture, &flow, &complete, &announced, &selected)).await,
        stored
    );
    let accepted = super::super::super::admin::publish(&fixture, &flow, &stored).await;
    assert_bank_output(accepted.service(), &selected);
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_revision_with_schema(
        &fixture, &flow, &stored, &selected, 27,
    ))
    .await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    live_client::write_new(&directory.join("native-object-selection-success.json"),&serde_json::to_vec_pretty(&json!({
        "probe_version":1,"case":CASE,"browser":complete,"three_complete_models_units_rasters_exact":true,
        "original_lifecycle_metadata_sources_night_package_and_config_preserved":true,
        "separate_explicit_publication_and_cold_reopen":true,"actual_qtm_or_client_or_full9A_claim":false,
    })).unwrap()).unwrap();
    fixture.state.pool.close().await;
}

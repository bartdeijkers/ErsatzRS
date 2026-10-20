//! Owned actual headed forms use the existing LiveOrigin/ready/marker boundary.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path, time::Duration};
const CASE: &str = "teletext-explicit-bank-dependencies";
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
        "design_id":concept.head().id,"revision":3,"digest":concept.digest(),
        "resources":[oracle::resource(false),oracle::resource(true)],
        "displays":([124,125,126].map(|page|channel_documents::address(page,32))),
        "marker_file":marker,"timeout_seconds":600})
}
async fn verify(
    fixture: &Fixture,
    flow: &Flow,
    complete: &BrowserComplete,
    expected: &[BankExpected],
) -> TeletextDesignRevision {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, flow.saved[0].head().id);
    assert_eq!(complete.revision, 9);
    let final_stored = current(fixture, complete.design_id, 9).await;
    assert_eq!(final_stored.digest, complete.digest);
    for (index, expected) in expected.iter().enumerate() {
        let revision = i64::try_from(index).unwrap() + 4;
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
            fixture,
            flow,
            &stored,
            expected,
            if index >= 2 { 26 } else { 25 },
        ))
        .await;
    }
    assert_original_pointers(fixture, flow).await;
    final_stored
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires actual headed browsers, matched assets and a fresh explicit ready path"]
async fn explicit_bank_creation_and_dependencies_headed_localized_forms_preview_publication_and_cold_reopen(
) {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(directory.is_dir());
    let expected = Box::pin(oracle::qualify_all()).await;
    let before = Box::pin(qualify_dormant()).await;
    let fixture = Fixture::new_persistent().await;
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    let (mut fixture, flow, concept) =
        Box::pin(retained_definitions::setup_retained_on(&before, fixture)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
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
    assert_eq!(
        Box::pin(verify(&fixture, &flow, &complete, &expected)).await,
        stored
    );
    let accepted = publish(&fixture, &flow, &stored).await;
    assert_bank_output(accepted.service(), &expected[5]);
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_revision_with_schema(
        &fixture,
        &flow,
        &stored,
        &expected[5],
        26,
    ))
    .await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    live_client::write_new(&directory.join("native-explicit-bank-success.json"),&serde_json::to_vec_pretty(&json!({
        "probe_version":1,"case":CASE,"browser":complete,"six_complete_models_units_rasters_exact":true,
        "original_lifecycle_metadata_sources_night_and_package_preserved":true,
        "separate_explicit_publication_and_cold_reopen":true,"actual_qtm_or_client_or_full9A_claim":false,
    })).unwrap()).unwrap();
    fixture.state.pool.close().await;
}

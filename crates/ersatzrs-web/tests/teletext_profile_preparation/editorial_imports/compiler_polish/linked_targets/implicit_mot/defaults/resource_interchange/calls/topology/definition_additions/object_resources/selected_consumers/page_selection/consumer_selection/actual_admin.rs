//! Reuse the owned localhost/both-headed-browser boundary for Main and Local callers.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path, time::Duration};
const CASE: &str = "teletext-main-local-object-selection";

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct BrowserComplete {
    probe_version: u8,
    case: String,
    design_id: i64,
    revision: i64,
    digest: String,
}
fn ready(origin: &live_client::LiveOrigin, flow: &Flow, marker: &Path) -> Value {
    json!({"probe_version":1,"case":CASE,"base_url":origin.base_url(),"channel_id":1,
        "design_id":flow.originals[0].design_id,"revision":1,"digest":flow.originals[0].digest,
        "original_page":seed::target(20).resource().unwrap().page(),"selected_page":resources()[0].page(),
        "caller":active(),"original_target":old_passive(),"selected_target":oracle::target(),
        "main_original_target":seed::target(20),"main_selected_target":active(),
        "marker_file":marker,"timeout_seconds":600})
}
async fn verify(
    fixture: &Fixture,
    flow: &Flow,
    complete: &BrowserComplete,
    original: &BankExpected,
    selected: &BankExpected,
) -> TeletextDesignRevision {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, flow.originals[0].design_id);
    assert_eq!(complete.revision, 4);
    let head = fixture
        .state
        .application
        .teletext_design_head(complete.design_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, 4);
    let mut last = None;
    for (revision, expected, on) in [
        (1, original, false),
        (2, selected, true),
        (3, original, false),
        (4, selected, true),
    ] {
        last = Some(
            Box::pin(check(
                fixture,
                flow,
                complete.design_id,
                revision,
                expected,
                on,
            ))
            .await,
        );
    }
    let stored = last.unwrap();
    assert_eq!(stored.digest, complete.digest);
    assert_original_pointers(fixture, flow).await;
    stored
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both actual headed browsers, matched assets and a fresh Main/Local ready path"]
async fn main_local_consumer_choices_headed_forms_publish_and_cold_reopen() {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(directory.is_dir());
    let [original, selected, _] = Box::pin(source::all()).await;
    let fixture = Fixture::new_persistent().await;
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        source::original,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_bank_output(flow.accepted[0].service(), &original);
    let marker = directory.join("browser-complete.json");
    assert!(!marker.exists());
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
        .expect("owned browser proof completes within600seconds")
        .expect("closed browser marker");
        let stored = Box::pin(verify(&fixture, &flow, &complete, &original, &selected)).await;
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
    closed.expect("owned Main/Local browser router closes");
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(verify(&fixture, &flow, &complete, &original, &selected)).await,
        stored
    );
    let accepted = super::super::super::admin::publish(&fixture, &flow, &stored).await;
    assert_bank_output(accepted.service(), &selected);
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, stored.design_id, 4, &selected, true)).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    live_client::write_new(&directory.join("native-main-local-selection-success.json"),
        &serde_json::to_vec_pretty(&json!({"probe_version":1,"case":CASE,"browser":complete,
            "complete_models_units_rasters_exact":true,"immutable_source_metadata_config_packages_night_preserved":true,
            "separate_explicit_publication_and_cold_reopen":true,"actual_editor_stream_receiver_or_full9A_claim":false})).unwrap()).unwrap();
    fixture.state.pool.close().await;
}

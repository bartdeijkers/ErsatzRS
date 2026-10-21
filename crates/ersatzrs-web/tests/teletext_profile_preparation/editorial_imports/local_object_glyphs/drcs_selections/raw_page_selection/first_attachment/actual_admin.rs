//! Existing owned headed-browser protocol for retained-role first attachment.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path, time::Duration};
const CASE: &str = "teletext-first-retained-role-attachment";

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
    let roles = json!([{"role":"drcs","selected_page":source::target(TeletextX27LinkRole::Drcs),"mask":"0003"}]);
    json!({"probe_version":1,"case":CASE,"base_url":origin.base_url(),"channel_id":1,
        "design_id":flow.originals[0].design_id,"revision":1,"digest":flow.originals[0].digest,
        "selected_display":{"page":227,"subcode":32},"preserved_display":{"page":125,"subcode":32},
        "raw_roles":roles,"marker_file":marker,"timeout_seconds":600})
}
async fn revision(
    fixture: &Fixture,
    flow: &Flow,
    revision: i64,
    expected: &EncodedTeletextAuthoredService,
    selected: bool,
) -> TeletextDesignRevision {
    let stored = fixture
        .state
        .application
        .teletext_design_revision(
            flow.originals[0].design_id,
            revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    if selected {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 31);
        // The form stores one exact original Unselected Normal DRCS attachment.
        let mut expected_choices = choices(flow.accepted[0].service())
            .into_iter()
            .filter(|saved| saved.change().role() == TeletextX27LinkRole::Drcs)
            .collect::<Vec<_>>();
        expected_choices.sort_by_key(|saved| saved.change().role().wire_code());
        assert_eq!(overlay.object_page_selections(), expected_choices);
        assert!(overlay.resource_object_program_overlays().is_empty());
        let mut document: Value = serde_json::from_slice(&stored.document).unwrap();
        document
            .as_object_mut()
            .unwrap()
            .remove("editorial_overlay");
        assert_eq!(
            document,
            serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
        );
    } else {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
    }
    let current = reload_original_teletext_design_revision(
        &fixture.state.application,
        stored.design_id,
        stored.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.transmissions(), original.transmissions());
    assert_eq!(current.sources().len(), original.sources().len());
    for (actual, expected) in current.sources().iter().zip(original.sources()) {
        assert_eq!(actual.bytes(), expected.bytes());
        assert_eq!(actual.authoring(), expected.authoring());
    }
    assert_eq!(
        current.producers()[0].original(),
        original.producers()[0].original()
    );
    checks::same(current.encoded().unwrap(), expected);
    let preview = ersatzrs_web::host::teletext_authoring::preview_original_teletext_design(
        &fixture.state.application,
        ersatzrs_web::host::teletext_authoring::TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: stored.design_id,
            revision: stored.revision,
            owner: Some(source::display()),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    let is_normal =
        |choice: &ersatzrs_web::host::teletext_authoring::TeletextObjectPageRoleObservation| {
            choice.display == source::display() && choice.role == TeletextResourceKind::Drcs
        };
    assert_eq!(
        preview
            .object_choices()
            .unselected_roles
            .iter()
            .any(is_normal),
        !selected
    );
    assert_eq!(
        preview
            .object_choices()
            .restorable_attachments
            .iter()
            .any(is_normal),
        selected
    );
    stored
}
async fn verify(
    fixture: &Fixture,
    flow: &Flow,
    complete: &BrowserComplete,
    target: &Flow,
) -> TeletextDesignRevision {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, flow.originals[0].design_id);
    assert_eq!(complete.revision, 4);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(complete.design_id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        4
    );
    let mut last = None;
    for (number, selected) in [(1, false), (2, true), (3, false), (4, true)] {
        let expected = if selected {
            target.accepted[0].service()
        } else {
            flow.accepted[0].service()
        };
        last = Some(Box::pin(revision(fixture, flow, number, expected, selected)).await);
    }
    let stored = last.unwrap();
    assert_eq!(stored.digest, complete.digest);
    assert_original_pointers(fixture, flow).await;
    stored
}
async fn browsers(
    fixture: &Fixture,
    flow: &Flow,
    ready_path: &Path,
    target: &Flow,
) -> (BrowserComplete, TeletextDesignRevision) {
    let directory = ready_path.parent().unwrap();
    let marker = directory.join("browser-complete.json");
    assert!(!marker.exists());
    let mut origin = live_client::LiveOrigin::start(fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        live_client::write_new(
            ready_path,
            &serde_json::to_vec_pretty(&ready(&origin, flow, &marker)).unwrap(),
        )
        .unwrap();
        let complete: BrowserComplete = tokio::time::timeout(
            Duration::from_mins(10),
            resource_bank_admin::marker(directory, "browser-complete.json"),
        )
        .await
        .expect("owned headed browser proof completes within600seconds")
        .expect("closed browser marker");
        let stored = Box::pin(verify(fixture, flow, &complete, target)).await;
        (complete, stored)
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let value = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    closed.expect("owned DRCS selection browser router closes");
    value
}
async fn publish(
    fixture: &Fixture,
    stored: &TeletextDesignRevision,
    previous: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    use ersatzrs_core::repositories::TeletextPublicationMutation;
    let pending =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            stored.design_id,
            stored.revision,
            stored.digest.clone(),
            stored.revision,
            Some(previous.generation().request.id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!("separate publication uses current request CAS")
    };
    generations::assert_same_output(&recover(fixture, 1).await, previous);
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: stored.revision,
                expected_generation_id: Some(previous.generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}
async fn finish(
    mut fixture: Fixture,
    flow: &Flow,
    stored: &TeletextDesignRevision,
    target: &Flow,
    retained: &resource_bank_admin::Retained,
) {
    let accepted = publish(&fixture, stored, &flow.accepted[0]).await;
    checks::same(accepted.service(), target.accepted[0].service());
    let result = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: stored.design_id,
            revision: stored.revision,
            design_digest: stored.digest.clone(),
            expected_design_head: stored.revision,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = result else {
        panic!("fresh current selected export succeeds")
    };
    let current = receipt(&fixture, &exported).await;
    let undone = lifecycle::restore(&fixture, &current, vec![TeletextX27LinkRole::Drcs]).await;
    assert_eq!(undone.head().revision, 5);
    Box::pin(checks::concept(
        &fixture,
        flow,
        &undone,
        flow.accepted[0].service(),
        &[],
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    let root = super::super::lifecycle::publish(&fixture, &undone, &accepted).await;
    checks::same(root.service(), flow.accepted[0].service());
    fixture = fixture.reopen().await;
    Box::pin(checks::concept(
        &fixture,
        flow,
        &undone,
        flow.accepted[0].service(),
        &[],
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, &root);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both actual headed browsers, matched assets and a fresh DRCS selection ready path"]
async fn retained_first_attachment_admin_roundtrip() {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(directory.is_dir());
    let (target_fixture, target) = Box::pin(setup_with_fixture_policy_and_writer(
        source::normal,
        Fixture::new_persistent().await,
        source::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    target_fixture.state.pool.close().await;
    let fixture = Fixture::new_persistent().await;
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        source::absent,
        fixture,
        source::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (complete, stored) = Box::pin(browsers(&fixture, &flow, &ready_path, &target)).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(verify(&fixture, &flow, &complete, &target)).await,
        stored
    );
    Box::pin(finish(fixture, &flow, &stored, &target, &retained)).await;
    live_client::write_new(
        &directory.join("native-first-attachment-success.json"),
        &serde_json::to_vec_pretty(&json!({"probe_version":1,"case":CASE,"browser":complete,
            "complete_models_units_rasters_and_v31_exact":true,"browser_exact_root_undo":true,
            "separate_current_cas_publication_typed_undo_and_cold_reopen":true,
            "immutable_source_metadata_config_packages_night_preserved":true,
            "actual_editor_stream_receiver_or_full9A_claim":false}))
        .unwrap(),
    )
    .unwrap();
}

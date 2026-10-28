//! Existing owned router and completion markers, exercising real native forms.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, time::Duration};

const CASE: &str = "teletext-secondary-default";
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Complete {
    probe_version: u8,
    case: String,
    design_id: i64,
    revision: i64,
    digest: String,
}

async fn verify(fixture: &Fixture, flow: &Flow, complete: &Complete) {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, flow.originals[0].design_id);
    assert_eq!(complete.revision, 13);
    assert_eq!(complete.digest, flow.originals[0].digest);
    let head = fixture
        .state
        .application
        .teletext_design_head(complete.design_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, 13);
    for revision in 2..=13 {
        let stored = fixture
            .state
            .application
            .teletext_design_revision(head.id, revision, designs::budget().max_serialized_bytes)
            .await
            .unwrap()
            .unwrap();
        let decoded =
            decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
        let expected = match (revision - 2) % 3 {
            0 => Some(agreement(None)),
            1 => Some(agreement(Some(0x37))),
            _ => None,
        };
        assert_eq!(
            decoded
                .secondary_default_agreements()
                .first()
                .map(|v| v.agreement()),
            expected
        );
        assert_eq!(
            decoded.sources()[0].bytes(),
            decode_teletext_design_documents(&flow.originals[0].document, designs::budget())
                .unwrap()
                .sources()[0]
                .bytes()
        );
        if expected.is_none() {
            assert_eq!(stored.document, flow.originals[0].document);
            assert_eq!(stored.digest, flow.originals[0].digest);
        }
        let restored = reload_original_teletext_design_revision(
            &fixture.state.application,
            head.id,
            revision,
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            restored.producers()[0].secondary_default_agreement(),
            expected
        );
    }
    assert_original_pointers_with_width(fixture, flow, 40 * 12).await;
    assert_eq!(designs::config(fixture).await, flow.config);
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        flow.calls
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires actual headed browsers, matched assets and a fresh explicit ready path"]
async fn secondary_default_headed_localized_native_forms_reopen_and_exact_context_undo() {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(directory.is_dir());
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    let (mut fixture, flow) = Box::pin(setup_with_fixture(response, fixture)).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let ready = json!({"probe_version":1,"case":CASE,"base_url":origin.base_url(),
            "channel_id":1,"design_id":flow.originals[0].design_id,"revision":1,
            "digest":flow.originals[0].digest,"marker_file":directory.join("browser-complete.json"),
            "timeout_seconds":600});
        live_client::write_new(&ready_path, &serde_json::to_vec_pretty(&ready).unwrap()).unwrap();
        let complete: Complete = tokio::time::timeout(
            Duration::from_mins(10),
            resource_bank_admin::marker(directory, "browser-complete.json"),
        )
        .await
        .expect("owned browser trial must finish within600seconds")
        .expect("closed bounded completion marker");
        Box::pin(verify(&fixture, &flow, &complete)).await;
        complete
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let complete = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    closed.expect("owned headed router must close");
    fixture = fixture.reopen().await;
    Box::pin(verify(&fixture, &flow, &complete)).await;
    live_client::write_new(
        &directory.join("native-secondary-default-success.json"),
        &serde_json::to_vec_pretty(&json!({"probe_version":1,"case":CASE,
            "browser":complete,"twelve_choices_and_four_exact_undo":true,
            "cold_reopen_originals_night_config_and_live_pointers_preserved":true,
            "actual_qtm_or_receiver_claim":false}))
        .unwrap(),
    )
    .unwrap();
    fixture.state.pool.close().await;
}

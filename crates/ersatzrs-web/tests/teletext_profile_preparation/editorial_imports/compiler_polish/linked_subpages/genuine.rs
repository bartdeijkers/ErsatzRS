//! Same live receipt: actual QTM save, concept, publication and ordinary TCP.

use super::super::super::{client_capture, ordinary_stream_flow};
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::PathBuf};

#[path = "genuine/inputs.rs"]
mod inputs;

fn assert_accepted(flow: &Flow, accepted: &[AcceptedTeletextOutputCandidate; 2]) {
    assert_service(flow, accepted[0].service(), true);
    assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
    assert_eq!(accepted[1].selection(), flow.accepted[1].selection());
    assert_eq!(
        accepted[1].service().declared(),
        flow.accepted[1].service().declared()
    );
    for (before, after) in flow.accepted[1]
        .service()
        .pages()
        .iter()
        .zip(accepted[1].service().pages())
    {
        assert_eq!(before.data_units(), after.data_units());
    }
    let day = &accepted[0].selection().bindings[0];
    let night = &accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
    assert_eq!(accepted[1].service().compiler_drcs_resources().len(), 0);
}

async fn publish_actual(
    mut fixture: Fixture,
    flow: &Flow,
    directory: &Path,
) -> (Fixture, [AcceptedTeletextOutputCandidate; 2], Value) {
    let (request, proof) = inputs::request(flow, directory);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, flow).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_service(flow, restored.encoded().unwrap(), true);
    let previews: Vec<_> = [124, 125]
        .into_iter()
        .map(|page| external_editor_probe::preview_page(restored.encoded().unwrap(), page))
        .collect();
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, flow).await;
    let day = publish_and_accept(&fixture, flow, &concept).await;
    let accepted = [day, recover(&fixture, 2).await];
    assert_accepted(flow, &accepted);
    for (page, preview) in [124, 125].into_iter().zip(previews) {
        assert_eq!(
            external_editor_probe::preview_page(accepted[0].service(), page),
            preview
        );
    }
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let asset = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| asset.name == "drcs-set-normal-2a0.tti")
        .unwrap();
    let TeletextExportAssetScope::DrcsTtiSet { resource_names } = &asset.scope else {
        panic!("mixed indexed set")
    };
    assert_eq!(resource_names.len(), 2);
    assert!(resource_names.iter().all(Option::is_some));
    polish_delivery::write_json(
        directory,
        "native-proof.json",
        &json!({"probe_version":1,
        "scope":"same live export and genuine pinned QTM Base edit; native concept/preview/explicit publication/reopen and mixed indexed reexport",
        "editor_proof":proof,"design_id":concept.head().id,"revision":2,"digest":concept.digest(),
        "generation_id":accepted[0].generation().id,"shared_two_page_previews_equal":true,
        "mixed_bank":asset,"generated_normal_s1":1,"slot":0,"mask":MASK,
        "original_authored_banks_byte_exact":true,"night_unchanged":true,
        "ordinary_delivery_proven":false,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}),
    );
    (fixture, accepted, proof)
}

async fn check_state(
    fixture: &Fixture,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    directory: &Path,
    hashes: &[(String, String)],
) {
    retained.assert_unchanged(fixture, flow).await;
    assert_pointers(fixture, &accepted[0], &accepted[1]).await;
    assert_service(flow, recover(fixture, 1).await.service(), true);
    assert_eq!(
        recover(fixture, 2).await.service().declared(),
        flow.accepted[1].service().declared()
    );
    assert_eq!(inputs::hashes(flow, directory), hashes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires pinned actual QTM save after this live fresh export and FFmpeg9/ffprobe/TSDuck; waits bounded1200s; independent/client acceptance remains separate"]
async fn actual_qtm_linked_s1_concept_serves_two_ordinary_channels() {
    let directory = PathBuf::from(client_capture::selected("ERSATZRS_TELETEXT_LINKED_S1_DIR"));
    std::fs::create_dir(&directory).expect("live linked-S1 proof destination must be new");
    live_updates::live_tcp::terminal_trace(&directory).unwrap();
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        occupied_response,
        fixture,
        encoding,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    inputs::await_editor(&fixture, &flow, &directory).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let hashes = inputs::hashes(&flow, &directory);
    let (mut fixture, accepted, proof) = Box::pin(publish_actual(fixture, &flow, &directory)).await;
    check_state(&fixture, &flow, &retained, &accepted, &directory, &hashes).await;
    compiler_polish_delivery::assert_source(&fixture, &directory, &source);
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let (day, night) = tokio::join!(drcs_bitmap_delivery::capture_tcp(origin.url(1)), drcs_bitmap_delivery::capture_tcp(origin.url(2)));
        let day = polish_delivery::verify_capture(&directory, &tools, &source, 1, &day, &accepted[0]).await;
        let night = polish_delivery::verify_capture(&directory, &tools, &source, 2, &night, &accepted[1]).await;
        check_state(&fixture, &flow, &retained, &accepted, &directory, &hashes).await;
        compiler_polish_delivery::assert_source(&fixture, &directory, &source);
        polish_delivery::write_json(&directory,"stream-proof.json",&json!({"probe_version":1,
            "route":"/iptv/hls-direct/{channel}.ts","transport":"actual owned application loopback TCP host",
            "scope":"genuine live linked-S1 QTM edit, shared native preview/publication and cold two-profile ordinary delivery",
            "source_sha256":live_client::hash(&source),"channels":[day,night],"editor_proof":proof,"inputs":hashes,
            "generated_resource":{"page":"2A0","s1":1,"slot":0},"authored_resources":["2A0/S0","1A1/S0"],
            "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}));
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned linked-S1 origin must close");
    cleaned.expect("both response producers must release capacity");
    fixture = fixture.reopen().await;
    check_state(&fixture, &flow, &retained, &accepted, &directory, &hashes).await;
    compiler_polish_delivery::assert_source(&fixture, &directory, &source);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "success.json",
        &json!({"probe_version":1,
        "genuine_qtm_native_preview_publication_ordinary_av_transport_model_reopen":true,"input_hashes":hashes,
        "origin_closed":true,"capacity_released":true,"independent_decoding_proven":false,"actual_client_proven":false,"full_engine_acceptance":false}),
    );
}

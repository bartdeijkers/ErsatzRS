//! One fresh live receipt joins four actual saves to native publication and TCP.

use super::super::super::{client_capture, ordinary_stream_flow};
use super::*;
use futures_util::FutureExt as _;
use std::{
    panic::AssertUnwindSafe,
    path::{Path, PathBuf},
};

async fn check_state(
    published: &Published,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
    directory: &Path,
    hashes: &[(String, String)],
) {
    Box::pin(assert_retained(published, flow, retained)).await;
    assert_eq!(inputs::hashes(flow, directory), hashes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires four actual pinned QTM saves after this fresh live export and FFmpeg9/ffprobe/TSDuck; bounded1200s; independent/client acceptance remains separate"]
async fn actual_qtm_four_romanian_variants_concept_serves_two_ordinary_channels() {
    let directory = PathBuf::from(client_capture::selected("ERSATZRS_TELETEXT_ROMANIAN_DIR"));
    std::fs::create_dir(&directory).expect("Romanian live proof destination must be new");
    live_updates::live_tcp::terminal_trace(&directory).unwrap();
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        input::response,
        fixture,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    oracle::assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    inputs::await_editor(&fixture, &flow, &directory).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let hashes = inputs::hashes(&flow, &directory);
    let (request, proofs) = inputs::request(&flow, &directory);
    let mut published = Box::pin(publish(fixture, &flow, request)).await;
    check_state(&published, &flow, &retained, &directory, &hashes).await;
    polish_delivery::write_json(
        &directory,
        "native-proof.json",
        &json!({"probe_version":1,
        "scope":"same live fresh receipt, four reported pinned QTM Base edits, native concept/shared previews/explicit publication/reopen and actual indexed reexport",
        "editor_proofs":proofs,"input_hashes":hashes,
        "design_id":published.concept.head().id,"revision":2,"digest":published.concept.digest(),
        "generation_id":published.accepted[0].generation().id,"request_id":published.accepted[0].generation().request.id,
        "shared_four_page_previews_equal":true,"indexed_assets":published.exported.sidecar().project.assets,
        "generated_resource":{"page":"1A0","s1":0,"slots":[4,5,6,7]},
        "variants":(124..128).map(|page|json!({"page":page,"primary":31,"code":code(page),"slot":slot(page),"mask":oracle::mask(page,true),"original_d5":variant(page)%2==1,"physical_drcs_d5":false})).collect::<Vec<_>>(),
        "night_original_unchanged":true,"ordinary_delivery_proven":false,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}),
    );
    compiler_polish_delivery::assert_source(&published.fixture, &directory, &source);
    ordinary_stream_flow::reset_source_start(&published.fixture).await;
    let mut origin = live_client::LiveOrigin::start(&published.fixture)
        .await
        .unwrap();
    let outcome = AssertUnwindSafe(async {
        let (day, night) = tokio::join!(
            drcs_bitmap_delivery::capture_tcp(origin.url(1)),
            drcs_bitmap_delivery::capture_tcp(origin.url(2))
        );
        let day = polish_delivery::verify_capture(
            &directory,
            &tools,
            &source,
            1,
            &day,
            &published.accepted[0],
        )
        .await;
        let night = polish_delivery::verify_capture(
            &directory,
            &tools,
            &source,
            2,
            &night,
            &published.accepted[1],
        )
        .await;
        check_state(&published, &flow, &retained, &directory, &hashes).await;
        compiler_polish_delivery::assert_source(&published.fixture, &directory, &source);
        polish_delivery::write_json(
            &directory,
            "stream-proof.json",
            &json!({"probe_version":1,
                "route":"/iptv/hls-direct/{channel}.ts","transport":"actual owned application loopback TCP host",
                "scope":"four genuine same-live QTM saves, native shared preview/publication and cold two-profile ordinary delivery",
                "source_file":"source.ts","source_sha256":live_client::hash(&source),"source_bytes":source.len(),
                "channels":[day,night],"editor_proofs":proofs,"input_hashes":hashes,
                "generated_resource":{"page":"1A0","s1":0,"slots":[4,5,6,7]},"authored_resource_count":0,
                "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}),
        );
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        published.fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned Romanian origin must close");
    cleaned.expect("both response producers must release capacity");
    published.fixture = published.fixture.reopen().await;
    check_state(&published, &flow, &retained, &directory, &hashes).await;
    compiler_polish_delivery::assert_source(&published.fixture, &directory, &source);
    assert!(published.fixture.playout.try_recv().is_err());
    published.fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "success.json",
        &json!({"probe_version":1,
        "genuine_four_qtm_native_preview_publication_ordinary_av_transport_model_reopen":true,
        "input_hashes":hashes,"origin_closed":true,"capacity_released":true,
        "independent_decoding_proven":false,"actual_client_proven":false,"full_engine_acceptance":false}),
    );
}

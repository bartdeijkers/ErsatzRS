//! Exact genuine underlined Polish edit to cold ordinary TCP on two profiles.
//! Native AV/transport/model proof does not grant independent/client acceptance.

use super::super::{client_capture, ordinary_stream_flow};
use super::compiler_polish_underline as underline;
use super::*;
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{panic::AssertUnwindSafe, path::PathBuf};

fn assert_profiles(accepted: &[AcceptedTeletextOutputCandidate; 2]) {
    let day = &accepted[0].selection().bindings[0];
    let night = &accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
    assert!(accepted[1].service().compiler_drcs_resources().is_empty());
    assert!(accepted[1]
        .service()
        .pages()
        .iter()
        .all(|page| page.resource_header().is_none()));
}

fn assert_theme(candidate: &AcceptedTeletextOutputCandidate, foreground: [u8; 3]) {
    // Existing native G0 Latin A golden; page125 is unchanged and has no DRCS.
    let mask = [0_u16, 0xf0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];
    let service = candidate.service();
    let left = usize::from(
        polish_carrier::composition(service)
            .display_geometry()
            .left_width(),
    );
    let raster = external_editor_probe::preview_page(service, 125);
    for (row, mask) in mask.into_iter().enumerate() {
        for x in 0..12 {
            let expected = if mask & (1 << (11 - x)) == 0 {
                [0, 0, 0]
            } else {
                foreground
            };
            assert!(
                matches!(raster.pixel((left+8)*12+x,10+row), Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get() == expected),
                "page125 G0 A pixel{x},{row}"
            );
        }
    }
}

fn write_previews(
    directory: &Path,
    flow: &Flow,
    preview: &TeletextRaster,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
) {
    assert_eq!(
        external_editor_probe::preview_page(accepted[0].service(), 124),
        *preview
    );
    assert_eq!(
        external_editor_probe::preview_page(accepted[0].service(), 125),
        external_editor_probe::preview_page(flow.accepted[0].service(), 125)
    );
    assert_eq!(
        external_editor_probe::preview_page(accepted[1].service(), 125),
        external_editor_probe::preview_page(flow.accepted[1].service(), 125)
    );
    assert_theme(&accepted[0], [15, 0, 0]);
    assert_theme(&accepted[1], [0, 0, 15]);
    polish_delivery::write_json(
        directory,
        "native-preview.json",
        &json!({
        "probe_version":1,"scope":"complete native shared preview equality and literal underlined wide Polish mask; no browser image claim",
        "day124":{"width":preview.width(),"height":preview.height(),"concept_equals_published":true,"wide_mask_pixels_checked":240},
        "generated_normal_resource_page_hex":"1A0","generated_s1":0,"generated_slot":1,"original_d5":true,"physical_drcs_d5":false,
        "page125":"unchanged G0 Latin A, fixed 120pixels per channel","page125_foreground_rgb4_by_channel":[[15,0,0],[0,0,15]],
        "night_original_unchanged":true,"night_generated_resource_count":0}),
    );
}

async fn publish_actual(
    fixture: &mut Fixture,
    flow: &Flow,
    retained: &polish_delivery::Retained,
    actual: &Path,
    directory: &Path,
) -> ([AcceptedTeletextOutputCandidate; 2], Value) {
    let (request, proof) = compiler_polish::actual_request(flow, actual);
    retained
        .assert_unchanged(fixture, flow, &flow.accepted, actual)
        .await;
    let concept = import(fixture, request).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(fixture, flow).await;
    retained
        .assert_unchanged(fixture, flow, &flow.accepted, actual)
        .await;
    let preview = underline::assert_restored(fixture, flow, &concept).await;
    let day = publish_and_accept(fixture, flow, &concept).await;
    let accepted = [day, recover(fixture, 2).await];
    assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
    underline::assert_concept(flow, accepted[0].service());
    compiler_polish::assert_pointers(fixture, &accepted[0], &accepted[1]).await;
    assert_profiles(&accepted);
    write_previews(directory, flow, &preview, &accepted);
    retained
        .assert_unchanged(fixture, flow, &accepted, actual)
        .await;
    (accepted, proof)
}

async fn capture_channels(
    directory: &Path,
    origin: &live_client::LiveOrigin,
    tools: &drcs_bitmap_delivery::Tools,
    source: &[u8],
    accepted: &[AcceptedTeletextOutputCandidate; 2],
) -> Vec<Value> {
    let (day, night) = tokio::join!(
        drcs_bitmap_delivery::capture_tcp(origin.url(1)),
        drcs_bitmap_delivery::capture_tcp(origin.url(2))
    );
    let day =
        polish_delivery::verify_capture(directory, tools, source, 1, &day, &accepted[0]).await;
    let night =
        polish_delivery::verify_capture(directory, tools, source, 2, &night, &accepted[1]).await;
    vec![day, night]
}

async fn assert_retained(
    fixture: &mut Fixture,
    flow: &Flow,
    retained: &polish_delivery::Retained,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    actual: &Path,
    hashes: &[(String, String)],
    media: (&Path, &[u8]),
) {
    retained
        .assert_unchanged(fixture, flow, accepted, actual)
        .await;
    compiler_polish::assert_pointers(fixture, &accepted[0], &accepted[1]).await;
    underline::assert_concept(flow, accepted[0].service());
    assert_profiles(accepted);
    assert_theme(&accepted[0], [15, 0, 0]);
    assert_theme(&accepted[1], [0, 0, 15]);
    assert_eq!(compiler_polish::actual_input_hashes(flow, actual), hashes);
    compiler_polish_delivery::assert_source(fixture, media.0, media.1);
}

/// Select the fresh underlined QTM input corpus with the existing compiler
/// selector and a create-new output child with the existing delivery selector.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires fresh underlined no-bank actual QTM save and FFmpeg9/ffprobe/TSDuck; independent decoder/client proof is separate"]
async fn actual_qtm_no_bank_polish_underline_concept_serves_two_ordinary_channels() {
    Box::pin(run(underline::response, underline::assert_originals)).await;
}

pub(super) async fn run(source_response: fn(&str, u16, u16) -> Value, verify_originals: fn(&Flow)) {
    let actual = PathBuf::from(client_capture::selected("ERSATZRS_QTM_COMPILER_POLISH_DIR"));
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory)
        .expect("underlined ordinary output must be a new child directory");
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        source_response,
        fixture,
        channel_documents::policy,
    ))
    .await;
    verify_originals(&flow);
    let retained = polish_delivery::Retained::capture(&fixture, &flow, &actual).await;
    let hashes = compiler_polish::actual_input_hashes(&flow, &actual);
    let (accepted, proof) = Box::pin(publish_actual(
        &mut fixture,
        &flow,
        &retained,
        &actual,
        &directory,
    ))
    .await;
    compiler_polish_delivery::assert_source(&fixture, &directory, &source);
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let channels = capture_channels(&directory, &origin, &tools, &source, &accepted).await;
        assert_retained(&mut fixture, &flow, &retained, &accepted, &actual, &hashes, (&directory, &source)).await;
        polish_delivery::write_json(&directory, "stream-proof.json", &json!({
            "probe_version":1,"route":"/iptv/hls-direct/{channel}.ts","transport":"actual application loopback TCP host",
            "scope":"exact genuine QTM underlined no-bank Polish save, explicit native publication, cold ordinary delivery",
            "source_file":"source.ts","source_sha256":live_client::hash(&source),"source_bytes":source.len(),
            "channels":channels,"genuine_inputs":hashes,"editor":proof,
            "generated_normal_resource_page_hex":"1A0","generated_s1":0,"generated_slot":1,
            "original_d5":true,"physical_drcs_d5":false,"night_generated_resource_count":0,
            "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}));
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned underlined ordinary server must close");
    cleaned.expect("both ordinary response producers must release owned capacity");
    fixture = fixture.reopen().await;
    assert_retained(
        &mut fixture,
        &flow,
        &retained,
        &accepted,
        &actual,
        &hashes,
        (&directory, &source),
    )
    .await;
    fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "success.json",
        &json!({
        "probe_version":1,"genuine_underlined_no_bank_import_preview_publication_ordinary_transport_source_av_complete_model_passed":true,
        "originals_profiles_bindings_package_receipts_config_source_and_inputs_unchanged_after_reopen":true,
        "owned_origin_closed":true,"capacity_released":true,"independent_decoding_proven":false,
        "actual_client_proven":false,"full_level25_proven":false,
        "limitations":"Cold native ordinary delivery only. Independent observation remains separate; no headed admin/client, automatic resource acquisition, warm or full-engine/platform acceptance claim"}),
    );
}

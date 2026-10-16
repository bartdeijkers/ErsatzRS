//! Genuine fresh QTM no-bank edit to cold ordinary TCP delivery on two profiles.
//! Actual emitted resources and AV/transport/model equality are native evidence;
//! independent decoding and actual client acceptance remain separate.

use super::super::{client_capture, ordinary_stream_flow};
use super::*;
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{panic::AssertUnwindSafe, path::PathBuf};

fn assert_source(fixture: &Fixture, directory: &Path, source: &[u8]) {
    for path in [
        directory.join("source.ts"),
        fixture.root.path().join("synthetic-client-source.ts"),
    ] {
        assert_eq!(
            live_client::read_bounded(&path, source.len()).unwrap(),
            source
        );
    }
}

fn assert_generated_owner(accepted: &[AcceptedTeletextOutputCandidate; 2]) {
    let resource = &accepted[0].service().compiler_drcs_resources()[0];
    assert_eq!(resource.table().identity().page().magazine(), 1);
    assert_eq!(resource.table().identity().page().page(), 0xa0);
    assert_eq!(resource.table().identity().logical_subpage().get(), 0);
    assert_eq!(resource.table().glyphs()[0].slot.get(), 0);
    assert!(accepted[1].service().compiler_drcs_resources().is_empty());
    assert!(accepted[1]
        .service()
        .pages()
        .iter()
        .all(|page| page.resource_header().is_none()));
    let day = &accepted[0].selection().bindings[0];
    let night = &accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
}

fn theme_mask(candidate: &AcceptedTeletextOutputCandidate, foreground: [u8; 3]) -> Vec<bool> {
    let service = candidate.service();
    let left = usize::from(
        polish_carrier::composition(service)
            .display_geometry()
            .left_width(),
    );
    let raster = external_editor_probe::preview_page(service, 125);
    let mut mask = Vec::with_capacity(120);
    for y in 10..20 {
        for x in (left + 8) * 12..(left + 9) * 12 {
            let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) =
                raster.pixel(x, y)
            else {
                panic!("page125 G0 calibration must resolve every native pixel");
            };
            let rgb = value.get();
            assert!(rgb == [0, 0, 0] || rgb == foreground);
            mask.push(rgb == foreground);
        }
    }
    assert!(
        mask.contains(&true),
        "page125 G0 A must visibly carry its profile theme"
    );
    mask
}

fn assert_previews(
    directory: &Path,
    restored: &ersatzrs_web::host::teletext_designs::RestoredOriginalTeletextDesign,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
) {
    let previews: Vec<_> = [124,125].into_iter().map(|page| {
        let concept = external_editor_probe::preview_page(restored.encoded().unwrap(),page);
        let published = external_editor_probe::preview_page(accepted[0].service(),page);
        assert_eq!(concept,published,"complete shared preview must survive publication");
        json!({"page":page,"width":published.width(),"height":published.height(),"concept_equals_published":true})
    }).collect();
    assert_eq!(
        theme_mask(&accepted[0], [15, 0, 0]),
        theme_mask(&accepted[1], [0, 0, 15])
    );
    polish_delivery::write_json(
        directory,
        "native-preview.json",
        &json!({"probe_version":1,
        "scope":"native complete shared preview equality and literal wide Polish mask; no browser image claim",
        "day_previews":previews,"literal_polish_base_row":1,"literal_polish_base_column":7,
        "literal_polish_code":91,"primary":8,"wide_mask_pixels_checked":240,
        "generated_normal_resource_page_hex":"1A0","generated_s1":0,"generated_slot":0,
        "night_original_unchanged":true,"night_generated_resource_count":0,
        "page125_foreground_rgb4_by_channel":[[15,0,0],[0,0,15]],"same_g0_bitmap_both_themes":true,
        "page125":"unchanged G0 calibration; no authored DRCS-bank oracle"}),
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
    let restored = drcs_bitmap::restored(fixture, &concept).await;
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    assert_eq!(
        restored.transmissions(),
        flow.saved[0].candidate().transmissions()
    );
    assert_eq!(restored.metadata(), flow.saved[0].candidate().metadata());
    assert_eq!(
        restored.sources().len(),
        flow.saved[0].candidate().sources().len()
    );
    for (before, after) in flow.saved[0]
        .candidate()
        .sources()
        .iter()
        .zip(restored.sources())
    {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    compiler_polish::assert_concept(flow, restored.encoded().unwrap(), 0x5b);
    let day = publish_and_accept(fixture, flow, &concept).await;
    let accepted = [day, recover(fixture, 2).await];
    assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
    compiler_polish::assert_concept(flow, accepted[0].service(), 0x5b);
    compiler_polish::assert_pointers(fixture, &accepted[0], &accepted[1]).await;
    assert_generated_owner(&accepted);
    assert_previews(directory, &restored, &accepted);
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
    compiler_polish::assert_concept(flow, accepted[0].service(), 0x5b);
    assert_generated_owner(accepted);
    assert_eq!(compiler_polish::actual_input_hashes(flow, actual), hashes);
    assert_source(fixture, media.0, media.1);
}

/// Select retained QTM inputs with `ERSATZRS_QTM_COMPILER_POLISH_DIR` and a
/// create-new output child with existing `ERSATZRS_TELETEXT_POLISH_DELIVERY_DIR`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained fresh no-bank QTM save and exact FFmpeg9/ffprobe/TSDuck; independent decoder/client proof is separate"]
async fn actual_qtm_no_bank_polish_concept_serves_two_ordinary_channels() {
    let actual = PathBuf::from(client_capture::selected("ERSATZRS_QTM_COMPILER_POLISH_DIR"));
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).expect("no-bank ordinary output must be a new child directory");
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        compiler_polish::response,
        fixture,
        channel_documents::policy,
    ))
    .await;
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
    assert_source(&fixture, &directory, &source);
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let channels = capture_channels(&directory,&origin,&tools,&source,&accepted).await;
        assert_retained(&mut fixture,&flow,&retained,&accepted,&actual,&hashes,(&directory,&source)).await;
        polish_delivery::write_json(&directory,"stream-proof.json",&json!({"probe_version":1,
            "route":"/iptv/hls-direct/{channel}.ts","transport":"actual application loopback TCP host",
            "scope":"exact genuine QTM no-bank Polish save, explicit native publication, cold ordinary delivery",
            "source_file":"source.ts","source_sha256":live_client::hash(&source),"source_bytes":source.len(),
            "channels":channels,"genuine_inputs":hashes,"original_sha256":proof["original_sha256"],"saved_sha256":proof["saved_sha256"],
            "generated_normal_resource_page_hex":"1A0","generated_s1":0,"generated_slot":0,"night_generated_resource_count":0,
            "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}));
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned no-bank ordinary server must close");
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
        &json!({"probe_version":1,
        "genuine_no_bank_import_preview_publication_ordinary_transport_source_av_complete_model_passed":true,
        "originals_profiles_bindings_package_receipts_config_source_and_inputs_unchanged_after_reopen":true,
        "owned_origin_closed":true,"capacity_released":true,"independent_decoding_proven":false,
        "actual_client_proven":false,"full_level25_proven":false,
        "limitations":"Cold native ordinary delivery only; independent imager observation is separate. No headed admin/client, automatic resource acquisition, warm or full-engine/platform acceptance claim"}),
    );
}

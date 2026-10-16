//! Exact genuine Turkish Save As to two cold ordinary application TCP streams.
//! Transport/native model evidence does not grant independent/client acceptance.

use super::super::{client_capture, ordinary_stream_flow};
use super::compiler_turkish as turkish;
use super::*;
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{panic::AssertUnwindSafe, path::PathBuf, time::Duration};

async fn await_actual_editor(fixture: &Fixture, flow: &Flow, actual: &Path) {
    let retained = resource_bank_admin::Retained::capture(fixture).await;
    turkish::write_editor_inputs_at(flow, actual);
    let original = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let sidecar =
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap();
    polish_delivery::write_json(
        actual,
        "editor-ready.json",
        &json!({"probe_version":1,
        "scope":"same owned live fixture exports then waits for actual QTM before any import/publication",
        "design_id":flow.saved[0].head().id,"revision":1,"original_digest":flow.saved[0].digest(),
        "receipt_id":flow.exported[0].metadata().id,"receipt_digest":flow.exported[0].metadata().digest,
        "original_file":"native-original-1/page-124-0020.tti","original_sha256":live_client::hash(&original.bytes),
        "sidecar_file":"native-original-1/project.ersatzrs.json","sidecar_sha256":live_client::hash(&sidecar),
        "save_as":"editor-1/edited.tti","completion":"editor-1/editor-proof.json","timeout_seconds":600,
        "edit":{"row":1,"column":7,"before":65,"after":35,"primary":22},
        "publication":"only after exact fresh sidecar, all files, pinned actual editor and fixed literal edit checks"}),
    );
    let result = tokio::time::timeout(
        Duration::from_mins(10),
        resource_bank_admin::marker::<Value>(actual, "editor-1/editor-proof.json"),
    )
    .await;
    assert_original_pointers(fixture, flow).await;
    retained.assert_unchanged(fixture, flow).await;
    let _ = result
        .expect("actual QTM did not complete within 600 seconds")
        .expect("actual editor completion must be bounded valid JSON");
}

fn assert_theme(candidate: &AcceptedTeletextOutputCandidate, foreground: [u8; 3]) {
    // Page125 retains the existing G0 Latin A, not the newly generated bank.
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
                matches!(raster.pixel((left+8)*12+x,10+row),
                Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==expected),
                "unchanged page125 G0 A pixel{x},{row}"
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
    for (before, after) in flow.accepted.iter().zip(accepted) {
        assert_eq!(
            external_editor_probe::preview_page(after.service(), 125),
            external_editor_probe::preview_page(before.service(), 125)
        );
    }
    assert_theme(&accepted[0], [15, 0, 0]);
    assert_theme(&accepted[1], [0, 0, 15]);
    polish_delivery::write_json(
        directory,
        "native-preview.json",
        &json!({"probe_version":1,
        "scope":"complete native shared preview equality and literal wide Turkish TL mask; no browser image claim",
        "day124":{"width":preview.width(),"height":preview.height(),"concept_equals_published":true,"wide_pixels_checked":240},
        "primary":22,"literal_code":35,"generated_normal_resource_page_hex":"1A0","s1":0,"slot":2,
        "literal_mask":[0,0,1008,192,216,216,216,222,0,0],
        "page125":"unchanged G0 Latin A; fixed 120 pixels per channel","page125_foreground_rgb4_by_channel":[[15,0,0],[0,0,15]],
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
    let (request, proof) = turkish::actual_request(flow, actual);
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
    let preview = turkish::assert_restored(fixture, flow, &concept).await;
    let day = publish_and_accept(fixture, flow, &concept).await;
    let accepted = [day, recover(fixture, 2).await];
    assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
    turkish::assert_concept(flow, accepted[0].service());
    compiler_polish::assert_pointers(fixture, &accepted[0], &accepted[1]).await;
    compiler_polish_underline_delivery::assert_profiles(&accepted);
    write_previews(directory, flow, &preview, &accepted);
    retained
        .assert_unchanged(fixture, flow, &accepted, actual)
        .await;
    polish_delivery::write_json(
        actual,
        "native-proof.json",
        &json!({"probe_version":1,
        "scope":"same live fresh export and actual QTM Turkish Base23 save to native concept, shared preview and explicit publication",
        "editor":proof,"input_hashes":turkish::actual_input_hashes(flow,actual),
        "design_id":concept.head().id,"revision":concept.head().revision,"digest":concept.digest(),
        "generation_id":accepted[0].generation().id,"request_id":accepted[0].generation().request.id,
        "primary":22,"literal_code":35,"normal_resource_page_hex":"1A0","s1":0,"slot":2,
        "fresh_receipt_and_exact_project_metadata_verified":true,
        "ordinary_delivery_proven":false,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}),
    );
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
        drcs_bitmap_delivery::capture_tcp(origin.url(2)),
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
    turkish::assert_concept(flow, accepted[0].service());
    compiler_polish_underline_delivery::assert_profiles(accepted);
    assert_eq!(turkish::actual_input_hashes(flow, actual), hashes);
    compiler_polish_delivery::assert_source(fixture, media.0, media.1);
}

/// Both roots must be new. This same fixture exports, waits for genuine Save As,
/// imports and publishes, then serves cold ordinary streams and reopens its DB.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "exports a new Turkish editor root and waits up to 600 seconds for actual pinned QTM A7-to-23 Save As; requires FFmpeg9/ffprobe/TSDuck; independent/client proof is separate"]
async fn actual_qtm_no_bank_turkish_concept_serves_two_ordinary_channels() {
    let actual = PathBuf::from(client_capture::selected("ERSATZRS_QTM_TURKISH_DIR"));
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).expect("Turkish ordinary output must be a new child directory");
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        turkish::response,
        fixture,
        channel_documents::policy,
    ))
    .await;
    turkish::assert_originals(&flow);
    Box::pin(await_actual_editor(&fixture, &flow, &actual)).await;
    let retained = polish_delivery::Retained::capture(&fixture, &flow, &actual).await;
    let hashes = turkish::actual_input_hashes(&flow, &actual);
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
        let channels = capture_channels(&directory,&origin,&tools,&source,&accepted).await;
        assert_retained(&mut fixture,&flow,&retained,&accepted,&actual,&hashes,(&directory,&source)).await;
        polish_delivery::write_json(&directory,"stream-proof.json",&json!({"probe_version":1,
            "route":"/iptv/hls-direct/{channel}.ts","transport":"actual application loopback TCP host",
            "scope":"genuine fresh QTM Turkish Base23 save, explicit publication, cold ordinary delivery",
            "source_file":"source.ts","source_sha256":live_client::hash(&source),"source_bytes":source.len(),
            "channels":channels,"genuine_inputs":hashes,"editor":proof,
            "primary":22,"literal_code":35,"generated_normal_resource_page_hex":"1A0","s1":0,"slot":2,
            "night_generated_resource_count":0,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}));
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned Turkish ordinary origin must close");
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
        "genuine_turkish_import_preview_publication_ordinary_transport_source_av_complete_model_passed":true,
        "originals_profiles_bindings_package_receipts_config_source_and_inputs_unchanged_after_reopen":true,
        "owned_origin_closed":true,"capacity_released":true,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false,
        "limitations":"Cold native ordinary delivery. No headed admin, independent decoding, automatic resource acquisition, warm transition, suitable-client or full-engine/platform acceptance claim"}),
    );
}

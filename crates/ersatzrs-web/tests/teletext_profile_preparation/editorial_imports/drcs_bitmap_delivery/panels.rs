//! Retained genuine singleton bank bytes through authenticated panel permissions.
//! Native pixels and complete ordinary transport/model equality only: no panel
//! independent decoder, editor panel rendering or suitable-client claim.

use super::*;

fn inputs(actual: &Path) -> Value {
    let mut inputs = serde_json::Map::new();
    for name in ["native-input.tti", "edited.tti"] {
        let bytes = drcs_bitmap::read_retained_editor_file(&actual.join(name));
        inputs.insert(name.into(), json!(live_client::hash(&bytes)));
    }
    Value::Object(inputs)
}

fn fresh_export(directory: &Path, flow: &Flow) -> Value {
    let export = directory.join("native-original-1");
    std::fs::create_dir(&export).unwrap();
    let request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let mut files = serde_json::Map::new();
    for file in request.files {
        live_client::write_new(&export.join(&file.name), &file.bytes).unwrap();
        files.insert(file.name, json!(live_client::hash(&file.bytes)));
    }
    live_client::write_new(&export.join("project.ersatzrs.json"), &request.sidecar).unwrap();
    json!({"receipt_id":request.receipt_id,"receipt_digest":request.receipt_digest,
        "files":files,"sidecar_sha256":live_client::hash(&request.sidecar),
        "authority":"Current owned V4 receipt, complete current project/sidecar and captured producer permissions; no retained GUI metadata reused"})
}

async fn assert_state(
    fixture: &mut Fixture,
    flow: &Flow,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    retained: &resource_bank_admin::Retained,
    actual: &Path,
    before: &Value,
    media: (&Path, &[u8]),
) {
    retained.assert_unchanged(fixture, flow).await;
    for (index, expected) in accepted.iter().enumerate() {
        generations::assert_same_output(
            &recover(fixture, i64::try_from(index + 1).unwrap()).await,
            expected,
        );
    }
    drcs_bitmap::panels::Case::MainAndRight
        .assert_pixels(flow.accepted[0].service(), accepted[0].service());
    generations::assert_same_output(&accepted[1], &flow.accepted[1]);
    compiler_polish_delivery::assert_source(fixture, media.0, media.1);
    assert_eq!(inputs(actual), *before);
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the pinned retained actual QTM singleton bank save and exact FFmpeg9/ffprobe/TSDuck; no independent panel decoder or headed client claim"]
async fn retained_actual_qtm_panel_bitmap_serves_two_ordinary_tcp_channels() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_DRCS_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).expect("panel delivery destination must be new");
    let actual = PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_DRCS_CALIBRATION_DIR",
    ));
    let before = inputs(&actual);
    let tools = Tools::load().await;
    let (fixture, source) = playback_fixture(&tools, &directory).await;
    let (mut fixture, flow, accepted, retained) = Box::pin(published_case(
        fixture,
        &actual,
        drcs_bitmap::panels::Case::MainAndRight,
    ))
    .await;
    let retained = retained.expect("MainAndRight captures immutable profile/package state");
    let export = fresh_export(&directory, &flow);
    Box::pin(assert_state(
        &mut fixture,
        &flow,
        &accepted,
        &retained,
        &actual,
        &before,
        (&directory, &source),
    ))
    .await;
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let (day, night) = tokio::join!(capture_tcp(origin.url(1)), capture_tcp(origin.url(2)));
        let day = polish_delivery::verify_capture(&directory, &tools, &source, 1, &day, &accepted[0]).await;
        let night = polish_delivery::verify_capture(&directory, &tools, &source, 2, &night, &accepted[1]).await;
        Box::pin(assert_state(&mut fixture, &flow, &accepted, &retained, &actual, &before, (&directory, &source))).await;
        polish_delivery::write_json(&directory, "stream-proof.json", &json!({
            "probe_version":1,"scope":"Retained genuine QTM singleton bank Save As reused after exact fresh bank equality; cold Main+RightPanel ordinary delivery",
            "route":"/iptv/hls-direct/{channel}.ts","transport":"actual application loopback TCP host",
            "retained_inputs":before,"fresh_export":export,"source_file":"source.ts",
            "source_sha256":live_client::hash(&source),"source_bytes":source.len(),"channels":[day,night],
            "retained_gui_provenance":"tmp/phase9-drcs-edit/identity.json and editor-final-result.json; retained actual-2 bank files/screenshots",
            "fresh_editor_launched":false,"native_geometry_cells":[56,25],"native_geometry_pixels":[672,250],
            "exact_day_added_red_pixels_per_page":[[154,10],[538,10]],"native_pages":[124,125],
            "night_original_exact":true,"headers_fallback_and_other_native_units_unchanged":true,
            "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}));
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned panel delivery origin must close");
    cleaned.expect("ordinary response producers must release owned capacity");
    fixture = fixture.reopen().await;
    Box::pin(assert_state(
        &mut fixture,
        &flow,
        &accepted,
        &retained,
        &actual,
        &before,
        (&directory, &source),
    ))
    .await;
    fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "success.json",
        &json!({
        "probe_version":1,"retained_genuine_bank_import_native_panel_preview_explicit_publication_reopen_passed":true,
        "two_ordinary_routes_source_av_tsduck_complete_model_passed":true,
        "originals_receipts_profiles_bindings_config_package_source_night_and_retained_inputs_unchanged":true,
        "owned_origin_closed":true,"capacity_released":true,"fresh_editor_launched":false,
        "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false,
        "limitations":"Cold native ordinary TCP and shared native panel pixels only; supplied first512 units can end within acquisition. Independent panel decoding, editor panel display, suitable-client navigation, warm handover, localized admin, full engine and platform acceptance remain separate"}),
    );
}

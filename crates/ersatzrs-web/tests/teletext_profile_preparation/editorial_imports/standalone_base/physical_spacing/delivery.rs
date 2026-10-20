//! Retained genuine QTM bytes to two ordinary TCP streams at default capacity.
//! Transport/model identity is not suitable-client or full fallback acceptance.

use super::*;
use crate::{client_capture, ordinary_stream_flow};
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::PathBuf};

fn input_hashes(flow: &Flow, actual: &Path) -> Vec<(String, String)> {
    let mut names: Vec<_> = flow.exported[0]
        .files()
        .iter()
        .map(|file| format!("original/{}", file.name))
        .collect();
    names.extend([
        "original/project.ersatzrs.json".into(),
        "ready.json".into(),
        "editor-saved.json".into(),
        "edited.tti".into(),
    ]);
    names
        .into_iter()
        .map(|name| {
            let bytes = live_client::read_bounded(&actual.join(&name), 262_144).unwrap();
            (name, live_client::hash(&bytes))
        })
        .collect()
}

async fn retained_request(flow: &Flow, actual: &Path) -> NativeTeletextEditorialImportRequest {
    qtm::completion(actual).await;
    let ready: Value = serde_json::from_slice(
        &live_client::read_bounded(&actual.join("ready.json"), 16_384).unwrap(),
    )
    .unwrap();
    assert_eq!(ready["case"], "enhanced-physical-spacing");
    assert_eq!(ready["edit"]["editor_level"], "2.5");
    for file in flow.exported[0].files() {
        assert_eq!(
            live_client::read_bounded(&actual.join("original").join(&file.name), 262_144).unwrap(),
            file.bytes,
            "retained editor input must equal this fresh native export"
        );
    }
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap()
        .bytes = live_client::read_bounded(&actual.join("edited.tti"), 262_144).unwrap();
    let expected = expected_request(flow);
    national_editor::assert_files(&request.files, &expected.files);
    assert_eq!(request.sidecar, expected.sidecar);
    // Revalidate identical retained editor bytes under a fresh current receipt.
    // Never rewrite or consume the historical sidecar as current authority.
    request
}

async fn assert_retained(
    fixture: &Fixture,
    flow: &Flow,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    actual: &Path,
    hashes: &[(String, String)],
) {
    assert_retained_storage(fixture, flow).await;
    assert_eq!(input_hashes(flow, actual), hashes);
    for (index, channel) in [1, 2].into_iter().enumerate() {
        generations::assert_same_output_width(
            &recover(fixture, channel).await,
            &accepted[index],
            480,
        );
    }
    generations::assert_same_output_width(&accepted[1], &flow.accepted[1], 480);
    assert!(fixture.playout.is_empty());
}

async fn capture_channels(
    directory: &Path,
    origin: &live_client::LiveOrigin,
    tools: &drcs_bitmap_delivery::Tools,
    source: &[u8],
    flow: &Flow,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
) -> Value {
    let day_prefix = directory.join("channel-1-failed-prefix");
    let night_prefix = directory.join("channel-2-failed-prefix");
    let (day, night) = tokio::join!(
        drcs_bitmap_delivery::capture_tcp_with_failure_prefix(origin.url(1), &day_prefix),
        drcs_bitmap_delivery::capture_tcp_with_failure_prefix(origin.url(2), &night_prefix)
    );
    let wrong_generation_rejected = std::panic::catch_unwind(AssertUnwindSafe(|| {
        ordinary_stream_flow::assert_emitted_model(&day, flow.accepted[0].service());
    }))
    .is_err();
    assert!(
        wrong_generation_rejected,
        "original generation must not match the edited stream"
    );
    let mut channels = Vec::new();
    for (index, bytes) in [&day, &night].into_iter().enumerate() {
        channels.push(
            polish_delivery::verify_capture(
                directory,
                tools,
                source,
                i64::try_from(index + 1).unwrap(),
                bytes,
                &accepted[index],
            )
            .await,
        );
    }
    json!({"channels":channels,"wrong_generation_rejected":wrong_generation_rejected})
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained actual QTM spacing save and exact FFmpeg9/ffprobe/TSDuck; no client proof"]
async fn actual_qtm_spacing_publication_serves_exact_two_channel_models_at_default_capacity() {
    let actual = PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_SPACING_RETAINED_DIR",
    ));
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_SPACING_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).expect("delivery proof must use a fresh directory");
    live_updates::live_tcp::terminal_trace(&directory).unwrap();
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    // The shared fixture supports older four-viewer trials. This case has only
    // two output responses and explicitly retains the product default of two.
    ordinary_stream_flow::setting(
        &fixture,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
        "2",
    )
    .await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        held_mosaics::response,
        fixture,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    let hashes = input_hashes(&flow, &actual);
    let concept = import(&fixture, retained_request(&flow, &actual).await).await;
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        external_editor_probe::preview_page(day.service(), 124),
        preview
    );
    fixture = fixture.reopen().await;
    let accepted = [recover(&fixture, 1).await, recover(&fixture, 2).await];
    generations::assert_same_output_width(&accepted[0], &day, 480);
    assert_retained(&fixture, &flow, &accepted, &actual, &hashes).await;
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let delivery = capture_channels(&directory,&origin,&tools,&source,&flow,&accepted).await;
        assert_retained(&fixture, &flow, &accepted, &actual, &hashes).await;
        compiler_polish_delivery::assert_source(&fixture, &directory, &source);
        polish_delivery::write_json(&directory,"stream-proof.json",&json!({
            "scope":"Retained genuine QTM spacing bytes -> fresh receipt/concept -> explicit publication/cold recovery -> ordinary TCP. No suitable-client/full9A claim.",
            "route":"/iptv/hls-direct/{channel}.ts","active_transcode_limit":2,
            "delivery":delivery,"input_hashes":hashes,
            "source_sha256":live_client::hash(&source),"source_bytes":source.len(),
            "editor_replayed":false,"historical_sidecar_rewritten":false
        }));
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    polish_delivery::write_json(
        &directory,
        "cleanup-attempt.json",
        &json!({
            "origin_closed":closed.is_ok(),"output_capacity_released":cleaned.is_ok(),
            "capture_and_validation_passed":outcome.is_ok()
        }),
    );
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned spacing origin must close");
    cleaned.expect("both output leases must release");
    fixture = fixture.reopen().await;
    assert_retained(&fixture, &flow, &accepted, &actual, &hashes).await;
    fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "cleanup.json",
        &json!({
            "origin_closed":true,"output_capacity_released":true,"reopened_state_unchanged":true
        }),
    );
}

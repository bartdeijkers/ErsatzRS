//! Retained actual QTM Save As to cold ordinary TCP delivery and headed clients.
//! Warm lifecycle, side panels and complete engine acceptance remain separate.

use super::super::{client_capture, ordinary_stream_flow};
use super::*;
use ersatzrs_ffmpeg::teletext::source_assembly::{
    SuppliedTeletextAssembler, TeletextAssemblyEvent,
};
use futures_util::{FutureExt as _, StreamExt as _};
use serde_json::{json, Value};
use std::{io, panic::AssertUnwindSafe, path::PathBuf};

const SOURCE_MAXIMUM: usize = 48 * 1024 * 1024;
const CAPTURE_MAXIMUM: usize = 8 * 1024 * 1024;
const CAPTURE_TARGET: usize = 40_000 * 188;

pub(super) struct Tools {
    ffmpeg: String,
    ffprobe: String,
    tsanalyze: String,
}

impl Tools {
    pub(super) async fn load() -> Self {
        let tools = Self {
            ffmpeg: client_capture::selected("ERSATZRS_TELETEXT_TEST_FFMPEG"),
            ffprobe: client_capture::selected("FFPROBE_PATH"),
            tsanalyze: client_capture::selected("ERSATZRS_TELETEXT_TEST_TSANALYZE"),
        };
        client_capture::check_tools(&tools.ffmpeg, &tools.ffprobe).await;
        tools
    }
}

pub(super) async fn playback_fixture(tools: &Tools, directory: &Path) -> (Fixture, Vec<u8>) {
    let fixture = Fixture::new_persistent().await;
    ordinary_stream_flow::setting(
        &fixture,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
        "4",
    )
    .await;
    ordinary_stream_flow::setting(
        &fixture,
        ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE,
        "en-us",
    )
    .await;
    ordinary_stream_flow::setting(
        &fixture,
        ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
        "false",
    )
    .await;
    let source_path =
        client_capture::generate_source_with_duration(&tools.ffmpeg, fixture.root.path(), 60).await;
    client_capture::configure_source_for_channels(&fixture, &source_path, &[1, 2]).await;
    ordinary_stream_flow::enable_output_policy(&fixture).await;
    let source = live_client::read_bounded(&source_path, SOURCE_MAXIMUM).unwrap();
    live_client::write_new(&directory.join("source.ts"), &source).unwrap();
    (fixture, source)
}

async fn published_fixture(
    fixture: Fixture,
    actual_directory: &Path,
) -> (Fixture, Flow, [AcceptedTeletextOutputCandidate; 2]) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        drcs_bitmap::response,
        fixture,
        drcs_companion::encoding_policy,
    ))
    .await;
    let request = drcs_bitmap::actual_editor_request(&flow.receipts[0], actual_directory);
    let concept = import(&fixture, request).await;
    assert_original_pointers(&fixture, &flow).await;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        concept.head().id,
        concept.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    drcs_bitmap::assert_only_glyph_pixels(flow.accepted[0].service(), restored.encoded().unwrap());
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    drcs_bitmap::assert_only_glyph_pixels(flow.accepted[0].service(), day.service());
    fixture = fixture.reopen().await;
    let accepted = [recover(&fixture, 1).await, recover(&fixture, 2).await];
    generations::assert_same_output(&accepted[0], &day);
    generations::assert_same_output(&accepted[1], &flow.accepted[1]);
    assert_eq!(
        accepted[0].selection().bindings[0].addon_id,
        accepted[1].selection().bindings[0].addon_id
    );
    assert_ne!(
        accepted[0].selection().bindings[0].profile_id,
        accepted[1].selection().bindings[0].profile_id
    );
    assert_retained_storage(&fixture, &flow).await;
    (fixture, flow, accepted)
}

pub(super) async fn capture_tcp(url: String) -> Vec<u8> {
    tokio::time::timeout(Duration::from_secs(35), async {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let response = client.get(url).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while bytes.len() < CAPTURE_TARGET {
            let chunk = stream
                .next()
                .await
                .expect("ordinary TCP stream ended early")
                .unwrap();
            assert!(bytes.len().checked_add(chunk.len()).unwrap() <= CAPTURE_MAXIMUM);
            bytes.extend_from_slice(&chunk);
        }
        drop(stream);
        // Retain an exact response prefix ending at its last complete authored
        // PES. No packet is rewritten, synthesized, reordered or appended.
        let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
        let mut end = 0;
        for (index, packet) in bytes.chunks_exact(188).enumerate() {
            if matches!(
                assembler.push(packet.try_into().unwrap()).unwrap(),
                TeletextAssemblyEvent::Complete(_)
            ) {
                end = (index + 1) * 188;
            }
        }
        assert!(end >= 1024 * 1024);
        bytes.truncate(end);
        bytes
    })
    .await
    .expect("ordinary TCP capture exceeded its deadline")
}

fn captured_units(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
    let mut units = Vec::new();
    for packet in bytes.chunks_exact(188) {
        if let TeletextAssemblyEvent::Complete(pes) =
            assembler.push(packet.try_into().unwrap()).unwrap()
        {
            for unit in pes.units().filter_map(|unit| unit.teletext_unit()) {
                if units.len() < 512 {
                    units.push(unit.to_vec());
                }
            }
        }
    }
    assembler.finish().unwrap();
    assert!(!units.is_empty());
    units
}

async fn verify_capture(
    directory: &Path,
    tools: &Tools,
    source: &[u8],
    channel: i64,
    bytes: &[u8],
    accepted: &AcceptedTeletextOutputCandidate,
) -> Value {
    verify_capture_mode(
        directory,
        tools,
        source,
        channel,
        bytes,
        accepted,
        "singleton-edited",
    )
    .await
}

pub(super) async fn verify_capture_mode(
    directory: &Path,
    tools: &Tools,
    source: &[u8],
    channel: i64,
    bytes: &[u8],
    accepted: &AcceptedTeletextOutputCandidate,
    mode: &str,
) -> Value {
    let filename = format!("channel-{channel}.ts");
    live_client::write_new(&directory.join(&filename), bytes).unwrap();
    let _report = client_capture::verify_transport_with_qualified_authored_pts_gaps(
        &tools.tsanalyze,
        &tools.ffprobe,
        bytes,
        0,
        Some(&directory.join(format!("channel-{channel}-tsduck.json"))),
        false,
    )
    .await;
    client_capture::verify_av(&tools.ffmpeg, source, bytes).await;
    ordinary_stream_flow::assert_emitted_model(bytes, accepted.service());
    let units_name = format!("channel-{channel}-drcs-units.json");
    let units_path = directory.join(&units_name);
    live_client::write_new(
        &units_path,
        &serde_json::to_vec(&captured_units(bytes)).unwrap(),
    )
    .unwrap();
    let theme = if channel == 1 { "day" } else { "night" };
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-editor-drcs-decoder.py");
    let output = client_capture::probe(
        "python3",
        &[
            script.to_str().unwrap(),
            units_path.to_str().unwrap(),
            theme,
            mode,
        ],
        &[],
        262_144,
    )
    .await;
    let decoder: Value = serde_json::from_slice(&output).unwrap();
    live_client::write_new(
        &directory.join(format!("channel-{channel}-independent.json")),
        &output,
    )
    .unwrap();
    for page in [124, 125] {
        let text = client_capture::decode_page_text(&tools.ffmpeg, bytes, &page.to_string()).await;
        let glyph = if mode == "indexed-edited" && page == 125 {
            '\u{f440}'
        } else {
            '\u{f400}'
        };
        assert!(text.contains(glyph) && text.contains('Z') && !text.contains('Q'));
        live_client::write_new(
            &directory.join(format!("channel-{channel}-page-{page}.srt")),
            text.as_bytes(),
        )
        .unwrap();
    }
    json!({"channel_id":channel,"theme":theme,"capture":filename,"bytes":bytes.len(),
        "sha256":live_client::hash(bytes),"generation_id":accepted.generation().id,
        "revision":accepted.generation().request.revision,
        "complete_emitted_model_equal":true,"source_av_passed":true,"tsduck_transport_passed":true,
        "units_file":units_name,"independent_decoder":decoder,
        "scope":"Exact ordinary TCP prefix ending at a complete authored PES; cold receivers only"})
}

fn ready(
    origin: &live_client::LiveOrigin,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    source: &[u8],
) -> Value {
    let channels: Vec<_> = accepted.iter().enumerate().map(|(index, candidate)| {
        let channel = index + 1;
        let binding = &candidate.selection().bindings[0];
        json!({"channel_id":channel,"url":origin.url(i64::try_from(channel).unwrap()),
            "addon_id":binding.addon_id,"profile_id":binding.profile_id,
            "theme":if channel==1 {"day"} else {"night"},
            "generation_id":candidate.generation().id,"design_revision":candidate.generation().request.revision,
            "expected_pages":[124,125],"subcode":32,"expected_navigation":[124,125,124],
            "edited_q_page":null,"bitmap_first_row":if channel==1 {0x803} else {0x801},
            "changed_consumers":if channel==1 {vec![124,125]} else {vec![]}})
    }).collect();
    json!({"probe_version":1,"scope":"Retained actual QTM singleton Save As; cold actual ordinary URLs",
        "channels":channels,"source":{"file":"source.ts","sha256":live_client::hash(source),
        "bytes":source.len(),"duration_seconds":60},"active_transcode_limit":4,
        "completion_file":"client-complete.json","completion_timeout_seconds":180,
        "expected_completion":{"probe_version":1,"client_name":"VLC","client_version":"3.0.20",
            "decoder_version":"0.2.42","proof_suffix":"live-ordinary"},
        "client_actions":"Open both URLs and navigate124 to125 to124; inspect shared glyph at row1column8 on both pages, day0x803/night0x801. Close owned clients before completion.",
        "limitations":"Cold clients only. Warm resource lifecycle, panels, full Level2.5, admin and platform acceptance remain open."})
}

async fn assert_retained(
    fixture: &mut Fixture,
    flow: &Flow,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    package: &str,
) {
    for (index, candidate) in accepted.iter().enumerate() {
        generations::assert_same_output(
            &recover(fixture, i64::try_from(index + 1).unwrap()).await,
            candidate,
        );
    }
    assert_retained_storage(fixture, flow).await;
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        package
    );
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires retained genuine QTM DRCS Save As, exact FFmpeg9/TSDuck/libzvbi and separately operated headed VLC on ordinary URLs"]
async fn actual_qtm_singleton_bitmap_serves_two_ordinary_tcp_channels_and_cold_clients() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_DRCS_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).expect("DRCS delivery destination must not already exist");
    let actual = PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_DRCS_CALIBRATION_DIR",
    ));
    let tools = Tools::load().await;
    let (fixture, source) = playback_fixture(&tools, &directory).await;
    let (mut fixture, flow, accepted) = Box::pin(published_fixture(fixture, &actual)).await;
    let package = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let (day, night) = tokio::join!(capture_tcp(origin.url(1)), capture_tcp(origin.url(2)));
        let day = verify_capture(&directory, &tools, &source, 1, &day, &accepted[0]).await;
        let night = verify_capture(&directory, &tools, &source, 2, &night, &accepted[1]).await;
        live_client::write_new(&directory.join("stream-proof.json"), &serde_json::to_vec_pretty(&json!({
            "probe_version":1,"route":"/iptv/hls-direct/{channel}.ts","transport":"actual loopback TCP",
            "source_sha256":live_client::hash(&source),"source_bytes":source.len(),"channels":[day,night]
        })).unwrap())?;
        // The external marker is written only after actual headed observations;
        // it synchronizes teardown and is not independent rendering evidence.
        // Captures and validation use earlier cold requests. Start the separate
        // headed cold requests from the same source asset with a fresh schedule.
        ordinary_stream_flow::reset_source_start(&fixture).await;
        live_client::write_new(&directory.join("ready.json"), &serde_json::to_vec_pretty(&ready(&origin, &accepted, &source)).unwrap())?;
        let complete = live_client::wait_for_client(&directory).await?;
        assert_retained(&mut fixture, &flow, &accepted, &package).await;
        Ok::<_, io::Error>(complete)
    }).catch_unwind().await;
    let closed = origin.close().await;
    fixture.state.pool.close().await;
    let complete = match outcome {
        Ok(result) => result.expect("cold client trial did not complete"),
        Err(panic) => std::panic::resume_unwind(panic),
    };
    closed.expect("owned DRCS delivery origin did not close cleanly");
    live_client::write_new(&directory.join("success.json"), &serde_json::to_vec_pretty(&json!({
        "probe_version":1,"scope":"Actual saved QTM bitmap to cold ordinary TCP/client flow",
        "completion":complete,"source_av_transport_model_and_independent_pixels_passed":true,
        "published_reopened_channels_retained":true,"owned_server_closed":true,
        "headed_evidence":"Attach root-inspected client captures; marker alone is not rendering evidence",
        "limitations":"No warm resource-update, panel, full authoredLevel2.5, admin or platform acceptance"
    })).unwrap()).unwrap();
}

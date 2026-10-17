//! One real ordinary TCP response per viewer observes the actual QTM DRCS edit.
//! Independent decoding checks one retained receiver across the update; no headed
//! client, natural EOF, side-panel or complete engine acceptance is implied.

use super::super::{client_capture, ordinary_stream_flow};
use super::live_updates::{live_tcp, Capture};
use super::*;
use ersatzrs_core::repositories::TeletextPublicationMutation;
use ersatzrs_domain::TeletextResourceKind;
use ersatzrs_ffmpeg::teletext::{TeletextCarousel, TeletextCarouselBatchKind};
use futures_util::FutureExt as _;
use serde_json::{json, Value};
use std::{panic::AssertUnwindSafe, path::PathBuf, sync::Arc};

const MAX_UNITS: usize = 512;
const MAX_SOURCE: usize = 48 * 1024 * 1024;
const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_DRCS_WARM_DELIVERY_DIR";

type Unit = [u8; 46];
type Pattern = Vec<Vec<Unit>>;

#[path = "drcs_bitmap_warm_delivery/headed.rs"]
mod headed;
#[path = "drcs_bitmap_warm_delivery/page_presentation.rs"]
mod page_presentation;

pub(super) struct Tools {
    pub(super) ffmpeg: String,
    pub(super) ffprobe: String,
    pub(super) tsanalyze: String,
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

async fn source_fixture(tools: &Tools, directory: &Path) -> (Fixture, Flow, Vec<u8>) {
    source_fixture_with_response(
        tools,
        directory,
        drcs_bitmap::response,
        drcs_companion::encoding_policy,
    )
    .await
}

async fn source_fixture_with_response(
    tools: &Tools,
    directory: &Path,
    response: fn(&str, u16, u16) -> Value,
    encoding: fn() -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
) -> (Fixture, Flow, Vec<u8>) {
    source_fixture_with_response_and_writer(
        tools,
        directory,
        response,
        encoding,
        authoring_metadata::write,
    )
    .await
}

pub(super) async fn source_fixture_with_response_and_writer(
    tools: &Tools,
    directory: &Path,
    response: fn(&str, u16, u16) -> Value,
    encoding: fn() -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    write: fn(&Fixture, &str, &Value) -> Vec<u8>,
) -> (Fixture, Flow, Vec<u8>) {
    let fixture = Fixture::new_persistent().await;
    let source =
        client_capture::generate_source_with_duration(&tools.ffmpeg, fixture.root.path(), 60).await;
    client_capture::configure_source_for_channels(&fixture, &source, &[1, 2]).await;
    let bytes = live_tcp::read_bounded(&source, MAX_SOURCE).unwrap();
    live_tcp::write_new(&directory.join("source.ts"), &bytes).unwrap();
    let mut policy: Value = serde_json::from_str(include_str!(
        "../../fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    policy["version"] = json!(2);
    policy["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    for (key, value) in [
        (
            "teletext.output.policy",
            serde_json::to_string(&policy).unwrap(),
        ),
        (
            ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
            "3".to_owned(),
        ),
        (
            ersatzrs_web::host::teletext_work::MAX_CONCURRENT_SETTING,
            "4".to_owned(),
        ),
    ] {
        ordinary_stream_flow::setting(&fixture, key, &value).await;
    }
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response, fixture, encoding, write,
    ))
    .await;
    // Exactly one source reset precedes all requests. Publication below neither
    // reopens a viewer nor starts another source epoch.
    ordinary_stream_flow::reset_source_start(&fixture).await;
    (fixture, flow, bytes)
}

fn cycle_units(carousel: &mut TeletextCarousel) -> Vec<Unit> {
    let mut units = Vec::new();
    for _ in 0..4096 {
        let batch = carousel.next_batch().unwrap();
        assert!(units.len() + batch.data_units().len() <= MAX_UNITS);
        units.extend_from_slice(batch.data_units());
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            return units;
        }
    }
    panic!("fixed service exceeded its bounded cycle");
}

fn pattern(candidate: &AcceptedTeletextOutputCandidate, counter: usize) -> Pattern {
    assert_eq!(candidate.generation().artifact_version, 3);
    let service = candidate.service();
    let plan = service.resource_lifecycle_variants().unwrap();
    let mut carousel = TeletextCarousel::new_authored_shared(Arc::clone(service)).unwrap();
    let first = cycle_units(&mut carousel);
    let repeated = cycle_units(&mut carousel);
    assert_eq!(first.len(), repeated.len());
    first
        .into_iter()
        .zip(repeated)
        .map(|(first, repeat)| {
            if let Some(entry) = plan
                .entries()
                .iter()
                .find(|entry| service.pages()[entry.physical_page_index()].data_units()[0] == first)
            {
                assert!(matches!(
                    entry.resource().kind(),
                    TeletextResourceKind::Drcs | TeletextResourceKind::Mot
                ));
                // Exact checked first-changed and unchanged-repeat headers. S2 and
                // C8 are never masked, and every other byte remains authoritative.
                vec![
                    entry.headers()[counter * 2 + 1],
                    entry.headers()[counter * 2],
                ]
            } else {
                vec![first, repeat]
            }
        })
        .collect()
}

pub(super) struct ObservedCycle {
    pub(super) end: usize,
    pub(super) units: Vec<Unit>,
}
pub(super) async fn wait_cycle(
    capture: &Capture,
    after: usize,
    expected: &Pattern,
) -> ObservedCycle {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            let units = capture.ordered_units_after(after, MAX_UNITS);
            if let Some(start) = units.windows(expected.len()).position(|window| {
                window
                    .iter()
                    .zip(expected)
                    .all(|((_, unit), aliases)| aliases.contains(unit))
            }) {
                let actual = &units[start..start + expected.len()];
                return ObservedCycle {
                    end: actual.last().unwrap().0,
                    units: actual.iter().map(|(_, unit)| *unit).collect(),
                };
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("same ordinary response did not emit its exact complete resource cycle")
}

pub(super) fn write_units(directory: &Path, name: &str, cycle: &ObservedCycle) {
    live_tcp::write_new(
        &directory.join(name),
        &serde_json::to_vec(
            &cycle
                .units
                .iter()
                .map(|unit| unit.to_vec())
                .collect::<Vec<_>>(),
        )
        .unwrap(),
    )
    .unwrap();
}

async fn original_cycles(
    origin: &live_tcp::Origin,
    flow: &Flow,
    after: [usize; 3],
) -> [ObservedCycle; 3] {
    let day = pattern(&flow.accepted[0], 1);
    let night = pattern(&flow.accepted[1], 1);
    let views = origin.captures();
    let (a, b, c) = tokio::join!(
        wait_cycle(&views[0], after[0], &day),
        wait_cycle(&views[1], after[1], &day),
        wait_cycle(&views[2], after[2], &night)
    );
    [a, b, c]
}

async fn pending_actual_edit(
    fixture: &Fixture,
    flow: &Flow,
    actual: &Path,
) -> (SavedEditorialTeletextDesign, TeletextPublicationRequest) {
    let concept = import(
        fixture,
        drcs_bitmap::actual_editor_request(&flow.receipts[0], actual),
    )
    .await;
    // Check durable activation pointers while the streams run. Full raster and
    // recovered-model comparisons run after shutdown: their synchronous test
    // oracle must not starve the current-thread transport delivery clock.
    for (index, channel) in [1, 2].into_iter().enumerate() {
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(flow.pending[index].clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(flow.accepted[index].generation().clone())
        );
    }
    let requested =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            concept.head().id,
            concept.head().revision,
            concept.digest().to_owned(),
            2,
            Some(flow.pending[0].id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = requested else {
        panic!("actual edit must request publication");
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(flow.accepted[0].generation().clone())
    );
    (concept, pending.request().clone())
}

async fn transition(
    fixture: &Fixture,
    flow: &Flow,
    origin: &live_tcp::Origin,
    actual: &Path,
    directory: &Path,
) -> AcceptedTeletextOutputCandidate {
    let before = original_cycles(origin, flow, [0; 3]).await;
    for (index, observed) in before.iter().enumerate() {
        write_units(
            directory,
            &format!("viewer-{index}-original-units.json"),
            observed,
        );
    }
    let (_concept, pending) = pending_actual_edit(fixture, flow, actual).await;
    let offsets = origin.captures().each_ref().map(Capture::position);
    let held = original_cycles(origin, flow, offsets).await;
    let before_accept = origin.captures().each_ref().map(Capture::position);
    let edited = generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending,
                expected_design_head: 2,
                expected_generation_id: Some(flow.accepted[0].generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    );
    let day = pattern(&edited, 2);
    let night = pattern(&flow.accepted[1], 1);
    let views = origin.captures();
    let (a, b, c) = tokio::join!(
        wait_cycle(&views[0], before_accept[0], &day),
        wait_cycle(&views[1], before_accept[1], &day),
        wait_cycle(&views[2], before_accept[2], &night)
    );
    let changed = [a, b, c];
    let (a, b, c) = tokio::join!(
        wait_cycle(&views[0], changed[0].end, &day),
        wait_cycle(&views[1], changed[1].end, &day),
        wait_cycle(&views[2], changed[2].end, &night)
    );
    let repeated = [a, b, c];
    for index in 0..3 {
        write_units(
            directory,
            &format!("viewer-{index}-updated-units.json"),
            &changed[index],
        );
        write_units(
            directory,
            &format!("viewer-{index}-repeat-units.json"),
            &repeated[index],
        );
    }
    live_tcp::write_new(&directory.join("wire-offsets.json"), &serde_json::to_vec_pretty(&json!({
        "original_complete_pes_end":before.each_ref().map(|c|c.end),
        "pending_complete_pes_end":held.each_ref().map(|c|c.end),
        "pre_accept_offsets":before_accept,"updated_complete_pes_end":changed.each_ref().map(|c|c.end),
        "repeat_complete_pes_end":repeated.each_ref().map(|c|c.end),
        "day_s2":2,"night_s2":1,"same_response":true
    })).unwrap()).unwrap();
    edited
}

async fn verify(
    directory: &Path,
    tools: &Tools,
    source: &[u8],
    origin: &live_tcp::Origin,
) -> Vec<Value> {
    verify_with_decoder(
        directory,
        tools,
        source,
        origin,
        "teletext-editor-drcs-decoder.py",
        "singleton-warm-edited",
    )
    .await
}

async fn verify_with_decoder(
    directory: &Path,
    tools: &Tools,
    source: &[u8],
    origin: &live_tcp::Origin,
    script_name: &str,
    mode: &str,
) -> Vec<Value> {
    let mut receipts = Vec::new();
    for (index, capture) in origin.captures().iter().enumerate() {
        let mut bytes = capture.bytes();
        bytes.truncate(bytes.len() / 188 * 188);
        assert!(bytes.len() >= 188 * 1000);
        live_tcp::assert_uninterrupted_source_clock(&bytes);
        client_capture::verify_transport_with_qualified_authored_pts_gaps(
            &tools.tsanalyze,
            &tools.ffprobe,
            &bytes,
            0,
            Some(&directory.join(format!("viewer-{index}-tsduck.json"))),
            true,
        )
        .await;
        client_capture::verify_av_with_audio_bound(&tools.ffmpeg, source, &bytes, 6 * 1024 * 1024)
            .await;
        let prefix = directory.join(format!("viewer-{index}-original-units.json"));
        let suffix = directory.join(format!("viewer-{index}-updated-units.json"));
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/e2e")
            .join(script_name);
        let theme = if index == 2 { "night" } else { "day" };
        let decoded = client_capture::probe(
            "python3",
            &[
                script.to_str().unwrap(),
                prefix.to_str().unwrap(),
                theme,
                mode,
                suffix.to_str().unwrap(),
            ],
            &[],
            262_144,
        )
        .await;
        live_tcp::write_new(
            &directory.join(format!("viewer-{index}-independent.json")),
            &decoded,
        )
        .unwrap();
        receipts.push(json!({"viewer":index,"theme":theme,"capture_bytes":bytes.len(),
            "full_capture_sha256":live_tcp::hash(&capture.bytes()),"complete_packet_sha256":live_tcp::hash(&bytes),
            "source_clock_unchanged":true,"tsduck_passed":true,"source_av_passed":true,
            "independent_warm_bitmap":serde_json::from_slice::<Value>(&decoded).unwrap()}));
    }
    receipts
}

pub(super) async fn close(origin: &mut live_tcp::Origin, observers: &mut [live_tcp::Observer; 3]) {
    for observer in observers {
        observer.close().await.unwrap();
    }
    origin.close().await.unwrap();
    live_tcp::wait_for_cleanup().await.unwrap();
}

#[tokio::test]
#[ignore = "requires pinned actual QTM files, exact FFmpeg9/TSDuck and a create-new warm delivery directory"]
async fn same_ordinary_tcp_responses_deliver_actual_qtm_resource_edit_without_source_reset() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    let actual = PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_DRCS_CALIBRATION_DIR",
    ));
    std::fs::create_dir(&directory).unwrap();
    live_tcp::terminal_trace(&directory).unwrap();
    let tools = Tools::load().await;
    let (mut fixture, flow, source) = Box::pin(source_fixture(&tools, &directory)).await;
    let mut origin = live_tcp::Origin::start(&fixture, &directory).await.unwrap();
    let mut observers = [
        live_tcp::Observer::start_for(&origin, 1, "day"),
        live_tcp::Observer::start_for(&origin, 1, "observer"),
        live_tcp::Observer::start_for(&origin, 2, "night"),
    ];
    let outcome = AssertUnwindSafe(async {
        live_tcp::wait_attached(
            &origin,
            tokio::time::Instant::now() + Duration::from_secs(15),
        )
        .await
        .unwrap();
        Box::pin(transition(&fixture, &flow, &origin, &actual, &directory)).await
    })
    .catch_unwind()
    .await;
    let stopped = AssertUnwindSafe(close(&mut origin, &mut observers))
        .catch_unwind()
        .await;
    live_tcp::save_captures(&directory, &origin).unwrap();
    if let Err(panic) = stopped {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    let edited = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    assert!(origin.single_requests());
    drcs_bitmap::assert_only_glyph_pixels(flow.accepted[0].service(), edited.service());
    generations::assert_same_output(&recover(&fixture, 1).await, &edited);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    let receipts = verify(&directory, &tools, &source, &origin).await;
    live_tcp::write_new(&directory.join("warm-delivery.json"),&serde_json::to_vec_pretty(&json!({
        "schema_version":1,"source_requests_per_viewer":1,"source_resets_before_start":1,
        "source_resets_after_start":0,"source_sha256":live_tcp::hash(&source),"viewers":receipts,
        "owned_streams_closed":true,"channel2_unchanged":true,
        "limitations":"Actual native TCP/resource and independent warm bitmap proof only; no headed-client, natural EOF, side-panel, full authored Level2.5 or platform acceptance"
    })).unwrap()).unwrap();
}

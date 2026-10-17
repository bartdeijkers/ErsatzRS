//! Genuine resource edit, packet-extent growth and shrink on ordinary responses.
//! Raw lifecycle/transport proof does not certify receiver acquisition or display.
use super::*;
use crate::editorial_imports::{
    drcs_bitmap_warm_delivery as warm,
    live_updates::{live_tcp, Capture},
};
use crate::{client_capture, ordinary_stream_flow};
use ersatzrs_core::repositories::TeletextPublicationMutation;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, sync::Arc};
#[path = "ordinary/pattern.rs"]
mod pattern;
#[path = "ordinary/targets.rs"]
mod targets;

struct Views {
    origin: live_tcp::Origin,
    observers: [live_tcp::Observer; 3],
}
impl Views {
    async fn start(fixture: &Fixture, directory: &Path) -> Self {
        std::fs::create_dir(directory).unwrap();
        let origin = live_tcp::Origin::start(fixture, directory).await.unwrap();
        let observers = [
            live_tcp::Observer::start_for(&origin, 1, "day"),
            live_tcp::Observer::start_for(&origin, 1, "observer"),
            live_tcp::Observer::start_for(&origin, 2, "night"),
        ];
        Self { origin, observers }
    }
    async fn stop(&mut self, directory: &Path) {
        let stopped = AssertUnwindSafe(warm::close(&mut self.origin, &mut self.observers))
            .catch_unwind()
            .await;
        live_tcp::save_captures(directory, &self.origin).unwrap();
        if let Err(panic) = stopped {
            std::panic::resume_unwind(panic);
        }
        assert!(self.origin.single_requests());
    }
    fn positions(&self) -> [usize; 3] {
        self.origin.captures().each_ref().map(Capture::position)
    }
    async fn cycles(
        &self,
        directory: &Path,
        stage: &str,
        after: [usize; 3],
        day: &AcceptedTeletextOutputCandidate,
        night: &AcceptedTeletextOutputCandidate,
        counter: Option<usize>,
    ) -> [warm::ObservedCycle; 3] {
        let day = pattern::pattern(
            day,
            counter.map(|n| (seed::target(21).resource().unwrap(), n)),
        );
        let night = pattern::pattern(night, None);
        let views = self.origin.captures();
        let (a, b, c) = tokio::join!(
            warm::wait_cycle(&views[0], after[0], &day),
            warm::wait_cycle(&views[1], after[1], &day),
            warm::wait_cycle(&views[2], after[2], &night)
        );
        let cycles = [a, b, c];
        for (index, cycle) in cycles.iter().enumerate() {
            warm::write_units(
                directory,
                &format!("viewer-{index}-{stage}-units.json"),
                cycle,
            );
        }
        cycles
    }
}

async fn request_concept(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    previous: &TeletextPublicationRequest,
) -> TeletextPublicationRequest {
    let requested = request_original_teletext_design_publication(
        &fixture.state.application,
        concept.head().id,
        concept.head().revision,
        concept.digest().to_owned(),
        concept.head().revision,
        Some(previous.id),
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = requested else {
        panic!("exact resource concept must request publication");
    };
    requested.request().clone()
}

async fn accept_concept(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    pending: &TeletextPublicationRequest,
    before: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.clone(),
                expected_design_head: concept.head().revision,
                expected_generation_id: Some(before.generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}

struct Current {
    concept: SavedEditorialTeletextDesign,
    pending: TeletextPublicationRequest,
    accepted: AcceptedTeletextOutputCandidate,
}
async fn transition(
    fixture: &Fixture,
    flow: &Flow,
    views: &Views,
    directory: &Path,
    request: NativeTeletextEditorialImportRequest,
    previous: Option<&Current>,
    counter: usize,
) -> Current {
    let before = previous.map_or(&flow.accepted[0], |v| &v.accepted);
    let old_pending = previous.map_or(&flow.pending[0], |v| &v.pending);
    let concept = import(fixture, request).await;
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(before.generation().clone())
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(old_pending.clone())
    );
    let pending = request_concept(fixture, &concept, old_pending).await;
    let observed = views
        .cycles(
            directory,
            &format!("counter-{counter}-pending"),
            views.positions(),
            before,
            &flow.accepted[1],
            previous.map(|_| counter - 1),
        )
        .await;
    let pre_accept = views.positions();
    let accepted = accept_concept(fixture, &concept, &pending, before).await;
    let updated = views
        .cycles(
            directory,
            &format!("counter-{counter}-updated"),
            pre_accept,
            &accepted,
            &flow.accepted[1],
            Some(counter),
        )
        .await;
    let repeated = views
        .cycles(
            directory,
            &format!("counter-{counter}-repeat"),
            updated.each_ref().map(|c| c.end),
            &accepted,
            &flow.accepted[1],
            Some(counter),
        )
        .await;
    for index in 0..3 {
        pattern::assert_transition(
            if index == 2 {
                &flow.accepted[1]
            } else {
                &accepted
            },
            if index == 2 {
                None
            } else {
                Some((seed::target(21).resource().unwrap(), counter))
            },
            &views.origin.captures()[index].ordered_units_after(pre_accept[index], 512),
            updated[index].end,
            &repeated[index].units,
        );
    }
    live_tcp::write_new(
        &directory.join(format!("counter-{counter}-offsets.json")),
        &serde_json::to_vec_pretty(&json!({"pending_complete_pes_end":observed.each_ref().map(|c|c.end),
            "pre_accept_offsets":pre_accept,"updated_complete_pes_end":updated.each_ref().map(|c|c.end),
            "repeat_complete_pes_end":repeated.each_ref().map(|c|c.end),"changed_resource_counter":counter,
            "other_resource_counters":1,"same_three_responses":true}))
        .unwrap(),
    )
    .unwrap();
    Current {
        concept,
        pending,
        accepted,
    }
}

async fn later_request(
    fixture: &Fixture,
    previous: &Current,
    bytes: &[u8],
) -> NativeTeletextEditorialImportRequest {
    let exported = drcs_bitmap::reexport(fixture, &previous.concept).await;
    let receipt = receipt(fixture, &exported).await;
    let mut request = drcs_bitmap::from_receipt(&receipt, previous.concept.head().revision);
    let name = companion(&exported);
    request
        .files
        .iter_mut()
        .find(|f| f.name == name)
        .unwrap()
        .bytes = bytes.to_vec();
    request
}

async fn verify_captures(
    directory: &Path,
    tools: &warm::Tools,
    source: &[u8],
    views: &Views,
    models: Option<[&AcceptedTeletextOutputCandidate; 3]>,
) -> Vec<Value> {
    let mut reports = Vec::new();
    for (index, capture) in views.origin.captures().iter().enumerate() {
        let raw = capture.bytes();
        let mut bytes = raw.clone();
        bytes.truncate(bytes.len() / 188 * 188);
        assert!(bytes.len() >= 188 * 1000);
        if let Some(models) = models {
            ordinary_stream_flow::assert_emitted_model(&bytes, models[index].service());
        }
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
        reports.push(
            json!({"viewer":index,"raw_bytes":raw.len(),"raw_sha256":live_tcp::hash(&raw),
            "complete_packet_bytes":bytes.len(),"complete_packet_sha256":live_tcp::hash(&bytes),
            "source_clock_unchanged":true,"zero_qualified_pts_gaps":true,
            "tsduck_passed":true,"source_av_passed":true}),
        );
    }
    reports
}

async fn warm_stages(
    fixture: &Fixture,
    flow: &Flow,
    views: &Views,
    directory: &Path,
    saved: &[u8],
    grown: &[u8],
) -> [Current; 3] {
    live_tcp::wait_attached(
        &views.origin,
        tokio::time::Instant::now() + Duration::from_secs(15),
    )
    .await
    .unwrap();
    views
        .cycles(
            directory,
            "original",
            [0; 3],
            &flow.accepted[0],
            &flow.accepted[1],
            None,
        )
        .await;
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request
        .files
        .iter_mut()
        .find(|f| f.name == companion(&flow.exported[0]))
        .unwrap()
        .bytes = saved.to_vec();
    let first = transition(fixture, flow, views, directory, request, None, 2).await;
    let request = later_request(fixture, &first, grown).await;
    let second = transition(fixture, flow, views, directory, request, Some(&first), 3).await;
    let request = later_request(fixture, &second, saved).await;
    let third = transition(fixture, flow, views, directory, request, Some(&second), 4).await;
    [first, second, third]
}

async fn cold_cycles(
    fixture: &Fixture,
    flow: &Flow,
    views: &Views,
    directory: &Path,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    live_tcp::wait_attached(
        &views.origin,
        tokio::time::Instant::now() + Duration::from_secs(15),
    )
    .await
    .unwrap();
    let first = views
        .cycles(
            directory,
            "first",
            [0; 3],
            accepted,
            &flow.accepted[1],
            None,
        )
        .await;
    views
        .cycles(
            directory,
            "repeat",
            first.each_ref().map(|c| c.end),
            accepted,
            &flow.accepted[1],
            None,
        )
        .await;
    tokio::time::timeout(Duration::from_secs(8), async {
        while views
            .origin
            .captures()
            .iter()
            .any(|c| c.bytes().len() < 1024 * 1024)
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("fresh ordinary clients must retain an AV prefix");
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(accepted.generation().clone())
    );
}

#[tokio::test]
#[ignore = "requires pinned saved POP editor file, exact FFmpeg9/TSDuck and a fresh explicit ordinary-delivery directory"]
async fn ordinary_pop_edit_growth_shrink_delivers_on_existing_and_cold_responses() {
    let directory = std::path::PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_POP_DELIVERY_DIR",
    ));
    let actual = std::path::PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_POP_EDITOR_CASE",
    ));
    std::fs::create_dir(&directory).unwrap();
    live_tcp::terminal_trace(&directory).unwrap();
    let tools = warm::Tools::load().await;
    let grown = Box::pin(targets::grown()).await;
    let (_, pixels) = Box::pin(qualified_wide_target()).await;
    let (mut fixture, flow, source) = Box::pin(warm::source_fixture_with_response_and_writer(
        &tools,
        &directory,
        targets::response,
        targets::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let saved = targets::actual_saved(&flow, &actual);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let warm_dir = directory.join("warm");
    let mut warm = Views::start(&fixture, &warm_dir).await;
    let result = AssertUnwindSafe(warm_stages(
        &fixture, &flow, &warm, &warm_dir, &saved, &grown.tti,
    ))
    .catch_unwind()
    .await;
    warm.stop(&warm_dir).await;
    let stages = match result {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic)
        }
    };
    let last = &stages[2].accepted;
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, last);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    let cold_dir = directory.join("cold");
    let mut cold = Views::start(&fixture, &cold_dir).await;
    let result = AssertUnwindSafe(cold_cycles(&fixture, &flow, &cold, &cold_dir, last))
        .catch_unwind()
        .await;
    cold.stop(&cold_dir).await;
    if let Err(panic) = result {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    // Synchronous full-raster and pointer checks run only after all paced workers stop.
    targets::assert_services(&flow, &stages, &grown.service, &pixels);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_pointers(&fixture, last, &flow.accepted[1]).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    let warm_reports = verify_captures(&warm_dir, &tools, &source, &warm, None).await;
    let cold_reports = verify_captures(
        &cold_dir,
        &tools,
        &source,
        &cold,
        Some([last, last, &flow.accepted[1]]),
    )
    .await;
    live_tcp::write_new(&directory.join("success.json"),&serde_json::to_vec_pretty(&json!({
        "probe_version":1,"scope":"ordinary POP edit/growth/shrink transport, not receiver acquisition/display",
        "editor_saved_sha256":live_tcp::hash(&saved),"source_sha256":live_tcp::hash(&source),
        "source_resets_before_first_attachment":1,"source_resets_after_first_attachment":0,
        "warm_resource_counters":[1,2,3,4],"other_resource_counters":1,"fresh_resource_counters":1,
        "warm":warm_reports,"cold":cold_reports,"three_original_responses_unchanged":true,
        "full_rasters_exact":true,"packet_extent_and_following_pointer_grow_and_shrink":true,
        "explicit_publication_and_reopen":true,"channel2_profiles_originals_packages_unchanged":true,
        "owned_producers_closed":true,"independent_or_suitable_client_acceptance_claim":false,
        "full_engine_acceptance_claim":false
    })).unwrap()).unwrap();
}

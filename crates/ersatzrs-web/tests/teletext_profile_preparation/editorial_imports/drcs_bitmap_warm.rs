//! Actual imported resource updates use this response's completed lifecycle.
//! Carousel owner completion is simulated; this is not HTTP, sink or AV evidence.

use super::super::client_capture;
use super::*;
use ersatzrs_core::repositories::TeletextPublicationMutation;
use ersatzrs_domain::TeletextResourceKind;
use ersatzrs_ffmpeg::teletext::source_header::{
    observe_dvb_teletext_header, TeletextHeaderObservation,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextCarousel, TeletextCarouselBatch,
    TeletextCarouselBatchKind, TeletextResourceFullUpdate,
};
use serde_json::{json, Value};
use std::sync::Arc;

fn unchanged_mot_policy(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = drcs_companion::encoding_policy();
    for transmission in &mut policy.transmissions {
        if transmission.resource.kind() == TeletextResourceKind::Mot {
            transmission.update = TeletextResourceFullUpdate::Unchanged;
        }
    }
    policy
}

fn cycle(carousel: &mut TeletextCarousel) -> Vec<TeletextCarouselBatch> {
    let mut batches = Vec::new();
    for _ in 0..4096 {
        let batch = carousel.next_batch().unwrap();
        let done = batch.kind() == TeletextCarouselBatchKind::Terminator;
        batches.push(batch);
        if done {
            assert!(carousel.is_cycle_boundary());
            return batches;
        }
    }
    panic!("fixed synthetic service exceeded the bounded cycle extent");
}

fn assert_counters(batches: &[TeletextCarouselBatch], drcs: u8, mot: u8) {
    let mut observed = [0_usize; 2];
    for batch in batches {
        if batch.kind() != TeletextCarouselBatchKind::Header {
            continue;
        }
        assert_eq!(batch.data_units().len(), 1);
        let TeletextHeaderObservation::Header(header) =
            observe_dvb_teletext_header(&batch.data_units()[0]).unwrap()
        else {
            panic!("planned header must decode as a header");
        };
        assert_eq!(header.corrected_header_bytes, 0);
        let (index, expected) = match header.wire_page {
            0xc0 => (0, drcs),
            0xfe => (1, mot),
            _ => continue,
        };
        assert_eq!(header.packet_address.wire_magazine, 1);
        assert_eq!(header.subcode & 15, 0, "logical S1 stays immutable");
        assert_eq!((header.subcode >> 4) & 7, u16::from(expected));
        observed[index] += 1;
    }
    assert!(observed.into_iter().all(|count| count > 0));
}

fn assert_same_bodies(a: &[TeletextCarouselBatch], b: &[TeletextCarouselBatch]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert_eq!(a.kind(), b.kind());
        if a.kind() != TeletextCarouselBatchKind::Header {
            assert_eq!(a.data_units(), b.data_units());
        }
    }
}

fn baseline(service: &EncodedTeletextAuthoredService) -> Vec<Vec<[u8; 46]>> {
    assert!(service.resource_lifecycle_variants().is_some());
    for page in service.pages() {
        if let Some(header) = page.resource_header() {
            assert_eq!(header.update_counter().get(), 1);
        }
    }
    designs::units(service)
}

fn assert_second_preview(
    first: &EncodedTeletextAuthoredService,
    second: &EncodedTeletextAuthoredService,
) {
    assert_eq!(first.magazine_units(), second.magazine_units());
    assert_eq!(first.inventory(), second.inventory());
    for number in [124, 125] {
        let a = external_editor_probe::preview_page(first, number);
        let b = external_editor_probe::preview_page(second, number);
        assert_eq!((a.width(), a.height()), (b.width(), b.height()));
        let changed: Vec<_> = a
            .pixels()
            .iter()
            .zip(b.pixels())
            .enumerate()
            .filter_map(|(index, (a, b))| (a != b).then_some(index))
            .collect();
        assert_eq!(changed, [(4 + 8) * 12 + 9 + a.width() * 10]);
    }
    for (a, b) in first.pages().iter().zip(second.pages()) {
        assert_eq!(a.resource_header(), b.resource_header());
        if a.resource_header()
            .is_none_or(|h| h.identity().kind() != TeletextResourceKind::Drcs)
        {
            assert_eq!(a.data_units(), b.data_units());
        }
    }
}

async fn request_second(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    previous: &AcceptedTeletextOutputCandidate,
) -> TeletextPublicationRequest {
    let requested =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            concept.head().id,
            concept.head().revision,
            concept.digest().to_owned(),
            concept.head().revision,
            Some(previous.generation().request.id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = requested else {
        panic!("checked second concept must admit explicit publication intent");
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(previous.generation().clone())
    );
    pending.request().clone()
}

async fn accept_second(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    pending: TeletextPublicationRequest,
    previous: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending,
                expected_design_head: concept.head().revision,
                expected_generation_id: Some(previous.generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}

async fn first_update(
    fixture: &Fixture,
    flow: &Flow,
    actual: &Path,
) -> (
    SavedEditorialTeletextDesign,
    AcceptedTeletextOutputCandidate,
) {
    let concept = import(
        fixture,
        drcs_bitmap::actual_editor_request(&flow.receipts[0], actual),
    )
    .await;
    assert_original_pointers(fixture, flow).await;
    let historical = reload_original_teletext_design_revision(
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
    assert!(historical
        .encoded()
        .unwrap()
        .resource_lifecycle_variants()
        .is_none());
    let accepted = publish_and_accept(fixture, flow, &concept).await;
    assert_eq!(accepted.generation().artifact_version, 3);
    drcs_bitmap::assert_only_glyph_pixels(flow.accepted[0].service(), accepted.service());
    (concept, accepted)
}

fn completed_first(
    original: &Arc<EncodedTeletextAuthoredService>,
    first: &Arc<EncodedTeletextAuthoredService>,
    unchanged_mot: bool,
) -> TeletextCarousel {
    let mut warm = TeletextCarousel::new_authored_shared(Arc::clone(original)).unwrap();
    assert_counters(&cycle(&mut warm), 1, 1);
    // Replaced-but-never-emitted candidates acknowledge nothing. Both
    // selections must still resolve from completed original 1.
    warm.replace_authored_shared_at_boundary(Arc::clone(first))
        .unwrap();
    warm.replace_authored_shared_at_boundary(Arc::clone(first))
        .unwrap();
    let first_cycle = cycle(&mut warm);
    let mot = if unchanged_mot { 1 } else { 2 };
    assert_counters(&first_cycle, 2, mot);
    let mut cold = TeletextCarousel::new_authored_shared(Arc::clone(first)).unwrap();
    let cold_cycle = cycle(&mut cold);
    assert_counters(&cold_cycle, 1, 1);
    assert_same_bodies(&first_cycle, &cold_cycle);
    let repeated = cycle(&mut warm);
    assert_counters(&repeated, 2, mot);
    assert_same_bodies(&first_cycle, &repeated);
    warm
}

async fn corrected_flow(actual: &Path, unchanged_mot: bool) -> Value {
    let policy = if unchanged_mot {
        unchanged_mot_policy
    } else {
        drcs_companion::encoding_policy
    };
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        drcs_bitmap::response,
        Fixture::new_persistent().await,
        policy,
    ))
    .await;
    let original_units = baseline(flow.accepted[0].service());
    let (concept, accepted) = first_update(&fixture, &flow, actual).await;
    let edited_units = baseline(accepted.service());
    let mut warm = completed_first(
        flow.accepted[0].service(),
        accepted.service(),
        unchanged_mot,
    );
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let second = import(&fixture, drcs_bitmap::bitmap_request(&latest, 2, b'G')).await;
    assert_eq!(second.head().revision, 3);
    let pending = request_second(&fixture, &second, &accepted).await;
    assert_counters(&cycle(&mut warm), 2, if unchanged_mot { 1 } else { 2 });
    let next = accept_second(&fixture, &second, pending, &accepted).await;
    assert_eq!(next.generation().artifact_version, 3);
    let next_units = baseline(next.service());
    assert_second_preview(accepted.service(), next.service());
    warm.replace_authored_shared_at_boundary(Arc::clone(next.service()))
        .unwrap();
    let third = cycle(&mut warm);
    assert_counters(&third, 3, if unchanged_mot { 1 } else { 3 });
    assert_counters(&cycle(&mut warm), 3, if unchanged_mot { 1 } else { 3 });
    // A different response skips the first accepted edit before emitting it.
    // It must choose 2 from completed original 1, rather than inherit warm's 3.
    let mut skipped =
        TeletextCarousel::new_authored_shared(Arc::clone(flow.accepted[0].service())).unwrap();
    assert_counters(&cycle(&mut skipped), 1, 1);
    skipped
        .replace_authored_shared_at_boundary(Arc::clone(accepted.service()))
        .unwrap();
    skipped
        .replace_authored_shared_at_boundary(Arc::clone(next.service()))
        .unwrap();
    let selected = cycle(&mut skipped);
    assert_counters(&selected, 2, if unchanged_mot { 1 } else { 2 });
    assert_same_bodies(&selected, &third);
    assert_eq!(designs::units(flow.accepted[0].service()), original_units);
    assert_eq!(designs::units(accepted.service()), edited_units);
    assert_eq!(designs::units(next.service()), next_units);
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &next);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    json!({"unchanged_mot_baseline":unchanged_mot,"artifact_version":3,
        "selected_drcs_counters":[1,2,3],"repeat_counter_stable":true,
        "pending_and_skipped_do_not_advance_history":true,
        "second_response_selected_counter":2,"accepted_baseline_units_immutable":true,
        "channel2_unchanged":true,"http_sink_av_or_client_proven":false})
}

#[tokio::test]
#[ignore = "requires retained actual QTM files and a create-new delivery evidence directory"]
async fn retained_actual_qtm_resource_updates_follow_completed_history() {
    let actual = std::path::PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_DRCS_CALIBRATION_DIR",
    ));
    let directory = std::path::PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_DRCS_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).unwrap();
    let cases = [
        Box::pin(corrected_flow(&actual, false)).await,
        Box::pin(corrected_flow(&actual, true)).await,
    ];
    live_client::write_new(
        &directory.join("warm-lifecycle.json"),
        &serde_json::to_vec_pretty(&json!({"schema_version":1,"cases":cases})).unwrap(),
    )
    .unwrap();
}

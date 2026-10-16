//! Opt-in reproduction of the imported-resource warm publication lifecycle gap.
//! Carousel owner completion is simulated; this is not HTTP, sink or AV evidence.

use super::super::client_capture;
use super::*;
use ersatzrs_ffmpeg::teletext::{
    TeletextCarousel, TeletextCarouselBatch, TeletextCarouselBatchKind, TeletextResourceFullUpdate,
    TeletextTransportError,
};
use serde_json::{json, Value};
use std::sync::Arc;

fn unchanged_mot_policy(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = drcs_companion::encoding_policy();
    for transmission in &mut policy.transmissions {
        if transmission.resource.kind() == ersatzrs_domain::TeletextResourceKind::Mot {
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

async fn diagnostic(actual: &Path, unchanged_mot: bool) -> Value {
    let policy = if unchanged_mot {
        unchanged_mot_policy
    } else {
        drcs_companion::encoding_policy
    };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        drcs_bitmap::response,
        Fixture::new_persistent().await,
        policy,
    ))
    .await;
    let concept = import(
        &fixture,
        drcs_bitmap::actual_editor_request(&flow.receipts[0], actual),
    )
    .await;
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    drcs_bitmap::assert_only_glyph_pixels(flow.accepted[0].service(), accepted.service());
    let mut cold = TeletextCarousel::new_authored_shared(Arc::clone(accepted.service())).unwrap();
    assert!(!cycle(&mut cold).is_empty());

    let mut warm =
        TeletextCarousel::new_authored_shared(Arc::clone(flow.accepted[0].service())).unwrap();
    let old_cycle = cycle(&mut warm);
    let failure = warm
        .replace_authored_shared_at_boundary(Arc::clone(accepted.service()))
        .expect_err("known imported-resource lifecycle gap must remain visible");
    let TeletextTransportError::ResourceUpdate {
        resource,
        previous,
        requested,
        expected,
        transmission,
        expected_transmission,
    } = failure
    else {
        panic!("expected exact resource-counter rejection, got {failure:?}");
    };
    assert_eq!(previous.get(), 1);
    assert_eq!(requested.get(), 1);
    assert_eq!(expected.get(), 2);
    assert_eq!(transmission, expected_transmission);
    assert_eq!(resource.kind(), ersatzrs_domain::TeletextResourceKind::Drcs);
    assert_eq!(resource.page().magazine(), 1);
    assert_eq!(resource.page().page(), 0xc0);
    assert_eq!(resource.logical_subpage().get(), 0);
    let continuing = cycle(&mut warm);
    assert_eq!(old_cycle.len(), continuing.len());
    for (before, after) in old_cycle.iter().zip(&continuing) {
        assert_eq!(before.kind(), after.kind());
        if before.kind() == TeletextCarouselBatchKind::Page {
            assert_eq!(before.data_units(), after.data_units());
        }
    }
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    fixture.state.pool.close().await;
    json!({"unchanged_mot_baseline":unchanged_mot,"rejected_resource":resource,
        "previous":previous.get(),"requested":requested.get(),"expected":expected.get(),
        "cold_plan_admitted":true,"warm_gap_reproduced":true,
        "old_page_bodies_continue_after_atomic_rejection":true,
        "http_sink_av_or_client_proven":false})
}

#[tokio::test]
#[ignore = "requires retained actual QTM files and a create-new diagnostic directory; reproduces an open publication gap"]
async fn retained_actual_qtm_resource_warm_boundary_diagnostic() {
    let actual = std::path::PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_DRCS_CALIBRATION_DIR",
    ));
    let directory = std::path::PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_DRCS_DELIVERY_DIR",
    ));
    std::fs::create_dir(&directory).unwrap();
    let cases = [
        Box::pin(diagnostic(&actual, false)).await,
        Box::pin(diagnostic(&actual, true)).await,
    ];
    live_client::write_new(
        &directory.join("warm-diagnostic.json"),
        &serde_json::to_vec_pretty(&json!({"schema_version":1,"cases":cases})).unwrap(),
    )
    .unwrap();
}

//! Fresh-receipt composition and separate publication preserve the old output.

use super::*;

async fn publish(
    fixture: &Fixture,
    saved: &SavedEditorialTeletextDesign,
    previous: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    use ersatzrs_core::repositories::TeletextPublicationMutation;
    let pending =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            saved.head().id,
            saved.head().revision,
            saved.digest().to_owned(),
            saved.head().revision,
            Some(previous.generation().request.id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!("explicit publication must use the current request CAS")
    };
    generations::assert_same_output(&recover(fixture, 1).await, previous);
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: saved.head().revision,
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

async fn next(
    fixture: &Fixture,
    current: &SavedEditorialTeletextDesign,
    expected: &Flow,
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, current).await;
    let latest = receipt(fixture, &exported).await;
    import(
        fixture,
        target_request(&latest, current.head().revision, expected),
    )
    .await
}

async fn undo(
    fixture: &Fixture,
    flow: &Flow,
    empty: &SavedEditorialTeletextDesign,
) -> SavedEditorialTeletextDesign {
    let undone = Box::pin(next(fixture, empty, flow)).await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        drcs_bitmap::stored(fixture, &undone).await.document,
        flow.originals[0].document,
        "fresh original TTI restores the exact immutable source document"
    );
    Box::pin(checks::concept(fixture, flow, &undone, flow, 0)).await;
    undone
}

pub(super) async fn run(mut fixture: Fixture, flow: Flow, grown: &Flow, empty: &Flow) {
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, target_request(&flow.receipts[0], 1, grown)).await;
    assert_eq!(concept.head().revision, 2);
    Box::pin(checks::concept(&fixture, &flow, &concept, grown, 1)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(checks::concept(&fixture, &flow, &concept, grown, 1)).await;
    let deleted = Box::pin(next(&fixture, &concept, empty)).await;
    assert_eq!(deleted.head().revision, 3);
    Box::pin(checks::concept(&fixture, &flow, &deleted, empty, 2)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let accepted = Box::pin(publish(&fixture, &deleted, &flow.accepted[0])).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(empty.accepted[0].service())
    );
    fixture = fixture.reopen().await;
    checks::output(&fixture, &accepted, &flow.accepted[1]).await;
    let undone = Box::pin(undo(&fixture, &flow, &deleted)).await;
    checks::output(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    checks::output(&fixture, &accepted, &flow.accepted[1]).await;
    Box::pin(checks::concept(&fixture, &flow, &undone, &flow, 0)).await;
    let restored = Box::pin(publish(&fixture, &undone, &accepted)).await;
    assert_eq!(
        designs::units(restored.service()),
        designs::units(flow.accepted[0].service())
    );
    fixture = fixture.reopen().await;
    checks::output(&fixture, &restored, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

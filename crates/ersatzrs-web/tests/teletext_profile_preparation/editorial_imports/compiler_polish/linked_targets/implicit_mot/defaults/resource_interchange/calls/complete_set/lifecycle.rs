//! Complete-set concept, exact re-export, explicit publication and root undo.
use super::*;

async fn saved(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    wanted: &EncodedTeletextAuthoredService,
) {
    let restored = drcs_bitmap::restored(fixture, concept).await;
    exact_service(restored.encoded().unwrap(), wanted);
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    for (after, before) in restored.producers().iter().zip(original.producers()) {
        assert_eq!(after.original(), before.original());
    }
    assert_retained_storage(fixture, flow).await;
}

async fn exact_reexport(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    target: &Flow,
) -> ExportedOriginalTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    // Every fresh receipt snapshots the complete CURRENT catalogue. Read-only
    // aliases are immutable within its import, not frozen across later exports.
    let expected = target.exported[0].files();
    assert_eq!(exported.files().len(), expected.len());
    for (actual, wanted) in exported.files().iter().zip(expected) {
        assert_eq!(actual.name, wanted.name);
        let differences: Vec<_> = actual
            .bytes
            .iter()
            .zip(&wanted.bytes)
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .take(8)
            .map(|(index, (a, b))| (index, *a, *b))
            .collect();
        assert!(
            actual.bytes == wanted.bytes,
            "complete file {} differs: lengths {}/{}, first differences {:?}",
            actual.name,
            actual.bytes.len(),
            wanted.bytes.len(),
            differences
        );
    }
    exported
}

async fn publish(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    previous: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    use ersatzrs_core::repositories::TeletextPublicationMutation;
    let pending = request_original_teletext_design_publication(
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
    let TeletextPublicationMutation::Requested(pending) = pending else {
        panic!("current exact revision must request publication")
    };
    generations::assert_same_output(&recover(fixture, 1).await, previous);
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
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

pub(super) async fn finish(
    mut fixture: Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Flow,
) {
    let wanted = target.accepted[0].service();
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    Box::pin(saved(&fixture, flow, concept, wanted)).await;
    let _ = Box::pin(exact_reexport(&fixture, concept, target)).await;
    assert_original_pointers(&fixture, flow).await;
    fixture = fixture.reopen().await;
    Box::pin(saved(&fixture, flow, concept, wanted)).await;
    let _ = Box::pin(exact_reexport(&fixture, concept, target)).await;
    assert_original_pointers(&fixture, flow).await;
    let accepted = publish_and_accept(&fixture, flow, concept).await;
    exact_service(accepted.service(), wanted);
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    let exported = Box::pin(exact_reexport(&fixture, concept, target)).await;
    let receipt = receipt(&fixture, &exported).await;
    let mut undo = drcs_bitmap::from_receipt(&receipt, concept.head().revision);
    // Only inverse the two editable roots in the fresh complete export; retain
    // its current immutable sibling snapshots and complete native resource files.
    for (name, before, after) in [
        (companion(&flow.exported[0]), 0xc28, 0x428),
        (passive_name(flow), 0x28a41, 0x28241),
    ] {
        edit_word(
            undo.files
                .iter_mut()
                .find(|file| file.name == name)
                .unwrap(),
            before,
            after,
        );
    }
    let undone = import(&fixture, undo).await;
    assert_eq!(undone.head().revision, 3);
    Box::pin(saved(&fixture, flow, &undone, flow.accepted[0].service())).await;
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    assert_eq!(
        drcs_bitmap::reexport(&fixture, &undone).await.files(),
        flow.exported[0].files()
    );
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    let restored = publish(&fixture, &undone, &accepted).await;
    exact_service(restored.service(), flow.accepted[0].service());
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &restored);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, flow).await;
    assert_retained_storage(&fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

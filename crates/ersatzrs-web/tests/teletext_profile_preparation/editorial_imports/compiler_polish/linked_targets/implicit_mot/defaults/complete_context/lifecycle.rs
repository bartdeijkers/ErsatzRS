//! The admitted complete model survives concept storage, publication and root undo.
use super::*;
use ersatzrs_app::teletext_export_project::TeletextExportFile;

fn service(actual: &EncodedTeletextAuthoredService, wanted: &EncodedTeletextAuthoredService) {
    assert_eq!(actual.declared(), wanted.declared());
    assert_eq!(designs::units(actual), designs::units(wanted));
    assert_eq!(actual.magazine_units(), wanted.magazine_units());
    assert_eq!(preview(actual), preview(wanted));
}

async fn saved(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    wanted: &EncodedTeletextAuthoredService,
    files: &[TeletextExportFile],
) {
    let restored = Box::pin(drcs_bitmap::restored(fixture, concept)).await;
    service(restored.encoded().unwrap(), wanted);
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(restored.sources().len(), original.sources().len());
    assert_retained_storage(fixture, flow).await;
    let exported = Box::pin(drcs_bitmap::reexport(fixture, concept)).await;
    assert_eq!(exported.files(), files);
}

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
        panic!("explicit current request CAS must succeed")
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

pub(super) async fn finish(
    mut fixture: Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Flow,
    directory: Option<&std::path::Path>,
) {
    let wanted = target.accepted[0].service();
    Box::pin(saved(
        &fixture,
        flow,
        concept,
        wanted,
        target.exported[0].files(),
    ))
    .await;
    assert_original_pointers(&fixture, flow).await;
    fixture = fixture.reopen().await;
    Box::pin(saved(
        &fixture,
        flow,
        concept,
        wanted,
        target.exported[0].files(),
    ))
    .await;
    assert_original_pointers(&fixture, flow).await;
    let accepted = publish_and_accept(&fixture, flow, concept).await;
    service(accepted.service(), wanted);
    if let Some(directory) = directory {
        editor::published(
            directory,
            flow.accepted[0].service(),
            wanted,
            accepted.service(),
        );
    }
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    let exported = drcs_bitmap::reexport(&fixture, concept).await;
    let receipt = receipt(&fixture, &exported).await;
    let mut request = drcs_bitmap::from_receipt(&receipt, concept.head().revision);
    let original = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    request
        .files
        .iter_mut()
        .find(|file| file.name == original.name)
        .unwrap()
        .bytes
        .clone_from(&original.bytes);
    let undone = import(&fixture, request).await;
    assert_eq!(undone.head().revision, 3);
    Box::pin(saved(
        &fixture,
        flow,
        &undone,
        flow.accepted[0].service(),
        flow.exported[0].files(),
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    let restored = publish(&fixture, &undone, &accepted).await;
    service(restored.service(), flow.accepted[0].service());
    if let Some(directory) = directory {
        editor::completed(directory, restored.service());
    }
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &restored);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

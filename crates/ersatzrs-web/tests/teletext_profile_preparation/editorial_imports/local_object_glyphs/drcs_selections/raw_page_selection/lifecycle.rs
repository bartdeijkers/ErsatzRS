//! Native reconstruction, explicit publication CAS and exact-root restoration.
use super::*;

pub(super) fn request(
    receipt: &TeletextDesignExportReceipt,
    service: &EncodedTeletextAuthoredService,
    selected: bool,
) -> NativeTeletextObjectPagesSelectRequest {
    NativeTeletextObjectPagesSelectRequest {
        name: "Explicit retained Normal and Global DRCS raw-page choices".into(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: receipt.metadata.revision,
        selections: choices(service, selected),
        programs: vec![],
    }
}
pub(super) async fn save(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
    service: &EncodedTeletextAuthoredService,
    selected: bool,
) -> SavedEditorialTeletextDesign {
    let result = select_teletext_editorial_object_pages(
        &fixture.state.application,
        request(receipt, service, selected),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("atomic role-only selection saves an unpublished concept: {result:?}")
    };
    saved
}
pub(super) fn same(
    actual: &EncodedTeletextAuthoredService,
    expected: &EncodedTeletextAuthoredService,
) {
    assert_eq!(actual.declared(), expected.declared());
    assert_eq!(designs::units(actual), designs::units(expected));
    assert_eq!(actual.magazine_units(), expected.magazine_units());
    for page in [124, 125] {
        let preview = project_teletext_preview(
            actual,
            channel_documents::address(page, 32),
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            preview_budget(),
        )
        .unwrap();
        assert_eq!(
            preview.raster(),
            &external_editor_probe::preview_page(expected, page)
        );
        assert!(preview.raster().diagnostics().is_empty());
    }
}
pub(super) async fn concept(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
    selected: bool,
) {
    let stored = drcs_bitmap::stored(fixture, saved).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    if selected {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 30);
        assert_eq!(
            overlay.object_page_selections(),
            choices(flow.accepted[0].service(), true)
        );
        assert!(overlay.resource_object_program_overlays().is_empty());
        assert!(overlay.pages().is_empty());
        assert!(overlay.drcs_bitmap_overlays().is_empty());
        assert!(overlay.local_program_operands().is_empty());
        assert!(overlay.local_object_operands().is_empty());
        let mut document: Value = serde_json::from_slice(&stored.document).unwrap();
        document
            .as_object_mut()
            .unwrap()
            .remove("editorial_overlay");
        assert_eq!(
            document,
            serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
        );
    } else {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
    }
    let current = drcs_bitmap::restored(fixture, saved).await;
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.transmissions(), original.transmissions());
    assert_eq!(current.sources().len(), original.sources().len());
    for (actual, expected) in current.sources().iter().zip(original.sources()) {
        assert_eq!(actual.bytes(), expected.bytes());
        assert_eq!(actual.authoring(), expected.authoring());
    }
    assert_eq!(
        current.producers()[0].original(),
        original.producers()[0].original()
    );
    same(current.encoded().unwrap(), expected);
}
pub(super) async fn publish(
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
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(previous.generation().clone())
    );
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
pub(super) async fn undo(
    mut fixture: Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
    retained: &resource_bank_admin::Retained,
) {
    let exported = drcs_bitmap::reexport(&fixture, saved).await;
    let receipt = receipt(&fixture, &exported).await;
    let before = drcs_bitmap::restored(&fixture, saved).await;
    let undone = save(&fixture, &receipt, before.encoded().unwrap(), false).await;
    assert_eq!(undone.head().revision, 3);
    Box::pin(concept(
        &fixture,
        flow,
        &undone,
        flow.accepted[0].service(),
        false,
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, accepted);
    let root = publish(&fixture, &undone, accepted).await;
    same(root.service(), flow.accepted[0].service());
    fixture = fixture.reopen().await;
    Box::pin(concept(
        &fixture,
        flow,
        &undone,
        flow.accepted[0].service(),
        false,
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, &root);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

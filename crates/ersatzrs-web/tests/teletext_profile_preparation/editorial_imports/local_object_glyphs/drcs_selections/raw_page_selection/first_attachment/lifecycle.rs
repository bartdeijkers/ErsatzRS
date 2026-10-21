//! Receipt-bound concept, fresh TTI, explicit publication and root-exact undo.
use super::*;
use ersatzrs_web::host::teletext_editorial_designs::{
    restore_teletext_editorial_page_attachments, NativeTeletextPageAttachmentsRestoreRequest,
};
fn restore_request(
    receipt: &TeletextDesignExportReceipt,
    roles: Vec<(ersatzrs_domain::TeletextPageAddress, TeletextX27LinkRole)>,
) -> NativeTeletextPageAttachmentsRestoreRequest {
    NativeTeletextPageAttachmentsRestoreRequest {
        name: "Restore exact retained-role roots".into(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: receipt.metadata.revision,
        roles,
    }
}

async fn save(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
    selections: Vec<TeletextSavedObjectPageSelectionChange>,
) -> SavedEditorialTeletextDesign {
    let result = select_teletext_editorial_object_pages(
        &fixture.state.application,
        NativeTeletextObjectPagesSelectRequest {
            name: "First retained four-role attachment".into(),
            receipt_id: receipt.metadata.id,
            receipt_digest: receipt.metadata.digest.clone(),
            expected_design_head: receipt.metadata.revision,
            selections,
            programs: vec![],
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("first attachment must save an unpublished concept: {result:?}")
    };
    saved
}
async fn fresh_tti(fixture: &Fixture, flow: &Flow, saved: &SavedEditorialTeletextDesign) {
    let exported = drcs_bitmap::reexport(fixture, saved).await;
    let receipt = receipt(fixture, &exported).await;
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        drcs_bitmap::from_receipt(&receipt, saved.head().revision),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(result,Err(NativeTeletextEditorialImportError::NoChanges)),
        "unchanged current TTI retains first-attachment intent without creating a revision: {result:?}");
    if saved.head().revision == 2 {
        let name = &flow.exported[0]
            .sidecar()
            .project
            .pages
            .iter()
            .find(|page| page.page.page == 227)
            .unwrap()
            .name;
        let original = flow.exported[0]
            .files()
            .iter()
            .find(|file| &file.name == name)
            .unwrap();
        let mut request = drcs_bitmap::from_receipt(&receipt, saved.head().revision);
        request
            .files
            .iter_mut()
            .find(|file| &file.name == name)
            .unwrap()
            .bytes
            .clone_from(&original.bytes);
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(
            matches!(result, Err(NativeTeletextEditorialImportError::Tti(_))),
            "raw X27 cannot manufacture root-undo authority: {result:?}"
        );
    }
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(saved.head().id)
            .await
            .unwrap(),
        Some(saved.head().clone())
    );
}

pub(super) async fn restore(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
    roles: Vec<TeletextX27LinkRole>,
) -> SavedEditorialTeletextDesign {
    let result = restore_teletext_editorial_page_attachments(
        &fixture.state.application,
        restore_request(
            receipt,
            roles
                .into_iter()
                .map(|role| (source::display(), role))
                .collect(),
        ),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("native root restoration saves one concept: {result:?}")
    };
    saved
}
async fn current_receipt(
    fixture: &Fixture,
    saved: &SavedEditorialTeletextDesign,
) -> TeletextDesignExportReceipt {
    receipt(fixture, &drcs_bitmap::reexport(fixture, saved).await).await
}
async fn reject_stale_and_nonattachment_roots(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    latest: &TeletextDesignExportReceipt,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    for rejected in [
        restore_request(
            &flow.receipts[0],
            vec![(source::display(), TeletextX27LinkRole::Pop)],
        ),
        restore_request(
            latest,
            vec![(
                channel_documents::address(124, 32),
                TeletextX27LinkRole::Drcs,
            )],
        ),
    ] {
        let result = restore_teletext_editorial_page_attachments(
            &fixture.state.application,
            rejected,
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(matches!(result,Err(_)|Ok(TeletextDesignMutation::Rejected(_))),"stale receipts and originally selected roles cannot restore a fabricated unselected root: {result:?}");
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(saved.head().id)
                .await
                .unwrap(),
            Some(saved.head().clone())
        );
        generations::assert_same_output(&recover(fixture, 1).await, accepted);
    }
}
async fn retarget_to_retained_bank(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    latest: &TeletextDesignExportReceipt,
    expected: &Flow,
    original_choices: &[TeletextSavedObjectPageSelectionChange],
    accepted: &AcceptedTeletextOutputCandidate,
) -> SavedEditorialTeletextDesign {
    let current = drcs_bitmap::restored(fixture, saved).await;
    let service = current.encoded().unwrap();
    let metadata = &service.declared().semantic().page_enhancements()[&source::display()];
    let resolution = service
        .declared()
        .resolutions()
        .iter()
        .find(|v| v.display() == source::display())
        .unwrap();
    let retarget = TeletextSavedObjectPageSelectionChange::new(
        0,
        TeletextObjectPageSelectionChange::new(
            source::display(),
            TeletextX27LinkRole::Drcs,
            metadata.compositional_links,
            *resolution.role(TeletextX27LinkRole::Drcs),
            source::alternate_declaration(),
        )
        .unwrap(),
    )
    .unwrap();
    let changed = save(fixture, latest, vec![retarget]).await;
    assert_eq!(changed.head().revision, 3);
    let retarget_choices = original_choices
        .iter()
        .map(|saved| {
            if saved.change().role() == TeletextX27LinkRole::Drcs {
                TeletextSavedObjectPageSelectionChange::new(
                    0,
                    TeletextObjectPageSelectionChange::new_attachment(
                        source::display(),
                        TeletextX27LinkRole::Drcs,
                        saved.change().expected_links(),
                        source::alternate_declaration(),
                    )
                    .unwrap(),
                )
                .unwrap()
            } else {
                *saved
            }
        })
        .collect::<Vec<_>>();
    Box::pin(checks::concept(
        fixture,
        flow,
        &changed,
        expected.accepted[0].service(),
        &retarget_choices,
    ))
    .await;
    generations::assert_same_output(&recover(fixture, 1).await, accepted);
    changed
}
pub(super) async fn complete(
    mut fixture: Fixture,
    flow: &Flow,
    target: &Flow,
    retargeted: &Flow,
    partial: &Flow,
) {
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let original_choices = choices(flow.accepted[0].service());
    let saved = save(&fixture, &flow.receipts[0], original_choices.clone()).await;
    assert_eq!(saved.head().revision, 2);
    Box::pin(checks::concept(
        &fixture,
        flow,
        &saved,
        target.accepted[0].service(),
        &original_choices,
    ))
    .await;
    assert_original_pointers(&fixture, flow).await;
    retained.assert_unchanged(&fixture, flow).await;
    Box::pin(fresh_tti(&fixture, flow, &saved)).await;
    fixture = fixture.reopen().await;
    Box::pin(checks::concept(
        &fixture,
        flow,
        &saved,
        target.accepted[0].service(),
        &original_choices,
    ))
    .await;
    let accepted = super::super::lifecycle::publish(&fixture, &saved, &flow.accepted[0]).await;
    checks::same(accepted.service(), target.accepted[0].service());
    let latest = current_receipt(&fixture, &saved).await;
    reject_stale_and_nonattachment_roots(&fixture, flow, &saved, &latest, &accepted).await;
    let changed = retarget_to_retained_bank(
        &fixture,
        flow,
        &saved,
        &latest,
        retargeted,
        &original_choices,
        &accepted,
    )
    .await;
    let latest = current_receipt(&fixture, &changed).await;
    let partially_restored = restore(&fixture, &latest, vec![TeletextX27LinkRole::Drcs]).await;
    assert_eq!(partially_restored.head().revision, 4);
    let remaining = original_choices
        .into_iter()
        .filter(|saved| saved.change().role() != TeletextX27LinkRole::Drcs)
        .collect::<Vec<_>>();
    Box::pin(checks::concept(
        &fixture,
        flow,
        &partially_restored,
        partial.accepted[0].service(),
        &remaining,
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    let latest = current_receipt(&fixture, &partially_restored).await;
    let undone = restore(
        &fixture,
        &latest,
        remaining
            .iter()
            .map(|saved| saved.change().role())
            .collect(),
    )
    .await;
    assert_eq!(undone.head().revision, 5);
    Box::pin(checks::concept(
        &fixture,
        flow,
        &undone,
        flow.accepted[0].service(),
        &[],
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    let root = super::super::lifecycle::publish(&fixture, &undone, &accepted).await;
    checks::same(root.service(), flow.accepted[0].service());
    fixture = fixture.reopen().await;
    Box::pin(checks::concept(
        &fixture,
        flow,
        &undone,
        flow.accepted[0].service(),
        &[],
    ))
    .await;
    Box::pin(fresh_tti(&fixture, flow, &undone)).await;
    generations::assert_same_output(&recover(&fixture, 1).await, &root);
    retained.assert_unchanged(&fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

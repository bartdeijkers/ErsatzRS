//! Existing held-worker actions, immutable roots and explicit publication CAS.
use super::*;

fn attach_request(receipt: &TeletextDesignExportReceipt) -> NativeTeletextObjectPagesChooseRequest {
    NativeTeletextObjectPagesChooseRequest {
        name: "First attach newly created POP and GPOP banks".into(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: receipt.metadata.revision,
        selections: [TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop]
            .into_iter()
            .zip(factories::resources())
            .map(|(role, resource)| TeletextObjectPageChoice {
                display: source::display(),
                role,
                target: resource.page(),
                required_subpages: TeletextSubpageRequirementMask::new(1),
            })
            .collect(),
        calls: vec![],
    }
}
async fn attach(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
) -> SavedEditorialTeletextDesign {
    let result = attach_teletext_editorial_object_pages(
        &fixture.state.application,
        attach_request(receipt),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!(
            "newly created complete banks must attach without replacing their source: {result:?}"
        )
    };
    saved
}
async fn fresh_tti(
    fixture: &Fixture,
    saved: &SavedEditorialTeletextDesign,
) -> TeletextDesignExportReceipt {
    let exported = drcs_bitmap::reexport(fixture, saved).await;
    let current = receipt(fixture, &exported).await;
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        drcs_bitmap::from_receipt(&current, saved.head().revision),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(
        matches!(result, Err(NativeTeletextEditorialImportError::NoChanges)),
        "fresh TTI must retain banks and attachment intent without a new revision: {result:?}"
    );
    current
}
async fn refused(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
    expected: &ersatzrs_domain::TeletextDesignHead,
) {
    let result = attach_teletext_editorial_object_pages(
        &fixture.state.application,
        attach_request(receipt),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(
        !matches!(result, Ok(TeletextDesignMutation::Saved(_))),
        "a missing bank or stale receipt cannot create a bank or alter the head: {result:?}"
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(expected.id)
            .await
            .unwrap(),
        Some(expected.clone())
    );
}

async fn add_banks(
    fixture: &Fixture,
    receipt: &TeletextDesignExportReceipt,
) -> SavedEditorialTeletextDesign {
    let result = add_teletext_editorial_object_resources(
        &fixture.state.application,
        NativeTeletextObjectResourcesAddRequest {
            name: "Create populated invocation-free POP and GPOP banks".into(),
            receipt_id: receipt.metadata.id,
            receipt_digest: receipt.metadata.digest.clone(),
            expected_design_head: 1,
            banks: factories::additions(),
            definitions: vec![],
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("explicit native bank creation saves one concept: {result:?}")
    };
    saved
}

pub(super) async fn complete(mut fixture: Fixture, flow: &Flow, banks: &Flow, target: &Flow) {
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    refused(&fixture, &flow.receipts[0], flow.saved[0].head()).await;
    assert_original_pointers(&fixture, flow).await;
    let bank_only = add_banks(&fixture, &flow.receipts[0]).await;
    assert_eq!(bank_only.head().revision, 2);
    Box::pin(oracles::concept(
        &fixture,
        flow,
        &bank_only,
        banks.accepted[0].service(),
        false,
    ))
    .await;
    assert_original_pointers(&fixture, flow).await;
    let current = fresh_tti(&fixture, &bank_only).await;
    let attached = attach(&fixture, &current).await;
    assert_eq!(attached.head().revision, 3);
    Box::pin(oracles::concept(
        &fixture,
        flow,
        &attached,
        target.accepted[0].service(),
        true,
    ))
    .await;
    assert_original_pointers(&fixture, flow).await;
    let current = fresh_tti(&fixture, &attached).await;
    fixture = fixture.reopen().await;
    Box::pin(oracles::concept(
        &fixture,
        flow,
        &attached,
        target.accepted[0].service(),
        true,
    ))
    .await;
    let accepted =
        super::super::super::lifecycle::publish(&fixture, &attached, &flow.accepted[0]).await;
    checks::same(accepted.service(), target.accepted[0].service());
    refused(&fixture, &flow.receipts[0], attached.head()).await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    let restored = lifecycle::restore(
        &fixture,
        &current,
        vec![TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop],
    )
    .await;
    assert_eq!(restored.head().revision, 4);
    Box::pin(oracles::concept(
        &fixture,
        flow,
        &restored,
        banks.accepted[0].service(),
        false,
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    fresh_tti(&fixture, &restored).await;
    fixture = fixture.reopen().await;
    Box::pin(oracles::concept(
        &fixture,
        flow,
        &restored,
        banks.accepted[0].service(),
        false,
    ))
    .await;
    let root = super::super::super::lifecycle::publish(&fixture, &restored, &accepted).await;
    checks::same(root.service(), banks.accepted[0].service());
    fixture = fixture.reopen().await;
    Box::pin(oracles::concept(
        &fixture,
        flow,
        &restored,
        banks.accepted[0].service(),
        false,
    ))
    .await;
    compiler_polish::assert_pointers(&fixture, &root, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

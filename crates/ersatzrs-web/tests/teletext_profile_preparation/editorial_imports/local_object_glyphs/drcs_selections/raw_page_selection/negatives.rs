//! Fresh TTI retains native role intent; protected DRCS consumers remain protected.
use super::*;

pub(super) async fn fresh_tti(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
) {
    let exported = drcs_bitmap::reexport(fixture, saved).await;
    let receipt = receipt(fixture, &exported).await;
    let request = drcs_bitmap::from_receipt(&receipt, 2);
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(
        matches!(result, Err(NativeTeletextEditorialImportError::NoChanges)),
        "fresh unchanged TTI must retain role choices: {result:?}"
    );
    let mut request = drcs_bitmap::from_receipt(&receipt, 2);
    let page = flow.exported[0]
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124)
        .unwrap();
    let original = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == page.name)
        .unwrap();
    request
        .files
        .iter_mut()
        .find(|file| file.name == page.name)
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
        "raw X27 cannot authorize a role-target undo: {result:?}"
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(saved.head().id)
            .await
            .unwrap(),
        Some(saved.head().clone())
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(saved.head().id, 3, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn retained_drcs_raw_page_choices_refuse_protected_shared_and_equal_pixel_consumers() {
    for factory in [
        source::protected as fn(&str, u16, u16) -> Value,
        source::equal_pixel_protected,
    ] {
        let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            source::encoding,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let result = select_teletext_editorial_object_pages(
            &fixture.state.application,
            lifecycle::request(&flow.receipts[0], flow.accepted[0].service(), true),
            policy(),
            designs::deadline(),
        )
        .await;
        let Err(NativeTeletextEditorialImportError::Effects(error)) = result else {
            panic!("shared Local DRCS target changes require permission even with equal pixels: {result:?}")
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert_eq!(error.position, Some((1, 46)));
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(flow.saved[0].head().id)
                .await
                .unwrap(),
            Some(flow.saved[0].head().clone())
        );
        assert!(fixture
            .state
            .application
            .teletext_design_revision(
                flow.saved[0].head().id,
                2,
                designs::budget().max_serialized_bytes
            )
            .await
            .unwrap()
            .is_none());
        assert_original_pointers(&fixture, &flow).await;
        fixture = fixture.reopen().await;
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        assert!(fixture.playout.try_recv().is_err());
        fixture.state.pool.close().await;
    }
}

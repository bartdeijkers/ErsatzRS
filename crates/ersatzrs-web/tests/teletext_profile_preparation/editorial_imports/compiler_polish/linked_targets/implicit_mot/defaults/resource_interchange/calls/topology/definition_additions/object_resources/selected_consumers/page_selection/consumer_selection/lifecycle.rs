//! Exact complete native targets, fresh TTI, root undo and separate publication.
use super::*;

async fn setup_source(factory: fn(&str, u16, u16) -> Value) -> (Fixture, Flow) {
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await
}
async fn first_selection(
    fixture: &Fixture,
    flow: &Flow,
    before: &BankExpected,
    selected: &BankExpected,
) -> SavedEditorialTeletextDesign {
    let result = select_teletext_editorial_object_consumers(
        &fixture.state.application,
        request(&flow.receipts[0], 1, before, selected, true),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("first explicit Main/Local selection saves concept");
    };
    saved
}

#[tokio::test]
async fn atomic_main_local_resource_choices_retain_tti_preview_root_undo_publication_and_cold_state(
) {
    let [before, selected, edited] = Box::pin(source::all()).await;
    let (mut fixture, flow) = Box::pin(setup_source(source::original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_bank_output(flow.accepted[0].service(), &before);
    let concept = Box::pin(first_selection(&fixture, &flow, &before, &selected)).await;
    let id = concept.head().id;
    assert_eq!(concept.head().revision, 2);
    Box::pin(check(&fixture, &flow, id, 2, &selected, true)).await;
    assert_original_pointers(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let unchanged = import_teletext_editorial_design(
        &fixture.state.application,
        drcs_bitmap::from_receipt(&current, 2),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(
        matches!(
            unchanged,
            Err(NativeTeletextEditorialImportError::NoChanges)
        ),
        "ordinary TTI cannot author Main authority: {unchanged:?}"
    );
    let concept = import(&fixture, scalar_edit(&exported, &current, 2, b'Q', b'R')).await;
    assert_eq!(concept.head().revision, 3);
    Box::pin(check(&fixture, &flow, id, 3, &edited, true)).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let concept = import(&fixture, scalar_edit(&exported, &current, 3, b'R', b'Q')).await;
    assert_eq!(concept.head().revision, 4);
    fixture = fixture.reopen().await;
    Box::pin(check(&fixture, &flow, id, 4, &selected, true)).await;
    let concept = Box::pin(select_all(&fixture, &concept, &selected, &before, false)).await;
    assert_eq!(concept.head().revision, 5);
    Box::pin(check(&fixture, &flow, id, 5, &before, false)).await;
    let concept = Box::pin(select_all(&fixture, &concept, &before, &selected, true)).await;
    assert_eq!(concept.head().revision, 6);
    Box::pin(check(&fixture, &flow, id, 6, &selected, true)).await;
    assert_original_pointers(&fixture, &flow).await;
    let stored = drcs_bitmap::stored(&fixture, &concept).await;
    let accepted = super::super::super::admin::publish(&fixture, &flow, &stored).await;
    assert_bank_output(accepted.service(), &selected);
    fixture = fixture.reopen().await;
    for (revision, expected, on) in [
        (2, &selected, true),
        (3, &edited, true),
        (4, &selected, true),
        (5, &before, false),
        (6, &selected, true),
    ] {
        Box::pin(check(&fixture, &flow, id, revision, expected, on)).await;
    }
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn atomic_main_local_consumers_refuse_missing_shared_authority_and_preserve_source_and_playback(
) {
    let [before, selected, _] = Box::pin(source::all()).await;
    let (fixture, flow) = Box::pin(setup_source(source::original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for variant in 0..8 {
        let mut input = request(&flow.receipts[0], 1, &before, &selected, true);
        let mut bounds = policy();
        let mut deadline = designs::deadline();
        match variant {
            0 => input.expected_design_head = 2,
            1 => {
                input.main_resource_invocations.remove(1);
            }
            2 => {
                input.local_programs.remove(1);
            }
            3 => input.programs.clear(),
            4 => input
                .main_resource_invocations
                .push(input.main_resource_invocations[0]),
            5 => {
                input.main_resource_invocations = main_programs(&selected, &before);
            }
            6 => deadline = std::time::Instant::now(),
            7 => bounds.document.validation.service.max_structure_items = 0,
            _ => unreachable!(),
        }
        let result = select_teletext_editorial_object_consumers(
            &fixture.state.application,
            input,
            bounds,
            deadline,
        )
        .await;
        if variant == 0 {
            assert!(
                matches!(
                    result,
                    Ok(TeletextDesignMutation::Rejected(
                        ersatzrs_core::repositories::TeletextDesignRejection::Conflict
                    ))
                ),
                "stale exact head: {result:?}"
            );
        } else {
            assert!(result.is_err(), "atomic refusal {variant}: {result:?}");
        }
        let head = fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(head.revision, 1);
        assert_original_pointers(&fixture, &flow).await;
        assert_eq!(
            qtm_carrier::stored_receipt(&fixture, flow.receipts[0].metadata.id).await,
            flow.receipts[0]
        );
    }
    retained.assert_unchanged(&fixture, &flow).await;
    fixture.state.pool.close().await;
    Box::pin(generated_refusal(&before, &selected)).await;
}
async fn generated_refusal(before: &BankExpected, selected: &BankExpected) {
    let (mut fixture, flow) = Box::pin(setup_source(source::generated)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_bank_output(flow.accepted[0].service(), before);
    let result = select_teletext_editorial_object_consumers(
        &fixture.state.application,
        request(&flow.receipts[0], 1, before, selected, true),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(
        result.is_err(),
        "new Main/Local writers cannot acquire Generated authority: {result:?}"
    );
    fixture = fixture.reopen().await;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        flow.saved[0].head().id,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_bank_output(restored.encoded().unwrap(), before);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

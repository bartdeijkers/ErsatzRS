//! Existing native lifecycle boundaries with exact cross-scope target assertions.
use super::*;

pub(super) async fn check(
    fixture: &Fixture,
    flow: &Flow,
    id: i64,
    revision: i64,
    expected: &BankExpected,
    crossing: bool,
) -> TeletextDesignRevision {
    let stored = fixture
        .state
        .application
        .teletext_design_revision(id, revision, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    if crossing {
        let overlay = document.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 28);
        assert!(overlay.object_page_selections().is_empty());
        assert_eq!(overlay.main_resource_invocations().len(), 3);
        assert_eq!(overlay.local_object_glyph_program_overlays().len(), 3);
        assert_eq!(overlay.resource_object_program_overlays().len(), 1);
    } else {
        assert!(document.editorial_overlay().is_none());
    }
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        id,
        revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (actual, source) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(actual.bytes(), source.bytes());
        assert_eq!(actual.authoring(), source.authoring());
    }
    assert_bank_output(restored.encoded().unwrap(), expected);
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: id,
            revision,
            owner: Some(channel_documents::address(124, 32)),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(preview.raster(), Some(&expected.rasters[0]));
    assert_eq!(preview.object_choices().main_calls.len(), 3);
    for call in &preview.object_choices().main_calls {
        assert_eq!(call.target, if crossing { public(60) } else { active() });
    }
    for call in &preview.object_choices().calls {
        if call.caller.owner().is_some() {
            assert_eq!(
                call.target,
                if crossing {
                    public(61)
                } else {
                    oracle::target()
                }
            );
        } else if call.caller == public(60) {
            assert_eq!(
                call.target,
                if crossing {
                    oracle::target()
                } else {
                    public(61)
                }
            );
        }
    }
    stored
}

pub(super) async fn target_refusals(fixture: &Fixture, flow: &Flow) {
    for target in [public(65001), public(61)] {
        let mut input = choices(&flow.receipts[0], 1, true);
        input.main_calls[0].target = target;
        let result = choose_teletext_editorial_object_consumers(
            &fixture.state.application,
            input,
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(
            result.is_err(),
            "missing or mistyped Main target refuses: {result:?}"
        );
        let head = fixture
            .state
            .application
            .teletext_design_head(flow.originals[0].design_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(head.revision, 1);
        assert_original_pointers(fixture, flow).await;
        assert_eq!(
            qtm_carrier::stored_receipt(fixture, flow.receipts[0].metadata.id).await,
            flow.receipts[0]
        );
    }
}

pub(super) async fn generated_refusal(original: &BankExpected) {
    let (mut fixture, flow) = Box::pin(setup(source::generated)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_bank_output(flow.accepted[0].service(), original);
    let result = choose_teletext_editorial_object_consumers(
        &fixture.state.application,
        choices(&flow.receipts[0], 1, true),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(
        result.is_err(),
        "cross-role writers cannot gain Generated authority: {result:?}"
    );
    fixture = fixture.reopen().await;
    Box::pin(check(
        &fixture,
        &flow,
        flow.originals[0].design_id,
        1,
        original,
        false,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

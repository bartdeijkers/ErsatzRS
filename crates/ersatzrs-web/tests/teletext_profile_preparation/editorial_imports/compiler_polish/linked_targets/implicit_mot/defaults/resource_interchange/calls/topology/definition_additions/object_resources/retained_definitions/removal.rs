//! Removing an unused retained-bank copy undoes only that explicit addition.
use super::*;
use ersatzrs_core::repositories::TeletextDesignRejection;
use ersatzrs_web::host::teletext_editorial_designs::{
    remove_unused_teletext_editorial_resource_definition,
    NativeTeletextResourceDefinitionRemoveRequest,
};

async fn removal_request(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
) -> NativeTeletextResourceDefinitionRemoveRequest {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let current = receipt(fixture, &exported).await;
    NativeTeletextResourceDefinitionRemoveRequest {
        name: "Remove unused retained copy".into(),
        receipt_id: current.metadata.id,
        receipt_digest: current.metadata.digest.clone(),
        expected_design_head: concept.head().revision,
        identity: copied_identity(),
    }
}

#[tokio::test]
async fn unused_copy_removal_restores_exact_prior_concept_native_export_and_explicit_publication() {
    let first = Box::pin(qualify_banks(false, false)).await;
    let (mut fixture, flow, original) = Box::pin(setup_retained(&first)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let copied = Box::pin(copy_retained(&fixture, &original)).await;
    let request = Box::pin(removal_request(&fixture, &copied)).await;
    let removed = remove_unused_teletext_editorial_resource_definition(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .expect("eligible unused copy must rebuild through the complete current draft");
    let TeletextDesignMutation::Saved(mut removed) = removed else {
        panic!("current unused-copy removal must save an unpublished concept")
    };
    assert_eq!(removed.head().revision, copied.head().revision + 1);
    assert_eq!(removed.digest(), original.digest());
    Box::pin(assert_bank_concept(&fixture, &flow, &removed, &first)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_concept(&fixture, &flow, &removed, &first)).await;
    // The same action must handle standalone additions to an original resource,
    // not just copies folded into a newly retained bank. Keep one functional
    // lifecycle and require the same exact prior concept/native service.
    let source = first
        .declared
        .semantic()
        .objects()
        .definitions()
        .iter()
        .find(|definition| definition.identity.resource().is_some())
        .unwrap()
        .identity;
    let exported = drcs_bitmap::reexport(&fixture, &removed).await;
    let current = receipt(&fixture, &exported).await;
    let copied = copy_teletext_editorial_resource_definition(
        &fixture.state.application,
        NativeTeletextResourceDefinitionCopyRequest {
            name: "Copy in original resource".into(),
            receipt_id: current.metadata.id,
            receipt_digest: current.metadata.digest.clone(),
            expected_design_head: removed.head().revision,
            source,
            new_object_id: TeletextObjectId::new(33),
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(copied) = copied else {
        panic!("current original-resource copy must save a draft")
    };
    let resource = source.resource().unwrap();
    let mut request = Box::pin(removal_request(&fixture, &copied)).await;
    request.identity = match resource.kind() {
        TeletextResourceKind::Pop => {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(33)).unwrap()
        }
        TeletextResourceKind::Gpop => {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(33)).unwrap()
        }
        _ => panic!("resource object must be Public or Global"),
    };
    let result = remove_unused_teletext_editorial_resource_definition(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("unused standalone addition must save a draft")
    };
    removed = saved;
    assert_eq!(removed.digest(), original.digest());
    Box::pin(assert_bank_concept(&fixture, &flow, &removed, &first)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &removed).await;
    assert_bank_output(accepted.service(), &first);
    fixture = fixture.reopen().await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn unused_copy_removal_refuses_original_missing_and_stale_targets_without_head_damage() {
    let first = Box::pin(qualify_banks(false, false)).await;
    let (mut fixture, flow, original) = Box::pin(setup_retained(&first)).await;
    let copied = Box::pin(copy_retained(&fixture, &original)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for variant in 0..4 {
        let mut request = Box::pin(removal_request(&fixture, &copied)).await;
        match variant {
            0 => {
                request.identity = first.declared.semantic().objects().definitions()[0].identity;
            }
            1 => {
                request.identity = TeletextObjectIdentity::public(
                    bank_resources()[1],
                    TeletextObjectId::new(65000),
                )
                .unwrap();
            }
            2 => request.expected_design_head -= 1,
            _ => request.receipt_digest = "0".repeat(64),
        }
        let result = remove_unused_teletext_editorial_resource_definition(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await;
        if variant == 2 {
            assert!(matches!(
                result,
                Ok(TeletextDesignMutation::Rejected(
                    TeletextDesignRejection::Conflict
                ))
            ));
        } else {
            assert!(
                result.is_err(),
                "invalid removal variant {variant} saved a draft"
            );
        }
        let current = fixture
            .state
            .application
            .teletext_design_head(copied.head().id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(current, *copied.head());
        retained.assert_unchanged(&fixture, &flow).await;
        assert_original_pointers(&fixture, &flow).await;
    }
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

//! Typed application intent cannot upgrade an authentic old carrier's authority.
use super::*;
use ersatzrs_domain::TeletextResourceObjectProgramOverlay;

// This deliberately confirms typed application storage only. Native restore
// must independently authenticate the receipt and reject its missing capability.
struct TypedResourceProgramConfirmation;

#[async_trait::async_trait]
impl ersatzrs_app::teletext_designs::TeletextOriginalDesignValidator
    for TypedResourceProgramConfirmation
{
    async fn confirm_original(
        &self,
        document: EncodedTeletextDesignDocuments,
    ) -> Result<
        EncodedTeletextDesignDocuments,
        ersatzrs_app::teletext_designs::TeletextOriginalDesignValidationError,
    > {
        let restored =
            decode_teletext_design_documents(document.bytes(), designs::budget()).unwrap();
        let overlay = restored.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 15);
        assert_eq!(overlay.resource_object_program_overlays().len(), 1);
        assert!(overlay.resource_object_operands().is_empty());
        Ok(document)
    }
}

async fn persist_program(
    fixture: &Fixture,
    flow: &Flow,
    legacy: &TeletextDesignExportReceipt,
) -> TeletextDesignRevision {
    let target = seed::target(21);
    let definition = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(target)
        .unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Passive);
    assert_eq!(definition.program.commands().len(), 5);
    let glyph = TeletextEnhancementCommand::AtColumn {
        column: 1,
        operation: TeletextColumnOperation::G0 {
            code: ersatzrs_domain::TeletextGlyphCode::new(b'P').unwrap(),
        },
    };
    assert_eq!(definition.program.commands()[4], glyph);
    let mut replacement = definition.program.commands().to_vec();
    replacement.insert(4, glyph);
    let change = TeletextResourceObjectProgramOverlay::new(
        target,
        definition.program.clone(),
        ersatzrs_domain::TeletextEnhancementProgram::new(replacement).unwrap(),
        vec![Some(0), Some(1), Some(2), Some(3), None, Some(4)],
        policy().encoding.validation.service,
    )
    .unwrap();
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_program_overlays(
        TeletextSavedEditorialProvenance {
            export_receipt_id: legacy.metadata.id,
            export_receipt_digest: legacy.metadata.digest.clone(),
            original_design_id: flow.saved[0].head().id,
            original_revision: 1,
            original_digest: flow.saved[0].digest().to_owned(),
        },
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        (vec![], vec![change.clone()]),
    )
    .unwrap();
    let document = append_teletext_design_editorial_overlay(
        &flow.originals[0].document,
        &overlay,
        designs::budget(),
    )
    .unwrap();
    let mutation = fixture
        .state
        .application
        .save_original_teletext_design(
            "Typed resource program without native legacy capability".to_owned(),
            Some((flow.saved[0].head().id, 1)),
            document,
            &TypedResourceProgramConfirmation,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap();
    let TeletextDesignMutation::Saved(head) = mutation else {
        panic!("application-only program witness must append an immutable concept");
    };
    assert_eq!(head.revision, 2);
    let stored = fixture
        .state
        .application
        .teletext_design_revision(head.id, 2, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    assert_eq!(
        decoded
            .editorial_overlay()
            .unwrap()
            .resource_object_program_overlays(),
        &[change]
    );
    assert_ne!(stored.digest, flow.originals[0].digest);
    stored
}

#[tokio::test]
async fn typed_resource_program_against_legacy_v4_cannot_restore_or_publish() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let legacy = legacy_receipt(&fixture, &flow).await;
    let stored = persist_program(&fixture, &flow, &legacy).await;
    for reopen in [false, true] {
        if reopen {
            fixture = fixture.reopen().await;
        }
        let error = reload_original_teletext_design_revision(
            &fixture.state.application,
            stored.design_id,
            2,
            designs::budget(),
            seed::encoding(),
            designs::deadline(),
        )
        .await
        .err()
        .unwrap();
        assert_native_resource_refusal(error);
        let error = request_original_teletext_design_publication(
            &fixture.state.application,
            stored.design_id,
            2,
            stored.digest.clone(),
            2,
            Some(flow.pending[0].id),
            designs::budget(),
            seed::encoding(),
            designs::deadline(),
        )
        .await
        .err()
        .unwrap();
        let NativeTeletextPublicationError::Design(error) = error else {
            panic!("legacy publication must refuse unqualified resource structure");
        };
        assert_native_resource_refusal(error);
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        assert_eq!(
            qtm_carrier::stored_receipt(&fixture, legacy.metadata.id).await,
            legacy
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_revision(
                    stored.design_id,
                    2,
                    designs::budget().max_serialized_bytes,
                )
                .await
                .unwrap()
                .unwrap(),
            stored
        );
        let original = reload_original_teletext_design_revision(
            &fixture.state.application,
            stored.design_id,
            1,
            designs::budget(),
            seed::encoding(),
            designs::deadline(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(original.digest(), flow.saved[0].digest());
        assert!(fixture.playout.try_recv().is_err());
    }
    fixture.state.pool.close().await;
}

//! Stored resource intent must not restore/publish as its unchanged original.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    EncodedTeletextDesignDocuments, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance,
};
use ersatzrs_domain::TeletextResourceObjectOperandOverlay;
use ersatzrs_web::host::teletext_designs::{
    request_original_teletext_design_publication, NativeTeletextDesignError,
    NativeTeletextPublicationError,
};

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = seed::response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let region = page["regions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|region| region["id"] == "guide-content")
            .unwrap();
        region["kind"] = json!("zone");
        region["width"] = json!(1);
        page["dynamic_fields"] = json!([]);
    }
    value
}

// Deliberately admits typed application bytes only, following designs.rs's
// structural negative fixture. This is not a native-validation witness.
struct TypedResourceFixtureConfirmation;

#[async_trait::async_trait]
impl ersatzrs_app::teletext_designs::TeletextOriginalDesignValidator
    for TypedResourceFixtureConfirmation
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
        assert_eq!(overlay.schema_version(), 14);
        assert_eq!(overlay.resource_object_operands().len(), 1);
        Ok(document)
    }
}

fn original_resource_operand(flow: &Flow) -> TeletextResourceObjectOperandOverlay {
    let target = seed::target(21);
    let definition = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(target)
        .unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Passive);
    assert_eq!(
        definition.program.commands()[2],
        TeletextEnhancementCommand::AtColumn {
            column: 1,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(2).unwrap()
            },
        }
    );
    let name = protected_file(&flow.exported[0], TeletextResourceKindWire::Pop, 0xc0);
    assert_eq!(
        std::path::Path::new(name).extension(),
        Some(std::ffi::OsStr::new("dvb"))
    );
    let raw = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    assert!(!raw.bytes.is_empty());
    let prefix = name.trim_end_matches(".dvb");
    let companion = flow.exported[0]
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| {
            asset.name.starts_with(prefix)
                && std::path::Path::new(&asset.name).extension()
                    == Some(std::ffi::OsStr::new("tti"))
        });
    // This genuine old receipt authorizes no resource-object TTI editor view.
    assert!(companion.is_none());
    assert_eq!(
        flow.exported[0].sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v4"
    );
    let colour = |value| TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(value).unwrap(),
    };
    TeletextResourceObjectOperandOverlay::new(target, 2, 1, colour(2), colour(3)).unwrap()
}

async fn persist_typed_resource_intent(fixture: &Fixture, flow: &Flow) -> TeletextDesignRevision {
    let operand = original_resource_operand(flow);
    let exported = &flow.exported[0];
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_operands(
        TeletextSavedEditorialProvenance {
            export_receipt_id: exported.metadata().id,
            export_receipt_digest: exported.metadata().digest.clone(),
            original_design_id: flow.saved[0].head().id,
            original_revision: 1,
            original_digest: flow.saved[0].digest().to_owned(),
        },
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![operand],
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
            "Typed resource intent without native editor authority".to_owned(),
            Some((flow.saved[0].head().id, 1)),
            document,
            &TypedResourceFixtureConfirmation,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap();
    let TeletextDesignMutation::Saved(head) = mutation else {
        panic!("application-only typed fixture must append one immutable revision");
    };
    assert_eq!(head.revision, 2);
    let stored = fixture
        .state
        .application
        .teletext_design_revision(head.id, 2, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let restored = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    assert_eq!(
        restored
            .editorial_overlay()
            .unwrap()
            .resource_object_operands(),
        &[operand]
    );
    assert_ne!(stored.digest, flow.originals[0].digest);
    stored
}

fn assert_native_resource_refusal(error: NativeTeletextDesignError) {
    assert!(matches!(error, NativeTeletextDesignError::Editorial(outer)
        if matches!(outer.as_ref(), NativeTeletextEditorialImportError::Design(inner)
            if matches!(inner.as_ref(), NativeTeletextDesignError::EditorialValidationUnavailable))));
}

#[tokio::test]
async fn typed_resource_concept_against_old_receipt_cannot_restore_or_publish_as_original() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let stored = persist_typed_resource_intent(&fixture, &flow).await;
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
            panic!("publication must refuse native resource qualification");
        };
        assert_native_resource_refusal(error);
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        let restored = reload_original_teletext_design_revision(
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
        assert_eq!(restored.digest(), flow.saved[0].digest());
        generations::assert_same_output(&recover(&fixture, 1).await, &flow.accepted[0]);
        generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
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
        assert!(fixture.playout.try_recv().is_err());
    }
    fixture.state.pool.close().await;
}

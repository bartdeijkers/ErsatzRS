//! Receipt-bound POP operands, including fail-closed legacy receipt recovery.
#[path = "resource_interchange/actual_editor_trial.rs"]
mod actual_editor_trial;
#[path = "resource_interchange/fragments.rs"]
mod fragments;

use super::*;
use ersatzrs_app::teletext_design_documents::{
    append_teletext_design_editorial_overlay, decode_teletext_design_documents,
    EncodedTeletextDesignDocuments, TeletextSavedEditorialOverlay,
    TeletextSavedEditorialProvenance,
};
use ersatzrs_app::teletext_export_project::{
    parse_teletext_export_receipt_body, write_teletext_export_receipt_body,
};
use ersatzrs_core::repositories::TeletextDesignExportSave;
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
    original_resource_operand_at(flow, 2)
}
fn original_resource_operand_at(
    flow: &Flow,
    command: usize,
) -> TeletextResourceObjectOperandOverlay {
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
        definition.program.commands()[command],
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
    let colour = |value| TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(value).unwrap(),
    };
    TeletextResourceObjectOperandOverlay::new(target, command, 1, colour(2), colour(3)).unwrap()
}

// Application-only witness for an authentic retained pre-v6 server receipt.
// Native regeneration must still authenticate its exact legacy carrier body.
struct LegacyExportConfirmation<'a>(&'a TeletextDesignRevision, &'a [u8]);

#[async_trait::async_trait]
impl ersatzrs_app::teletext_design_exports::TeletextDesignExportValidator
    for LegacyExportConfirmation<'_>
{
    async fn confirm_export(
        &self,
        original: &TeletextDesignRevision,
        export: &TeletextDesignExportSave,
    ) -> Result<(), ersatzrs_app::teletext_design_exports::TeletextDesignExportValidationError>
    {
        assert_eq!(original, self.0);
        assert_eq!(export.receipt, self.1);
        Ok(())
    }
}

async fn legacy_receipt(fixture: &Fixture, flow: &Flow) -> TeletextDesignExportReceipt {
    let mut body =
        parse_teletext_export_receipt_body(&flow.receipts[0].receipt, policy().project).unwrap();
    assert_eq!(body.project.editor_profile, "qteletextmaker-1.0-carrier-v6");
    let names: Vec<_> = body
        .project
        .assets
        .iter()
        .filter_map(|asset| {
            matches!(asset.scope, TeletextExportAssetScope::ObjectTtiV1 { .. })
                .then_some(asset.name.clone())
        })
        .collect();
    assert_eq!(names.len(), 3);
    body.project
        .assets
        .retain(|asset| !names.contains(&asset.name));
    body.files.retain(|file| !names.contains(&file.name));
    "qteletextmaker-1.0-carrier-v4".clone_into(&mut body.project.editor_profile);
    let bytes = write_teletext_export_receipt_body(&body, policy().project).unwrap();
    let original = &flow.originals[0];
    let mutation = fixture
        .state
        .application
        .create_teletext_design_export(
            TeletextDesignExportSave {
                design_id: original.design_id,
                revision: 1,
                design_digest: original.digest.clone(),
                expected_design_head: 1,
                receipt_version: 1,
                receipt: bytes.clone(),
            },
            &LegacyExportConfirmation(original, &bytes),
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap();
    let TeletextDesignMutation::Saved(metadata) = mutation else {
        panic!("canonical retained V4 receipt must persist");
    };
    let receipt = qtm_carrier::stored_receipt(fixture, metadata.id).await;
    assert_eq!(receipt.receipt, bytes);
    receipt
}

async fn persist_typed_resource_intent(
    fixture: &Fixture,
    flow: &Flow,
    receipt: &TeletextDesignExportReceipt,
) -> TeletextDesignRevision {
    let operand = original_resource_operand(flow);
    let overlay = TeletextSavedEditorialOverlay::new_with_resource_object_operands(
        TeletextSavedEditorialProvenance {
            export_receipt_id: receipt.metadata.id,
            export_receipt_digest: receipt.metadata.digest.clone(),
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
    let legacy = legacy_receipt(&fixture, &flow).await;
    let stored = persist_typed_resource_intent(&fixture, &flow, &legacy).await;
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
        assert_eq!(
            qtm_carrier::stored_receipt(&fixture, legacy.metadata.id).await,
            legacy
        );
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

fn resource_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    from: u8,
    to: u8,
) -> NativeTeletextEditorialImportRequest {
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    assert_eq!(body.project.editor_profile, "qteletextmaker-1.0-carrier-v6");
    let resource = body
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope, TeletextExportAssetScope::Resource { identity, .. }
            if identity.kind == TeletextResourceKindWire::Pop && identity.page.page == 0xc0)
        })
        .unwrap();
    let asset = body
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope, TeletextExportAssetScope::ObjectTtiV1 {
            resource_name, object_id: 21, editor_page: 0x100, editor_ordinal: 0,
        } if resource_name == &resource.name)
        })
        .unwrap();
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Prepared original POP foreground concept".clone_into(&mut request.name);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == asset.name)
        .unwrap();
    file.bytes = edit_foreground_file(&file.bytes, from, to);
    request
}

fn edit_foreground_file(bytes: &[u8], from: u8, to: u8) -> Vec<u8> {
    let original = parse_teletext_tti(bytes, policy().tti).unwrap();
    let mut records = original.records().to_vec();
    // Literal X26 column1, mode0: only the authenticated foreground value moves.
    let old = 1 | (u32::from(from) << 11);
    let new = 1 | (u32::from(to) << 11);
    match (from, to) {
        (2, 3) => assert_eq!((old, new), (0x1001, 0x1801)),
        (3, 2) => assert_eq!((old, new), (0x1801, 0x1001)),
        _ => panic!("fixture requires the literal green/yellow transition"),
    }
    let mut positions = Vec::new();
    for (record_index, record) in records.iter_mut().enumerate() {
        if let TeletextTtiRecord::Packet { row: 26, words, .. } = record {
            for (index, word) in words.iter_mut().enumerate() {
                if *word == old {
                    positions.push((record_index, index));
                    *word = new;
                }
            }
        }
    }
    assert_eq!(
        positions.len(),
        1,
        "one original definition body, not its consumers"
    );
    let mut reverted = records.clone();
    let (record, index) = positions[0];
    let TeletextTtiRecord::Packet { words, .. } = &mut reverted[record] else {
        unreachable!()
    };
    words[index] = old;
    assert_eq!(reverted, original.records());
    write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap()
}

async fn assert_resource_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    colour: u8,
) -> Vec<TeletextRaster> {
    Box::pin(assert_resource_concept_at(
        fixture, flow, concept, colour, 2,
    ))
    .await
}

async fn assert_resource_concept_at(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    colour: u8,
    command: usize,
) -> Vec<TeletextRaster> {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let changes = if colour == 2 {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
        vec![]
    } else {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 14);
        assert!(overlay.pages().is_empty());
        assert!(overlay.local_program_operands().is_empty());
        assert!(overlay.local_object_operands().is_empty());
        assert!(overlay.local_object_glyph_program_overlays().is_empty());
        assert!(overlay.drcs_bitmap_overlays().is_empty());
        let changes = vec![original_resource_operand_at(flow, command)];
        assert_eq!(overlay.resource_object_operands(), changes);
        changes
    };
    let current = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.transmissions(), original.transmissions());
    assert_eq!(current.sources().len(), original.sources().len());
    for (source, expected) in current.sources().iter().zip(original.sources()) {
        assert_eq!(source.bytes(), expected.bytes());
        assert_eq!(source.authoring(), expected.authoring());
    }
    assert_eq!(
        current.producers()[0].original(),
        original.producers()[0].original()
    );
    let before = flow.accepted[0].service();
    let after = current.encoded().unwrap();
    let expected = if changes.is_empty() {
        before.declared().clone()
    } else {
        before
            .declared()
            .clone()
            .with_resource_object_operand_overlays(&changes, policy().encoding.validation.service)
            .unwrap()
    };
    assert_eq!(after.declared(), &expected);
    assert_eq!(after.magazine_units(), before.magazine_units());
    let mask = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    (124..127).map(|page| {
        let preview = project_teletext_preview(
            after, channel_documents::address(page, 32),
            TeletextColourPixelOptions { reveal_concealed: false, flash_phase: TeletextFlashPhase::Original },
            preview_budget(),
        ).unwrap();
        let old = external_editor_probe::preview_page(before, page);
        let raster = preview.raster();
        assert_eq!((raster.width(), raster.height()), (672, 250));
        for y in 0..250 {
            for x in 0..672 {
                if (60..72).contains(&x) && (40..50).contains(&y) {
                    let rgb = if mask[y - 40] & (1 << (11 - (x - 60))) == 0 {
                        [0, 0, 0]
                    } else if colour == 2 { [0, 15, 0] } else { [15, 15, 0] };
                    assert!(matches!(raster.pixel(x,y),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get() == rgb));
                } else {
                    assert_eq!(raster.pixel(x,y), old.pixel(x,y), "outside shared Passive P effect");
                }
            }
        }
        raster.clone()
    }).collect()
}

#[tokio::test]
async fn original_pop_tti_scalar_previews_publishes_repeats_undoes_and_reopens() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(
        flow.accepted[0].selection().bindings[0].addon_id,
        flow.accepted[1].selection().bindings[0].addon_id
    );
    assert_ne!(
        flow.accepted[0].selection().bindings[0].profile_id,
        flow.accepted[1].selection().bindings[0].profile_id
    );
    let concept = import(&fixture, resource_request(&flow.receipts[0], 1, 2, 3)).await;
    assert_eq!(concept.head().revision, 2);
    let previews = Box::pin(assert_resource_concept(&fixture, &flow, &concept, 3)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_resource_concept(&fixture, &flow, &concept, 3)).await,
        previews
    );
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        (124..127)
            .map(|page| external_editor_probe::preview_page(day.service(), page))
            .collect::<Vec<_>>(),
        previews
    );
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(&fixture, resource_request(&latest, 2, 3, 2)).await;
    assert_eq!(undone.head().revision, 3);
    Box::pin(assert_resource_concept(&fixture, &flow, &undone, 2)).await;
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(&fixture, resource_request(&latest, 3, 2, 3)).await;
    assert_eq!(repeated.head().revision, 4);
    assert_eq!(
        Box::pin(assert_resource_concept(&fixture, &flow, &repeated, 3)).await,
        previews
    );
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_resource_concept(&fixture, &flow, &repeated, 3)).await,
        previews
    );
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn generated_consumer(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    let protected = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|page| page["page"]["page"] == 125)
        .unwrap();
    let region = protected["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "guide-content")
        .unwrap();
    region["kind"] = json!("generated");
    value
}

#[tokio::test]
async fn original_pop_tti_scalar_cannot_change_a_generated_shared_consumer() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated_consumer,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        resource_request(&flow.receipts[0], 1, 2, 3),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("shared generated consumer must fail effect permissions: {error:?}");
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(125, 32)));
    assert_eq!(error.position, Some((4, 5)));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap()
            .unwrap(),
        *flow.saved[0].head()
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

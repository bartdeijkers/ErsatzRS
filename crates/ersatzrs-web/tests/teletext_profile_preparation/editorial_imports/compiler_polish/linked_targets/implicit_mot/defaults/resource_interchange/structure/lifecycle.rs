//! Resource structure concepts survive explicit publication, undo and durable reopen.
use super::*;
use ersatzrs_core::repositories::TeletextPublicationMutation;
use ersatzrs_domain::{TeletextEnhancementProgram, TeletextResourceObjectProgramOverlay};

fn pixels(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
) -> Vec<TeletextRaster> {
    (124..127)
        .map(|page| external_editor_probe::preview_page(service, page))
        .collect()
}
async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    inserted: bool,
    expected_pixels: &[TeletextRaster],
) -> Vec<TeletextResourceObjectProgramOverlay> {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let changes = if inserted {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 15);
        assert!(overlay.pages().is_empty());
        assert!(overlay.local_program_operands().is_empty());
        assert!(overlay.local_object_operands().is_empty());
        assert!(overlay.local_object_glyph_program_overlays().is_empty());
        assert!(overlay.resource_object_operands().is_empty());
        assert!(overlay.drcs_bitmap_overlays().is_empty());
        let changes = overlay.resource_object_program_overlays();
        assert_eq!(changes.len(), 1);
        let original = flow.accepted[0]
            .service()
            .declared()
            .semantic()
            .objects()
            .definition(seed::target(21))
            .unwrap();
        let change = &changes[0];
        assert_eq!(change.identity(), seed::target(21));
        assert_eq!(change.expected(), &original.program);
        assert_eq!(change.expected().commands().len(), 5);
        assert_eq!(change.replacement().commands().len(), 6);
        let mut expected = original.program.commands().to_vec();
        expected.insert(4, expected[4]);
        assert_eq!(
            change.replacement(),
            &TeletextEnhancementProgram::new(expected).unwrap()
        );
        assert_eq!(change.retained_commands().len(), 6);
        assert_eq!(
            change
                .retained_commands()
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        assert_eq!(
            change
                .retained_commands()
                .iter()
                .filter(|command| command.is_none())
                .count(),
            1
        );
        changes.to_vec()
    } else {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
        vec![]
    };
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (source, root) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(source.bytes(), root.bytes());
        assert_eq!(source.authoring(), root.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let service = restored.encoded().unwrap();
    let before = flow.accepted[0].service();
    let expected = if changes.is_empty() {
        before.declared().clone()
    } else {
        before
            .declared()
            .clone()
            .with_resource_object_program_overlays(&changes, policy().encoding.validation.service)
            .unwrap()
    };
    assert_eq!(service.declared(), &expected);
    assert_eq!(service.magazine_units(), before.magazine_units());
    assert_eq!(service.inventory(), before.inventory());
    assert_eq!(pixels(service).as_slice(), expected_pixels);
    for (old, new) in before.pages().iter().zip(service.pages()) {
        let changed_resource = old
            .resource_header()
            .is_some_and(|header| Some(header.identity()) == seed::target(21).resource());
        if !inserted || !changed_resource {
            assert_eq!(old.data_units(), new.data_units());
        } else {
            assert_ne!(old.data_units(), new.data_units());
        }
    }
    changes
}

fn replace_only_companion(
    flow: &Flow,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
) -> NativeTeletextEditorialImportRequest {
    let name = companion(exported);
    let root_name = companion(&flow.exported[0]);
    assert_eq!(name, root_name);
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let original = request.files.clone();
    let sidecar = request.sidecar.clone();
    let replacement = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == root_name)
        .unwrap()
        .bytes
        .clone();
    request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes = replacement;
    assert_eq!(request.sidecar, sidecar);
    for (before, after) in original.iter().zip(&request.files) {
        assert_eq!(before.name, after.name);
        if before.name != name {
            assert_eq!(before.bytes, after.bytes);
        }
    }
    request
}

async fn publish_current(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    current: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    let mutation = request_original_teletext_design_publication(
        &fixture.state.application,
        concept.head().id,
        concept.head().revision,
        concept.digest().to_owned(),
        concept.head().revision,
        Some(current.generation().request.id),
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(pending) = mutation else {
        panic!("explicit current-head publication must admit");
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(current.generation().clone())
    );
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: concept.head().revision,
                expected_generation_id: Some(current.generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}

async fn repeat_after_root_undo(
    mut fixture: Fixture,
    flow: &Flow,
    undone: &SavedEditorialTeletextDesign,
    qualified: &[TeletextTtiRecord],
    expected_pixels: &[TeletextRaster],
    first: &[TeletextResourceObjectProgramOverlay],
) -> Fixture {
    let exported = drcs_bitmap::reexport(&fixture, undone).await;
    assert_protected_reexport(flow, &exported);
    let latest = receipt(&fixture, &exported).await;
    let mut repeated_request = drcs_bitmap::from_receipt(&latest, 3);
    let first_request = request(flow, true, qualified);
    let name = companion(&exported);
    let edited = &first_request
        .files
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    repeated_request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes
        .clone_from(edited);
    let repeated = import(&fixture, repeated_request).await;
    assert_eq!(repeated.head().revision, 4);
    assert_eq!(
        Box::pin(assert_concept(
            &fixture,
            flow,
            &repeated,
            true,
            expected_pixels
        ))
        .await,
        first
    );
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_concept(
            &fixture,
            flow,
            &repeated,
            true,
            expected_pixels
        ))
        .await,
        first
    );
    fixture
}

#[tokio::test]
async fn original_pop_structural_concept_publishes_reopens_undoes_and_repeats_root_intent() {
    let (qualified, expected_pixels) = Box::pin(qualified_target(true)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(
        flow.exported[0].sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v6"
    );
    assert_eq!(
        expected_pixels,
        pixels(flow.accepted[0].service()),
        "same-cell Insert Copy preserves all three original visible P consumers"
    );
    let concept = import(&fixture, request(&flow, true, &qualified)).await;
    assert_eq!(concept.head().revision, 2);
    let first = Box::pin(assert_concept(
        &fixture,
        &flow,
        &concept,
        true,
        &expected_pixels,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(pixels(day.service()), expected_pixels);
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_concept(
            &fixture,
            &flow,
            &concept,
            true,
            &expected_pixels
        ))
        .await,
        first
    );
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(
        &fixture,
        replace_only_companion(&flow, &exported, &latest, 2),
    )
    .await;
    assert_eq!(undone.head().revision, 3);
    Box::pin(assert_concept(
        &fixture,
        &flow,
        &undone,
        false,
        &expected_pixels,
    ))
    .await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(
        &fixture,
        &flow,
        &undone,
        false,
        &expected_pixels,
    ))
    .await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    let published_undo = Box::pin(publish_current(&fixture, &undone, &day)).await;
    assert_ne!(published_undo.generation().id, day.generation().id);
    for (actual, original) in published_undo
        .service()
        .pages()
        .iter()
        .zip(flow.accepted[0].service().pages())
    {
        assert_eq!(actual.data_units(), original.data_units());
    }
    fixture = Box::pin(repeat_after_root_undo(
        fixture,
        &flow,
        &undone,
        &qualified,
        &expected_pixels,
        &first,
    ))
    .await;
    assert_pointers(&fixture, &published_undo, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn original_pop_structural_delete_cannot_remove_a_generated_shared_consumer() {
    let (qualified, _) = Box::pin(qualified_target(false)).await;
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
        request(&flow, false, &qualified),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("visible deletion must fail Generated consumer permissions: {error:?}");
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
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

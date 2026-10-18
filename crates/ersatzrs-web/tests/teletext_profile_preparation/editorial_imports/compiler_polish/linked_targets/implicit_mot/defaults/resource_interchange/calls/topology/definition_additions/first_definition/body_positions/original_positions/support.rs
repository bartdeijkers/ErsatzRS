//! Receipt/file and native-source assertions shared only by these fixed cases.
use super::*;

type Response = fn(&str, u16, u16) -> Value;
pub(super) async fn qualify(response: Response, target: TeletextObjectIdentity) -> BodyExpected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let expected = expected(&flow, target);
    fixture.state.pool.close().await;
    expected
}
pub(super) fn expected(flow: &Flow, target: TeletextObjectIdentity) -> BodyExpected {
    let service = flow.accepted[0].service();
    BodyExpected {
        definition: service
            .declared()
            .semantic()
            .objects()
            .definition(target)
            .unwrap()
            .clone(),
        records: file_records(&flow.exported[0], name(&flow.exported[0], target)),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    }
}
pub(super) fn name(
    exported: &ExportedOriginalTeletextDesign,
    target: TeletextObjectIdentity,
) -> &str {
    let id = target.object_id().get();
    &exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(asset.scope,
                TeletextExportAssetScope::ObjectTtiV1 {object_id, ..}
                | TeletextExportAssetScope::ObjectContextTtiV1 {object_id, ..} if object_id == id
            )
        })
        .expect("the fixed POP/GPOP original has exactly one selected object companion")
        .name
}
pub(super) fn request(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    target: TeletextObjectIdentity,
    records: &[TeletextTtiRecord],
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, receipt.metadata.revision);
    let filename = name(exported, target);
    let sidecar = request.sidecar.clone();
    let originals = request.files.clone();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == filename)
        .unwrap();
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records.to_vec(), policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(
        parse_teletext_tti(&file.bytes, policy().tti)
            .unwrap()
            .records(),
        records
    );
    assert_eq!(request.sidecar, sidecar);
    for original in originals.iter().filter(|file| file.name != filename) {
        assert_eq!(
            request
                .files
                .iter()
                .find(|file| file.name == original.name)
                .unwrap(),
            original
        );
    }
    request
}

pub(super) async fn assert_saved(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: &BodyExpected,
    require_positions: bool,
) -> TeletextDesignRevision {
    let stored = drcs_bitmap::stored(fixture, saved).await;
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    if require_positions {
        assert_eq!(document.editorial_overlay().unwrap().schema_version(), 19);
    }
    if let Some(overlay) = document.editorial_overlay() {
        assert!(
            overlay.resource_object_definition_additions().is_empty(),
            "no Add authority for an original object"
        );
    }
    let restored = Box::pin(reload_original_teletext_design_revision(
        &fixture.state.application,
        stored.design_id,
        stored.revision,
        designs::budget(),
        encoding(),
        designs::deadline(),
    ))
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (actual, captured) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(actual.bytes(), captured.bytes());
        assert_eq!(actual.authoring(), captured.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared().resource_inventory(), resources());
    assert_eq!(
        service
            .declared()
            .semantic()
            .objects()
            .definition(expected.definition.identity),
        Some(&expected.definition)
    );
    assert_eq!(
        service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        expected.units
    );
    assert_eq!(
        generations::preview(service).raster(),
        generations::preview(flow.accepted[0].service()).raster()
    );
    let preview = Box::pin(preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 1,
            design_id: stored.design_id,
            revision: stored.revision,
            owner: Some(channel_documents::address(124, 32)),
        },
        designs::deadline(),
    ))
    .await
    .unwrap();
    assert_eq!(preview.object_resources(), resources());
    stored
}

pub(super) async fn accept_reopen(
    mut fixture: Fixture,
    flow: &Flow,
    before: &Retained,
    saved: &SavedEditorialTeletextDesign,
    expected: &BodyExpected,
) {
    let accepted = publish_and_accept(&fixture, flow, saved).await;
    assert_eq!(
        accepted
            .service()
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        expected.units
    );
    fixture = fixture.reopen().await;
    Box::pin(assert_saved(&fixture, flow, saved, expected, false)).await;
    generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 40 * 12);
    generations::assert_same_output_width(&recover(&fixture, 2).await, &flow.accepted[1], 40 * 12);
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(accepted.generation().clone())
    );
    assert_retained_storage(&fixture, flow).await;
    assert_eq!(retained(&fixture).await, *before);
    assert!(fixture.playout.is_empty());
    fixture.state.pool.close().await;
}

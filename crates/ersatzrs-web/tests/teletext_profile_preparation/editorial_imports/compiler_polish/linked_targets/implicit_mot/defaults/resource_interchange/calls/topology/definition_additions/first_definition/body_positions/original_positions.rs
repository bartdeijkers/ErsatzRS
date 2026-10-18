//! Original producer-owned resources need their own authenticated position intent.
//! This is not authority from a saved Add, and expected bytes come from a
//! separately installed original producer, not the editorial importer.
use super::*;

fn original_outer_rows(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = body_p(theme, page, subcode);
    value["service"]["objects"][0]["program"]["commands"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"kind":"set_position","position":{"row":3,"column":0}}),
            json!({"kind":"at_column","column":4,"operation":{"kind":"g0","code":81}}),
        ]);
    validate_response(&value);
    value
}
fn original_inserted_row(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_outer_rows(theme, page, subcode);
    value["service"]["objects"][0]["program"]["commands"]
        .as_array_mut()
        .unwrap()
        .splice(
            2..2,
            [
                json!({"kind":"set_position","position":{"row":2,"column":0}}),
                json!({"kind":"at_column","column":6,"operation":{"kind":"g0","code":82}}),
            ],
        );
    validate_response(&value);
    value
}
fn inserted_words() -> Vec<(usize, u32)> {
    [
        POSITION,
        glyph(b'P'),
        0x2a | (4 << 6),
        6 | (9 << 6) | (82 << 11),
        0x2b | (4 << 6),
        4 | (9 << 6) | (81 << 11),
        TERMINATION,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, value)| (index + 3, value))
    .collect()
}
async fn independent_inserted_original() -> BodyExpected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_inserted_row,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let expected = BodyExpected {
        definition: service
            .declared()
            .semantic()
            .objects()
            .definition(identity(0))
            .unwrap()
            .clone(),
        records: file_records(&flow.exported[0], object_name(&flow.exported[0])),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    };
    assert_eq!(expected.definition.program.commands().len(), 6);
    for (index, value) in inserted_words() {
        assert_eq!(word(&expected.records, index), value);
    }
    fixture.state.pool.close().await;
    expected
}

async fn assert_original_position_body(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: &BodyExpected,
) -> TeletextDesignRevision {
    let stored = drcs_bitmap::stored(fixture, saved).await;
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(
        overlay.schema_version(),
        19,
        "original-resource position authority has its own saved role"
    );
    assert!(
        overlay.resource_object_definition_additions().is_empty(),
        "original definitions were never added by the editor"
    );
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
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
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
            .definition(identity(0)),
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
        generations::preview(flow.accepted[0].service()).raster(),
        "the original object is uncalled; inserting its row changes no display"
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

#[tokio::test]
async fn original_producer_object_interior_position_import_preserves_exact_body_and_draft_preview()
{
    let expected = Box::pin(independent_inserted_original()).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_outer_rows,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let before = retained(&fixture).await;
    assert_eq!(flow.originals[0].revision, 1);
    let original_documents =
        decode_teletext_design_documents(&flow.originals[0].document, designs::budget()).unwrap();
    assert!(
        original_documents.editorial_overlay().is_none(),
        "genuine producer original, no saved Add or edit intent"
    );
    let exported = &flow.exported[0];
    let original = receipt(&fixture, exported).await;
    assert_eq!(original.metadata.revision, 1);
    let records = file_records(exported, object_name(exported));
    assert_eq!(
        (3..8)
            .map(|index| word(&records, index))
            .collect::<Vec<_>>(),
        [
            POSITION,
            glyph(b'P'),
            0x2b | (4 << 6),
            4 | (9 << 6) | (81 << 11),
            TERMINATION
        ]
    );
    let request = edited_request(exported, &original, &inserted_words());
    let edited = request
        .files
        .iter()
        .find(|file| file.name == object_name(exported))
        .unwrap();
    assert_eq!(
        parse_teletext_tti(&edited.bytes, policy().tti)
            .unwrap()
            .records(),
        expected.records
    );
    // This positive import is the RED boundary until genuine original-owned
    // position intent is implemented. Do not substitute saved Add authority.
    let saved = import(&fixture, request).await;
    assert_eq!(saved.head().revision, 2);
    let stored = Box::pin(assert_original_position_body(
        &fixture, &flow, &saved, &expected,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(
        file_records(&exported, object_name(&exported)),
        expected.records
    );
    assert_retained_storage(&fixture, &flow).await;
    generations::assert_same_output_width(&recover(&fixture, 2).await, &flow.accepted[1], 40 * 12);
    assert!(fixture.playout.is_empty());
    fixture.state.pool.close().await;
}

#[path = "original_positions/lifecycle.rs"]
mod lifecycle;
#[path = "original_positions/matrix.rs"]
mod matrix;
#[path = "original_positions/protected.rs"]
mod protected;
#[path = "original_positions/support.rs"]
mod support;

#[path = "original_positions/actual_editor_reuse.rs"]
mod actual_editor_reuse;

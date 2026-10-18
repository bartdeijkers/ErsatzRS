//! A later position edit keeps the exact first body and its saved Add authority.
use super::*;

const SECOND_POSITION: u32 = 0x2a | (4 << 6);
const SECOND_GLYPH: u32 = 4 | (9 << 6) | (81 << 11);

fn two_rows(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = body_p(theme, page, subcode);
    value["service"]["objects"][0]["program"]["commands"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"kind":"set_position","position":{"row":2,"column":0}}),
            json!({"kind":"at_column","column":4,"operation":{"kind":"g0","code":81}}),
        ]);
    validate_response(&value);
    value
}
async fn qualify_two_rows() -> BodyExpected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        two_rows,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(identity(0))
        .unwrap()
        .clone();
    assert_eq!(definition.program.commands().len(), 4);
    let records = file_records(&flow.exported[0], object_name(&flow.exported[0]));
    assert_eq!(
        (3..8)
            .map(|index| word(&records, index))
            .collect::<Vec<_>>(),
        [
            POSITION,
            glyph(b'P'),
            SECOND_POSITION,
            SECOND_GLYPH,
            TERMINATION
        ]
    );
    let result = BodyExpected {
        definition,
        records,
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    };
    fixture.state.pool.close().await;
    result
}
async fn assert_no_extension(
    fixture: &Fixture,
    flow: &Flow,
    before: &Retained,
    first: &SavedEditorialTeletextDesign,
    expected: &BodyExpected,
) {
    let head = fixture
        .state
        .application
        .teletext_design_head(first.head().id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head, *first.head());
    assert!(fixture
        .state
        .application
        .teletext_design_revision(head.id, 4, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    Box::pin(assert_body(fixture, flow, first, expected)).await;
    unchanged(fixture, flow, before).await;
}
async fn reject_retained_anchor_mutations(
    fixture: &Fixture,
    flow: &Flow,
    before: &Retained,
    first: &SavedEditorialTeletextDesign,
    expected: &BodyExpected,
    exported: &ExportedOriginalTeletextDesign,
    latest: &TeletextDesignExportReceipt,
) {
    let mutations = [
        vec![(5, SECOND_POSITION | (40 << 11)), (6, SECOND_GLYPH)],
        vec![
            (3, POSITION | (1 << 11)),
            (5, SECOND_POSITION),
            (6, SECOND_GLYPH),
        ],
        vec![
            (3, glyph(b'P')),
            (4, SECOND_POSITION),
            (5, SECOND_GLYPH),
            (6, TERMINATION),
        ],
    ];
    for replacement in mutations {
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            edited_request(exported, latest, &replacement),
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(
            result.is_err(),
            "invalid new column or changed/dropped retained AP cannot save"
        );
        Box::pin(assert_no_extension(fixture, flow, before, first, expected)).await;
    }
}
async fn reject_original_definition_anchor_change() {
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        body_p,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let before = retained(&fixture).await;
    // Row2/P followed by row3/Q is legal geometry, but changes producer AP1.
    // Original-position insertion intent must preserve that original anchor.
    let request = edited_request(
        &flow.exported[0],
        &flow.receipts[0],
        &[
            (3, SECOND_POSITION),
            (5, 0x2b | (4 << 6)),
            (6, SECOND_GLYPH),
        ],
    );
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(
        result.is_err(),
        "new rows cannot change original producer position anchors"
    );
    stored(&fixture, &flow, 0).await;
    unchanged(&fixture, &flow, &before).await;
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn populated_added_definition_appends_row_without_changing_retained_anchors_or_publication() {
    Box::pin(reject_original_definition_anchor_change()).await;
    let empty = Box::pin(qualify_empty(1)).await;
    let p = Box::pin(qualify_body(b'P')).await;
    let two = Box::pin(qualify_two_rows()).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_empty(fixture)).await;
    let before = retained(&fixture).await;
    assert_eq!(
        submit(&fixture, &fields(&flow.originals[0], 0)).await["saved"],
        "draft"
    );
    let added = Box::pin(assert_concept(&fixture, &flow, &empty)).await;
    let exported = Box::pin(export_stored(&fixture, &added)).await;
    let latest = receipt(&fixture, &exported).await;
    let first = import(
        &fixture,
        edited_request(&exported, &latest, &[(3, POSITION), (4, glyph(b'P'))]),
    )
    .await;
    assert_eq!(first.head().revision, 3);
    let current = Box::pin(assert_body(&fixture, &flow, &first, &p)).await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &current)).await;
    assert_eq!(file_records(&exported, object_name(&exported)), p.records);
    let latest = receipt(&fixture, &exported).await;
    Box::pin(reject_retained_anchor_mutations(
        &fixture, &flow, &before, &first, &p, &exported, &latest,
    ))
    .await;
    let request = edited_request(
        &exported,
        &latest,
        &[(5, SECOND_POSITION), (6, SECOND_GLYPH)],
    );
    let edited = request
        .files
        .iter()
        .find(|file| file.name == object_name(&exported))
        .unwrap();
    assert_eq!(
        parse_teletext_tti(&edited.bytes, policy().tti)
            .unwrap()
            .records(),
        two.records
    );
    // The prior checkpoint accepts the first body. This later legal AP is the
    // new runtime RED; expected success must preserve the first typed AP/glyph.
    let second = import(&fixture, request).await;
    assert_eq!(second.head().revision, 4);
    let current = Box::pin(assert_body(&fixture, &flow, &second, &two)).await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &current)).await;
    assert_eq!(file_records(&exported, object_name(&exported)), two.records);
    fixture = fixture.reopen().await;
    Box::pin(assert_body(&fixture, &flow, &second, &two)).await;
    unchanged(&fixture, &flow, &before).await;
    fixture.state.pool.close().await;
}

//! A fresh receipt must undo retained colour and inserted position state together.
use super::*;

fn coloured_root(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = original_outer_rows(theme, page, subcode);
    response["service"]["objects"][0]["program"]["commands"]
        .as_array_mut()
        .unwrap()
        .insert(
            1,
            json!({"kind":"at_column","column":8,
            "operation":{"kind":"foreground","colour":2}}),
        );
    validate_response(&response);
    response
}

fn recoloured(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = coloured_root(theme, page, subcode);
    response["service"]["objects"][0]["program"]["commands"][1]["operation"]["colour"] = json!(3);
    validate_response(&response);
    response
}

fn inserted(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = recoloured(theme, page, subcode);
    response["service"]["objects"][0]["program"]["commands"]
        .as_array_mut()
        .unwrap()
        .splice(
            3..3,
            [
                json!({"kind":"set_position","position":{"row":2,"column":0}}),
                json!({"kind":"at_column","column":6,"operation":{"kind":"g0","code":82}}),
            ],
        );
    validate_response(&response);
    response
}

async fn prepared() -> (
    Fixture,
    Flow,
    Retained,
    SavedEditorialTeletextDesign,
    BodyExpected,
) {
    let coloured = Box::pin(support::qualify(recoloured, identity(0))).await;
    let inserted = Box::pin(support::qualify(inserted, identity(0))).await;
    assert_eq!(word(&coloured.records, 4), 8 | (3 << 11));
    assert_eq!(word(&inserted.records, 6), 0x2a | (4 << 6));
    assert_eq!(word(&inserted.records, 7), 6 | (9 << 6) | (82 << 11));
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        coloured_root,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let before = retained(&fixture).await;
    assert_eq!(
        word(&support::expected(&flow, identity(0)).records, 4),
        8 | (2 << 11)
    );
    let colour = import(
        &fixture,
        support::request(
            &flow.exported[0],
            &flow.receipts[0],
            identity(0),
            &coloured.records,
        ),
    )
    .await;
    assert_eq!(colour.head().revision, 2);
    let stored = Box::pin(support::assert_saved(
        &fixture, &flow, &colour, &coloured, false,
    ))
    .await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    let current = receipt(&fixture, &exported).await;
    let saved = import(
        &fixture,
        support::request(&exported, &current, identity(0), &inserted.records),
    )
    .await;
    assert_eq!(saved.head().revision, 3);
    Box::pin(support::assert_saved(
        &fixture, &flow, &saved, &inserted, true,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    (fixture, flow, before, saved, inserted)
}

#[tokio::test]
async fn original_resource_retained_colour_and_inserted_row_root_undo_publish_and_reopen() {
    let (mut fixture, flow, before, saved, inserted) = Box::pin(prepared()).await;
    fixture = fixture.reopen().await;
    let stored = Box::pin(support::assert_saved(
        &fixture, &flow, &saved, &inserted, true,
    ))
    .await;
    let root = support::expected(&flow, identity(0));
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    let current = receipt(&fixture, &exported).await;
    assert_eq!(current.metadata.revision, 3);
    assert!(
        import_teletext_editorial_design(
            &fixture.state.application,
            edited_request(&exported, &current, &[(8, 0x2c | (4 << 6))]),
            policy(),
            designs::deadline(),
        )
        .await
        .is_err(),
        "combined history never authorizes changing producer AP3 to AP4"
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(saved.head().id)
            .await
            .unwrap(),
        Some(saved.head().clone())
    );
    unchanged(&fixture, &flow, &before).await;
    let mut undo = support::request(&exported, &current, identity(0), &root.records);
    let root_file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == object_name(&flow.exported[0]))
        .unwrap();
    undo.files
        .iter_mut()
        .find(|file| file.name == object_name(&exported))
        .unwrap()
        .bytes
        .clone_from(&root_file.bytes);
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        undo,
        policy(),
        designs::deadline(),
    )
    .await
    .expect("fresh-receipt exact root undo must remove BOTH retained colour and inserted row");
    let TeletextDesignMutation::Saved(undone) = result else {
        panic!("combined root undo must save a new concept");
    };
    assert_eq!(undone.head().revision, 4);
    let stored = Box::pin(support::assert_saved(
        &fixture, &flow, &undone, &root, false,
    ))
    .await;
    assert!(
        decode_teletext_design_documents(&stored.document, designs::budget())
            .unwrap()
            .editorial_overlay()
            .is_none()
    );
    let original = fixture
        .state
        .application
        .teletext_design_revision(
            flow.saved[0].head().id,
            flow.saved[0].head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.document, original.document);
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(
        file_records(&exported, object_name(&exported)),
        root.records
    );
    unchanged(&fixture, &flow, &before).await;
    Box::pin(support::accept_reopen(
        fixture, &flow, &before, &undone, &root,
    ))
    .await;
}

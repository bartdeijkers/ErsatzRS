//! Repeat and complete undo use the current file inventory and a fresh receipt.
use super::support;
use super::*;

#[path = "lifecycle/mixed_root_undo.rs"]
mod mixed_root_undo;

fn scalar_s(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_inserted_row(theme, page, subcode);
    value["service"]["objects"][0]["program"]["commands"][3]["operation"]["code"] = json!(83);
    validate_response(&value);
    value
}
async fn reject_original_anchor_mutations(
    fixture: &Fixture,
    flow: &Flow,
    before: &Retained,
    saved: &SavedEditorialTeletextDesign,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
) {
    // Original row3->row4 is a valid forward program, but changes a captured AP
    // anchor. Invalid inserted column40 must also fail without saving anything.
    for replacement in [(7, 0x2c | (4 << 6)), (5, 0x2a | (4 << 6) | (40 << 11))] {
        let result = import_teletext_editorial_design(
            &fixture.state.application,
            edited_request(exported, receipt, &[replacement]),
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(
            result.is_err(),
            "captured anchors and teletext column bounds remain mandatory"
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
        assert!(fixture
            .state
            .application
            .teletext_design_revision(
                saved.head().id,
                saved.head().revision + 1,
                designs::budget().max_serialized_bytes
            )
            .await
            .unwrap()
            .is_none());
        unchanged(fixture, flow, before).await;
        assert_retained_storage(fixture, flow).await;
    }
}

async fn repeated_original() -> (
    Fixture,
    Flow,
    Retained,
    SavedEditorialTeletextDesign,
    BodyExpected,
) {
    let inserted = Box::pin(independent_inserted_original()).await;
    let repeated_expected = Box::pin(support::qualify(scalar_s, identity(0))).await;
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
    let first = import(
        &fixture,
        support::request(
            &flow.exported[0],
            &flow.receipts[0],
            identity(0),
            &inserted.records,
        ),
    )
    .await;
    assert_eq!(first.head().revision, 2);
    let stored = Box::pin(support::assert_saved(
        &fixture, &flow, &first, &inserted, true,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    let current = receipt(&fixture, &exported).await;
    assert_eq!(current.metadata.revision, 2);
    let repeated = import(
        &fixture,
        support::request(&exported, &current, identity(0), &repeated_expected.records),
    )
    .await;
    assert_eq!(repeated.head().revision, 3);
    Box::pin(support::assert_saved(
        &fixture,
        &flow,
        &repeated,
        &repeated_expected,
        true,
    ))
    .await;
    (fixture, flow, before, repeated, repeated_expected)
}

#[tokio::test]
async fn original_resource_positions_repeat_and_fresh_receipt_root_undo_publish_and_reopen() {
    let (mut fixture, flow, before, repeated, repeated_expected) =
        Box::pin(repeated_original()).await;
    let root = support::expected(&flow, identity(0));
    fixture = fixture.reopen().await;
    let stored = Box::pin(support::assert_saved(
        &fixture,
        &flow,
        &repeated,
        &repeated_expected,
        true,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    let current = receipt(&fixture, &exported).await;
    assert_eq!(current.metadata.revision, 3);
    assert_ne!(current.metadata.id, flow.receipts[0].metadata.id);
    Box::pin(reject_original_anchor_mutations(
        &fixture, &flow, &before, &repeated, &exported, &current,
    ))
    .await;
    // Restore genuine root file bytes under THIS current receipt and sidecar;
    // neither an administrative reset nor a historical receipt is submitted.
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
    assert_eq!(
        undo.sidecar,
        write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap()
    );
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        undo,
        policy(),
        designs::deadline(),
    )
    .await
    .expect("fresh-receipt restoration of the exact original body must import");
    let TeletextDesignMutation::Saved(undone) = result else {
        panic!("restoring the exact root is a new saved concept");
    };
    assert_eq!(
        undone.head().revision,
        4,
        "returning to root is a real edit of the latest concept"
    );
    let stored = Box::pin(support::assert_saved(
        &fixture, &flow, &undone, &root, false,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(
        file_records(&exported, object_name(&exported)),
        root.records
    );
    fixture = fixture.reopen().await;
    Box::pin(support::assert_saved(
        &fixture, &flow, &undone, &root, false,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    Box::pin(support::accept_reopen(
        fixture, &flow, &before, &undone, &root,
    ))
    .await;
}

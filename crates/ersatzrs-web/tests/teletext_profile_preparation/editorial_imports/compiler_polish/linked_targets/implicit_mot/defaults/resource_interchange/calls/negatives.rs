//! A changed allocated call needs every consumer; a valid wire word grants no target.
use super::*;

fn generated_nested_consumer(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_response(theme, page, subcode);
    let protected = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["page"]["page"] == 125)
        .unwrap();
    let region = protected["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "nested-call-zone")
        .unwrap();
    region["kind"] = json!("generated");
    value
}

async fn assert_no_mutation(fixture: &Fixture, flow: &Flow) {
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
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .is_none());
    assert_original_pointers(fixture, flow).await;
}

#[tokio::test]
async fn existing_resource_call_offset_cannot_move_a_generated_shared_consumer() {
    let (target, _) = Box::pin(qualified_target()).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated_nested_consumer,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request(&flow, &target),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("valid allocated call must reach shared-consumer effect permission: {error:?}");
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(125, 32)));
    let (row, column) = error.position.unwrap();
    assert_eq!(row, 5);
    assert!(
        (15..=16).contains(&column),
        "only the old/new nested P cells differ"
    );
    assert_no_mutation(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn resource_call_cannot_select_an_unallocated_valid_table28_pointer() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for page in flow.accepted[0].service().pages() {
        if let EncodedTeletextAuthoredPage::Object(page) = page {
            assert!(page
                .definition_positions()
                .iter()
                .all(|p| p.object_number() != 511));
        }
    }
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let original_files = request.files.clone();
    let sidecar = request.sidecar.clone();
    let name = companion(&flow.exported[0]);
    let file = request.files.iter_mut().find(|f| f.name == name).unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let words = records
        .iter_mut()
        .find_map(|r| match r {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    assert_eq!((words[8] >> 6) & 31, 19, "original Passive invocation");
    assert_eq!(words[8] & 0x3c, 0x30, "original Public scope");
    // Nine-bit pointer511 is legal Table28 syntax but has no original allocation.
    // The original resource/page/type and origin modifier remain unchanged.
    words[8] = (words[8] & !(3 | (127 << 11))) | 3 | (127 << 11);
    assert!(words[8] <= 0x3ffff);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    for (before, after) in original_files.iter().zip(&request.files) {
        assert_eq!(before.name, after.name);
        if before.name != name {
            assert_eq!(before.bytes, after.bytes);
        }
    }
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, NativeTeletextEditorialImportError::Tti(_)),
        "an absent catalogue selector must fail authenticated TTI reconciliation: {error:?}"
    );
    assert_no_mutation(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

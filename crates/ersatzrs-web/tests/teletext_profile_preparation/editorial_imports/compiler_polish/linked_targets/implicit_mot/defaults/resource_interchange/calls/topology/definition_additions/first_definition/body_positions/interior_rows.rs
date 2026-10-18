//! Existing saved Add authority survives row insertion inside a populated body.
use super::*;

#[path = "interior_rows/actual_editor.rs"]
mod actual_editor;

fn outer_rows(theme: &str, page: u16, subcode: u16) -> Value {
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
fn inserted_row(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = outer_rows(theme, page, subcode);
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
async fn qualify(response: fn(&str, u16, u16) -> Value) -> BodyExpected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let result = BodyExpected {
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
    fixture.state.pool.close().await;
    result
}
fn edits(insert: bool) -> Vec<(usize, u32)> {
    let mut words = vec![POSITION, glyph(b'P')];
    if insert {
        words.extend([0x2a | (4 << 6), 6 | (9 << 6) | (82 << 11)]);
    }
    words.extend([0x2b | (4 << 6), 4 | (9 << 6) | (81 << 11), TERMINATION]);
    words
        .into_iter()
        .enumerate()
        .map(|(index, word)| (index + 3, word))
        .collect()
}

#[tokio::test]
async fn populated_added_definition_inserts_row_with_exact_survivors_preview_and_reopen() {
    Box::pin(run(false)).await;
}

#[tokio::test]
#[ignore = "requires pinned headed QTM inserted-row driver and a fresh explicit proof directory"]
async fn actual_qtm_populated_added_definition_inserts_row_save_import_preview_publish_reopen() {
    Box::pin(run(true)).await;
}

async fn run(actual: bool) {
    let empty = Box::pin(qualify_empty(1)).await;
    let outer = Box::pin(qualify(outer_rows)).await;
    let inserted = Box::pin(qualify(inserted_row)).await;
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
    let original = receipt(&fixture, &exported).await;
    let populated = import(
        &fixture,
        edited_request(&exported, &original, &edits(false)),
    )
    .await;
    let stored = Box::pin(assert_body(&fixture, &flow, &populated, &outer)).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    let original = receipt(&fixture, &exported).await;
    let (request, proof) = if actual {
        let (request, proof) = actual_editor::request(&exported, &original, &inserted).await;
        (request, Some(proof))
    } else {
        (edited_request(&exported, &original, &edits(true)), None)
    };
    let file = request
        .files
        .iter()
        .find(|file| file.name == object_name(&exported))
        .unwrap();
    if !actual {
        assert_eq!(
            parse_teletext_tti(&file.bytes, policy().tti)
                .unwrap()
                .records(),
            inserted.records
        );
    }
    let saved = import(&fixture, request).await;
    assert_eq!(saved.head().revision, 4);
    let stored = Box::pin(assert_body(&fixture, &flow, &saved, &inserted)).await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(
        file_records(&exported, object_name(&exported)),
        inserted.records
    );
    fixture = fixture.reopen().await;
    Box::pin(assert_body(&fixture, &flow, &saved, &inserted)).await;
    unchanged(&fixture, &flow, &before).await;
    let accepted = publish_and_accept(&fixture, &flow, &saved).await;
    assert_eq!(
        accepted
            .service()
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        inserted.units
    );
    fixture = fixture.reopen().await;
    Box::pin(assert_body(&fixture, &flow, &saved, &inserted)).await;
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
    assert_retained_storage(&fixture, &flow).await;
    assert_eq!(retained(&fixture).await, before);
    assert!(fixture.playout.is_empty());
    if let Some(proof) = proof {
        proof.success(&saved);
    }
    fixture.state.pool.close().await;
}

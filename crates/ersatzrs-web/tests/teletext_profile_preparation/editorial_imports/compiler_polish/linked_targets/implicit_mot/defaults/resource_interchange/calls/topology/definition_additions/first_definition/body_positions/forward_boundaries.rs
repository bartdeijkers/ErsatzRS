//! Represented forward positions and the row24/column39 wire boundary.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Case {
    ForwardMultiple,
    LastCell,
}
fn multiple(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = with_definitions(theme, page, subcode, 1);
    value["service"]["objects"][0]["program"]["commands"] = json!([
        {"kind":"set_position","position":{"row":1,"column":0}},
        {"kind":"at_column","column":8,"operation":{"kind":"g0","code":80}},
        {"kind":"set_position","position":{"row":2,"column":0}},
        {"kind":"at_column","column":4,"operation":{"kind":"g0","code":81}}
    ]);
    validate_response(&value);
    value
}
fn last_cell(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = with_definitions(theme, page, subcode, 1);
    value["service"]["objects"][0]["program"]["commands"] = json!([
        {"kind":"set_position","position":{"row":24,"column":39}},
        {"kind":"at_column","column":39,"operation":{"kind":"g0","code":80}}
    ]);
    validate_response(&value);
    value
}
fn replacements(case: Case) -> Vec<(usize, u32)> {
    match case {
        Case::ForwardMultiple => vec![
            (3, 0x29 | (4 << 6)),
            (4, 8 | (9 << 6) | (80 << 11)),
            (5, 0x2a | (4 << 6)),
            (6, 4 | (9 << 6) | (81 << 11)),
        ],
        // Wire address40 is row24, not row0. No hidden row0 anchor is inferred.
        Case::LastCell => vec![
            (3, 0x28 | (4 << 6) | (39 << 11)),
            (4, 0x27 | (9 << 6) | (80 << 11)),
        ],
    }
}
async fn qualify(case: Case) -> BodyExpected {
    let response = match case {
        Case::ForwardMultiple => multiple as fn(&str, u16, u16) -> Value,
        Case::LastCell => last_cell,
    };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
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
    let records = file_records(&flow.exported[0], object_name(&flow.exported[0]));
    let edits = replacements(case);
    for &(index, expected) in &edits {
        assert_eq!(
            word(&records, index),
            expected,
            "actual source compiler must retain the represented {case:?} position"
        );
    }
    assert_eq!(word(&records, 3 + edits.len()), TERMINATION);
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
async fn run(case: Case, empty: &EmptyExpected) {
    let expected = Box::pin(qualify(case)).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_empty(fixture)).await;
    let before = retained(&fixture).await;
    assert_eq!(
        submit(&fixture, &fields(&flow.originals[0], 0)).await["saved"],
        "draft"
    );
    let added = Box::pin(assert_concept(&fixture, &flow, empty)).await;
    let exported = Box::pin(export_stored(&fixture, &added)).await;
    let baseline = receipt(&fixture, &exported).await;
    let request = edited_request(&exported, &baseline, &replacements(case));
    let actual = request
        .files
        .iter()
        .find(|file| file.name == object_name(&exported))
        .unwrap();
    assert_eq!(
        parse_teletext_tti(&actual.bytes, policy().tti)
            .unwrap()
            .records(),
        expected.records
    );
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 3);
    let stored = Box::pin(assert_body(&fixture, &flow, &concept, &expected)).await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(
        file_records(&exported, object_name(&exported)),
        expected.records
    );
    fixture = fixture.reopen().await;
    Box::pin(assert_body(&fixture, &flow, &concept, &expected)).await;
    unchanged(&fixture, &flow, &before).await;
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn first_empty_definition_forward_positions_and_last_row_column_preserve_exact_native_body() {
    let empty = Box::pin(qualify_empty(1)).await;
    for case in [Case::ForwardMultiple, Case::LastCell] {
        Box::pin(run(case, &empty)).await;
    }
}

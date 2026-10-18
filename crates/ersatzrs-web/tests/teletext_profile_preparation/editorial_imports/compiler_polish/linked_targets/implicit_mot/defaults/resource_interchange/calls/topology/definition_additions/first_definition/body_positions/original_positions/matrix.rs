//! Both original POP and GPOP use the same first/append/interior lifecycle.
use super::support;
use super::*;

type Response = fn(&str, u16, u16) -> Value;
fn response(theme: &str, page: u16, subcode: u16, global: bool, stage: u8) -> Value {
    let mut value = with_definitions(theme, page, subcode, 3);
    let mut commands = Vec::new();
    if stage >= 1 {
        commands.extend([
            json!({"kind":"set_position","position":{"row":1,"column":0}}),
            json!({"kind":"at_column","column":8,"operation":{"kind":"g0","code":80}}),
        ]);
    }
    if stage >= 3 {
        commands.extend([
            json!({"kind":"set_position","position":{"row":2,"column":0}}),
            json!({"kind":"at_column","column":6,"operation":{"kind":"g0","code":82}}),
        ]);
    }
    if stage >= 2 {
        commands.extend([
            json!({"kind":"set_position","position":{"row":3,"column":0}}),
            json!({"kind":"at_column","column":4,"operation":{"kind":"g0","code":81}}),
        ]);
    }
    value["service"]["objects"][if global { 2 } else { 0 }]["program"]["commands"] =
        json!(commands);
    validate_response(&value);
    value
}
fn pop_empty(t: &str, p: u16, s: u16) -> Value {
    response(t, p, s, false, 0)
}
fn pop_first(t: &str, p: u16, s: u16) -> Value {
    response(t, p, s, false, 1)
}
fn pop_append(t: &str, p: u16, s: u16) -> Value {
    response(t, p, s, false, 2)
}
fn pop_interior(t: &str, p: u16, s: u16) -> Value {
    response(t, p, s, false, 3)
}
fn gpop_empty(t: &str, p: u16, s: u16) -> Value {
    response(t, p, s, true, 0)
}
fn gpop_first(t: &str, p: u16, s: u16) -> Value {
    response(t, p, s, true, 1)
}
fn gpop_append(t: &str, p: u16, s: u16) -> Value {
    response(t, p, s, true, 2)
}
fn gpop_interior(t: &str, p: u16, s: u16) -> Value {
    response(t, p, s, true, 3)
}

async fn lifecycle(empty: Response, stages: [Response; 3], target: TeletextObjectIdentity) {
    let first = Box::pin(support::qualify(stages[0], target)).await;
    let appended = Box::pin(support::qualify(stages[1], target)).await;
    let interior = Box::pin(support::qualify(stages[2], target)).await;
    assert_eq!(
        [
            first.definition.program.commands().len(),
            appended.definition.program.commands().len(),
            interior.definition.program.commands().len()
        ],
        [2, 4, 6]
    );
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        empty,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let before = retained(&fixture).await;
    let original = support::expected(&flow, target);
    assert!(
        original.definition.program.commands().is_empty(),
        "empty typed original has no invented NoOp"
    );
    let mut exported = None;
    let mut saved = None;
    for (index, expected) in [&first, &appended, &interior].into_iter().enumerate() {
        let current_export = exported.as_ref().unwrap_or(&flow.exported[0]);
        let current = receipt(&fixture, current_export).await;
        assert_eq!(current.metadata.revision, i64::try_from(index + 1).unwrap());
        let actual = import(
            &fixture,
            support::request(current_export, &current, target, &expected.records),
        )
        .await;
        assert_eq!(actual.head().revision, i64::try_from(index + 2).unwrap());
        let stored = Box::pin(support::assert_saved(
            &fixture, &flow, &actual, expected, true,
        ))
        .await;
        unchanged(&fixture, &flow, &before).await;
        exported = Some(Box::pin(export_stored(&fixture, &stored)).await);
        let latest = exported.as_ref().unwrap();
        assert_eq!(
            file_records(latest, support::name(latest, target)),
            expected.records
        );
        saved = Some(actual);
    }
    let saved = saved.unwrap();
    fixture = fixture.reopen().await;
    Box::pin(support::assert_saved(
        &fixture, &flow, &saved, &interior, true,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    Box::pin(support::accept_reopen(
        fixture, &flow, &before, &saved, &interior,
    ))
    .await;
}

#[tokio::test]
async fn original_pop_positions_first_append_interior_preview_publish_and_cold_reopen() {
    Box::pin(lifecycle(
        pop_empty,
        [pop_first, pop_append, pop_interior],
        identity(0),
    ))
    .await;
}
#[tokio::test]
async fn original_gpop_positions_first_append_interior_preview_publish_and_cold_reopen() {
    Box::pin(lifecycle(
        gpop_empty,
        [gpop_first, gpop_append, gpop_interior],
        identity(2),
    ))
    .await;
}

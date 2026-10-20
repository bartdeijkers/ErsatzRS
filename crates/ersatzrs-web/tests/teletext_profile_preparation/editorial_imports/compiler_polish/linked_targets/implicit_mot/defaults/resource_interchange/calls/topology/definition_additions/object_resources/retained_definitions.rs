//! Existing Add/Copy controls must work on explicitly retained new banks.
use super::*;
use ersatzrs_web::host::teletext_editorial_designs::{
    copy_teletext_editorial_resource_definition, NativeTeletextResourceDefinitionCopyRequest,
};

#[path = "retained_definitions/actual_admin.rs"]
mod actual_admin;

fn added_identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(bank_resources()[2], TeletextObjectId::new(32)).unwrap()
}
fn copied_identity() -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(bank_resources()[1], TeletextObjectId::new(33)).unwrap()
}
fn filled_response(copied: bool) -> Value {
    let mut value = complete_response("day", 124, 32);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let mut empty = objects
        .iter()
        .find(|object| object["identity"]["object_id"] == 20)
        .unwrap()
        .clone();
    empty["identity"] = wire_target(added_identity());
    empty["program"]["commands"] = json!([]);
    let old_add = objects
        .iter()
        .position(|object| object["identity"]["object_id"] == 25)
        .unwrap();
    objects.insert(old_add, empty);
    if copied {
        let source = objects
            .iter()
            .position(|object| object["identity"] == wire_target(new_target(1)))
            .unwrap();
        let mut copy = objects[source].clone();
        copy["identity"] = wire_target(copied_identity());
        objects.insert(source + 1, copy);
    }
    value["service"]["empty_object_pages"]
        .as_array_mut()
        .unwrap()
        .retain(|owner| *owner != wire_resource(bank_resources()[2]));
    value
}
async fn add_retained(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    definition: TeletextEnhancementObjectDefinition,
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let current = receipt(fixture, &exported).await;
    let result = add_teletext_editorial_resource_definitions(
        &fixture.state.application,
        add_request(&current, concept.head().revision, definition),
        policy(),
        designs::deadline(),
    )
    .await
    .expect("retained new-bank Add must use its exact current native receipt");
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("current retained-bank Add must save an unpublished concept")
    };
    saved
}
async fn copy_retained(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let current = receipt(fixture, &exported).await;
    let result = copy_teletext_editorial_resource_definition(
        &fixture.state.application,
        NativeTeletextResourceDefinitionCopyRequest {
            name: "Copy in retained new bank".into(),
            receipt_id: current.metadata.id,
            receipt_digest: current.metadata.digest.clone(),
            expected_design_head: concept.head().revision,
            source: new_target(1),
            new_object_id: TeletextObjectId::new(33),
        },
        policy(),
        designs::deadline(),
    )
    .await
    .expect("retained new-bank Copy must resolve the exact current native definition");
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("current retained-bank Copy must save an unpublished concept")
    };
    saved
}

pub(super) async fn setup_retained(
    first: &BankExpected,
) -> (Fixture, Flow, SavedEditorialTeletextDesign) {
    Box::pin(setup_retained_on(first, Fixture::new_persistent().await)).await
}
async fn setup_retained_on(
    first: &BankExpected,
    fixture: Fixture,
) -> (Fixture, Flow, SavedEditorialTeletextDesign) {
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        super::super::super::response,
        fixture,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let concept = add(&fixture, &flow.receipts[0], first.definition25.clone()).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let concept = add_banks(
        &fixture,
        bank_request(
            &current,
            2,
            first.banks.clone(),
            vec![first.definition26.clone()],
        ),
    )
    .await;
    (fixture, flow, concept)
}

#[tokio::test]
async fn retained_bank_add_copy_preserves_complete_models_preview_publication_and_cold_reopen() {
    let first = Box::pin(qualify_banks(false, false)).await;
    let filled = Box::pin(qualify_response(filled_response(false), false)).await;
    let copied = Box::pin(qualify_response(filled_response(true), false)).await;
    let (mut fixture, flow, concept) = Box::pin(setup_retained(&first)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &first)).await;
    let definition = filled
        .declared
        .semantic()
        .objects()
        .definition(added_identity())
        .unwrap()
        .clone();
    assert!(definition.program.commands().is_empty());
    let concept = Box::pin(add_retained(&fixture, &concept, definition)).await;
    assert_eq!(concept.head().revision, 4);
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &filled)).await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    let concept = Box::pin(copy_retained(&fixture, &concept)).await;
    assert_eq!(concept.head().revision, 5);
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &copied)).await;
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_bank_output(accepted.service(), &copied);
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &copied)).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn retained_bank_add_copy_refuses_stale_occupied_missing_owner_and_shared_limits_atomically()
{
    let first = Box::pin(qualify_banks(false, false)).await;
    let filled = Box::pin(qualify_response(filled_response(false), false)).await;
    let (mut fixture, flow, concept) = Box::pin(setup_retained(&first)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let current = receipt(&fixture, &exported).await;
    let original = filled
        .declared
        .semantic()
        .objects()
        .definition(added_identity())
        .unwrap();
    for variant in 0..4 {
        let mut definition = original.clone();
        let mut head = 3;
        let mut bounds = policy();
        match variant {
            0 => head = 2,
            1 => definition.identity = new_target(1),
            2 => {
                definition.identity =
                    TeletextObjectIdentity::public(bank_resources()[3], TeletextObjectId::new(32))
                        .unwrap();
            }
            3 => {
                let objects = first.declared.semantic().objects().definitions().len();
                bounds.document.validation.service.max_objects = objects;
                bounds.encoding.composition.service.max_objects = objects;
            }
            _ => unreachable!(),
        }
        let result = add_teletext_editorial_resource_definitions(
            &fixture.state.application,
            add_request(&current, head, definition),
            bounds,
            designs::deadline(),
        )
        .await;
        assert!(
            !matches!(result, Ok(TeletextDesignMutation::Saved(_))),
            "retained Add variant{variant}"
        );
        assert_head_only(&fixture, &concept).await;
        assert_original_pointers(&fixture, &flow).await;
    }
    for variant in 0..4 {
        let (source, id, head) = match variant {
            0 => (new_target(1), 33, 2),
            1 => (new_target(1), 30, 3),
            2 => (
                TeletextObjectIdentity::public(bank_resources()[1], TeletextObjectId::new(65_535))
                    .unwrap(),
                33,
                3,
            ),
            3 => (
                TeletextObjectIdentity::public(bank_resources()[3], TeletextObjectId::new(30))
                    .unwrap(),
                33,
                3,
            ),
            _ => unreachable!(),
        };
        let result = copy_teletext_editorial_resource_definition(
            &fixture.state.application,
            NativeTeletextResourceDefinitionCopyRequest {
                name: "Retained bank copy controls".into(),
                receipt_id: current.metadata.id,
                receipt_digest: current.metadata.digest.clone(),
                expected_design_head: head,
                source,
                new_object_id: TeletextObjectId::new(id),
            },
            policy(),
            designs::deadline(),
        )
        .await;
        assert!(
            !matches!(result, Ok(TeletextDesignMutation::Saved(_))),
            "retained Copy variant{variant}"
        );
        assert_head_only(&fixture, &concept).await;
        assert_original_pointers(&fixture, &flow).await;
    }
    fixture = fixture.reopen().await;
    Box::pin(assert_bank_concept(&fixture, &flow, &concept, &first)).await;
    assert_head_only(&fixture, &concept).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

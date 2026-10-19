//! Native Main-call structure, retaining the complete same-owner Local catalogue.

use super::*;

fn target_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    if theme == "day" && page == 124 {
        let commands = value["service"]["page_enhancements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|page| page["address"]["page"] == 124)
            .unwrap()["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        let calls: Vec<_> = commands
            .iter()
            .enumerate()
            .filter_map(|(index, command)| {
                (command["kind"] == "invoke_object" && command["target"]["object_id"] == 65520)
                    .then_some(index)
            })
            .collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(commands[calls[0]]["offset"]["column"], 24);
        assert_eq!(commands[calls[1]]["offset"]["column"], 28);
        commands.remove(calls[0]);
    }
    value
}

fn deletion_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let original = response("day", 124, 32);
    let command = original["service"]["page_enhancements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["address"]["page"] == 124)
        .unwrap()["local_program"]["commands"]
        .as_array()
        .unwrap()
        .iter()
        .position(|command| {
            command["kind"] == "invoke_object"
                && command["target"]["object_id"] == 65520
                && command["offset"]["column"] == 24
        })
        .unwrap();
    let service = flow.accepted[0].service();
    let packed = encoded(service);
    let mut positions = packed.command_positions().iter().filter(|position| {
        position.source().object.is_none() && position.original_local_command() == Some(command)
    });
    let removed = positions.next().unwrap();
    assert!(positions.next().is_none());
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    "Delete one original Main Local invocation".clone_into(&mut request.name);
    let name = page_name(&request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    let end = words
        .iter()
        .position(|value| *value == word(63, 31, 5))
        .unwrap();
    words.truncate(end + 1);
    assert_eq!(
        words[removed.semantic_word()],
        invocation(definitions(service)[0])
    );
    let count = removed.semantic_word() + 1 - removed.first_word();
    words.drain(removed.first_word()..=removed.semantic_word());
    // Every definition survives. Only its packed position, and exact pointers
    // to it, move with the removed Main invocation atom.
    for definition in packed.definition_positions() {
        let old = usize::from(definition.absolute_index());
        assert!(old > removed.semantic_word());
        for value in &mut words {
            for mode in 17..=23 {
                if *value == pointer(old, mode) {
                    *value = pointer(old - count, mode);
                    break;
                }
            }
        }
    }
    replace_packets(&mut request, words);
    request
}

#[tokio::test]
async fn original_main_local_invocation_deletion_preserves_complete_definition_catalogue() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (target_fixture, target_flow) = Box::pin(setup_with_fixture_policy(
        target_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let before = flow.accepted[0].service();
    let expected = target_flow.accepted[0].service();
    for id in [65520, 65521] {
        assert_eq!(
            before
                .declared()
                .semantic()
                .objects()
                .definition(identity(id)),
            expected
                .declared()
                .semantic()
                .objects()
                .definition(identity(id)),
            "all original Local definitions remain complete, including the uncalled body"
        );
    }
    assert_eq!(non_x26_records(before), non_x26_records(expected));
    assert_eq!(
        designs::units(flow.accepted[1].service()),
        designs::units(target_flow.accepted[1].service())
    );
    let request = deletion_request(&flow);
    let name = page_name(&request);
    let target = drcs_bitmap::from_receipt(&target_flow.receipts[0], 1);
    assert_eq!(
        request
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes,
        target
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes,
        "ENTIRE edited Main TTI equals independently compiled/exported target before import"
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(restored.encoded().unwrap().declared(), expected.declared());
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(expected)
    );
    assert_eq!(restored.metadata(), flow.saved[0].candidate().metadata());
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    assert_eq!(
        restored.sources().len(),
        flow.saved[0].candidate().sources().len()
    );
    for (current, original) in restored
        .sources()
        .iter()
        .zip(flow.saved[0].candidate().sources())
    {
        assert_eq!(current.bytes(), original.bytes());
        assert_eq!(current.authoring(), original.authoring());
    }
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(expected)
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
}

#[path = "main_calls/lifecycle.rs"]
mod lifecycle;
#[path = "main_calls/refusals.rs"]
mod refusals;

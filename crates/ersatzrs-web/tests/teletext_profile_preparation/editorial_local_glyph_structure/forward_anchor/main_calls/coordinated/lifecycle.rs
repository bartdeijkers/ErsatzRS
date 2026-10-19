//! Current receipts compose scalar and call edits, without implicit publication.

use super::*;

pub(super) fn later_response<const REMOVE: bool>(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = if REMOVE {
        response_with_glyph::<true>(theme, page, subcode)
    } else {
        response_with_glyph::<false>(theme, page, subcode)
    };
    if theme == "day" && page == 124 {
        let commands = value["service"]["page_enhancements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|page| page["address"]["page"] == 124)
            .unwrap()["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        assert_eq!(commands[2]["operation"]["kind"], "g0");
        commands[2]["operation"]["code"] = json!(if REMOVE { 82 } else { 81 });
        {
            let at = commands
                .iter()
                .position(|command| {
                    command["kind"] == "invoke_object"
                        && command["target"]["object_id"] == 65520
                        && command["offset"]["column"] == 28
                })
                .unwrap();
            let mut inserted = commands[at].clone();
            inserted["offset"]["column"] = json!(25);
            commands.insert(at, inserted);
        }
    }
    value
}

pub(super) fn change_glyph(
    request: &mut NativeTeletextEditorialImportRequest,
    current: &EncodedTeletextAuthoredService,
    old: u32,
    new: u32,
) {
    let mut spans = encoded(current)
        .command_positions()
        .iter()
        .filter(|span| span.source().object.is_none() && span.original_local_command() == Some(2));
    let glyph = spans.next().unwrap();
    assert!(spans.next().is_none());
    assert_eq!(glyph.first_word(), glyph.semantic_word());
    let name = page_name(request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    assert_eq!(words[glyph.semantic_word()], word(8, 9, old));
    words[glyph.semantic_word()] = word(8, 9, new);
    replace_packets(request, words);
}

fn remove_original_call(
    latest: &TeletextDesignExportReceipt,
    current: &EncodedTeletextAuthoredService,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(latest, 2);
    let name = page_name(&request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    let pair = [word(40, 16, 24), invocation(definitions(current)[0])];
    let starts: Vec<_> = words
        .windows(2)
        .enumerate()
        .filter_map(|(at, words)| (words == pair).then_some(at))
        .collect();
    assert_eq!(starts.len(), 1);
    let at = starts[0];
    let end = words
        .iter()
        .position(|value| *value == word(63, 31, 5))
        .unwrap();
    words.truncate(end + 1);
    words.drain(at..at + 2);
    for definition in encoded(current).definition_positions() {
        let old = usize::from(definition.absolute_index());
        assert!(old > at + 1);
        for value in &mut words {
            for mode in 17..=23 {
                if *value == pointer(old, mode) {
                    *value = pointer(old - 2, mode);
                    break;
                }
            }
        }
    }
    replace_packets(&mut request, words);
    change_glyph(&mut request, current, 81, 82);
    request
}

async fn undo(fixture: &Fixture, flow: &Flow, concept: &SavedEditorialTeletextDesign) {
    let pending = fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap();
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let latest = receipt(fixture, &exported).await;
    let mut request = drcs_bitmap::from_receipt(&latest, 3);
    let original = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let name = page_name(&request);
    request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes
        .clone_from(
            &original
                .files
                .iter()
                .find(|file| file.name == name)
                .unwrap()
                .bytes,
        );
    assert_eq!(
        request.sidecar,
        drcs_bitmap::from_receipt(&latest, 3).sidecar
    );
    let undone = import(fixture, request).await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        drcs_bitmap::stored(fixture, &undone).await.document,
        flow.originals[0].document
    );
    let restored = drcs_bitmap::restored(fixture, &undone).await;
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(flow.accepted[0].service())
    );
    assert_eq!(
        external_editor_probe::preview_page(restored.encoded().unwrap(), 124),
        external_editor_probe::preview_page(flow.accepted[0].service(), 124)
    );
    generations::assert_same_output(&recover(fixture, 1).await, &flow.accepted[0]);
    generations::assert_same_output(&recover(fixture, 2).await, &flow.accepted[1]);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        pending
    );
}

async fn repeat(
    fixture: &Fixture,
    flow: &Flow,
    expected: &Flow,
    first: &SavedEditorialTeletextDesign,
) {
    let exported = drcs_bitmap::reexport(fixture, first).await;
    let latest = receipt(fixture, &exported).await;
    let current = drcs_bitmap::restored(fixture, first).await;
    let request = remove_original_call(&latest, current.encoded().unwrap());
    assert_complete_tti(&request, expected);
    let deleted = import(fixture, request).await;
    assert_eq!(deleted.head().revision, 3);
    Box::pin(assert_coordinated_concept(
        fixture,
        flow,
        &deleted,
        expected.accepted[0].service(),
    ))
    .await;
    assert_original_pointers(fixture, flow).await;
    Box::pin(undo(fixture, flow, &deleted)).await;
}

#[tokio::test]
async fn coordinated_main_scalar_call_insertion_repeat_deletion_and_fresh_root_undo() {
    let (mut fixture, flow) = target(response_with_glyph::<false>).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (first_fixture, first) = target(later_response::<false>).await;
    let (second_fixture, second) = target(later_response::<true>).await;
    for target in [&first, &second] {
        assert_target_preserves_catalogue(&flow, target);
    }
    let mut request = insertion_request(&flow.receipts[0], 1, flow.accepted[0].service(), 25);
    change_glyph(&mut request, flow.accepted[0].service(), 80, 81);
    assert_complete_tti(&request, &first);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    Box::pin(assert_coordinated_concept(
        &fixture,
        &flow,
        &concept,
        first.accepted[0].service(),
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_coordinated_concept(
        &fixture,
        &flow,
        &concept,
        first.accepted[0].service(),
    ))
    .await;
    Box::pin(repeat(&fixture, &flow, &second, &concept)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    first_fixture.state.pool.close().await;
    second_fixture.state.pool.close().await;
}

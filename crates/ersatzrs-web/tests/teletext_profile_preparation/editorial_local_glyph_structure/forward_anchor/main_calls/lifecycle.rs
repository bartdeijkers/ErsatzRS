//! Fresh receipts preserve retained Main calls and the immutable Local catalogue.

use super::*;

type Response = fn(&str, u16, u16) -> Value;

pub(super) fn inserted_response<const REPEAT: bool>(theme: &str, page: u16, subcode: u16) -> Value {
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
        commands.insert(at, inserted.clone());
        if REPEAT {
            inserted["offset"]["column"] = json!(26);
            commands.insert(at + 1, inserted);
        }
    }
    value
}

pub(super) async fn target(factory: Response) -> (Fixture, Flow) {
    Box::pin(setup_with_fixture_policy(
        factory,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await
}

pub(super) fn assert_target_preserves_catalogue(flow: &Flow, target: &Flow) {
    let before = flow.accepted[0].service();
    let after = target.accepted[0].service();
    assert_eq!(
        before.declared().semantic().objects(),
        after.declared().semantic().objects()
    );
    assert_eq!(non_x26_records(before), non_x26_records(after));
    assert_eq!(
        designs::units(flow.accepted[1].service()),
        designs::units(target.accepted[1].service())
    );
}

pub(super) fn assert_complete_tti(request: &NativeTeletextEditorialImportRequest, target: &Flow) {
    let name = page_name(request);
    let exported = drcs_bitmap::from_receipt(&target.receipts[0], 1);
    assert_eq!(
        request
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes,
        exported
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes,
        "ENTIRE edited Main TTI equals independently compiled/exported target"
    );
}

pub(super) fn insertion_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    current: &EncodedTeletextAuthoredService,
    column: u32,
) -> NativeTeletextEditorialImportRequest {
    assert!(matches!(column, 25 | 26));
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Insert same-owner Local Main invocation beside retained calls".clone_into(&mut request.name);
    let name = page_name(&request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    let marker = definitions(current)[0];
    let retained = [word(40, 16, 28), invocation(marker)];
    let at: Vec<_> = words
        .windows(2)
        .enumerate()
        .filter_map(|(index, pair)| (pair == retained).then_some(index))
        .collect();
    assert_eq!(
        at.len(),
        1,
        "one retained Main offset28 call anchors insertion"
    );
    let end = words
        .iter()
        .position(|value| *value == word(63, 31, 5))
        .unwrap();
    words.truncate(end + 1);
    words.splice(at[0]..at[0], [word(40, 16, column), invocation(marker)]);
    for definition in encoded(current).definition_positions() {
        let old = usize::from(definition.absolute_index());
        assert!(old > at[0]);
        for value in &mut words {
            for mode in 17..=23 {
                if *value == pointer(old, mode) {
                    *value = pointer(old + 2, mode);
                    break;
                }
            }
        }
    }
    replace_packets(&mut request, words);
    request
}

async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let mut original: Value = serde_json::from_slice(&stored.document).unwrap();
    assert!(original
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay")
        .is_some());
    assert_eq!(
        original,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let restored = drcs_bitmap::restored(fixture, concept).await;
    assert_eq!(restored.encoded().unwrap().declared(), expected.declared());
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(expected)
    );
    assert_eq!(
        external_editor_probe::preview_page(restored.encoded().unwrap(), 124),
        external_editor_probe::preview_page(expected, 124)
    );
    let initial = flow.saved[0].candidate();
    assert_eq!(restored.metadata(), initial.metadata());
    assert_eq!(restored.selection(), initial.selection());
    assert_eq!(restored.transmissions(), initial.transmissions());
    assert_eq!(restored.sources().len(), initial.sources().len());
    for (current, original) in restored.sources().iter().zip(initial.sources()) {
        assert_eq!(current.bytes(), original.bytes());
        assert_eq!(current.authoring(), original.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        initial.producers()[0].original()
    );
}

async fn root_undo(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    let pending = fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap();
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let latest = receipt(fixture, &exported).await;
    let mut request = drcs_bitmap::from_receipt(&latest, 3);
    let name = page_name(&request);
    let original = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
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
    generations::assert_same_output(&recover(fixture, 1).await, accepted);
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
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(2)
            .await
            .unwrap(),
        Some(flow.pending[1].clone())
    );
}

#[tokio::test]
async fn main_local_invocation_insertion_repeat_root_undo_publication_and_cold_recovery() {
    let (mut fixture, flow) = target(response).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (first_fixture, first) = target(inserted_response::<false>).await;
    let (second_fixture, second) = target(inserted_response::<true>).await;
    assert_target_preserves_catalogue(&flow, &first);
    assert_target_preserves_catalogue(&flow, &second);
    let request = insertion_request(&flow.receipts[0], 1, flow.accepted[0].service(), 25);
    assert_complete_tti(&request, &first);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    Box::pin(assert_concept(
        &fixture,
        &flow,
        &concept,
        first.accepted[0].service(),
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(
        &fixture,
        &flow,
        &concept,
        first.accepted[0].service(),
    ))
    .await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let current = drcs_bitmap::restored(&fixture, &concept).await;
    let request = insertion_request(&latest, 2, current.encoded().unwrap(), 26);
    assert_complete_tti(&request, &second);
    let repeated = import(&fixture, request).await;
    assert_eq!(repeated.head().revision, 3);
    Box::pin(assert_concept(
        &fixture,
        &flow,
        &repeated,
        second.accepted[0].service(),
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &repeated).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(second.accepted[0].service())
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    Box::pin(root_undo(&fixture, &flow, &repeated, &accepted)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    first_fixture.state.pool.close().await;
    second_fixture.state.pool.close().await;
}

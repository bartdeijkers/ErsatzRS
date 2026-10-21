//! Derived AP writers cannot authorize extra words or erase authored scalars.

use super::*;
use ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource;

fn duplicate_carrier(words: &mut Vec<u32>, service: &EncodedTeletextAuthoredService, at: usize) {
    let end = words
        .iter()
        .position(|value| *value == word(63, 31, 5))
        .unwrap();
    words.truncate(end + 1);
    let definitions: Vec<_> = encoded(service)
        .definition_positions()
        .iter()
        .map(|definition| usize::from(definition.absolute_index()))
        .collect();
    assert!(definitions.iter().all(|old| *old > at));
    let relocate = |value: u32| {
        definitions
            .iter()
            .find_map(|old| {
                (17..=23)
                    .find_map(|mode| (value == pointer(*old, mode)).then(|| pointer(old + 1, mode)))
            })
            .unwrap_or(value)
    };
    let old_markers: Vec<_> = definitions.iter().map(|old| words[*old]).collect();
    let original_words = words.clone();
    let carrier = words[at];
    words.insert(at + 1, carrier);
    for value in words.iter_mut() {
        *value = relocate(*value);
    }
    for (old, marker) in definitions.iter().zip(old_markers) {
        assert_eq!(words[old + 1], relocate(marker));
        for mode in 17..=23 {
            assert_eq!(
                original_words
                    .iter()
                    .filter(|value| **value == pointer(*old, mode))
                    .count(),
                words
                    .iter()
                    .filter(|value| **value == pointer(old + 1, mode))
                    .count(),
                "every Local marker/invocation alias retains its relocated definition"
            );
        }
    }
}

fn forged_request(flow: &Flow, grown: &Flow, case: usize) -> NativeTeletextEditorialImportRequest {
    let mut request = target_request(&flow.receipts[0], 1, grown);
    let service = grown.accepted[0].service();
    let at = checks::header_carrier(service).unwrap();
    let mut words = native_words(service);
    assert_eq!(words[at], word(8, 0, 7));
    match case {
        0 => words[at] = word(8, 0, 6),
        1 => words[at] = word(9, 0, 7),
        2 => duplicate_carrier(&mut words, service, at),
        _ => unreachable!(),
    }
    replace_packets(&mut request, words);
    request
}

#[tokio::test]
async fn direct_main_deferred_ap_refuses_changed_moved_and_extra_derived_writers_atomically() {
    let (mut fixture, original) = target(direct_response::<0>).await;
    let (target_fixture, grown) = target(direct_response::<1>).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_target_preserves_catalogue(&original, &grown);
    for (stage, factory) in [(0, &original), (1, &grown)] {
        checks::main_provenance(factory.accepted[0].service(), stage);
    }
    for case in 0..3 {
        let error = reject(&fixture, &original, forged_request(&original, &grown, case)).await;
        assert!(
            matches!(error, NativeTeletextEditorialImportError::Tti(_)),
            "derived Main writer forgery {case} must fail before mutation: {error:?}"
        );
        assert_original_pointers(&fixture, &original).await;
        checks::output(&fixture, &original.accepted[0], &original.accepted[1]).await;
        retained.assert_unchanged(&fixture, &original).await;
    }
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &original).await;
    retained.assert_unchanged(&fixture, &original).await;
    assert_retained_storage(&fixture, &original).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
}

fn authored_foreground_response<const STAGE: u8>(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = direct_response::<STAGE>(theme, page, subcode);
    if theme == "day" && page == 124 {
        let commands = value["service"]["page_enhancements"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|page| page["address"]["page"] == 124)
            .unwrap()["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        commands.insert(
            1,
            serde_json::to_value(column(
                8,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(7).unwrap(),
                },
            ))
            .unwrap(),
        );
    }
    value
}

fn authored_foreground_provenance(service: &EncodedTeletextAuthoredService) {
    assert_eq!(checks::header_carrier(service), None);
    assert!(encoded(service).command_positions().iter().all(|position| {
        !matches!(
            position.original_main_source(),
            Some(TeletextMainCompositionSource::HeaderPosition { .. })
        )
    }));
    let positions: Vec<_> = encoded(service)
        .command_positions()
        .iter()
        .filter(|position| position.original_local_command() == Some(1))
        .collect();
    assert_eq!(positions.len(), 1);
    let foreground = positions[0];
    assert_eq!(foreground.source().object, None);
    checks::leading_foreground_span(service, foreground.first_word(), foreground.semantic_word());
    assert_eq!(
        foreground.original_main_source(),
        Some(TeletextMainCompositionSource::OriginalPage { command: 1 })
    );
    assert_eq!(
        native_words(service)[foreground.semantic_word()],
        word(8, 0, 7)
    );
}

async fn authored_foreground_concept(
    fixture: &Fixture,
    original: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: &Flow,
) {
    let restored = drcs_bitmap::restored(fixture, saved).await;
    let service = restored.encoded().unwrap();
    let target = expected.accepted[0].service();
    assert_eq!(service.declared(), target.declared());
    assert_eq!(designs::units(service), designs::units(target));
    assert_eq!(
        external_editor_probe::preview_page(service, 124),
        external_editor_probe::preview_page(target, 124)
    );
    authored_foreground_provenance(service);
    let initial = original.saved[0].candidate();
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
    let stored = drcs_bitmap::stored(fixture, saved).await;
    let mut document: Value = serde_json::from_slice(&stored.document).unwrap();
    assert!(document
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay")
        .is_some());
    assert_eq!(
        document,
        serde_json::from_slice::<Value>(&original.originals[0].document).unwrap()
    );
    let exported = drcs_bitmap::reexport(fixture, saved).await;
    let latest = receipt(fixture, &exported).await;
    assert_complete_tti(
        &drcs_bitmap::from_receipt(&latest, saved.head().revision),
        expected,
    );
}

#[tokio::test]
async fn direct_main_deferred_ap_cannot_discard_physically_identical_authored_foreground() {
    let (mut fixture, original) = target(authored_foreground_response::<0>).await;
    let (target_fixture, empty) = target(direct_response::<2>).await;
    let (grown_fixture, grown) = target(authored_foreground_response::<1>).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_target_preserves_catalogue(&original, &empty);
    assert_target_preserves_catalogue(&original, &grown);
    authored_foreground_provenance(original.accepted[0].service());
    authored_foreground_provenance(grown.accepted[0].service());
    let request = target_request(&original.receipts[0], 1, &empty);
    let error = reject(&fixture, &original, request).await;
    assert!(
        matches!(error, NativeTeletextEditorialImportError::Tti(_)),
        "removing an actual authored Foreground cannot borrow HeaderPosition authority: {error:?}"
    );
    assert_original_pointers(&fixture, &original).await;
    checks::output(&fixture, &original.accepted[0], &original.accepted[1]).await;
    retained.assert_unchanged(&fixture, &original).await;
    // Keeping a genuine authored FG must still admit the existing Local-call
    // growth. Its physical equality to a derived carrier grants no deletion
    // permission and must not cause that required scalar to be stripped.
    let request = target_request(&original.receipts[0], 1, &grown);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    Box::pin(authored_foreground_concept(
        &fixture, &original, &concept, &grown,
    ))
    .await;
    assert_original_pointers(&fixture, &original).await;
    retained.assert_unchanged(&fixture, &original).await;
    fixture = fixture.reopen().await;
    Box::pin(authored_foreground_concept(
        &fixture, &original, &concept, &grown,
    ))
    .await;
    assert_original_pointers(&fixture, &original).await;
    retained.assert_unchanged(&fixture, &original).await;
    assert_retained_storage(&fixture, &original).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
    grown_fixture.state.pool.close().await;
}

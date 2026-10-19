//! One-word original Main glyph edits compose with same-owner Local call structure.
//! Existing fixture permissions stay exact; no multiword DRCS edit is submitted.

use super::{lifecycle::*, *};

fn response_with_glyph<const EDITED: bool>(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = if EDITED {
        target_response(theme, page, subcode)
    } else {
        response(theme, page, subcode)
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
        assert_eq!(commands[1]["operation"]["kind"], "foreground");
        assert_eq!(commands[2]["column"], 8);
        assert_eq!(commands[2]["operation"]["kind"], "drcs");
        // This is a genuinely captured original single-word scalar. The
        // untouched original FG18 remains scoped exactly as in the old fixture.
        commands[2]["operation"] = json!({"kind":"g0","code":if EDITED {81} else {80}});
    }
    value
}

fn coordinated_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = deletion_request(flow);
    "Edit one Main G0 glyph and delete one same-owner Local call".clone_into(&mut request.name);
    let mut positions = encoded(flow.accepted[0].service())
        .command_positions()
        .iter()
        .filter(|position| {
            position.source().object.is_none() && position.original_local_command() == Some(2)
        });
    let glyph = positions.next().unwrap();
    assert!(positions.next().is_none());
    assert_eq!(glyph.first_word(), glyph.semantic_word());
    let name = page_name(&request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    // The scalar precedes the deleted call, so its actual original position
    // stays fixed while all later retained calls/definitions relocate.
    assert_eq!(words[glyph.semantic_word()], word(8, 9, 80));
    words[glyph.semantic_word()] = word(8, 9, 81);
    replace_packets(&mut request, words);
    request
}

async fn assert_coordinated_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
) {
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
    let original = flow.saved[0].candidate();
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    assert_eq!(restored.sources().len(), original.sources().len());
    for (current, original) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(current.bytes(), original.bytes());
        assert_eq!(current.authoring(), original.authoring());
    }
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let mut document: Value = serde_json::from_slice(&stored.document).unwrap();
    assert!(document
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay")
        .is_some());
    assert_eq!(
        document,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
}

#[tokio::test]
async fn coordinated_main_singleword_glyph_and_call_deletion_matches_complete_target() {
    let (mut fixture, flow) = target(response_with_glyph::<false>).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (target_fixture, target_flow) = target(response_with_glyph::<true>).await;
    assert_target_preserves_catalogue(&flow, &target_flow);
    let request = coordinated_request(&flow);
    assert_complete_tti(&request, &target_flow);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    Box::pin(assert_coordinated_concept(
        &fixture,
        &flow,
        &concept,
        target_flow.accepted[0].service(),
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_coordinated_concept(
        &fixture,
        &flow,
        &concept,
        target_flow.accepted[0].service(),
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
}

#[path = "coordinated/lifecycle.rs"]
mod lifecycle;
#[path = "coordinated/refusals.rs"]
mod refusals;

//! Native structural import with a sealed, nonzero, zero-word position anchor.

use super::*;

fn anchor() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(0, 8).unwrap(),
    }
}
fn body(stage: u8) -> TeletextEnhancementProgram {
    let mut commands = super::commands(stage);
    for command in &mut commands[..] {
        let TeletextEnhancementCommand::AtColumn { column, .. } = command else {
            unreachable!()
        };
        *column += 8;
    }
    commands.insert(0, anchor());
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn words(stage: u8) -> Vec<u32> {
    glyph_words(stage)
        .into_iter()
        .map(|value| (value & !63) | ((value & 63) + 8))
        .collect()
}
fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = zone(theme, page, subcode);
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        if object["identity"]["object_id"] == 65520 && object["identity"]["owner"]["page"] == 124 {
            object["program"] = serde_json::to_value(body(0)).unwrap();
        }
    }
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        if page["address"]["page"] != 124 {
            continue;
        }
        for command in page["local_program"]["commands"].as_array_mut().unwrap() {
            if command["kind"] == "invoke_object" && command["target"]["object_id"] == 65520 {
                let column = command["offset"]["column"].as_u64().unwrap();
                assert!(matches!(column, 32 | 36));
                command["offset"]["column"] = json!(column - 8);
            }
        }
    }
    value
}
fn expected(stage: u8) -> TeletextLocalObjectGlyphProgramOverlay {
    let retained = match stage {
        1 => vec![
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            Some(5),
            None,
            None,
            None,
            None,
            Some(6),
            None,
            None,
            Some(7),
            Some(8),
            None,
            Some(9),
        ],
        2 => vec![Some(0), Some(1), Some(2), Some(3), None, Some(7), Some(9)],
        _ => unreachable!(),
    };
    TeletextLocalObjectGlyphProgramOverlay::new(identity(65520), body(0), body(stage), retained)
        .unwrap()
}
fn request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    request_body_words(
        &flow.receipts[0],
        1,
        flow.accepted[0].service(),
        &words(0),
        words(1),
    )
}

#[tokio::test]
async fn forward_anchor_native_structural_concept_preview_reopen_preserves_original() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let before = flow.accepted[0].service();
    assert_eq!(
        before
            .declared()
            .semantic()
            .objects()
            .definition(identity(65520))
            .unwrap()
            .program,
        body(0)
    );
    assert!(!encoded(before)
        .command_positions()
        .iter()
        .any(|position| position.source().object == Some(identity(65520))
            && position.source().command == 0));
    let concept = import(&fixture, request(&flow)).await;
    assert_eq!(concept.head().revision, 2);
    let preview = Box::pin(assert_hidden_zero_concept(
        &fixture,
        &flow,
        &concept,
        &expected(1),
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_hidden_zero_concept(
            &fixture,
            &flow,
            &concept,
            &expected(1)
        ))
        .await,
        preview
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    // Re-export the grown concept, then shrink and reposition its surviving
    // glyphs. The invisible original anchor keeps its source through composition.
    let current = drcs_bitmap::restored(&fixture, &concept).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let shrunk = import(
        &fixture,
        request_body_words(&latest, 2, current.encoded().unwrap(), &words(1), words(2)),
    )
    .await;
    assert_eq!(shrunk.head().revision, 3);
    let revision = drcs_bitmap::stored(&fixture, &shrunk).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 11);
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        &[expected(2)]
    );
    let current = drcs_bitmap::restored(&fixture, &shrunk).await;
    assert_rendered(before, current.encoded().unwrap(), 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &shrunk).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(
        &fixture,
        request_body_words(&latest, 3, current.encoded().unwrap(), &words(2), words(0)),
    )
    .await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    let restored = drcs_bitmap::restored(&fixture, &undone).await;
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(before)
    );
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn narrow_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        for region in page["regions"].as_array_mut().unwrap() {
            if region["id"] == "free-right-glyph" {
                region["width"] = json!(4);
            }
        }
    }
    value
}

#[tokio::test]
async fn forward_anchor_native_protects_second_consumer_without_mutation() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        narrow_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let error = reject(&fixture, &flow, request(&flow)).await;
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("a parsed forward-anchor candidate must still respect every shared consumer")
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(124, 32)));
    assert!(matches!(error.position, Some((1, 48..=51))));
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

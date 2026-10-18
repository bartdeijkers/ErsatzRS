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

fn wrapper_program() -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(vec![
        anchor(),
        TeletextEnhancementCommand::InvokeObject {
            target: identity(65520),
            object_type: TeletextObjectType::Passive,
            offset: None,
        },
    ])
    .unwrap()
}

fn wrapper_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = zone(theme, page, subcode);
    value["service"]["objects"].as_array_mut().unwrap().push(
        serde_json::to_value(TeletextEnhancementObjectDefinition {
            identity: identity(65522),
            object_type: TeletextObjectType::Adaptive,
            program: wrapper_program(),
        })
        .unwrap(),
    );
    let mut redirected = 0;
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
                command["target"]["object_id"] = json!(65522);
                command["object_type"] = json!("adaptive");
                command["offset"]["column"] = json!(column - 8);
                redirected += 1;
            }
        }
    }
    assert_eq!(redirected, 2);
    value
}

fn pointer(index: usize, mode: u32) -> u32 {
    let number = number(index);
    word(0x28 | (number >> 7), mode, number & 0x7f)
}

fn wrapper_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let before = flow.accepted[0].service();
    let wrapper = usize::from(
        encoded(before)
            .definition_positions()
            .iter()
            .find(|position| position.target() == identity(65522))
            .unwrap()
            .absolute_index(),
    );
    let mut request = request_words(&flow.receipts[0], 1, before, 0, glyph_words(1));
    let name = page_name(&request);
    let document = parse_teletext_tti(
        &request
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes,
        policy().tti,
    )
    .unwrap();
    let mut words = packet_words(document.records());
    let moved = wrapper + glyph_words(1).len() - glyph_words(0).len();
    assert_eq!(words[moved], pointer(wrapper, 22));
    words[moved] = pointer(moved, 22);
    let calls: Vec<_> = words[..definitions(before)[0]]
        .iter()
        .enumerate()
        .filter_map(|(index, word)| (*word == pointer(wrapper, 18)).then_some(index))
        .collect();
    assert_eq!(calls.len(), 2);
    for index in calls {
        words[index] = pointer(moved, 18);
    }
    replace_packets(&mut request, words);
    request
}

fn native_words(service: &EncodedTeletextAuthoredService) -> Vec<u32> {
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.page())
            }
            _ => None,
        })
        .unwrap();
    project_encoded_teletext_display(page, service.unit_count())
        .unwrap()
        .records
        .into_iter()
        .filter_map(|record| match record {
            TeletextEncodedDisplayRecord::Triplets { row: 26, words, .. } => Some(words),
            _ => None,
        })
        .flatten()
        .collect()
}

async fn assert_wrapper_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
    intent: &TeletextLocalObjectGlyphProgramOverlay,
) -> TeletextRaster {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 11);
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        std::slice::from_ref(intent)
    );
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    let mut body: Value = serde_json::from_slice(&stored.document).unwrap();
    body.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        body,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (current, saved) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(current.bytes(), saved.bytes());
        assert_eq!(current.authoring(), saved.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    assert_eq!(restored.encoded().unwrap().declared(), expected.declared());
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(expected)
    );
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    assert_eq!(preview, external_editor_probe::preview_page(expected, 124));
    preview
}

#[tokio::test]
async fn normalized_header_wrapper_native_child_structure_preview_reopen_preserves_original() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        wrapper_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let before = flow.accepted[0].service();
    let wrapper = before
        .declared()
        .semantic()
        .objects()
        .definition(identity(65522))
        .unwrap();
    assert_eq!(wrapper.object_type, TeletextObjectType::Adaptive);
    assert_eq!(wrapper.program, wrapper_program());
    assert_eq!(
        before
            .declared()
            .semantic()
            .objects()
            .definition(identity(65520))
            .unwrap()
            .program,
        program(0)
    );
    let intent = TeletextLocalObjectGlyphProgramOverlay::new(
        identity(65520),
        program(0),
        program(1),
        mapping(1),
    )
    .unwrap();
    let declared = before
        .declared()
        .clone()
        .with_local_object_glyph_program_overlays(
            std::slice::from_ref(&intent),
            policy().encoding.validation.service,
        )
        .unwrap();
    let expected = ersatzrs_ffmpeg::teletext::encode_teletext_authored_service(
        declared,
        flow.saved[0].candidate().transmissions(),
        &policy().encoding.encoding,
    )
    .expect("complete typed child edit independently compiles before interchange import");
    for target in [identity(65521), identity(65522)] {
        assert_eq!(
            expected.declared().semantic().objects().definition(target),
            before.declared().semantic().objects().definition(target)
        );
    }
    assert_eq!(non_x26_records(&expected), non_x26_records(before));
    let request = wrapper_request(&flow);
    let name = page_name(&request);
    let selected = request.files.iter().find(|file| file.name == name).unwrap();
    let records = parse_teletext_tti(&selected.bytes, policy().tti).unwrap();
    assert_eq!(
        packet_words(records.records()),
        native_words(&expected),
        "complete checked TTI equals independently compiled native target"
    );
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    let preview = Box::pin(assert_wrapper_concept(
        &fixture, &flow, &concept, &expected, &intent,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_wrapper_concept(
            &fixture, &flow, &concept, &expected, &intent
        ))
        .await,
        preview
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn altered_wrapper_background(flow: &Flow, remove: bool) -> NativeTeletextEditorialImportRequest {
    let mut request = wrapper_request(flow);
    let name = page_name(&request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    let before = encoded(flow.accepted[0].service());
    let position = before
        .command_positions()
        .iter()
        .find(|position| {
            position.source().object == Some(identity(65522))
                && matches!(
                    position.original_object_source(),
                    Some(
                        ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource::HeaderPosition {
                            command: 0
                        }
                    )
                )
        })
        .expect("the authentic wrapper AP is materialized by one generated background span");
    assert_eq!(position.first_word(), position.semantic_word());
    let index = position.semantic_word() + glyph_words(1).len() - glyph_words(0).len();
    assert_eq!(words[index], word(8, 3, 0));
    if remove {
        // Preserve all later marker/pointer relocations, so refusal specifically
        // protects the deleted physical AP witness rather than stale positions.
        for old in before
            .definition_positions()
            .iter()
            .map(|position| {
                usize::from(position.absolute_index()) + glyph_words(1).len() - glyph_words(0).len()
            })
            .filter(|old| *old > index)
        {
            for value in &mut words {
                for mode in 17..=23 {
                    if *value == pointer(old, mode) {
                        *value = pointer(old - 1, mode);
                        break;
                    }
                }
            }
        }
        words.remove(index);
    } else {
        words[index] = word(8, 3, 1);
    }
    replace_packets(&mut request, words);
    request
}

#[tokio::test]
async fn normalized_header_wrapper_native_protects_generated_background_span() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        wrapper_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for remove in [false, true] {
        assert!(matches!(
            reject(&fixture, &flow, altered_wrapper_background(&flow, remove)).await,
            NativeTeletextEditorialImportError::Tti(_)
        ));
    }
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn narrow_wrapper_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = wrapper_response(theme, page, subcode);
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
async fn normalized_header_wrapper_native_protects_second_child_consumer() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        narrow_wrapper_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let NativeTeletextEditorialImportError::Effects(error) =
        reject(&fixture, &flow, wrapper_request(&flow)).await
    else {
        panic!("a parsed child edit must retain every normalized wrapper consumer's permissions");
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

#[path = "forward_anchor/retained_calls.rs"]
mod retained_calls;

//! Coordinated edits still refuse malformed atoms and captured Generated effects.

use super::*;

fn malformed(request: &mut NativeTeletextEditorialImportRequest, case: u8) {
    let name = page_name(request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    match case {
        0 => {
            let at = words
                .iter()
                .position(|value| *value == word(8, 9, 81))
                .unwrap();
            words[at] = word(8, 9, 31); // G0 controls are not printable scalar glyphs.
        }
        1 => {
            let at = words
                .iter()
                .position(|value| *value == word(40, 16, 28))
                .unwrap();
            assert_eq!((words[at + 1] >> 6) & 31, 19);
            words[at + 1] = (words[at + 1] & !(31 << 6)) | (18 << 6);
        }
        2 => {
            let at = words
                .iter()
                .position(|value| *value == word(41, 4, 8))
                .unwrap();
            words[at] = word(42, 4, 8); // Retained non-scalar Main AP is immutable.
        }
        _ => unreachable!(),
    }
    replace_packets(request, words);
}

fn generated_scalar_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response_with_glyph::<false>(theme, page, subcode);
    for metadata in value["authoring"]["pages"].as_array_mut().unwrap() {
        let region = metadata["regions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|region| region["id"] == "free-glyph")
            .unwrap();
        assert_eq!(region["area"], "main");
        assert_eq!(
            (
                region["row"].as_u64(),
                region["column"].as_u64(),
                region["width"].as_u64()
            ),
            (Some(1), Some(8), Some(1))
        );
        region["kind"] = json!("generated");
    }
    value
}

async fn generated_refusal(expected: &Flow) {
    let (mut fixture, flow) = target(generated_scalar_response).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let request = coordinated_request(&flow);
    assert_complete_tti(&request, expected);
    let error = reject(&fixture, &flow, request).await;
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!(
            "valid coordinated glyph/call must reach Generated scalar effects refusal: {error:?}"
        )
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(124, 32)));
    assert_eq!(error.position, Some((1, 12))); // Four left-panel columns + Main8.
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn coordinated_main_scalar_and_call_changes_refuse_malformed_and_generated_atomically() {
    let (mut fixture, flow) = target(response_with_glyph::<false>).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (target_fixture, expected) = target(response_with_glyph::<true>).await;
    assert_target_preserves_catalogue(&flow, &expected);
    for case in 0..3 {
        let mut request = coordinated_request(&flow);
        assert_complete_tti(&request, &expected);
        malformed(&mut request, case);
        let error = reject(&fixture, &flow, request).await;
        assert!(
            matches!(error, NativeTeletextEditorialImportError::Tti(_)),
            "malformed coordinated Main atom {case}: {error:?}"
        );
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
    }
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    Box::pin(generated_refusal(&expected)).await;
    target_fixture.state.pool.close().await;
}

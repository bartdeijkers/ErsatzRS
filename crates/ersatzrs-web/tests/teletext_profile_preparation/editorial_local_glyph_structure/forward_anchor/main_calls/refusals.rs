//! Invalid Main call atoms cannot borrow receipt or original definition authority.

use super::{lifecycle::*, *};

fn inserted_word(request: &NativeTeletextEditorialImportRequest) -> usize {
    let name = page_name(request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let words = packet_words(document.records());
    let at: Vec<_> = words
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (*value == word(40, 16, 25)).then_some(index))
        .collect();
    assert_eq!(at.len(), 1);
    at[0]
}

fn alter(request: &mut NativeTeletextEditorialImportRequest, case: usize) {
    let at = inserted_word(request);
    let name = page_name(request);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    match case {
        0 => words[at + 1] = pointer(207, 19), // No owned definition at the target.
        1 => words[at + 1] = (words[at + 1] & !(31 << 6)) | (18 << 6),
        2 => words[at] = word(41, 16, 72), // Reserved column/origin atom.
        3 => {
            let retained = words
                .iter()
                .position(|value| *value == word(41, 4, 8))
                .unwrap();
            words[retained] = word(42, 4, 8); // Original noncall Main AP must stay exact.
        }
        4 => {
            let call = words[at + 1];
            let designation = ((call & 1) << 3) | ((call >> 15) & 7);
            let marker = usize::try_from(designation * 13 + ((call >> 11) & 15)).unwrap();
            assert_eq!(words[marker], pointer(marker, 23));
            words[marker] = pointer(marker, 22); // Original definition type is sealed.
        }
        5 => {
            // A Local invocation has an implicit owner. Relabelling the whole
            // display as another owner cannot import it through page124's file.
            let mut records = document.records().to_vec();
            let record = records
                .iter_mut()
                .find(|record| matches!(record, TeletextTtiRecord::PageNumber { .. }))
                .unwrap();
            let TeletextTtiRecord::PageNumber { page, .. } = record else {
                unreachable!()
            };
            assert_eq!(*page, 0x124);
            *page = 0x125;
            file.bytes = write_teletext_tti(
                &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
                policy().tti,
            )
            .unwrap();
            return;
        }
        _ => unreachable!(),
    }
    replace_packets(request, words);
}

#[tokio::test]
async fn main_local_invocation_structure_refuses_invalid_atoms_and_foreign_owner_atomically() {
    let (mut fixture, flow) = target(response).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (target_fixture, expected) = target(inserted_response::<false>).await;
    assert_target_preserves_catalogue(&flow, &expected);
    for case in 0..6 {
        let mut request = insertion_request(&flow.receipts[0], 1, flow.accepted[0].service(), 25);
        assert_complete_tti(&request, &expected);
        alter(&mut request, case);
        let error = reject(&fixture, &flow, request).await;
        assert!(
            matches!(error, NativeTeletextEditorialImportError::Tti(_)),
            "invalid Main call atom {case}: {error:?}"
        );
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
    }
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
}

fn generated_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        for region in page["regions"].as_array_mut().unwrap() {
            if region["id"] == "free-right-glyph" {
                assert_eq!(region["width"], 8);
                region["kind"] = json!("generated");
            }
        }
    }
    value
}

#[tokio::test]
async fn main_local_invocation_deletion_respects_generated_display_consumers_atomically() {
    let (mut fixture, flow) = target(generated_response).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (target_fixture, expected) = target(target_response).await;
    let request = deletion_request(&flow);
    assert_complete_tti(&request, &expected);
    let error = reject(&fixture, &flow, request).await;
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("valid Main deletion must reach exact Generated-consumer effects refusal: {error:?}")
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(124, 32)));
    assert_eq!(error.position, Some((1, 44)));
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    target_fixture.state.pool.close().await;
}

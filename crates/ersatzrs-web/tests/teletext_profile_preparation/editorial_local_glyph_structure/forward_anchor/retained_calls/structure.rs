//! Original Local body call structure stays receipt-bound and unpublished on import.
use super::*;

fn structure_target(flow: &Flow, calls: &[Edit]) -> EncodedTeletextAuthoredService {
    let mut value = origin_response("day", 124, 32)["service"].clone();
    let limits = policy().encoding.validation;
    let parse = |value: &Value| {
        ersatzrs_app::teletext_pages_v2::parse_declared_service(
            &serde_json::to_vec(value).unwrap(),
            GENERATION,
            limits.service,
            limits.declarations,
        )
        .unwrap()
    };
    assert_eq!(parse(&value), *flow.accepted[0].service().declared());
    let mut commands = vec![anchor()];
    commands.extend(calls.iter().map(|edit| edit.command()));
    let body = TeletextEnhancementProgram::new(commands).unwrap();
    let wrapper = value["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 65522)
        .unwrap();
    wrapper["program"] = serde_json::to_value(body).unwrap();
    let encoded = ersatzrs_ffmpeg::teletext::encode_teletext_authored_service(
        parse(&value),
        flow.saved[0].candidate().transmissions(),
        &policy().encoding.encoding,
    )
    .expect("independent Local call structure must compile before interchange import");
    assert_eq!(
        non_x26_records(flow.accepted[0].service()),
        non_x26_records(&encoded)
    );
    encoded
}

fn structure_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    expected: &EncodedTeletextAuthoredService,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Original Local body call structure".clone_into(&mut request.name);
    replace_packets(&mut request, native_words(expected));
    assert_request_native(&request, expected);
    request
}

fn structure_intent(
    calls: &[Edit],
    retained: Vec<Option<usize>>,
) -> TeletextLocalObjectGlyphProgramOverlay {
    let mut commands = vec![anchor()];
    commands.extend(calls.iter().map(|edit| edit.command()));
    TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_structure(
        identity(65522),
        Edit::Zero.program(),
        TeletextEnhancementProgram::new(commands).unwrap(),
        retained,
    )
    .unwrap()
}

#[tokio::test]
async fn local_body_call_deletion_native_import_preview_reopen() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        origin_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let expected = structure_target(&flow, &[]);
    let concept = import(&fixture, structure_request(&flow.receipts[0], 1, &expected)).await;
    assert_eq!(concept.head().revision, 2);
    let intent = structure_intent(&[], vec![Some(0)]);
    let preview = Box::pin(assert_call_concept(
        &fixture, &flow, &concept, &expected, &intent, 21,
    ))
    .await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_call_concept(
            &fixture, &flow, &concept, &expected, &intent, 21
        ))
        .await,
        preview
    );
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(&expected)
    );
    // A fresh receipt can add a call to the now empty original wrapper. Its
    // new subtree cannot borrow the deleted original call's source authority.
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let expected = structure_target(&flow, &[Edit::ColumnOne]);
    Box::pin(refuse_forged_fresh_carriers(
        &fixture, &flow, &latest, &expected, &accepted,
    ))
    .await;
    let added = import(&fixture, structure_request(&latest, 2, &expected)).await;
    assert_eq!(added.head().revision, 3);
    let intent = structure_intent(&[Edit::ColumnOne], vec![Some(0), None]);
    Box::pin(assert_call_concept(
        &fixture, &flow, &added, &expected, &intent, 21,
    ))
    .await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    let exported = drcs_bitmap::reexport(&fixture, &added).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(
        &fixture,
        structure_request(&latest, 3, flow.accepted[0].service()),
    )
    .await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

async fn refuse_forged_fresh_carriers(
    fixture: &Fixture,
    flow: &Flow,
    receipt: &TeletextDesignExportReceipt,
    expected: &EncodedTeletextAuthoredService,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    let carrier = encoded(expected)
        .command_positions()
        .iter()
        .find(|span| {
            span.source().object == Some(identity(65522))
                && span.original_object_source()
                    == Some(
                        ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource::HeaderPosition {
                            command: 0,
                        },
                    )
        })
        .unwrap();
    assert_eq!(carrier.first_word(), carrier.semantic_word());
    let current_publication = fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current_publication.revision, 2);
    assert_eq!(current_publication.design_id, flow.saved[0].head().id);
    assert_ne!(current_publication.id, flow.pending[0].id);
    for case in 0..2 {
        let mut request = structure_request(receipt, 2, expected);
        let mut words = native_words(expected);
        let at = carrier.semantic_word();
        assert_eq!(
            (words[at] & 63, (words[at] >> 6) & 31, words[at] >> 11),
            (8, 3, 0)
        );
        words[at] = if case == 0 {
            word(8, 3, 4)
        } else {
            word(9, 3, 0)
        };
        replace_packets(&mut request, words);
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, NativeTeletextEditorialImportError::Tti(_)),
            "forged fresh carrier {case}: {error:?}"
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(flow.saved[0].head().id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            2
        );
        assert!(fixture
            .state
            .application
            .teletext_design_revision(
                flow.saved[0].head().id,
                3,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .is_none());
        generations::assert_same_output(&recover(fixture, 1).await, accepted);
        generations::assert_same_output(&recover(fixture, 2).await, &flow.accepted[1]);
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(1)
                .await
                .unwrap(),
            Some(current_publication.clone())
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
        assert_retained_storage(fixture, flow).await;
    }
}

#[tokio::test]
async fn local_body_call_insertion_native_import_publication_and_cold_recovery() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        row_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let calls = [Edit::Zero, Edit::RowOne];
    let expected = structure_target(&flow, &calls);
    let concept = import(&fixture, structure_request(&flow.receipts[0], 1, &expected)).await;
    let intent = structure_intent(&calls, vec![Some(0), Some(1), None]);
    let preview = Box::pin(assert_call_concept(
        &fixture, &flow, &concept, &expected, &intent, 21,
    ))
    .await;
    assert_eq!(concept.head().revision, 2);
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_call_concept(
            &fixture, &flow, &concept, &expected, &intent, 21
        ))
        .await,
        preview
    );
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(&expected)
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn narrow_row_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = row_response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        for region in page["regions"].as_array_mut().unwrap() {
            if region["id"] == "free-right-glyph" {
                region["width"] = json!(4);
            }
        }
    }
    value
}

fn assert_original_call_rows(service: &EncodedTeletextAuthoredService, command: usize, row: u8) {
    let display = service
        .pages()
        .iter()
        .find_map(|page| match page {
            ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredPage::CompiledDisplay(display)
                if display.page().address() == channel_documents::address(124, 32) =>
            {
                Some(display)
            }
            _ => None,
        })
        .unwrap();
    let calls: Vec<_> = display
        .normalized()
        .original()
        .spatial_trace()
        .invocations()
        .iter()
        .filter(|call| {
            call.source().object == Some(identity(65522)) && call.source().command == command
        })
        .collect();
    assert_eq!(calls.len(), 2);
    for call in calls {
        assert_eq!(call.origin().row(), u16::from(row));
    }
}

#[tokio::test]
async fn local_body_call_structure_native_protects_each_original_consumer() {
    for (factory, calls, row) in [
        (
            narrow_origin_response as fn(&str, u16, u16) -> Value,
            vec![],
            1,
        ),
        (narrow_row_response, vec![Edit::Zero, Edit::RowOne], 2),
    ] {
        let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let expected = structure_target(&flow, &calls);
        // The immutable Main starts at AP1,8. Deleting its child changes row1;
        // the newly inserted row+1 call changes row2, never page header row0.
        assert_original_call_rows(flow.accepted[0].service(), 1, 1);
        if !calls.is_empty() {
            assert_original_call_rows(&expected, 2, 2);
        }
        let error = reject(
            &fixture,
            &flow,
            structure_request(&flow.receipts[0], 1, &expected),
        )
        .await;
        let NativeTeletextEditorialImportError::Effects(error) = error else {
            panic!(
                "changing a shared Local call structure must retain protected-consumer lineage checks"
            );
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert!(
            matches!(error.position, Some((actual, 48..=52)) if actual == row),
            "protected Local structure expected row {row}, columns48..52; actual {error:?}"
        );
        retained.assert_unchanged(&fixture, &flow).await;
        assert!(fixture.playout.try_recv().is_err());
        fixture.state.pool.close().await;
    }
}

#[tokio::test]
async fn local_body_call_insertion_native_refuses_invalid_call_atoms_atomically() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        row_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let expected = structure_target(&flow, &[Edit::Zero, Edit::RowOne]);
    let span = encoded(&expected)
        .command_positions()
        .iter()
        .find(|span| {
            span.original_object_source()
                == Some(
                    ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource::Object {
                        identity: identity(65522),
                        command: 2,
                    },
                )
        })
        .unwrap();
    let marker = encoded(&expected)
        .definition_positions()
        .iter()
        .find(|p| p.target() == identity(65520))
        .unwrap();
    for case in 0..4 {
        let mut request = structure_request(&flow.receipts[0], 1, &expected);
        let mut words = native_words(&expected);
        match case {
            0 => words[span.semantic_word()] = pointer(207, 19),
            1 => words[span.semantic_word()] = pointer(usize::from(marker.absolute_index()), 18),
            2 => words[span.first_word()] = word(41, 16, 72),
            3 => words.insert(span.first_word(), word(41, 16, 0)),
            _ => unreachable!(),
        }
        let end = words.iter().position(|w| *w == word(63, 31, 5)).unwrap();
        words.truncate(end + 1);
        replace_packets(&mut request, words);
        assert!(
            matches!(
                reject(&fixture, &flow, request).await,
                NativeTeletextEditorialImportError::Tti(_)
            ),
            "invalid inserted Local atom {case}"
        );
    }
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

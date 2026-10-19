//! Retained Local calls use exact original identity and fresh receipt authority.
use super::*;

#[derive(Clone, Copy, Debug)]
enum Edit {
    Zero,
    ColumnOne,
    RowOne,
    Absent,
    Retarget,
}
impl Edit {
    fn command(self) -> TeletextEnhancementCommand {
        TeletextEnhancementCommand::InvokeObject {
            target: identity(if matches!(self, Self::Retarget) {
                65521
            } else {
                65520
            }),
            object_type: TeletextObjectType::Passive,
            offset: match self {
                Self::Absent => None,
                Self::ColumnOne => Some(TeletextObjectOffset::new(0, 1).unwrap()),
                Self::RowOne => Some(TeletextObjectOffset::new(1, 0).unwrap()),
                Self::Zero | Self::Retarget => Some(TeletextObjectOffset::new(0, 0).unwrap()),
            },
        }
    }
    fn program(self) -> TeletextEnhancementProgram {
        TeletextEnhancementProgram::new(vec![anchor(), self.command()]).unwrap()
    }
}

fn origin_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = wrapper_response(theme, page, subcode);
    let wrapper = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 65522)
        .unwrap();
    wrapper["program"]["commands"][1]["offset"] = json!({"row":0,"column":0});
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        for region in page["regions"].as_array_mut().unwrap() {
            if region["id"] == "free-right-glyph" {
                region["width"] = json!(9);
            }
        }
    }
    value
}

fn origin_target(flow: &Flow, edit: Edit) -> EncodedTeletextAuthoredService {
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
    let before = flow.accepted[0].service();
    assert_eq!(
        parse(&value),
        *before.declared(),
        "independent original fixture has the exact accepted native service"
    );
    let wrapper = value["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 65522)
        .unwrap();
    assert_eq!(
        wrapper["program"]["commands"][1]["offset"],
        json!({"row":0,"column":0})
    );
    wrapper["program"] = serde_json::to_value(edit.program()).unwrap();
    let expected = ersatzrs_ffmpeg::teletext::encode_teletext_authored_service(
        parse(&value),
        flow.saved[0].candidate().transmissions(),
        &policy().encoding.encoding,
    )
    .expect("independent Local call-origin target must compile before interchange import");
    for target in [identity(65520), identity(65521)] {
        assert_eq!(
            before.declared().semantic().objects().definition(target),
            expected.declared().semantic().objects().definition(target)
        );
    }
    assert_eq!(non_x26_records(before), non_x26_records(&expected));
    expected
}

fn call_words(service: &EncodedTeletextAuthoredService, edit: Edit) -> Vec<u32> {
    let TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset,
    } = edit.command()
    else {
        unreachable!()
    };
    assert_eq!(object_type, TeletextObjectType::Passive);
    let marker = encoded(service)
        .definition_positions()
        .iter()
        .find(|position| position.target() == target)
        .unwrap();
    let mut words = Vec::new();
    if let Some(offset) = offset {
        words.push(word(
            40 + u32::from(offset.row()),
            16,
            u32::from(offset.column()),
        ));
    }
    words.push(invocation(usize::from(marker.absolute_index())));
    words
}

fn origin_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    service: &EncodedTeletextAuthoredService,
    from: Edit,
    to: Edit,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Retained Local wrapper call origin".clone_into(&mut request.name);
    let name = page_name(&request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(document.records());
    let before = encoded(service);
    let call = before
        .command_positions()
        .iter()
        .find(|position| {
            position.original_object_source()
                == Some(
                    ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource::Object {
                        identity: identity(65522),
                        command: 1,
                    },
                )
        })
        .unwrap();
    let old_words = call_words(service, from);
    assert_eq!(&words[call.first_word()..=call.semantic_word()], &old_words);
    let replacement = call_words(service, to);
    let grow = replacement.len() >= old_words.len();
    let delta = replacement.len().abs_diff(old_words.len());
    let end = words
        .iter()
        .position(|value| *value == word(63, 31, 5))
        .unwrap();
    words.truncate(end + 1);
    words.splice(call.first_word()..=call.semantic_word(), replacement);
    for marker in before
        .definition_positions()
        .iter()
        .map(|position| usize::from(position.absolute_index()))
        .filter(|index| *index > call.semantic_word())
    {
        let moved = if grow { marker + delta } else { marker - delta };
        for value in &mut words {
            for mode in 17..=23 {
                if *value == pointer(marker, mode) {
                    *value = pointer(moved, mode);
                    break;
                }
            }
        }
    }
    replace_packets(&mut request, words);
    request
}

async fn assert_origin_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
    edit: Edit,
) -> TeletextRaster {
    let intent = TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_changes(
        identity(65522),
        Edit::Zero.program(),
        edit.program(),
        vec![Some(0), Some(1)],
    )
    .unwrap();
    assert!(intent.has_invocation_changes());
    Box::pin(assert_call_concept(
        fixture, flow, concept, expected, &intent, 20,
    ))
    .await
}

async fn assert_call_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &EncodedTeletextAuthoredService,
    intent: &TeletextLocalObjectGlyphProgramOverlay,
    schema_version: u16,
) -> TeletextRaster {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), schema_version);
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        std::slice::from_ref(intent)
    );
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert!(overlay.resource_object_operands().is_empty());
    assert!(overlay.resource_object_program_overlays().is_empty());
    assert!(overlay.resource_object_definition_additions().is_empty());
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
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

fn assert_request_native(
    request: &NativeTeletextEditorialImportRequest,
    expected: &EncodedTeletextAuthoredService,
) {
    let name = page_name(request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    assert_eq!(
        packet_words(document.records()),
        native_words(expected),
        "complete edited TTI equals independent typed Local-call target"
    );
}

async fn repeat_and_root_undo(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    let current = drcs_bitmap::restored(fixture, concept).await;
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let latest = receipt(fixture, &exported).await;
    let expected = origin_target(flow, Edit::Absent);
    let request = origin_request(
        &latest,
        2,
        current.encoded().unwrap(),
        Edit::ColumnOne,
        Edit::Absent,
    );
    assert_request_native(&request, &expected);
    let repeated = import(fixture, request).await;
    assert_eq!(repeated.head().revision, 3);
    Box::pin(assert_origin_concept(
        fixture,
        flow,
        &repeated,
        &expected,
        Edit::Absent,
    ))
    .await;
    generations::assert_same_output(&recover(fixture, 1).await, accepted);
    let exported = drcs_bitmap::reexport(fixture, &repeated).await;
    let latest = receipt(fixture, &exported).await;
    let original = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let mut request = drcs_bitmap::from_receipt(&latest, 3);
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
    assert_request_native(&request, flow.accepted[0].service());
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
    generations::assert_same_output(&recover(fixture, 1).await, accepted);
    generations::assert_same_output(&recover(fixture, 2).await, &flow.accepted[1]);
}

fn row_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = origin_response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        for region in page["regions"].as_array_mut().unwrap() {
            if region["id"] == "free-right-glyph" {
                region["height"] = json!(2);
            }
        }
    }
    value
}

#[tokio::test]
async fn local_wrapper_retained_call_native_absence_row_and_original_target_keep_full_identity() {
    for (factory, edit) in [
        (origin_response as fn(&str, u16, u16) -> Value, Edit::Absent),
        (row_response, Edit::RowOne),
        (origin_response, Edit::Retarget),
    ] {
        let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let expected = origin_target(&flow, edit);
        let request = origin_request(
            &flow.receipts[0],
            1,
            flow.accepted[0].service(),
            Edit::Zero,
            edit,
        );
        assert_request_native(&request, &expected);
        let concept = import(&fixture, request).await;
        let preview = Box::pin(assert_origin_concept(
            &fixture, &flow, &concept, &expected, edit,
        ))
        .await;
        assert_original_pointers(&fixture, &flow).await;
        fixture = fixture.reopen().await;
        assert_eq!(
            Box::pin(assert_origin_concept(
                &fixture, &flow, &concept, &expected, edit
            ))
            .await,
            preview
        );
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        assert!(fixture.playout.try_recv().is_err());
        fixture.state.pool.close().await;
    }
}

fn narrow_origin_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = origin_response(theme, page, subcode);
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
async fn local_wrapper_retained_call_native_protects_second_consumer_even_with_equal_pixels() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        narrow_origin_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for edit in [Edit::ColumnOne, Edit::Absent] {
        let expected = origin_target(&flow, edit);
        if matches!(edit, Edit::Absent) {
            assert_eq!(
                external_editor_probe::preview_page(&expected, 124),
                external_editor_probe::preview_page(flow.accepted[0].service(), 124)
            );
        }
        let request = origin_request(
            &flow.receipts[0],
            1,
            flow.accepted[0].service(),
            Edit::Zero,
            edit,
        );
        assert_request_native(&request, &expected);
        let NativeTeletextEditorialImportError::Effects(error) =
            reject(&fixture, &flow, request).await
        else {
            panic!("retained-call changes must preserve every protected consumer's writer lineage");
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert!(matches!(error.position, Some((1, 48..=52))));
    }
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn local_wrapper_retained_call_native_rejects_unknown_target_type_and_origin_atoms() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        origin_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let before = encoded(flow.accepted[0].service());
    let call = before
        .command_positions()
        .iter()
        .find(|position| {
            position.original_object_source()
                == Some(
                    ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource::Object {
                        identity: identity(65522),
                        command: 1,
                    },
                )
        })
        .unwrap();
    assert!(!before
        .definition_positions()
        .iter()
        .any(|position| position.absolute_index() == 207));
    for case in 0..4 {
        let mut request = origin_request(
            &flow.receipts[0],
            1,
            flow.accepted[0].service(),
            Edit::Zero,
            Edit::ColumnOne,
        );
        let name = page_name(&request);
        let file = request.files.iter().find(|file| file.name == name).unwrap();
        let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
        let mut words = packet_words(document.records());
        match case {
            0 => words[call.semantic_word()] = pointer(207, 19),
            1 => {
                words[call.semantic_word()] =
                    pointer(definitions(flow.accepted[0].service())[0], 18);
            }
            2 => words[call.first_word()] = word(40, 16, 72),
            3 => {
                assert_eq!(words[call.semantic_word() + 1], word(63, 31, 5));
                words.insert(call.first_word(), word(40, 16, 0));
            }
            _ => unreachable!(),
        }
        let end = words
            .iter()
            .position(|value| *value == word(63, 31, 5))
            .unwrap();
        words.truncate(end + 1);
        replace_packets(&mut request, words);
        assert!(
            matches!(
                reject(&fixture, &flow, request).await,
                NativeTeletextEditorialImportError::Tti(_)
            ),
            "invalid Local call case {case}"
        );
    }
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn local_wrapper_retained_call_origin_native_import_preview_reopen_preserves_original() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        origin_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let expected = origin_target(&flow, Edit::ColumnOne);
    let request = origin_request(
        &flow.receipts[0],
        1,
        flow.accepted[0].service(),
        Edit::Zero,
        Edit::ColumnOne,
    );
    let original = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    assert_eq!(request.sidecar, original.sidecar);
    let name = page_name(&request);
    for file in request.files.iter().filter(|file| file.name != name) {
        assert_eq!(
            file.bytes,
            original
                .files
                .iter()
                .find(|saved| saved.name == file.name)
                .unwrap()
                .bytes
        );
    }
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    assert_eq!(
        packet_words(document.records()),
        native_words(&expected),
        "complete edited TTI equals independent typed Local-call target"
    );
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    let preview = Box::pin(assert_origin_concept(
        &fixture,
        &flow,
        &concept,
        &expected,
        Edit::ColumnOne,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_origin_concept(
            &fixture,
            &flow,
            &concept,
            &expected,
            Edit::ColumnOne
        ))
        .await,
        preview
    );
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(&expected)
    );
    Box::pin(repeat_and_root_undo(&fixture, &flow, &concept, &accepted)).await;
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[path = "retained_calls/structure.rs"]
mod structure;

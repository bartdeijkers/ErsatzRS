//! Prepared original DRCS scope/subtable selection edits; no editor/delivery/client claim.
use super::*;
use ersatzrs_domain::{
    TeletextDrcsBitmap, TeletextDrcsGlyphDefinition, TeletextDrcsGlyphReference,
    TeletextDrcsGlyphSlot, TeletextDrcsSubtable, TeletextDrcsSubtableIndex, TeletextDrcsTableScope,
    TeletextEnhancementCommand, TeletextLocalProgramOperandOverlay, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectType, TeletextRawDataPageNumber,
    TeletextResourceIdentity, TeletextResourceKind, TeletextResourceSubpageIndex,
    TeletextResourceUpdateCounter,
};
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextResourceFullUpdate, TeletextServiceResourceTransmission,
};

#[path = "drcs_selections/fixture.rs"]
pub(super) mod fixture;
#[path = "drcs_selections/oracle.rs"]
mod oracle;
use fixture::{
    bank, encoding, free, generated_right, identity, narrow_main, narrow_right, operation, scope,
    selector_word, word, zone, COLUMNS, COMMANDS, VISUAL,
};
use oracle::{assert_originals, assert_pixels, assert_rendered, selectors};

fn changes(
    index: u8,
) -> (
    Vec<TeletextLocalProgramOperandOverlay>,
    Vec<TeletextLocalObjectOperandOverlay>,
) {
    let mut page = Vec::new();
    let mut objects = Vec::new();
    for target in 0..4 {
        if operation(0, target) == operation(index, target) {
            continue;
        }
        if target < 2 {
            page.push(
                TeletextLocalProgramOperandOverlay::new(
                    channel_documents::address(124, 32),
                    COMMANDS[target],
                    COLUMNS[target],
                    operation(0, target),
                    operation(index, target),
                )
                .unwrap(),
            );
        } else {
            objects.push(
                TeletextLocalObjectOperandOverlay::new(
                    identity(124),
                    COMMANDS[target],
                    COLUMNS[target],
                    operation(0, target),
                    operation(index, target),
                )
                .unwrap(),
            );
        }
    }
    (page, objects)
}

fn positions(service: &EncodedTeletextAuthoredService) -> [usize; 4] {
    let program = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.normalized().encoded())
            }
            _ => None,
        })
        .unwrap();
    std::array::from_fn(|target| {
        let mut found = program.command_positions().iter().filter(|position| {
            if target < 2 {
                position.source().object.is_none()
                    && position.original_local_command() == Some(COMMANDS[target])
            } else {
                position.source().object == Some(identity(124))
                    && position.source().command == COMMANDS[target]
                    && position.original_local_command().is_none()
            }
        });
        let position = found.next().unwrap();
        assert!(found.next().is_none());
        assert!(
            position.first_word() <= position.semantic_word() && position.semantic_word() < 208
        );
        position.semantic_word()
    })
}

fn request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    baseline: &EncodedTeletextAuthoredService,
    transition: (u8, u8),
    malformed: Option<(usize, u32)>,
) -> NativeTeletextEditorialImportRequest {
    let positions = positions(baseline);
    let selectors = selectors(baseline, transition.0);
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Prepared original page/Local DRCS selection bundle".clone_into(&mut request.name);
    let sidecar: Value = serde_json::from_slice(&request.sidecar).unwrap();
    let name = sidecar["project"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"]["page"] == 124 && page["page"]["subcode"] == 32)
        .unwrap()["name"]
        .as_str()
        .unwrap();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let original = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut records = original.records().to_vec();
    let mut seen = [0; 6];
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (group, &position) in selectors.iter().enumerate() {
                if usize::from(*designation) == position / 13 {
                    assert_eq!(words[position % 13], selector_word(transition.0, group));
                    words[position % 13] = selector_word(transition.1, group);
                    seen[4 + group] += 1;
                }
            }
            for (target, &position) in positions.iter().enumerate() {
                if usize::from(*designation) == position / 13 {
                    assert_eq!(words[position % 13], word(transition.0, target));
                    words[position % 13] = malformed
                        .filter(|(t, _)| *t == target)
                        .map_or_else(|| word(transition.1, target), |(_, word)| word);
                    seen[target] += 1;
                }
            }
        }
    }
    assert_eq!(seen, [1; 6]);
    let mut reverted = records.clone();
    for record in &mut reverted {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (group, &position) in selectors.iter().enumerate() {
                if usize::from(*designation) == position / 13 {
                    words[position % 13] = selector_word(transition.0, group);
                }
            }
            for (target, &position) in positions.iter().enumerate() {
                if usize::from(*designation) == position / 13 {
                    words[position % 13] = word(transition.0, target);
                }
            }
        }
    }
    assert_eq!(
        reverted,
        original.records(),
        "only four original glyph words and two existing shared selectors may change"
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    index: u8,
) {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    let (page, objects) = changes(index);
    assert_eq!(overlay.schema_version(), 10);
    assert!(overlay.pages().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.local_program_operands(), page.as_slice());
    assert_eq!(overlay.local_object_operands(), objects.as_slice());
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
    let mut source: Value = serde_json::from_slice(&revision.document).unwrap();
    source.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        source,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let current = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.transmissions(), original.transmissions());
    assert_eq!(current.sources().len(), original.sources().len());
    for (source, expected) in current.sources().iter().zip(original.sources()) {
        assert_eq!(source.bytes(), expected.bytes());
        assert_eq!(source.authoring(), expected.authoring());
    }
    assert_eq!(
        current.producers()[0].original(),
        original.producers()[0].original()
    );
    let before = flow.accepted[0].service();
    let expected = before
        .declared()
        .clone()
        .with_local_program_operand_overlays(&page, policy().encoding.validation.service)
        .unwrap()
        .with_local_object_operand_overlays(&objects, policy().encoding.validation.service)
        .unwrap();
    let after = current.encoded().unwrap();
    assert_eq!(after.declared(), &expected);
    assert_rendered(before, after, index);
}

fn selector_request(
    flow: &Flow,
    target: usize,
    replacement: u32,
) -> NativeTeletextEditorialImportRequest {
    let mut request = request(
        &flow.receipts[0],
        1,
        flow.accepted[0].service(),
        (0, 1),
        None,
    );
    let first = selectors(flow.accepted[0].service(), 0)[target];
    // request(0,1) above has already changed this shared prefix to S1.
    let staged = selector_word(1, target);
    let name = flow.exported[0]
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 124 && page.page.subcode == 32)
        .unwrap()
        .name
        .as_str();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let parsed = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut records = parsed.records().to_vec();
    let mut seen = 0;
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            if usize::from(*designation) == first / 13 {
                assert_eq!(words[first % 13], staged);
                words[first % 13] = replacement;
                seen += 1;
            }
        }
    }
    assert_eq!(seen, 1);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

async fn malformed(fixture: &Fixture, flow: &Flow) {
    for (target, bad) in [
        (0, (word(1, 0) & 0x7ff) | (48 << 11)),
        (3, (word(1, 3) & 0x7ff) | (0x70 << 11)),
        (0, word(1, 0) ^ 1),
        (3, (word(1, 3) & !(31 << 6)) | (15 << 6)),
    ] {
        assert!(matches!(
            super::reject(
                fixture,
                flow,
                request(
                    &flow.receipts[0],
                    1,
                    flow.accepted[0].service(),
                    (0, 1),
                    Some((target, bad))
                )
            )
            .await,
            NativeTeletextEditorialImportError::Tti(_)
        ));
        assert_retained_storage(fixture, flow).await;
    }
    for group in 0..2 {
        for bad in [
            selector_word(0, group) ^ 1,
            selector_word(0, group) ^ (0x10 << 11),
        ] {
            assert!(matches!(
                super::reject(fixture, flow, selector_request(flow, group, bad)).await,
                NativeTeletextEditorialImportError::Tti(_)
            ));
            assert_retained_storage(fixture, flow).await;
        }
    }
}
async fn undefined_slots(fixture: &Fixture, flow: &Flow) {
    use ersatzrs_domain::{
        TeletextCollectedDrcsError, TeletextDrcsResourceError, TeletextEffectiveProgramSource,
        TeletextEnhancementSource, TeletextProducerCompositionFailure, TeletextServiceError,
    };
    use ersatzrs_web::host::{
        teletext_channel_documents::ChannelTeletextDocumentEncodingError,
        teletext_designs::NativeTeletextDesignError,
    };
    for target in [0, 3] {
        let replacement = (word(1, target) & 0x7ff) | ((if target == 0 { 0x42 } else { 2 }) << 11);
        let NativeTeletextEditorialImportError::Design(error) = super::reject(
            fixture,
            flow,
            request(
                &flow.receipts[0],
                1,
                flow.accepted[0].service(),
                (0, 1),
                Some((target, replacement)),
            ),
        )
        .await
        else {
            panic!("undefined slot2 must reach semantic bank validation");
        };
        let NativeTeletextDesignError::Encoding(error) = *error else {
            panic!("missing glyph requires native reconstruction refusal");
        };
        let ChannelTeletextDocumentEncodingError::Composition(error) = *error else {
            panic!("undefined glyph must reject during service reconstruction");
        };
        let TeletextProducerCompositionFailure::Semantic(reason) = error.reason else {
            panic!("existing bank has no authored slot2");
        };
        let TeletextServiceError::CollectedDrcs {
            program_source: TeletextEffectiveProgramSource::Local,
            error:
                TeletextCollectedDrcsError {
                    source_location: Some(source),
                    error:
                        TeletextDrcsResourceError::MissingGlyph {
                            display,
                            identity: resource,
                            slot,
                            ..
                        },
                },
        } = *reason
        else {
            panic!("missing exact original resource/glyph diagnostic required");
        };
        assert_eq!(display, channel_documents::address(124, 32));
        assert_eq!(resource, bank(scope(1, target), 1));
        assert_eq!(slot.get(), 2);
        assert_eq!(
            source,
            TeletextEnhancementSource {
                object: if target == 0 {
                    None
                } else {
                    Some(identity(124))
                },
                command: COMMANDS[target],
            }
        );
        assert_retained_storage(fixture, flow).await;
    }
}

async fn repeat_and_undo(fixture: &Fixture, flow: &Flow, concept: &SavedEditorialTeletextDesign) {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let latest = receipt(fixture, &exported).await;
    let current = drcs_bitmap::restored(fixture, concept).await;
    let repeated = import(
        fixture,
        request(&latest, 2, current.encoded().unwrap(), (1, 2), None),
    )
    .await;
    assert_eq!(repeated.head().revision, 3);
    assert_concept(fixture, flow, &repeated, 2).await;
    let exported = drcs_bitmap::reexport(fixture, &repeated).await;
    let latest = receipt(fixture, &exported).await;
    let current = drcs_bitmap::restored(fixture, &repeated).await;
    let undone = import(
        fixture,
        request(&latest, 3, current.encoded().unwrap(), (2, 0), None),
    )
    .await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        drcs_bitmap::stored(fixture, &undone).await.document,
        flow.originals[0].document
    );
    assert_eq!(
        designs::units(
            drcs_bitmap::restored(fixture, &undone)
                .await
                .encoded()
                .unwrap()
        ),
        designs::units(flow.accepted[0].service())
    );
}

async fn positive(factory: fn(&str, u16, u16) -> Value) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        factory,
        Fixture::new_persistent().await,
        encoding,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_originals(&flow);
    Box::pin(malformed(&fixture, &flow)).await;
    Box::pin(undefined_slots(&fixture, &flow)).await;
    let concept = import(
        &fixture,
        request(
            &flow.receipts[0],
            1,
            flow.accepted[0].service(),
            (0, 1),
            None,
        ),
    )
    .await;
    assert_eq!(concept.head().revision, 2);
    assert_concept(&fixture, &flow, &concept, 1).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_concept(&fixture, &flow, &concept, 1).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    let current = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(current.encoded().unwrap())
    );
    assert_pixels(accepted.service(), 1, false);
    Box::pin(repeat_and_undo(&fixture, &flow, &concept)).await;
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn retained_page_and_local_drcs_selection_bundle_publishes_repeats_and_undoes() {
    for factory in [free as fn(&str, u16, u16) -> Value, zone] {
        Box::pin(positive(factory)).await;
    }
}

#[tokio::test]
async fn retained_drcs_selection_bundle_requires_every_main_and_panel_permission() {
    for (factory, position) in [
        (narrow_main as fn(&str, u16, u16) -> Value, (1, 13)),
        (narrow_right, (1, 47)),
        (generated_right, (1, 46)),
    ] {
        let (fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            encoding,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        assert_originals(&flow);
        let NativeTeletextEditorialImportError::Effects(error) = super::reject(
            &fixture,
            &flow,
            request(
                &flow.receipts[0],
                1,
                flow.accepted[0].service(),
                (0, 1),
                None,
            ),
        )
        .await
        else {
            panic!("all shared DRCS selection consumers need captured permission");
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert_eq!(error.position, Some(position));
        retained.assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}

#[path = "drcs_selections/compaction.rs"]
mod compaction;

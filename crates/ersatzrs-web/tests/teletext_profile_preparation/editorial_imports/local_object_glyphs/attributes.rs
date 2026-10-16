//! Prepared mixed Local colour/size-preserving attributes, no actual editor proof.
use super::*;
use ersatzrs_domain::{
    TeletextColourReference, TeletextEnhancementCommand, TeletextObjectId, TeletextObjectIdentity,
    TeletextObjectOffset, TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;

const LATIN_A: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0];

fn operation(index: u8, command: usize) -> TeletextColumnOperation {
    let (foreground, background, invert) = match index {
        0 => (18, 0, false),
        1 => (20, 2, true),
        2 => (21, 3, false),
        _ => unreachable!(),
    };
    match command {
        0 => TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(foreground).unwrap(),
        },
        1 => TeletextColumnOperation::Background {
            colour: TeletextColourReference::new(background).unwrap(),
        },
        2 => TeletextColumnOperation::Attributes {
            double_height: false,
            double_width: false,
            boxed: false,
            conceal: false,
            invert,
            underline_or_separated_mosaics: false,
        },
        _ => unreachable!(),
    }
}

fn word(index: u8, command: usize) -> u32 {
    let (mode, data) = match operation(index, command) {
        TeletextColumnOperation::Foreground { colour } => (0, u32::from(colour.get())),
        TeletextColumnOperation::Background { colour } => (3, u32::from(colour.get())),
        TeletextColumnOperation::Attributes { invert, .. } => (12, u32::from(invert) << 4),
        _ => unreachable!(),
    };
    (mode << 6) | (data << 11)
}

fn response(theme: &str, page: u16, subcode: u16, permission: Permission) -> Value {
    let mut value = super::response(theme, page, subcode, Family::G0, permission);
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        let commands = object["program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[2]["operation"],
            serde_json::to_value(Family::G0.operation(0)).unwrap()
        );
        commands.insert(
            2,
            json!({"kind":"at_column","column":0,"operation":operation(0,2)}),
        );
    }
    value
}
fn free(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Permission::Free)
}
fn zone(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Permission::Zone)
}
fn narrow(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Permission::Narrow)
}
fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Permission::Generated)
}

fn identity(page: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        channel_documents::address(page, 32),
        TeletextObjectId::new(65520),
    )
}
fn changes(index: u8) -> Vec<TeletextLocalObjectOperandOverlay> {
    (0..3)
        .filter(|&command| operation(0, command) != operation(index, command))
        .map(|command| {
            TeletextLocalObjectOperandOverlay::new(
                identity(124),
                command,
                0,
                operation(0, command),
                operation(index, command),
            )
            .unwrap()
        })
        .collect()
}

fn positions(service: &EncodedTeletextAuthoredService) -> [usize; 3] {
    let program = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == identity(124).owner().unwrap() =>
            {
                Some(page.normalized().encoded())
            }
            _ => None,
        })
        .unwrap();
    std::array::from_fn(|command| {
        let mut found = program.command_positions().iter().filter(|position| {
            position.source().object == Some(identity(124)) && position.source().command == command
        });
        let position = found.next().unwrap();
        assert!(found.next().is_none());
        assert!(position.original_local_command().is_none());
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
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Prepared mixed retained Local colour/attributes".clone_into(&mut request.name);
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
    let mut seen = [0; 3];
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (command, &position) in positions.iter().enumerate() {
                if usize::from(*designation) == position / 13 {
                    assert_eq!(words[position % 13], word(transition.0, command));
                    words[position % 13] = malformed
                        .filter(|(target, _)| *target == command)
                        .map_or_else(|| word(transition.1, command), |(_, word)| word);
                    seen[command] += 1;
                }
            }
        }
    }
    assert_eq!(
        seen, [1; 3],
        "both invocations share one body for each operand"
    );
    let mut reverted = records.clone();
    for record in &mut reverted {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (command, &position) in positions.iter().enumerate() {
                if usize::from(*designation) == position / 13 {
                    words[position % 13] = word(transition.0, command);
                }
            }
        }
    }
    assert_eq!(
        reverted,
        original.records(),
        "all other records, markers and selectors remain exact"
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

fn assert_pixels(service: &EncodedTeletextAuthoredService, index: u8, night: bool) {
    let raster = external_editor_probe::preview_page(service, 124);
    assert_eq!((raster.width(), raster.height()), (672, 250));
    let (foreground, background) = match index {
        0 => (if night { [0, 0, 15] } else { [15, 0, 0] }, [0, 0, 0]),
        1 => ([0, 15, 0], [15, 0, 0]), // Invert swaps raw Fg20(red)/Bg2(green).
        2 => ([15, 0, 0], [15, 15, 0]),
        _ => unreachable!(),
    };
    for column in [44, 45] {
        for (row, bits) in LATIN_A.iter().enumerate() {
            for x in 0..12 {
                let rgb = if bits & (1 << (11 - x)) != 0 {
                    foreground
                } else {
                    background
                };
                let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                    raster.pixel(column * 12 + x, 10 + row)
                else {
                    panic!("shared Local glyph must have a settled RGB4 pixel");
                };
                assert_eq!(
                    pixel.get(),
                    rgb,
                    "fixed Latin A mask and exact mixed colours"
                );
            }
        }
    }
}

fn assert_originals(flow: &Flow) {
    let day = &flow.accepted[0].selection().bindings[0];
    let night = &flow.accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
    for (index, accepted) in flow.accepted.iter().enumerate() {
        let service = accepted.service();
        for page in [124, 125] {
            let definition = service
                .declared()
                .semantic()
                .objects()
                .definition(identity(page))
                .unwrap();
            assert_eq!(definition.object_type, TeletextObjectType::Passive);
            let expected: Vec<_> = (0..3)
                .map(|command| TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: operation(0, command),
                })
                .chain([TeletextEnhancementCommand::AtColumn {
                    column: 0,
                    operation: Family::G0.operation(0),
                }])
                .collect();
            assert_eq!(definition.program.commands(), expected);
            let calls: Vec<_> = service.declared().semantic().page_enhancements()
                [&channel_documents::address(page, 32)]
                .local_program
                .as_ref()
                .unwrap()
                .commands()
                .iter()
                .filter_map(|command| match command {
                    TeletextEnhancementCommand::InvokeObject {
                        target,
                        object_type,
                        offset,
                    } if *target == identity(page) => Some((*object_type, *offset)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                calls,
                [32, 33].map(|column| (
                    TeletextObjectType::Passive,
                    Some(TeletextObjectOffset::new(0, column).unwrap())
                ))
            );
        }
        assert_pixels(service, 0, index == 1);
    }
    assert_eq!(
        [word(0, 0), word(0, 1), word(0, 2)],
        [0x9000, 0x00c0, 0x0300]
    );
    assert_eq!(
        [word(1, 0), word(1, 1), word(1, 2)],
        [0xa000, 0x10c0, 0x8300]
    );
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
    let changes = changes(index);
    assert_eq!(overlay.schema_version(), 5);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.local_object_operands(), changes.as_slice());
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
    let after = current.encoded().unwrap();
    let before = flow.accepted[0].service();
    let expected = before
        .declared()
        .clone()
        .with_local_object_operand_overlays(&changes, policy().encoding.validation.service)
        .unwrap();
    assert_eq!(after.declared(), &expected);
    super::assert_rendered(before, after, changes[0]);
    assert_pixels(after, index, false);
}

async fn malformed(fixture: &Fixture, flow: &Flow) {
    for (command, word) in [
        (0, word(1, 0) ^ 1),
        (0, word(1, 0) ^ (1 << 6)),
        (2, word(1, 2) | (8 << 11)),
        (2, word(1, 2) | (1 << 11)),
        (2, word(1, 2) | (64 << 11)),
        (2, word(1, 2) | (65 << 11)),
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
                    Some((command, word))
                )
            )
            .await,
            NativeTeletextEditorialImportError::Tti(_)
        ));
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
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_originals(&flow);
    Box::pin(malformed(&fixture, &flow)).await;
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
async fn retained_local_object_mixed_colours_and_size_preserving_attributes_publish_repeat_and_undo(
) {
    for factory in [free as fn(&str, u16, u16) -> Value, zone] {
        Box::pin(positive(factory)).await;
    }
}

#[tokio::test]
async fn retained_local_object_mixed_attributes_protect_second_and_generated_consumers() {
    for factory in [narrow as fn(&str, u16, u16) -> Value, generated] {
        let (fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
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
            panic!("protected shared consumer must reject the entire mixed bundle");
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert_eq!(error.position, Some((1, 45)));
        retained.assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}

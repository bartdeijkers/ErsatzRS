//! Prepared same-origin extended glyph edits; no editor/delivery/client claim.
use super::*;
use ersatzrs_domain::{
    TeletextDiacriticIndex, TeletextEnhancementCommand, TeletextLocalProgramOperandOverlay,
    TeletextObjectId, TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;

// Existing glyph_mask_tests.rs:246–278, 422–448, 779–787; fixed font policy only.
const POUND: [u16; 10] = [0, 0x0fc, 0x186, 0x0c0, 0x3f0, 0x0c0, 0x3e6, 0x33c, 0, 0];
const ACUTE: [u16; 10] = [0x060, 0x0c0, 0, 0, 0, 0, 0, 0, 0, 0];
const E_LOWER: [u16; 10] = [0x060, 0x0c0, 0, 0x1fc, 0x306, 0x3fe, 0x300, 0x1fc, 0, 0];
const E_UPPER: [u16; 10] = [0x060, 0x0c0, 0x3fe, 0x300, 0x3fc, 0x300, 0x300, 0x3fe, 0, 0];
const A_MARK9: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0x060];
const A_MARK12: [u16; 10] = [0, 0x0f0, 0x198, 0x30c, 0x606, 0x7fe, 0x606, 0x606, 0, 0x7fe];
const COLUMNS: [u8; 4] = [8, 9, 0, 1];
const COMMANDS: [usize; 4] = [2, 5, 2, 3];
const VISUAL: [usize; 6] = [12, 13, 44, 45, 46, 47];

#[derive(Clone, Copy)]
enum Case {
    Free,
    Zone,
    NarrowMain,
    NarrowRight,
    GeneratedRight,
}

fn operation(index: u8, target: usize) -> TeletextColumnOperation {
    assert!(index <= 2);
    if matches!(target, 0 | 2) {
        TeletextColumnOperation::G2 {
            code: TeletextGlyphCode::new(if index == 1 { 0x42 } else { 0x23 }).unwrap(),
        }
    } else {
        let (code, mark) = if target == 1 {
            (if index == 0 { b'e' } else { b'E' }, 2)
        } else {
            (b'A', if index == 0 { 9 } else { 12 })
        };
        TeletextColumnOperation::G0Diacritic {
            code: TeletextGlyphCode::new(code).unwrap(),
            diacritic: TeletextDiacriticIndex::new(mark).unwrap(),
        }
    }
}
fn word(index: u8, target: usize) -> u32 {
    let (mode, data) = match operation(index, target) {
        TeletextColumnOperation::G2 { code } => (15, u32::from(code.get())),
        TeletextColumnOperation::G0Diacritic { code, diacritic } => {
            (16 | u32::from(diacritic.get()), u32::from(code.get()))
        }
        _ => unreachable!(),
    };
    u32::from(COLUMNS[target]) | (mode << 6) | (data << 11)
}
fn identity(page: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        channel_documents::address(page, 32),
        TeletextObjectId::new(65520),
    )
}
fn response(theme: &str, page: u16, subcode: u16, case: Case) -> Value {
    let permission = if matches!(case, Case::Zone) {
        Permission::Zone
    } else {
        Permission::Free
    };
    let mut value = super::response(theme, page, subcode, Family::G0, permission);
    assert_eq!(
        value["service"]["magazines"][0]["presentation"]["primary"],
        0
    );
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        let commands = object["program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[0]["operation"],
            json!({"kind":"foreground","colour":18})
        );
        assert_eq!(
            commands[1]["operation"],
            json!({"kind":"background","colour":0})
        );
        commands[2]["operation"] = serde_json::to_value(operation(0, 2)).unwrap();
        commands.push(json!({"kind":"at_column","column":1,"operation":operation(0,3)}));
    }
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let commands = metadata["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        assert_eq!(commands.len(), 5);
        commands[2]["operation"] = serde_json::to_value(operation(0, 0)).unwrap();
        assert_eq!(commands[3]["offset"]["column"], 32);
        commands[4]["offset"]["column"] = json!(34);
        // AtColumn advances AP: keep both calls at8 before the final column9.
        commands.push(json!({"kind":"at_column","column":9,"operation":operation(0,1)}));
    }
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = page["regions"].as_array_mut().unwrap();
        for region in regions.iter_mut() {
            if region["id"] == "free-glyph" {
                region["width"] = json!(if matches!(case, Case::NarrowMain) {
                    1
                } else {
                    2
                });
            }
            if region["id"] == "free-right-glyph" {
                region["width"] = json!(match case {
                    Case::NarrowRight => 3,
                    Case::GeneratedRight => 2,
                    _ => 4,
                });
            }
        }
        if matches!(case, Case::GeneratedRight) {
            regions.push(json!({"id":"generated-third","kind":"generated","area":"right_panel","row":1,"column":2,"width":1,"height":1}));
        }
    }
    value
}
fn free(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::Free)
}
fn zone(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::Zone)
}
fn narrow_main(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::NarrowMain)
}
fn narrow_right(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::NarrowRight)
}
fn generated_right(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::GeneratedRight)
}

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
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Prepared page/Local extended glyph bundle".clone_into(&mut request.name);
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
    let mut seen = [0; 4];
    for record in &mut records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
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
    assert_eq!(seen, [1; 4]);
    let mut reverted = records.clone();
    for record in &mut reverted {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
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
        "only four original retained operands may change"
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}

fn mask(index: u8, target: usize) -> [u16; 10] {
    match target {
        0 | 2 => {
            if index == 1 {
                ACUTE
            } else {
                POUND
            }
        }
        1 => {
            if index == 0 {
                E_LOWER
            } else {
                E_UPPER
            }
        }
        3 => {
            if index == 0 {
                A_MARK9
            } else {
                A_MARK12
            }
        }
        _ => unreachable!(),
    }
}
fn assert_pixels(service: &EncodedTeletextAuthoredService, index: u8, night: bool) {
    let raster = external_editor_probe::preview_page(service, 124);
    assert_eq!((raster.width(), raster.height()), (672, 250));
    for (visual, target) in VISUAL.into_iter().zip([0, 1, 2, 3, 2, 3]) {
        for (row, bits) in mask(index, target).into_iter().enumerate() {
            for x in 0..12 {
                let rgb = if bits & (1 << (11 - x)) == 0 {
                    [0, 0, 0]
                } else if night {
                    [0, 0, 15]
                } else {
                    [15, 0, 0]
                };
                let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                    raster.pixel(visual * 12 + x, 10 + row)
                else {
                    panic!("extended glyph must have a settled RGB4 pixel");
                };
                assert_eq!(
                    pixel.get(),
                    rgb,
                    "literal current Latin mask, not a Unicode alias"
                );
            }
        }
    }
}
fn assert_rendered(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    index: u8,
) {
    assert_eq!(before.pages().len(), after.pages().len());
    assert_eq!(before.magazine_units(), after.magazine_units());
    assert_eq!(before.inventory(), after.inventory());
    for (old, new) in before.pages().iter().zip(after.pages()) {
        assert_eq!(old.resource_header(), new.resource_header());
        let changed = matches!(old,EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address()==channel_documents::address(124,32));
        if changed {
            assert_ne!(old.data_units(), new.data_units());
        } else {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    assert_eq!(
        external_editor_probe::preview_page(before, 125),
        external_editor_probe::preview_page(after, 125)
    );
    let original = external_editor_probe::preview_page(before, 124);
    let edited = external_editor_probe::preview_page(after, 124);
    let mut changed = [false; 6];
    for (position, (a, b)) in original.pixels().iter().zip(edited.pixels()).enumerate() {
        if a != b {
            let (row, column) = (position / original.width(), position % original.width());
            assert!((10..20).contains(&row));
            let cell = VISUAL
                .iter()
                .position(|visual| *visual == column / 12)
                .expect("protected Main/panel/row pixels stay exact");
            changed[cell] = true;
        }
    }
    assert_eq!(
        changed,
        if index == 1 {
            [true; 6]
        } else {
            [false, true, false, true, false, true]
        }
    );
    assert_pixels(after, index, false);
}
fn assert_originals(flow: &Flow) {
    let day = &flow.accepted[0].selection().bindings[0];
    let night = &flow.accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
    for (index, accepted) in flow.accepted.iter().enumerate() {
        let service = accepted.service();
        assert!(service.compiler_drcs_resources().is_empty());
        for owner in [124, 125] {
            let definition = service
                .declared()
                .semantic()
                .objects()
                .definition(identity(owner))
                .unwrap();
            assert_eq!(definition.object_type, TeletextObjectType::Passive);
            assert_eq!(definition.program.commands().len(), 4);
            for target in [2, 3] {
                assert_eq!(
                    definition.program.commands()[COMMANDS[target]],
                    TeletextEnhancementCommand::AtColumn {
                        column: COLUMNS[target],
                        operation: operation(0, target)
                    }
                );
            }
            let commands = service.declared().semantic().page_enhancements()
                [&channel_documents::address(owner, 32)]
                .local_program
                .as_ref()
                .unwrap()
                .commands();
            for target in [0, 1] {
                assert_eq!(
                    commands[COMMANDS[target]],
                    TeletextEnhancementCommand::AtColumn {
                        column: COLUMNS[target],
                        operation: operation(0, target)
                    }
                );
            }
            let calls: Vec<_> = commands
                .iter()
                .filter_map(|command| match command {
                    TeletextEnhancementCommand::InvokeObject {
                        target,
                        object_type,
                        offset,
                    } if *target == identity(owner) => Some((*object_type, *offset)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                calls,
                [32, 34].map(|column| (
                    TeletextObjectType::Passive,
                    Some(TeletextObjectOffset::new(0, column).unwrap())
                ))
            );
        }
        assert_pixels(service, 0, index == 1);
    }
    assert_eq!(
        std::array::from_fn::<_, 4, _>(|target| word(0, target)),
        [0x11bc8, 0x32c89, 0x11bc0, 0x20e41]
    );
    assert_eq!(
        std::array::from_fn::<_, 4, _>(|target| word(1, target)),
        [0x213c8, 0x22c89, 0x213c0, 0x20f01]
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
    let (page, objects) = changes(index);
    assert_eq!(overlay.schema_version(), 6);
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
async fn malformed(fixture: &Fixture, flow: &Flow) {
    for (target, bad) in [
        (0, (word(1, 0) & !(31 << 6)) | (9 << 6)),
        (1, (word(1, 1) & !(31 << 6)) | (15 << 6)),
        (0, (word(1, 0) & 0x7ff) | (31 << 11)),
        (3, (word(1, 3) & 0x7ff) | (31 << 11)),
        (0, word(1, 0) ^ 1),
        (2, (word(1, 2) & !(31 << 6)) | (14 << 6)),
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
async fn retained_page_and_local_extended_glyph_bundle_publishes_repeats_and_undoes() {
    for factory in [free as fn(&str, u16, u16) -> Value, zone] {
        Box::pin(positive(factory)).await;
    }
}
#[tokio::test]
async fn retained_extended_glyph_bundle_requires_every_main_and_panel_permission() {
    for (factory, position) in [
        (narrow_main as fn(&str, u16, u16) -> Value, (1, 13)),
        (narrow_right, (1, 47)),
        (generated_right, (1, 46)),
    ] {
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
            panic!("all shared extended glyph consumers need captured permission");
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

//! Prepared original page/Local Flash and charset edits; no editor/client claim.
use super::*;
use ersatzrs_domain::{
    TeletextEnhancementCharsetDesignation, TeletextEnhancementCommand, TeletextFlashMode,
    TeletextFlashRate, TeletextLocalProgramOperandOverlay, TeletextObjectId,
    TeletextObjectIdentity, TeletextObjectOffset, TeletextObjectType,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;

// Source-fixed font_data/masks.bin: tables0/4, code43, ten LE u16 rows.
const LATIN_C: [u16; 10] = [0, 0x0fc, 0x186, 0x300, 0x300, 0x300, 0x186, 0x0fc, 0, 0];
const GREEK_GAMMA: [u16; 10] = [0, 0x7fe, 0x302, 0x300, 0x300, 0x300, 0x300, 0x780, 0, 0];
const COLUMNS: [u8; 4] = [8, 8, 0, 0];
const COMMANDS: [usize; 4] = [4, 5, 2, 3];
const VISUAL: [usize; 6] = [12, 13, 44, 45, 46, 47];

#[derive(Clone, Copy)]
enum Case {
    Free,
    Zone,
    NarrowMain,
    NarrowRight,
    GeneratedRight,
    ProtectedBlankRight,
}
fn operation(index: u8, target: usize) -> TeletextColumnOperation {
    assert!(index <= 2);
    if matches!(target, 0 | 2) {
        TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(if index == 1 {
                0x37
            } else {
                0
            })
            .unwrap(),
        }
    } else {
        let (mode, rate) = match index {
            0 => (TeletextFlashMode::Steady, TeletextFlashRate::Slow1Hz),
            1 => (TeletextFlashMode::Normal, TeletextFlashRate::Fast2HzPhase2),
            2 => (
                TeletextFlashMode::Inverted,
                TeletextFlashRate::Fast2HzPhase3,
            ),
            _ => unreachable!(),
        };
        TeletextColumnOperation::Flash { mode, rate }
    }
}
fn word(index: u8, target: usize) -> u32 {
    let (mode, data) = if matches!(target, 0 | 2) {
        (8, if index == 1 { 0x37 } else { 0 })
    } else {
        (
            7,
            match index {
                0 => 0,
                1 => 9,
                2 => 14,
                _ => unreachable!(),
            },
        )
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
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        let commands = object["program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(
            commands[2]["operation"],
            serde_json::to_value(Family::G0.operation(0)).unwrap()
        );
        commands.truncate(2);
        commands.extend([
            json!({"kind":"at_column","column":0,"operation":operation(0,2)}),
            json!({"kind":"at_column","column":0,"operation":operation(0,3)}),
            json!({"kind":"at_column","column":0,"operation":{"kind":"g0","code":67}}),
            json!({"kind":"at_column","column":1,"operation":{"kind":"g0","code":if matches!(case,Case::ProtectedBlankRight){32}else{67}}}),
            json!({"kind":"at_column","column":2,"operation":operation(0,2)}),
            json!({"kind":"at_column","column":2,"operation":operation(0,3)}),
        ]);
    }
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let commands = metadata["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        assert_eq!(commands.len(), 5);
        assert_eq!(commands[2]["operation"]["kind"], "drcs");
        let mut calls = [commands[3].clone(), commands[4].clone()];
        assert_eq!(calls[0]["offset"]["column"], 32);
        calls[1]["offset"]["column"] = json!(34);
        commands.truncate(2);
        // Passive calls retain the caller AP8. Run them before the Main words
        // advance AP to9/10; no backwards active-position reset is introduced.
        commands.extend(calls);
        // Fixed original restores bound context after the two Main glyphs.
        commands.extend([
            json!({"kind":"at_column","column":8,"operation":operation(0,0)}),
            json!({"kind":"at_column","column":8,"operation":operation(0,1)}),
            json!({"kind":"at_column","column":8,"operation":{"kind":"g0","code":67}}),
            json!({"kind":"at_column","column":9,"operation":{"kind":"g0","code":67}}),
            json!({"kind":"at_column","column":10,"operation":operation(0,0)}),
            json!({"kind":"at_column","column":10,"operation":operation(0,1)}),
        ]);
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
                    Case::NarrowRight | Case::ProtectedBlankRight => 3,
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
fn protected_blank_right(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Case::ProtectedBlankRight)
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
    selected_request(
        receipt,
        head,
        baseline,
        transition,
        malformed,
        &[0, 1, 2, 3],
    )
}
fn selected_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    baseline: &EncodedTeletextAuthoredService,
    transition: (u8, u8),
    malformed: Option<(usize, u32)>,
    targets: &[usize],
) -> NativeTeletextEditorialImportRequest {
    let positions = positions(baseline);
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Prepared page/Local Flash and charset bundle".clone_into(&mut request.name);
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
                if !targets.contains(&target) {
                    continue;
                }
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
    assert_eq!(
        seen,
        std::array::from_fn::<_, 4, _>(|target| usize::from(targets.contains(&target)))
    );
    let mut reverted = records.clone();
    for record in &mut reverted {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            for (target, &position) in positions.iter().enumerate() {
                if !targets.contains(&target) {
                    continue;
                }
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

fn phase_raster(
    service: &EncodedTeletextAuthoredService,
    page: u16,
    phase: TeletextFlashPhase,
) -> TeletextRaster {
    let preview = project_teletext_preview(
        service,
        channel_documents::address(page, 32),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: phase,
        },
        super::super::preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    preview.raster().clone()
}
fn assert_pixels(service: &EncodedTeletextAuthoredService, index: u8, night: bool) {
    for phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
        let raster = phase_raster(service, 124, phase);
        assert_eq!((raster.width(), raster.height()), (672, 250));
        let mask = if index == 1 { GREEK_GAMMA } else { LATIN_C };
        let hidden = matches!(
            (index, phase),
            (1, TeletextFlashPhase::Alternate) | (2, TeletextFlashPhase::Original)
        );
        for visual in VISUAL {
            for (row, bits) in mask.into_iter().enumerate() {
                for x in 0..12 {
                    let rgb = if hidden || bits & (1 << (11 - x)) == 0 {
                        [0, 0, 0]
                    } else if night {
                        [0, 0, 15]
                    } else {
                        [15, 0, 0]
                    };
                    let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                        raster.pixel(visual * 12 + x, 10 + row)
                    else {
                        panic!("every fixed Flash/charset pixel must be settled RGB4")
                    };
                    assert_eq!(
                        pixel.get(),
                        rgb,
                        "fixed charset masks and explicit flash phases; no wall clock inference"
                    );
                }
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
        let owner = matches!(old,EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address()==channel_documents::address(124,32));
        if owner {
            assert_ne!(old.data_units(), new.data_units());
        } else {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    for phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
        assert_eq!(
            phase_raster(before, 125, phase),
            phase_raster(after, 125, phase)
        );
        let original = phase_raster(before, 124, phase);
        let edited = phase_raster(after, 124, phase);
        let mut changed = [false; 6];
        for (position, (a, b)) in original.pixels().iter().zip(edited.pixels()).enumerate() {
            if a == b {
                continue;
            }
            let (row, column) = (position / original.width(), position % original.width());
            assert!((10..20).contains(&row));
            let cell = VISUAL
                .iter()
                .position(|visual| *visual == column / 12)
                .expect("every protected Main/panel/row pixel remains exact in both phases");
            changed[cell] = true;
        }
        assert_eq!(
            changed,
            if index == 2 && phase == TeletextFlashPhase::Alternate {
                [false; 6]
            } else {
                [true; 6]
            }
        );
    }
    assert_fallback_unchanged(before, after);
    assert_pixels(after, index, false);
}
fn assert_fallback_unchanged(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
) {
    let [mut original, mut edited] = [before, after].map(|service| {
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
        project_encoded_teletext_display(page, 128).unwrap()
    });
    for projection in [&mut original, &mut edited] {
        assert!(projection
            .records
            .iter()
            .any(|record| matches!(record, TeletextEncodedDisplayRecord::BaseRow { .. })));
        projection.records.retain(|record| {
            !matches!(
                record,
                TeletextEncodedDisplayRecord::Triplets { row: 26, .. }
            )
        });
    }
    assert_eq!(
        original, edited,
        "all native header/fallback and non-X26 records remain immutable"
    );
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
            assert_eq!(definition.program.commands().len(), 8);
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
        [0x208, 0x1c8, 0x200, 0x1c0]
    );
    assert_eq!(
        std::array::from_fn::<_, 4, _>(|target| word(1, target)),
        [0x1ba08, 0x49c8, 0x1ba00, 0x49c0]
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
    assert_eq!(overlay.schema_version(), 9);
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
        (0, word(1, 0) ^ 1),
        (0, (word(1, 0) & !(31 << 6)) | (9 << 6)),
        (2, (word(1, 2) & 0x7ff) | (0x38 << 11)), // reserved designation
        (1, (word(1, 1) & 0x7ff) | (24 << 11)),   // reserved flash rate6
        (3, (word(1, 3) & 0x7ff) | (64 << 11)),   // reserved D6
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
async fn retained_page_and_local_flash_charset_bundle_publishes_repeats_and_undoes() {
    for factory in [free as fn(&str, u16, u16) -> Value, zone] {
        Box::pin(positive(factory)).await;
    }
}
#[tokio::test]
async fn retained_flash_charset_bundle_requires_every_main_and_panel_permission() {
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
            panic!("all Flash and charset context consumers need captured permission");
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

#[tokio::test]
async fn retained_flash_phase_effects_require_permission_when_original_pixels_match() {
    for (factory, allowed) in [
        (free as fn(&str, u16, u16) -> Value, true),
        (narrow_right, false),
    ] {
        let (fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let request = selected_request(
            &flow.receipts[0],
            1,
            flow.accepted[0].service(),
            (0, 1),
            None,
            &[1, 3],
        );
        if allowed {
            let concept = import(&fixture, request).await;
            let current = drcs_bitmap::restored(&fixture, &concept).await;
            let after = current.encoded().unwrap();
            let before = flow.accepted[0].service();
            assert_eq!(
                phase_raster(before, 124, TeletextFlashPhase::Original),
                phase_raster(after, 124, TeletextFlashPhase::Original),
                "steady to Normal changes no Original-phase pixel"
            );
            assert_ne!(
                phase_raster(before, 124, TeletextFlashPhase::Alternate),
                phase_raster(after, 124, TeletextFlashPhase::Alternate),
                "actual alternate-phase effect must still require every consumer"
            );
            let revision = drcs_bitmap::stored(&fixture, &concept).await;
            let decoded =
                decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
            let overlay = decoded.editorial_overlay().unwrap();
            assert_eq!(overlay.schema_version(), 9);
            let (mut page, mut objects) = changes(1);
            page.retain(|change| {
                matches!(change.expected(), TeletextColumnOperation::Flash { .. })
            });
            objects.retain(|change| {
                matches!(change.expected(), TeletextColumnOperation::Flash { .. })
            });
            assert_eq!(overlay.local_program_operands(), page.as_slice());
            assert_eq!(overlay.local_object_operands(), objects.as_slice());
            let expected = before
                .declared()
                .clone()
                .with_local_program_operand_overlays(&page, policy().encoding.validation.service)
                .unwrap()
                .with_local_object_operand_overlays(&objects, policy().encoding.validation.service)
                .unwrap();
            assert_eq!(after.declared(), &expected);
        } else {
            let NativeTeletextEditorialImportError::Effects(error) =
                super::reject(&fixture, &flow, request).await
            else {
                panic!("protected second shared consumer must reject even an Original-phase-identical Flash edit")
            };
            assert_eq!(
                error.reason,
                TeletextEditorialEffectsFailure::ProtectedDisplay
            );
            assert_eq!(error.position, Some((1, 47)));
        }
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}

#[tokio::test]
async fn retained_charset_context_cannot_change_a_protected_coded_space() {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        protected_blank_right,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let original = flow.accepted[0].service();
    let definition = original
        .declared()
        .semantic()
        .objects()
        .definition(identity(124))
        .unwrap();
    assert_eq!(
        definition.program.commands()[5],
        TeletextEnhancementCommand::AtColumn {
            column: 1,
            operation: TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(32).unwrap()
            }
        }
    );
    let before = phase_raster(original, 124, TeletextFlashPhase::Original);
    for row in 10..20 {
        for column in 47 * 12..48 * 12 {
            let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(pixel))) =
                before.pixel(column, row)
            else {
                panic!("protected coded space is settled")
            };
            assert_eq!(pixel.get(), [0, 0, 0]);
        }
    }
    let NativeTeletextEditorialImportError::Effects(error) = super::reject(
        &fixture,
        &flow,
        selected_request(&flow.receipts[0], 1, original, (0, 1), None, &[0, 2]),
    )
    .await
    else {
        panic!("same-mask coded space must not acquire a different protected charset context")
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(124, 32)));
    assert_eq!(error.position, Some((1, 47)));
    retained.assert_unchanged(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

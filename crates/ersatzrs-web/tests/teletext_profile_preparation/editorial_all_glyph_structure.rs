//! Prepared all-glyph structure through native V12 concepts; no actual editor/client claim.

use super::super::local_object_glyphs;
use super::*;
use ersatzrs_ffmpeg::teletext::TeletextMainComposition;

#[path = "editorial_all_glyph_structure/actual_editor.rs"]
mod actual_editor;

fn family(index: usize, inserted: bool) -> TeletextColumnOperation {
    let value = match index {
        0 => json!({"kind":"g1_alpha","code":if inserted {66} else {65}}),
        1 => json!({"kind":"g2","code":if inserted {66} else {35}}),
        2 => json!({"kind":"g3","code":if inserted {95} else {47},"compatibility":"level15"}),
        3 => json!({"kind":"g3","code":if inserted {95} else {47},"compatibility":"level25"}),
        4 => json!({"kind":"g0_diacritic","code":65,"diacritic":if inserted {12} else {1}}),
        5 | 6 => {
            json!({"kind":"drcs","reference":{"table":if index==5 {"normal"} else {"global"},"subtable":0,"slot":u8::from(inserted)}})
        }
        _ => unreachable!(),
    };
    serde_json::from_value(value).unwrap()
}
fn all_commands(stage: u8) -> Vec<TeletextEnhancementCommand> {
    let mut commands = super::commands(0)[..3].to_vec();
    match stage {
        0 | 1 | 3 => {
            for index in 0..7 {
                let at = u8::try_from(index).unwrap();
                commands.push(column(at, family(index, false)));
                if stage == 1 {
                    commands.push(column(at, family(index, true)));
                }
            }
            commands.push(g0(7, b'B'));
        }
        2 => {
            for index in [0, 1, 2, 4, 5, 6] {
                let at = if index == 2 {
                    3
                } else {
                    u8::try_from(index).unwrap()
                };
                commands.push(column(at, family(index, true)));
            }
        }
        _ => unreachable!(),
    }
    commands.push(column(
        7,
        TeletextColumnOperation::Attributes {
            double_height: false,
            double_width: false,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: true,
        },
    ));
    commands.push(g0(7, b'C'));
    if stage == 3 {
        commands.push(g0(7, b'D'));
    }
    commands
}
fn all_program(stage: u8) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(all_commands(stage)).unwrap()
}
fn all_mapping(stage: u8) -> Vec<Option<usize>> {
    match stage {
        1 => vec![
            Some(0),
            Some(1),
            Some(2),
            Some(3),
            None,
            Some(4),
            None,
            Some(5),
            None,
            Some(6),
            None,
            Some(7),
            None,
            Some(8),
            None,
            Some(9),
            None,
            Some(10),
            Some(11),
            Some(12),
        ],
        2 => vec![
            Some(0),
            Some(1),
            Some(2),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(11),
            Some(12),
        ],
        3 => (0..13).map(Some).chain([None]).collect(),
        _ => unreachable!(),
    }
}
fn all_response(theme: &str, page: u16, subcode: u16, permission: Permission) -> Value {
    let mut value = super::response(theme, page, subcode, permission);
    local_object_glyphs::structural_original_banks(&mut value);
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        if object["identity"]["object_id"] == 65520 {
            object["program"] = serde_json::to_value(all_program(0)).unwrap();
        }
    }
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        for region in page["regions"].as_array_mut().unwrap() {
            if region["id"] == "free-right-glyph" {
                region["width"] = json!(if matches!(
                    permission,
                    Permission::Narrow | Permission::Generated
                ) {
                    8
                } else {
                    12
                });
            }
            if region["id"] == "generated-consumer" {
                region["column"] = json!(8);
                region["width"] = json!(4);
            }
        }
    }
    value
}
fn all_free(theme: &str, page: u16, subcode: u16) -> Value {
    all_response(theme, page, subcode, Permission::Free)
}
fn all_zone(theme: &str, page: u16, subcode: u16) -> Value {
    all_response(theme, page, subcode, Permission::Zone)
}
fn all_narrow(theme: &str, page: u16, subcode: u16) -> Value {
    all_response(theme, page, subcode, Permission::Narrow)
}
fn all_generated(theme: &str, page: u16, subcode: u16) -> Value {
    all_response(theme, page, subcode, Permission::Generated)
}

fn all_words(stage: u8) -> Vec<u32> {
    // EN300706 Table28 literal words, with independent per-definition DRCS latches.
    let mut result = Vec::new();
    let mut drcs = [false; 2];
    for command in all_commands(stage) {
        let TeletextEnhancementCommand::AtColumn { column, operation } = command else {
            unreachable!()
        };
        let (mode, data) = match operation {
            TeletextColumnOperation::Foreground { colour } => (0, u32::from(colour.get())),
            TeletextColumnOperation::Background { colour } => (3, u32::from(colour.get())),
            TeletextColumnOperation::DesignateCharset { designation } => {
                (8, u32::from(designation.get()))
            }
            TeletextColumnOperation::G0 { code } => (9, u32::from(code.get())),
            TeletextColumnOperation::G1Alpha { code } => (1, u32::from(code.get())),
            TeletextColumnOperation::G2 { code } => (15, u32::from(code.get())),
            TeletextColumnOperation::G3 {
                code,
                compatibility,
            } => (
                if compatibility == TeletextG3Compatibility::Level15 {
                    2
                } else {
                    11
                },
                u32::from(code.get()),
            ),
            TeletextColumnOperation::G0Diacritic { code, diacritic } => {
                (16 + u32::from(diacritic.get()), u32::from(code.get()))
            }
            TeletextColumnOperation::Attributes {
                underline_or_separated_mosaics: true,
                ..
            } => (12, 32),
            TeletextColumnOperation::Drcs { reference } => {
                assert_eq!(reference.subtable.get(), 0);
                let group = usize::from(reference.table == TeletextDrcsTableScope::Global);
                if !drcs[group] {
                    result.push(word(40, 24, if group == 0 { 80 } else { 16 }));
                    drcs[group] = true;
                }
                (
                    13,
                    u32::from(reference.slot.get()) | if group == 0 { 64 } else { 0 },
                )
            }
            _ => unreachable!(),
        };
        result.push(word(u32::from(column), mode, data));
    }
    result
}
fn all_request_words(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    service: &EncodedTeletextAuthoredService,
    from: u8,
    replacement: Vec<u32>,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Synthetic all-glyph Local structure".clone_into(&mut request.name);
    let name = page_name(&request);
    let file = request.files.iter().find(|file| file.name == name).unwrap();
    let records = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(records.records());
    let [target, following] = definitions(service);
    assert_eq!(words[target], marker(target));
    assert_eq!(words[following], marker(following));
    assert_eq!(&words[target + 1..following], all_words(from));
    let end = words
        .iter()
        .position(|word| *word == super::word(63, 31, 5))
        .unwrap();
    words.truncate(end + 1);
    let moved = target + 1 + replacement.len();
    words.splice(target + 1..following, replacement);
    words[moved] = marker(moved);
    let pointers: Vec<_> = words[..target]
        .iter()
        .enumerate()
        .filter_map(|(index, word)| (*word == invocation(following)).then_some(index))
        .collect();
    assert_eq!(
        pointers.len(),
        1,
        "one actual protected following-definition pointer must relocate"
    );
    words[pointers[0]] = invocation(moved);
    replace_packets(&mut request, words);
    request
}
fn all_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    service: &EncodedTeletextAuthoredService,
    from: u8,
    to: u8,
) -> NativeTeletextEditorialImportRequest {
    all_request_words(receipt, head, service, from, all_words(to))
}
fn composition(service: &EncodedTeletextAuthoredService) -> &TeletextMainComposition {
    service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.normalized().original())
            }
            _ => None,
        })
        .unwrap()
}
fn selected_operation(
    service: &EncodedTeletextAuthoredService,
    column: u8,
) -> TeletextColumnOperation {
    let composed = composition(service);
    let cell = composed.display_resolution().cell(1, column).unwrap();
    let candidate =
        composed.display_resolution().origins()[cell.origin().unwrap()].candidate_index();
    composed.glyph_layers().candidates()[candidate]
        .operation()
        .unwrap()
}
fn service_words(service: &EncodedTeletextAuthoredService) -> Vec<u32> {
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
    let mut result = Vec::new();
    for record in project_encoded_teletext_display(page, service.unit_count())
        .unwrap()
        .records
    {
        if let TeletextEncodedDisplayRecord::Triplets {
            row: 26,
            designation,
            words,
        } = record
        {
            assert_eq!(usize::from(designation), result.len() / 13);
            result.extend_from_slice(&words);
        }
    }
    result
}
fn assert_selected(service: &EncodedTeletextAuthoredService, stage: u8) {
    for index in if stage == 1 {
        vec![0, 1, 2, 3, 4, 5, 6]
    } else {
        vec![0, 1, 2, 4, 5, 6]
    } {
        let at = if stage == 2 && index == 2 {
            3
        } else {
            u8::try_from(index).unwrap()
        };
        assert_eq!(selected_operation(service, 48 + at), family(index, true));
    }
    let composed = composition(service);
    for (column, scope, page, mask) in [
        (
            53,
            TeletextDrcsTableScope::Normal,
            0xc0,
            [
                0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900,
            ],
        ),
        (54, TeletextDrcsTableScope::Global, 0xc1, [0x222; 10]),
    ] {
        let cell = composed.display_resolution().cell(1, column).unwrap();
        let candidate =
            composed.display_resolution().origins()[cell.origin().unwrap()].candidate_index();
        let executed = composed
            .drcs_execution()
            .records()
            .iter()
            .find(|record| record.candidate_index() == candidate)
            .unwrap();
        assert_eq!(executed.reference().table, scope);
        assert_eq!(
            (
                executed.reference().subtable.get(),
                executed.reference().slot.get()
            ),
            (0, 1)
        );
        assert_eq!(
            executed.resource(),
            TeletextResourceIdentity::new(
                if scope == TeletextDrcsTableScope::Normal {
                    TeletextResourceKind::Drcs
                } else {
                    TeletextResourceKind::Gdrcs
                },
                TeletextRawDataPageNumber::new(1, page).unwrap(),
                TeletextResourceSubpageIndex::new(0).unwrap(),
            )
            .unwrap()
        );
        assert_eq!(executed.bitmap().get(), mask);
    }
}
fn assert_all_rendered(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    stage: u8,
) {
    assert_eq!(non_x26_records(after), non_x26_records(before));
    assert_eq!(after.magazine_units(), before.magazine_units());
    assert_eq!(after.inventory(), before.inventory());
    assert_eq!(after.pages().len(), before.pages().len());
    let [target, following] = definitions(after);
    assert_eq!(target, definitions(before)[0]);
    assert_eq!(following, target + 1 + all_words(stage).len());
    let words = service_words(after);
    assert_eq!(words[target], marker(target));
    assert_eq!(words[following], marker(following));
    assert_eq!(&words[target + 1..following], all_words(stage));
    assert_eq!(
        words[..target]
            .iter()
            .filter(|word| **word == invocation(following))
            .count(),
        1
    );
    assert_eq!(
        after
            .declared()
            .semantic()
            .objects()
            .definition(identity(65521)),
        before
            .declared()
            .semantic()
            .objects()
            .definition(identity(65521))
    );
    for (old, new) in before.pages().iter().zip(after.pages()) {
        assert_eq!(old.resource_header(), new.resource_header());
        if !matches!(old, EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address()==channel_documents::address(124,32))
        {
            assert_eq!(old.data_units(), new.data_units());
        }
    }
    let old = external_editor_probe::preview_page(before, 124);
    let new = external_editor_probe::preview_page(after, 124);
    assert_eq!((new.width(), new.height()), (672, 250));
    let mut exclusive = [false; 2];
    for (position, (old_pixel, new_pixel)) in old.pixels().iter().zip(new.pixels()).enumerate() {
        if old_pixel == new_pixel {
            continue;
        }
        let (row, column) = (position / new.width(), position % new.width());
        assert!(
            (10..20).contains(&row) && (44 * 12..56 * 12).contains(&column),
            "all Main/protected definition/panel surroundings must stay exact"
        );
        if (44 * 12..48 * 12).contains(&column) {
            exclusive[0] = true;
        }
        if (52 * 12..56 * 12).contains(&column) {
            exclusive[1] = true;
        }
    }
    assert_eq!(
        exclusive,
        [true, true],
        "both invocation-exclusive regions must change"
    );
    assert_selected(after, stage);
}

async fn assert_all_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    stage: u8,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 12);
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
    let expected = TeletextLocalObjectGlyphProgramOverlay::new_with_all_glyphs(
        identity(65520),
        all_program(0),
        all_program(stage),
        all_mapping(stage),
    )
    .unwrap();
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        std::slice::from_ref(&expected)
    );
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
    let mut original: Value = serde_json::from_slice(&stored.document).unwrap();
    original
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay");
    assert_eq!(
        original,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let current = drcs_bitmap::restored(fixture, concept).await;
    let source = flow.saved[0].candidate();
    assert_eq!(current.selection(), source.selection());
    assert_eq!(current.metadata(), source.metadata());
    assert_eq!(current.transmissions(), source.transmissions());
    assert_eq!(current.sources().len(), source.sources().len());
    for (a, b) in current.sources().iter().zip(source.sources()) {
        assert_eq!(a.bytes(), b.bytes());
        assert_eq!(a.authoring(), b.authoring());
    }
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_local_object_glyph_program_overlays(&[expected], policy().encoding.validation.service)
        .unwrap();
    assert_eq!(current.encoded().unwrap().declared(), &expected);
    assert_all_rendered(
        flow.accepted[0].service(),
        current.encoded().unwrap(),
        stage,
    );
}

async fn all_positive(factory: fn(&str, u16, u16) -> Value) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        local_object_glyphs::structural_encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(definitions(flow.accepted[0].service()), [11, 27]);
    let concept = import(
        &fixture,
        all_request(&flow.receipts[0], 1, flow.accepted[0].service(), 0, 1),
    )
    .await;
    assert_eq!(concept.head().revision, 2);
    Box::pin(assert_all_concept(&fixture, &flow, &concept, 1)).await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_all_concept(&fixture, &flow, &concept, 1)).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    let current = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(current.encoded().unwrap())
    );
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(
        &fixture,
        all_request(&latest, 2, current.encoded().unwrap(), 1, 2),
    )
    .await;
    assert_eq!(repeated.head().revision, 3);
    Box::pin(assert_all_concept(&fixture, &flow, &repeated, 2)).await;
    let exported = drcs_bitmap::reexport(&fixture, &repeated).await;
    let latest = receipt(&fixture, &exported).await;
    let current = drcs_bitmap::restored(&fixture, &repeated).await;
    let undone = import(
        &fixture,
        all_request(&latest, 3, current.encoded().unwrap(), 2, 0),
    )
    .await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    assert_eq!(
        designs::units(
            drcs_bitmap::restored(&fixture, &undone)
                .await
                .encoded()
                .unwrap()
        ),
        designs::units(flow.accepted[0].service())
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn all_glyph_structure_native_free_zone_grow_shrink_publish_reopen_repeat_undo() {
    for factory in [all_free as fn(&str, u16, u16) -> Value, all_zone] {
        Box::pin(all_positive(factory)).await;
    }
}

#[tokio::test]
async fn all_glyph_structure_native_protects_second_shared_consumer_atomically() {
    for factory in [all_narrow as fn(&str, u16, u16) -> Value, all_generated] {
        let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
            factory,
            Fixture::new_persistent().await,
            local_object_glyphs::structural_encoding,
            authoring_metadata::write_compact,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let NativeTeletextEditorialImportError::Effects(error) = reject(
            &fixture,
            &flow,
            all_request(&flow.receipts[0], 1, flow.accepted[0].service(), 0, 1),
        )
        .await
        else {
            panic!("the protected second invocation requires an effects refusal");
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert!(matches!(error.position, Some((1, 52..=55))));
        retained.assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}

#[tokio::test]
async fn all_glyph_structure_native_keeps_strict_v11_for_basic_structure_with_wide_anchors() {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        all_free,
        Fixture::new_persistent().await,
        local_object_glyphs::structural_encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let concept = import(
        &fixture,
        all_request(&flow.receipts[0], 1, flow.accepted[0].service(), 0, 3),
    )
    .await;
    let stored = drcs_bitmap::stored(&fixture, &concept).await;
    let restored = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = restored.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 11);
    let expected = TeletextLocalObjectGlyphProgramOverlay::new(
        identity(65520),
        all_program(0),
        all_program(3),
        all_mapping(3),
    )
    .unwrap();
    assert_eq!(overlay.local_object_glyph_program_overlays(), &[expected]);
    assert_original_pointers(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

async fn assert_missing_glyph(fixture: &Fixture, flow: &Flow, scope: TeletextDrcsTableScope) {
    use ersatzrs_web::host::{
        teletext_channel_documents::ChannelTeletextDocumentEncodingError,
        teletext_designs::NativeTeletextDesignError,
    };
    let (at, command, data, page) = if scope == TeletextDrcsTableScope::Normal {
        (5, 14, 111, 0xc0)
    } else {
        (6, 16, 47, 0xc1)
    };
    let mut body = all_words(1);
    let index = body
        .iter()
        .position(|word| *word == super::word(at, 13, if at == 5 { 65 } else { 1 }))
        .unwrap();
    body[index] = word(at, 13, data);
    let NativeTeletextEditorialImportError::Design(error) = reject(
        fixture,
        flow,
        all_request_words(&flow.receipts[0], 1, flow.accepted[0].service(), 0, body),
    )
    .await
    else {
        panic!("valid checked references to undefined slot47 must reach native reconstruction");
    };
    let NativeTeletextDesignError::Encoding(error) = *error else {
        panic!("native encoding refusal required")
    };
    let ChannelTeletextDocumentEncodingError::Composition(error) = *error else {
        panic!("service reconstruction refusal required")
    };
    let TeletextProducerCompositionFailure::Semantic(reason) = error.reason else {
        panic!("exact service diagnostic required")
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
        panic!("exact original bank and missing glyph required")
    };
    assert_eq!(display, channel_documents::address(124, 32));
    assert_eq!(
        resource,
        TeletextResourceIdentity::new(
            if scope == TeletextDrcsTableScope::Normal {
                TeletextResourceKind::Drcs
            } else {
                TeletextResourceKind::Gdrcs
            },
            TeletextRawDataPageNumber::new(1, page).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap()
    );
    assert_eq!(slot.get(), 47);
    assert_eq!(
        source,
        TeletextEnhancementSource {
            object: Some(identity(65520)),
            command
        }
    );
}

#[tokio::test]
async fn all_glyph_structure_native_refuses_undefined_bankslots_and_raw_selector_atomically() {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        all_free,
        Fixture::new_persistent().await,
        local_object_glyphs::structural_encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for scope in [
        TeletextDrcsTableScope::Normal,
        TeletextDrcsTableScope::Global,
    ] {
        Box::pin(assert_missing_glyph(&fixture, &flow, scope)).await;
    }
    let mut body = all_words(1);
    let index = body
        .iter()
        .position(|word| *word == super::word(40, 24, 80))
        .unwrap();
    body[index] = word(40, 24, 64); // Raw seven-bit data with the Level2.5 selection flag absent.
    assert!(matches!(
        reject(
            &fixture,
            &flow,
            all_request_words(&flow.receipts[0], 1, flow.accepted[0].service(), 0, body)
        )
        .await,
        NativeTeletextEditorialImportError::Tti(_)
    ));
    retained.assert_unchanged(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

//! Same-position style values and authenticated calls form one atomic resource edit.
use super::*;
use ersatzrs_ffmpeg::teletext::TeletextPhysicalPageInventoryRecord;

#[derive(Clone, Copy)]
enum Style {
    Original,
    Added,
    Removed,
    EqualPixels,
}
fn response(theme: &str, page: u16, subcode: u16, style: Style) -> Value {
    let mut value = mixed::original(theme, page, subcode);
    let active = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 20)
        .unwrap();
    let before = active["program"]["commands"].as_array().unwrap().clone();
    assert_eq!(before.len(), 5);
    assert_eq!(before[3]["column"], 10);
    let (foreground, background, boxed) = match style {
        Style::Original => (7, 0, false),
        Style::Added => (1, 4, true),
        Style::Removed => (2, 1, true),
        Style::EqualPixels => (7, 0, true),
    };
    let mut attributes = before[3].clone();
    attributes["column"] = json!(8);
    assert_eq!(attributes["operation"]["kind"], "attributes");
    assert_eq!(attributes["operation"]["boxed"], false);
    attributes["operation"]["boxed"] = json!(boxed);
    let mut commands = vec![
        before[0].clone(),
        json!({"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":foreground}}),
        json!({"kind":"at_column","column":8,"operation":{"kind":"background","colour":background}}),
        attributes,
        json!({"kind":"at_column","column":8,"operation":{"kind":"g0","code":80}}),
        json!({"kind":"at_column","column":10,"operation":{"kind":"foreground","colour":7}}),
        json!({"kind":"at_column","column":10,"operation":{"kind":"background","colour":0}}),
        before[3].clone(),
    ];
    // The original complete FG/BG/attribute resets bound all style effects to
    // Main8..9, already captured as a Zone before the receipt is created.
    if !matches!(style, Style::Removed) {
        commands.push(before[4].clone());
    }
    if matches!(style, Style::Added | Style::EqualPixels) {
        let mut added = before[4].clone();
        if matches!(style, Style::EqualPixels) {
            added["offset"] = Value::Null;
        } else {
            added["offset"]["column"] = json!(1);
        }
        commands.push(added);
    }
    active["program"]["commands"] = json!(commands);
    value
}
fn original(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Style::Original)
}
fn added(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Style::Added)
}
fn removed(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Style::Removed)
}
fn equal_pixels(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Style::EqualPixels)
}
fn occupied(style: Style) -> [bool; 2] {
    match style {
        Style::Original | Style::EqualPixels => [true, false],
        Style::Added => [true, true],
        Style::Removed => [false, false],
    }
}
fn style_pixels(service: &EncodedTeletextAuthoredService, style: Style) -> Vec<TeletextRaster> {
    let rasters = pixels(service, occupied(style));
    let p = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    let (foreground, background) = match style {
        Style::Original | Style::EqualPixels => ([15, 15, 15], [0, 0, 0]),
        Style::Added => ([15, 0, 0], [0, 0, 15]),
        Style::Removed => ([0, 15, 0], [15, 0, 0]),
    };
    for raster in &rasters {
        // Literal normal-size Latin P at Main8/visual12. The inherited Adaptive
        // writer at Main9 cannot obscure this independent foreground/BG oracle.
        for (y, bits) in p.into_iter().enumerate() {
            for x in 0..12 {
                let expected = if bits & (1 << (11 - x)) != 0 {
                    foreground
                } else {
                    background
                };
                assert!(matches!(raster.pixel(144 + x,10 + y),
                    Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected),
                    "literal P colour and background at Main8; no style leak beyond original reset10");
            }
        }
    }
    rasters
}
struct Expected {
    target: Target,
    style: Style,
    retained: Vec<Option<usize>>,
    units: Vec<Vec<[u8; 46]>>,
    inventory: Vec<TeletextPhysicalPageInventoryRecord>,
}
fn expected(flow: &Flow, style: Style) -> Expected {
    let service = flow.accepted[0].service();
    let mut retained = vec![
        Some(0),
        None,
        None,
        None,
        Some(4),
        Some(5),
        Some(6),
        Some(7),
    ];
    match style {
        Style::Original => retained = (0..9).map(Some).collect(),
        Style::Added => retained.extend([Some(8), None]),
        Style::Removed => {}
        Style::EqualPixels => {
            retained[1] = Some(1);
            retained[2] = Some(2);
            retained.extend([Some(8), None]);
        }
    }
    Expected {
        target: Target {
            records: records(&flow.exported[0]),
            program: program(service).clone(),
            pixels: style_pixels(service, style),
            occupied: occupied(style),
        },
        style,
        retained,
        inventory: service.inventory().to_vec(),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    }
}
async fn qualify(style: Style) -> Expected {
    let factory: fn(&str, u16, u16) -> Value = match style {
        Style::Original => original,
        Style::Added => added,
        Style::Removed => removed,
        Style::EqualPixels => equal_pixels,
    };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let result = expected(&flow, style);
    let calls = &result.target.program.commands()[8..];
    match style {
        Style::Original => assert_eq!(calls, [call(0)]),
        Style::Added => assert_eq!(calls, [call(0), call(1)]),
        Style::Removed => assert!(calls.is_empty()),
        Style::EqualPixels => {
            let mut duplicate = call(0);
            let TeletextEnhancementCommand::InvokeObject { offset, .. } = &mut duplicate else {
                unreachable!()
            };
            *offset = None;
            assert_eq!(calls, [call(0), duplicate]);
        }
    }
    fixture.state.pool.close().await;
    result
}
fn assert_units(service: &EncodedTeletextAuthoredService, expected: &Expected) {
    assert_eq!(service.pages().len(), expected.units.len());
    for (page, units) in service.pages().iter().zip(&expected.units) {
        assert_eq!(
            page.data_units(),
            units,
            "every independently compiled display and resource unit"
        );
    }
}
async fn assert_style_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Expected,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let root = flow.accepted[0].service();
    let changes = if matches!(target.style, Style::Original) {
        assert_eq!(stored.document, flow.originals[0].document);
        assert!(documents.editorial_overlay().is_none());
        vec![]
    } else {
        let overlay = documents.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 17);
        assert!(overlay.pages().is_empty());
        assert!(overlay.local_program_operands().is_empty());
        assert!(overlay.local_object_operands().is_empty());
        assert!(overlay.local_object_glyph_program_overlays().is_empty());
        assert!(overlay.resource_object_operands().is_empty());
        assert!(overlay.drcs_bitmap_overlays().is_empty());
        let [change] = overlay.resource_object_program_overlays() else {
            panic!("one compound style/call overlay")
        };
        assert_eq!(change.identity(), seed::target(20));
        assert_eq!(change.expected(), program(root));
        assert_eq!(change.replacement(), &target.target.program);
        assert_eq!(
            change.retained_commands(),
            target.retained,
            "new style values have no original writer ancestry; exact original call survives"
        );
        assert!(change.has_invocation_structure_changes());
        vec![change.clone()]
    };
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let expected = if changes.is_empty() {
        root.declared().clone()
    } else {
        root.declared()
            .clone()
            .with_resource_object_program_overlays(&changes, policy().encoding.validation.service)
            .unwrap()
    };
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(service.magazine_units(), root.magazine_units());
    assert_eq!(service.inventory(), target.inventory);
    assert_eq!(style_pixels(service, target.style), target.target.pixels);
    assert_units(service, target);
}
async fn delete_undo_repeat(
    mut fixture: Fixture,
    flow: &Flow,
    inserted: &SavedEditorialTeletextDesign,
    removed: &Expected,
    root: &Expected,
    added: &Expected,
) -> Fixture {
    let exported = drcs_bitmap::reexport(&fixture, inserted).await;
    let latest = receipt(&fixture, &exported).await;
    let deleted = import(&fixture, upload(&exported, &latest, 2, &removed.target)).await;
    assert_eq!(deleted.head().revision, 3);
    Box::pin(assert_style_concept(&fixture, flow, &deleted, removed)).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_style_concept(&fixture, flow, &deleted, removed)).await;
    let exported = drcs_bitmap::reexport(&fixture, &deleted).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(&fixture, upload(&exported, &latest, 3, &root.target)).await;
    assert_eq!(undone.head().revision, 4);
    Box::pin(assert_style_concept(&fixture, flow, &undone, root)).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_style_concept(&fixture, flow, &undone, root)).await;
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(&fixture, upload(&exported, &latest, 4, &added.target)).await;
    assert_eq!(repeated.head().revision, 5);
    fixture = fixture.reopen().await;
    Box::pin(assert_style_concept(&fixture, flow, &repeated, added)).await;
    fixture
}
#[tokio::test]
async fn v8_same_position_style_values_and_calls_publish_reopen_delete_root_undo_and_repeat() {
    let added = Box::pin(qualify(Style::Added)).await;
    let removed = Box::pin(qualify(Style::Removed)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let root = expected(&flow, Style::Original);
    assert_ne!(root.target.pixels, added.target.pixels);
    assert_ne!(root.target.pixels, removed.target.pixels);
    let inserted = import(
        &fixture,
        upload(&flow.exported[0], &flow.receipts[0], 1, &added.target),
    )
    .await;
    assert_eq!(inserted.head().revision, 2);
    Box::pin(assert_style_concept(&fixture, &flow, &inserted, &added)).await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_style_concept(&fixture, &flow, &inserted, &added)).await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &inserted).await;
    assert_eq!(
        style_pixels(day.service(), Style::Added),
        added.target.pixels
    );
    assert_units(day.service(), &added);
    fixture = fixture.reopen().await;
    Box::pin(assert_style_concept(&fixture, &flow, &inserted, &added)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    let recovered = recover(&fixture, 1).await;
    generations::assert_same_output(&recovered, &day);
    assert_units(recovered.service(), &added);
    assert_eq!(
        style_pixels(recovered.service(), Style::Added),
        added.target.pixels
    );
    fixture = Box::pin(delete_undo_repeat(
        fixture, &flow, &inserted, &removed, &root, &added,
    ))
    .await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original(theme, page, subcode);
    let consumer = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|page| page["page"]["page"] == 125)
        .unwrap();
    let region = consumer["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "active-glyph-zone")
        .unwrap();
    assert_eq!(
        (region["column"].as_u64(), region["width"].as_u64()),
        (Some(8), Some(2))
    );
    region["kind"] = json!("generated");
    value
}
#[tokio::test]
async fn v8_same_position_style_values_and_calls_refuse_generated_consumer_atomically() {
    let added = Box::pin(qualify(Style::Added)).await;
    let removed = Box::pin(qualify(Style::Removed)).await;
    let unchanged_pixels = Box::pin(qualify(Style::EqualPixels)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(style_pixels(flow.accepted[0].service(), Style::Original), unchanged_pixels.target.pixels,
        "boxed attribute change plus exact duplicate passive call leaves every native pixel unchanged");
    for target in [&added, &removed, &unchanged_pixels] {
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            upload(&flow.exported[0], &flow.receipts[0], 1, &target.target),
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Effects(error) = error else {
            panic!("same-position styles must reach all-consumer permissions: {error:?}")
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(125, 32)));
        let (row, column) = error.position.unwrap();
        assert_eq!(row, 1);
        assert!(
            (12..=13).contains(&column),
            "new style writer must not inherit protected shared-consumer ancestry"
        );
        assert_unchanged(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
    }
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

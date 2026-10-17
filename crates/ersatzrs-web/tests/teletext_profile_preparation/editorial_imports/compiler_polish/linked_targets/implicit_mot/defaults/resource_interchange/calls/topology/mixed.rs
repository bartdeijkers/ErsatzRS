//! Existing same-column glyph edits compose atomically with resource call shape.
use super::*;

// Pinned font_data table0, Latin0x42, ten little-endian mask rows. This literal
// oracle is independent of the edited service/compiler output used for replay.
const B: [u16; 10] = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x306, 0x306, 0x3fc, 0, 0];

fn capture_glyph(mut value: Value) -> Value {
    let adaptive = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 22)
        .unwrap();
    let commands = adaptive["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 5);
    assert_eq!(
        commands[1],
        json!({"kind":"at_column","column":7,"operation":{"kind":"attributes",
            "double_width":true,"double_height":false,"boxed":false,"conceal":false,
            "invert":false,"underline_or_separated_mosaics":false}})
    );
    // This inherited Adaptive writer widens the Base origin7 over Active8,
    // even for a blank Base. Keep the original Base and every other command;
    // this mixed-edit fixture needs the Active glyph's left quarters visible.
    commands.remove(1);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = page["regions"].as_array_mut().unwrap();
        let inherited = regions
            .iter_mut()
            .find(|region| region["id"] == "free-national-base")
            .unwrap();
        assert_eq!(inherited["kind"], "free");
        assert_eq!(inherited["area"], "main");
        assert_eq!(inherited["row"], 1);
        assert_eq!(inherited["column"], 7);
        assert_eq!(inherited["width"], 2);
        assert_eq!(inherited["height"], 1);
        // Preserve inherited Free column7 while assigning column8 to the
        // glyph's Zone. Row2 is already Free; a height2 Zone would overlap it.
        inherited["width"] = json!(1);
        regions.push(json!({
            "id":"active-glyph-zone","kind":"zone","area":"main",
            "row":1,"column":8,"width":2,"height":1,
        }));
    }
    value
}
pub(super) fn original(theme: &str, page: u16, subcode: u16) -> Value {
    capture_glyph(super::response(theme, page, subcode))
}
fn mixed(theme: &str, page: u16, subcode: u16, shape: Shape) -> Value {
    let mut value = capture_glyph(shaped(theme, page, subcode, shape));
    let active = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["identity"]["object_id"] == 20)
        .unwrap();
    assert_eq!(
        active["program"]["commands"][2],
        json!({"kind":"at_column","column":8,
        "operation":{"kind":"g0","code":65}})
    );
    active["program"]["commands"][2]["operation"]["code"] = json!(66);
    value
}
fn added_b(theme: &str, page: u16, subcode: u16) -> Value {
    mixed(theme, page, subcode, Shape::Added)
}
fn removed_b(theme: &str, page: u16, subcode: u16) -> Value {
    mixed(theme, page, subcode, Shape::Removed)
}
pub(super) fn assert_b(rasters: &[TeletextRaster]) {
    assert_eq!(rasters.len(), 3);
    for (page, raster) in (124..127).zip(rasters) {
        // Main column8 has visual column12 with the original left4 panel. Its
        // top/bottom left quarters retain double-width AND double-height.
        for y in 0..20 {
            for x in 0..12 {
                let expected = if B[y / 2] & (1 << (11 - x / 2)) != 0 {
                    [15, 15, 15]
                } else {
                    [0, 0, 0]
                };
                let actual = raster.pixel(144 + x, 10 + y);
                assert!(
                    matches!(actual,
                    Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected),
                    "literal enlarged Latin B: page={page} pixel=({}, {}) cell=({}, 12) source=({}, {}) mask={:#05x} expected={expected:?} actual={actual:?}",
                    144 + x, 10 + y, 1 + y / 10, x / 2, y / 2, B[y / 2]
                );
            }
        }
    }
}
fn assert_b_changed(root: &Target, targets: [&Target; 2]) {
    for target in targets {
        for (before, after) in root.pixels.iter().zip(&target.pixels) {
            for start in [10, 20] {
                assert!(
                    (start..start + 10)
                        .any(|y| (144..156).any(|x| before.pixel(x, y) != after.pixel(x, y))),
                    "each visible enlarged half changes A to B before import"
                );
            }
        }
    }
}
async fn qualify_mixed(added: bool) -> (Target, Vec<u8>, DisplayUnitExpectations) {
    let factory: fn(&str, u16, u16) -> Value = if added { added_b } else { removed_b };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let program = program(service).clone();
    assert_eq!(
        program.commands()[2],
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: TeletextColumnOperation::G0 {
                code: ersatzrs_domain::TeletextGlyphCode::new(66).unwrap()
            },
        }
    );
    assert_eq!(
        &program.commands()[4..],
        if added {
            vec![call(0), call(1)]
        } else {
            vec![]
        }
    );
    let occupied = if added { [true, true] } else { [false, false] };
    let result = Target {
        records: records(&flow.exported[0]),
        program,
        pixels: pixels(service, occupied),
        occupied,
    };
    assert_b(&result.pixels);
    let display = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap()
        .bytes
        .clone();
    let display_units = service
        .pages()
        .iter()
        .filter_map(|page| {
            let address = match page {
                EncodedTeletextAuthoredPage::NativeDisplay(page) => page.encoded().page().address(),
                EncodedTeletextAuthoredPage::CompiledDisplay(page) => page.page().address(),
                _ => return None,
            };
            Some((address, page.data_units().to_vec()))
        })
        .collect();
    fixture.state.pool.close().await;
    (result, display, display_units)
}
async fn refuse_uploaded_compiler_word(
    fixture: &mut Fixture,
    flow: &Flow,
    target: &Target,
    compiled_display: &[u8],
) {
    let mut request = upload(&flow.exported[0], &flow.receipts[0], 1, target);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let original = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let candidate = parse_teletext_tti(compiled_display, policy().tti).unwrap();
    let protected = |records: &[TeletextTtiRecord]| {
        records
            .iter()
            .filter(|record| !matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        protected(original.records()),
        protected(candidate.records())
    );
    let mut records = original.records().to_vec();
    let words = |record: &TeletextTtiRecord| match record {
        TeletextTtiRecord::Packet {
            row: 26,
            designation: 1,
            words,
        } => Some(*words),
        _ => None,
    };
    let derived = candidate.records().iter().find_map(words).unwrap()[3];
    let record = records
        .iter_mut()
        .find(|record| words(record).is_some())
        .unwrap();
    let TeletextTtiRecord::Packet { words, .. } = record else {
        unreachable!();
    };
    assert_ne!(
        words[3], derived,
        "independently compiled display word differs"
    );
    words[3] = derived;
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Tti(error) = error else {
        panic!("resource changes must not authorize compiler-owned display uploads: {error:?}");
    };
    assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Packet);
    assert_eq!(error.resource, None);
    assert_eq!((error.page, error.subcode), (124, 32));
    assert_eq!(
        (error.row, error.designation, error.column),
        (Some(26), Some(1), Some(3))
    );
    assert_unchanged(fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
}
async fn import_stage(
    fixture: &Fixture,
    request: NativeTeletextEditorialImportRequest,
    stage: &str,
) -> SavedEditorialTeletextDesign {
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_or_else(|error| panic!("mixed lifecycle {stage}: {error:?}"));
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("mixed lifecycle {stage}: authorized edit must save a concept");
    };
    saved
}
async fn assert_mixed(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Target,
    units: &DisplayUnitExpectations,
) {
    Box::pin(assert_concept_with_displays(
        fixture,
        flow,
        concept,
        target,
        Some(units),
    ))
    .await;
}
async fn undo_and_repeat(
    mut fixture: Fixture,
    flow: &Flow,
    deleted: &SavedEditorialTeletextDesign,
    root: &Target,
    added: &Target,
    display_units: &DisplayUnitExpectations,
) -> Fixture {
    let exported = drcs_bitmap::reexport(&fixture, deleted).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import_stage(
        &fixture,
        upload(&exported, &latest, 3, root),
        "root undo to concept4",
    )
    .await;
    assert_eq!(undone.head().revision, 4);
    Box::pin(assert_concept(&fixture, flow, &undone, root)).await;
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import_stage(
        &fixture,
        upload(&exported, &latest, 4, added),
        "repeat insertion to concept5",
    )
    .await;
    assert_eq!(repeated.head().revision, 5);
    fixture = fixture.reopen().await;
    Box::pin(assert_mixed(
        &fixture,
        flow,
        &repeated,
        added,
        display_units,
    ))
    .await;
    assert_b(&added.pixels);
    fixture
}

#[tokio::test]
async fn v8_same_column_g0_and_call_topology_publish_reopen_repeat_and_undo_atomically() {
    let (added, compiled_display, added_units) = Box::pin(qualify_mixed(true)).await;
    let (removed, _, removed_units) = Box::pin(qualify_mixed(false)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let root = Target {
        records: records(&flow.exported[0]),
        program: program(flow.accepted[0].service()).clone(),
        pixels: pixels(flow.accepted[0].service(), [true, false]),
        occupied: [true, false],
    };
    Box::pin(refuse_uploaded_compiler_word(
        &mut fixture,
        &flow,
        &added,
        &compiled_display,
    ))
    .await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_b_changed(&root, [&added, &removed]);
    let inserted = import_stage(
        &fixture,
        upload(&flow.exported[0], &flow.receipts[0], 1, &added),
        "initial insertion to concept2",
    )
    .await;
    assert_eq!(inserted.head().revision, 2);
    Box::pin(assert_mixed(
        &fixture,
        &flow,
        &inserted,
        &added,
        &added_units,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &inserted).await;
    assert_eq!(pixels(day.service(), [true, true]), added.pixels);
    assert_b(&added.pixels);
    fixture = fixture.reopen().await;
    Box::pin(assert_mixed(
        &fixture,
        &flow,
        &inserted,
        &added,
        &added_units,
    ))
    .await;
    let exported = drcs_bitmap::reexport(&fixture, &inserted).await;
    let latest = receipt(&fixture, &exported).await;
    let deleted = import_stage(
        &fixture,
        upload(&exported, &latest, 2, &removed),
        "deletion to concept3",
    )
    .await;
    assert_eq!(deleted.head().revision, 3);
    Box::pin(assert_mixed(
        &fixture,
        &flow,
        &deleted,
        &removed,
        &removed_units,
    ))
    .await;
    fixture = fixture.reopen().await;
    Box::pin(assert_mixed(
        &fixture,
        &flow,
        &deleted,
        &removed,
        &removed_units,
    ))
    .await;
    fixture = Box::pin(undo_and_repeat(
        fixture,
        &flow,
        &deleted,
        &root,
        &added,
        &added_units,
    ))
    .await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn generated_glyph(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original(theme, page, subcode);
    let protected = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|p| p["page"]["page"] == 125)
        .unwrap();
    let zone = protected["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "active-glyph-zone")
        .unwrap();
    zone["kind"] = json!("generated");
    value
}
#[tokio::test]
async fn v8_mixed_glyph_and_call_edit_cannot_partially_save_a_generated_shared_glyph() {
    let (added, _, _) = Box::pin(qualify_mixed(true)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated_glyph,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let original = pixels(flow.accepted[0].service(), [true, false]);
    assert!(original
        .iter()
        .zip(&added.pixels)
        .all(|(before, after)| (10..20)
            .any(|y| (144..156).any(|x| before.pixel(x, y) != after.pixel(x, y)))));
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        upload(&flow.exported[0], &flow.receipts[0], 1, &added),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("combined glyph/call edits must reach all-consumer effect permissions: {error:?}")
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(125, 32)));
    let (row, column) = error.position.unwrap();
    assert_eq!(row, 1);
    assert!((12..=13).contains(&column));
    assert_unchanged(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

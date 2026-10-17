//! V8 call insertion/deletion is checked against independently compiled services.
use super::*;
use ersatzrs_domain::{TeletextEnhancementProgram, TeletextResourceObjectProgramOverlay};

#[derive(Clone, Copy)]
enum Shape {
    Added,
    Removed,
    EqualPixels,
}

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_response(theme, page, subcode);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let target = objects
        .iter()
        .find(|o| o["identity"]["object_id"] == 21)
        .unwrap()["identity"]
        .clone();
    let adaptive = objects
        .iter_mut()
        .find(|o| o["identity"]["object_id"] == 22)
        .unwrap();
    let commands = adaptive["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 4);
    // This immutable existing-catalogue edge preserves a genuine V8 export even
    // when Active20 has no calls. Its separate default P is Main(5,10).
    commands.push(
        json!({"kind":"invoke_object","target":target,"object_type":"passive",
        "offset":{"row":0,"column":0}}),
    );
    value
}
fn shaped(theme: &str, page: u16, subcode: u16, shape: Shape) -> Value {
    let mut value = response(theme, page, subcode);
    let object = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["identity"]["object_id"] == 20)
        .unwrap();
    let commands = object["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 5);
    match shape {
        Shape::Removed => {
            commands.pop();
        }
        Shape::Added | Shape::EqualPixels => {
            let mut call = commands[4].clone();
            if matches!(shape, Shape::Added) {
                call["offset"]["column"] = json!(1);
            } else {
                // Explicit zero and absent origin have equal output here, but
                // distinct typed calls force the original exact survivor first.
                call["offset"] = Value::Null;
            }
            commands.push(call);
        }
    }
    value
}
fn added(theme: &str, page: u16, subcode: u16) -> Value {
    shaped(theme, page, subcode, Shape::Added)
}
fn removed(theme: &str, page: u16, subcode: u16) -> Value {
    shaped(theme, page, subcode, Shape::Removed)
}
fn equal_pixels(theme: &str, page: u16, subcode: u16) -> Value {
    shaped(theme, page, subcode, Shape::EqualPixels)
}
fn call(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target: seed::target(21),
        object_type: TeletextObjectType::Passive,
        offset: Some(ersatzrs_domain::TeletextObjectOffset::new(0, column).unwrap()),
    }
}
fn program(service: &EncodedTeletextAuthoredService) -> &TeletextEnhancementProgram {
    &service
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(20))
        .unwrap()
        .program
}
fn records(exported: &ExportedOriginalTeletextDesign) -> Vec<TeletextTtiRecord> {
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
    let file = exported
        .files()
        .iter()
        .find(|f| f.name == companion(exported))
        .unwrap();
    parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec()
}
fn pixels(service: &EncodedTeletextAuthoredService, occupied: [bool; 2]) -> Vec<TeletextRaster> {
    let mask = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    (124..127).map(|page| {
        let raster=external_editor_probe::preview_page(service,page);
        assert_eq!((raster.width(),raster.height()),(672,250));
        for (index,present) in occupied.into_iter().enumerate() {
            for (row,bits) in mask.into_iter().enumerate() {
                for x in 0..12 {
                    let expected=if present && bits&(1<<(11-x))!=0 {[0,15,0]} else {[0,0,0]};
                    assert!(matches!(raster.pixel((15+index)*12+x,50+row),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected),
                        "fixed P at Main row5 columns11/12; immutable Adaptive default P stays at column10");
                }
            }
        }
        raster
    }).collect()
}
struct Target {
    records: Vec<TeletextTtiRecord>,
    program: TeletextEnhancementProgram,
    pixels: Vec<TeletextRaster>,
    occupied: [bool; 2],
}
async fn qualify(shape: Shape) -> Target {
    let factory: fn(&str, u16, u16) -> Value = match shape {
        Shape::Added => added,
        Shape::Removed => removed,
        Shape::EqualPixels => equal_pixels,
    };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let program = program(service).clone();
    let expected = match shape {
        Shape::Added => vec![call(0), call(1)],
        Shape::Removed => vec![],
        Shape::EqualPixels => {
            let mut absent = call(0);
            let TeletextEnhancementCommand::InvokeObject { offset, .. } = &mut absent else {
                unreachable!();
            };
            *offset = None;
            vec![call(0), absent]
        }
    };
    assert_eq!(&program.commands()[4..], expected);
    let occupied = match shape {
        Shape::Added => [true, true],
        Shape::Removed => [false, false],
        Shape::EqualPixels => [true, false],
    };
    let target = Target {
        records: records(&flow.exported[0]),
        program,
        pixels: pixels(service, occupied),
        occupied,
    };
    fixture.state.pool.close().await;
    target
}
fn upload(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    target: &Target,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let originals = request.files.clone();
    let sidecar = request.sidecar.clone();
    let name = companion(exported);
    let file = request.files.iter_mut().find(|f| f.name == name).unwrap();
    let before = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let non_x26 = |records: &[TeletextTtiRecord]| {
        records
            .iter()
            .filter(|r| !matches!(r, TeletextTtiRecord::Packet { row: 26, .. }))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        non_x26(before.records()),
        non_x26(&target.records),
        "editor framing and presentation stay exact"
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(target.records.clone(), policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    for (old, new) in originals.iter().zip(&request.files) {
        assert_eq!(old.name, new.name);
        if old.name != name {
            assert_eq!(old.bytes,new.bytes,"native resources, displays, callee companions and sibling context copies stay exact");
        }
    }
    request
}
async fn changes(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Target,
) -> Vec<TeletextResourceObjectProgramOverlay> {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let original = program(flow.accepted[0].service());
    if &target.program == original {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
        return vec![];
    }
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 17);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.local_object_glyph_program_overlays().is_empty());
    assert!(overlay.resource_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    let changes = overlay.resource_object_program_overlays();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].identity(), seed::target(20));
    assert_eq!(changes[0].expected(), original);
    assert_eq!(changes[0].replacement(), &target.program);
    let mut map = vec![Some(0), Some(1), Some(2), Some(3)];
    if target.program.commands().len() == 6 {
        map.extend([Some(4), None]);
    }
    assert_eq!(changes[0].retained_commands(), map);
    changes.to_vec()
}
type DisplayUnitExpectations = Vec<(TeletextPageAddress, Vec<[u8; 46]>)>;

async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Target,
) {
    Box::pin(assert_concept_with_displays(
        fixture, flow, concept, target, None,
    ))
    .await;
}
async fn assert_concept_with_displays(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Target,
    displays: Option<&DisplayUnitExpectations>,
) {
    let changes = changes(fixture, flow, concept, target).await;
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
    let root = flow.accepted[0].service();
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
    assert_eq!(pixels(service, target.occupied), target.pixels);
    assert_eq!(service.magazine_units(), root.magazine_units());
    assert_eq!(service.inventory(), root.inventory());
    let mut checked_displays = 0;
    for (before, after) in root.pages().iter().zip(service.pages()) {
        let address = match before {
            EncodedTeletextAuthoredPage::NativeDisplay(page) => {
                Some(page.encoded().page().address())
            }
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => Some(page.page().address()),
            _ => None,
        };
        if let (Some(expected), Some(address)) = (displays, address) {
            let mut matches = expected.iter().filter(|(owner, _)| *owner == address);
            let (_, units) = matches
                .next()
                .expect("every display has an independent target");
            assert!(matches.next().is_none(), "display target address is unique");
            assert_eq!(
                after.data_units(),
                units.as_slice(),
                "exact independently compiled {address:?}"
            );
            checked_displays += 1;
            continue;
        }
        if before
            .resource_header()
            .is_none_or(|h| Some(h.identity()) != seed::target(20).resource())
        {
            assert_eq!(before.data_units(), after.data_units());
        }
    }
    if let Some(expected) = displays {
        assert_eq!(checked_displays, expected.len(), "no extra display targets");
    }
}

#[tokio::test]
async fn v8_existing_catalogue_call_insertion_deletion_publish_reopen_repeat_and_root_undo() {
    let added = Box::pin(qualify(Shape::Added)).await;
    let removed = Box::pin(qualify(Shape::Removed)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
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
    assert_ne!(root.pixels, added.pixels);
    assert_ne!(root.pixels, removed.pixels);
    let inserted = import(
        &fixture,
        upload(&flow.exported[0], &flow.receipts[0], 1, &added),
    )
    .await;
    assert_eq!(inserted.head().revision, 2);
    Box::pin(assert_concept(&fixture, &flow, &inserted, &added)).await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &inserted).await;
    assert_eq!(pixels(day.service(), [true, true]), added.pixels);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &inserted, &added)).await;
    let exported = drcs_bitmap::reexport(&fixture, &inserted).await;
    let latest = receipt(&fixture, &exported).await;
    let deleted = import(&fixture, upload(&exported, &latest, 2, &removed)).await;
    assert_eq!(deleted.head().revision, 3);
    Box::pin(assert_concept(&fixture, &flow, &deleted, &removed)).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &deleted, &removed)).await;
    let exported = drcs_bitmap::reexport(&fixture, &deleted).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(&fixture, upload(&exported, &latest, 3, &root)).await;
    assert_eq!(undone.head().revision, 4);
    Box::pin(assert_concept(&fixture, &flow, &undone, &root)).await;
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(&fixture, upload(&exported, &latest, 4, &added)).await;
    assert_eq!(repeated.head().revision, 5);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &repeated, &added)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
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
        .find(|r| r["id"] == "nested-call-zone")
        .unwrap();
    zone["kind"] = json!("generated");
    value
}
async fn assert_unchanged(fixture: &Fixture, flow: &Flow) {
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap()
            .unwrap(),
        *flow.saved[0].head()
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(
            flow.saved[0].head().id,
            2,
            designs::budget().max_serialized_bytes
        )
        .await
        .unwrap()
        .is_none());
    assert_original_pointers(fixture, flow).await;
}
#[tokio::test]
async fn v8_inserted_calls_require_shared_consumer_authority_even_with_equal_pixels() {
    let added = Box::pin(qualify(Shape::Added)).await;
    let duplicate = Box::pin(qualify(Shape::EqualPixels)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(
        pixels(flow.accepted[0].service(), [true, false]),
        duplicate.pixels
    );
    for target in [&added, &duplicate] {
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            upload(&flow.exported[0], &flow.receipts[0], 1, target),
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Effects(error) = error else {
            panic!("inserted call must reach all-consumer permissions: {error:?}")
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(125, 32)));
        let (row, column) = error.position.unwrap();
        assert_eq!(row, 5);
        assert!((15..=16).contains(&column));
        assert_unchanged(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
    }
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn tamper_new_call(
    mut request: NativeTeletextEditorialImportRequest,
    name: &str,
    unknown: bool,
) -> NativeTeletextEditorialImportRequest {
    let file = request.files.iter_mut().find(|f| f.name == name).unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let words = records
        .iter_mut()
        .find_map(|r| match r {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    assert_eq!(words[9], 0x28 | (16 << 6) | (1 << 11));
    assert_eq!((words[10] >> 6) & 31, 19);
    if unknown {
        // Legal Local triplet207, absent from the authenticated small catalogue.
        words[10] = 0x29 | (19 << 6) | (124 << 11);
    } else {
        words[9] = 0x28 | (16 << 6) | (72 << 11); // OriginModifier column72 is reserved.
    }
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    request
}
#[tokio::test]
async fn v8_inserted_calls_reject_unknown_context_alias_and_reserved_origin() {
    let added = Box::pin(qualify(Shape::Added)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for unknown in [true, false] {
        let edited = tamper_new_call(
            upload(&flow.exported[0], &flow.receipts[0], 1, &added),
            companion(&flow.exported[0]),
            unknown,
        );
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            edited,
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, NativeTeletextEditorialImportError::Tti(_)),
            "invalid inserted call must fail authenticated TTI reconciliation: {error:?}"
        );
        assert_unchanged(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
    }
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[path = "topology/mixed.rs"]
mod mixed;

#[path = "topology/structure.rs"]
pub(super) mod structure;

#[path = "topology/position_anchors.rs"]
mod position_anchors;

//! Omitted resource positions remain typed anchors through explicit body callers.
use super::*;

type Response = fn(&str, u16, u16) -> Value;
#[derive(Clone, Copy)]
struct Case {
    original: Response,
    added: Response,
    anchor: usize,
}
fn response(theme: &str, page: u16, subcode: u16, adjacent: bool, added: bool) -> Value {
    let mut value = super::response(theme, page, subcode);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let active = objects
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 20)
        .unwrap();
    let identity = active["identity"].clone();
    let call = active["program"]["commands"][4].clone();
    let position =
        json!({"kind":"set_position","position":{"row":0,"column":if adjacent {9} else {8}}});
    let foreground =
        json!({"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":7}});
    let glyph = json!({"kind":"at_column","column":9,"operation":{"kind":"g0","code":65}});
    let mut commands = if adjacent {
        vec![foreground, glyph, position, call.clone()]
    } else {
        vec![position, foreground, glyph, call.clone()]
    };
    if added {
        let mut next_call = call;
        next_call["offset"]["column"] = json!(1);
        commands.extend([
            json!({"kind":"at_column","column":10,"operation":{"kind":"g0","code":66}}),
            next_call,
        ]);
    }
    active["program"]["commands"] = json!(commands);
    // A resource-relative row0 position is not display-header authority. Give
    // this original resource a real body caller and remove its default once.
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        assert_eq!(metadata["resources"]["defaults"][0]["target"], identity);
        metadata["resources"]["defaults"][0] = Value::Null;
        assert!(metadata["local_program"].is_null());
        metadata["local_program"] = json!({"commands":[
            {"kind":"set_position","position":{"row":1,"column":0}},
            {"kind":"invoke_object","target":identity,"object_type":"active","offset":null}
        ]});
    }
    let default = &mut value["service"]["mots"][0]["links"]["global_object"]["defaults"][0];
    assert_eq!(default["target"], identity);
    *default = json!({"kind":"no_object","raw_number":0});
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = page["regions"].as_array_mut().unwrap();
        let inherited = regions
            .iter()
            .position(|region| region["id"] == "free-glyph-10")
            .unwrap();
        assert_eq!(
            regions[inherited],
            json!({"id":"free-glyph-10","kind":"free","area":"main",
            "row":1,"column":10,"width":1,"height":1})
        );
        regions.remove(inherited);
        regions.push(
            json!({"id":"position-glyph-zone","kind":"zone","area":"main",
            "row":1,"column":9,"width":2,"height":1}),
        );
        let nested = regions
            .iter_mut()
            .find(|region| region["id"] == "nested-call-zone")
            .unwrap();
        assert_eq!(
            (
                nested["row"].as_u64(),
                nested["column"].as_u64(),
                nested["width"].as_u64()
            ),
            (Some(5), Some(11), Some(2))
        );
        nested["column"] = json!(10);
        nested["width"] = json!(3);
    }
    value
}
fn leading(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, false, false)
}
fn leading_added(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, false, true)
}
fn adjacent(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, true, false)
}
fn adjacent_added(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, true, true)
}
const CASES: [Case; 2] = [
    Case {
        original: leading,
        added: leading_added,
        anchor: 0,
    },
    Case {
        original: adjacent,
        added: adjacent_added,
        anchor: 2,
    },
];

fn pixels(service: &EncodedTeletextAuthoredService, added: bool) -> Vec<TeletextRaster> {
    let p = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    let b = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x306, 0x306, 0x3fc, 0, 0];
    (124..127).map(|page| {
        let raster = external_editor_probe::preview_page(service,page);
        assert_eq!((raster.width(),raster.height()),(672,250));
        for y in 0..10 {
            for x in 0..12 {
                for column in 14..=16 {
                    let present = column == 14 || (added && column == 16);
                    let expected = if present && p[y] & (1 << (11-x)) != 0 {[0,15,0]} else {[0,0,0]};
                    assert!(matches!(raster.pixel(column*12+x,50+y),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected),
                        "exact nested P at Main10/original and Main12/added, empty Main11");
                }
                if added {
                    let expected = if b[y] & (1 << (11-x)) != 0 {[15,15,15]} else {[0,0,0]};
                    assert!(matches!(raster.pixel(168+x,10+y),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected),
                        "literal body B at Main10 from parent AP(1,0), not header row0");
                }
            }
        }
        raster
    }).collect()
}
struct Expected {
    target: Target,
    units: Vec<Vec<[u8; 46]>>,
}
fn expected(flow: &Flow, added: bool) -> Expected {
    let service = flow.accepted[0].service();
    Expected {
        target: Target {
            records: records(&flow.exported[0]),
            program: program(service).clone(),
            pixels: pixels(service, added),
            occupied: [true, added],
        },
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    }
}
fn assert_units(service: &EncodedTeletextAuthoredService, target: &Expected) {
    assert_eq!(service.pages().len(), target.units.len());
    for (page, expected) in service.pages().iter().zip(&target.units) {
        assert_eq!(
            page.data_units(),
            expected,
            "complete independent native page target"
        );
    }
}
fn assert_omitted_anchor(flow: &Flow, case: Case) {
    let service = flow.accepted[0].service();
    let commands = program(service).commands();
    assert!(
        matches!(commands[case.anchor],TeletextEnhancementCommand::SetPosition {position}
        if position.row()==0 && position.column()==if case.anchor==0 {8} else {9})
    );
    let object = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(page)
                if Some(page.header().identity()) == seed::target(20).resource() =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let positions = object
        .command_positions()
        .iter()
        .filter(|position| position.source().object == Some(seed::target(20)))
        .map(|position| position.source().command)
        .collect::<Vec<_>>();
    assert_eq!(
        positions,
        if case.anchor == 0 {
            vec![1, 2, 3]
        } else {
            vec![0, 1, 3]
        }
    );
    let records = records(&flow.exported[0]);
    let words = records
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    // The actual emitted V8 root has exactly FG8/A9/origin0/invocation.
    // Its typed position is absent from both native and editor word streams.
    assert_eq!((words[2] >> 6) & 31, 21);
    assert_eq!(words[3], 8 | (7 << 11));
    assert_eq!(words[4], 9 | (9 << 6) | (65 << 11));
    assert_eq!(words[5], 0x28 | (16 << 6));
    assert_eq!((words[6] >> 6) & 31, 19);
    assert_eq!((words[7] >> 6) & 31, 22);
}
async fn qualify(case: Case) -> Expected {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        case.added,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let target = expected(&flow, true);
    assert_eq!(target.target.program.commands().len(), 6);
    assert_eq!(target.target.program.commands()[3], call(0));
    assert_eq!(target.target.program.commands()[5], call(1));
    fixture.state.pool.close().await;
    target
}
async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Expected,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let root = flow.accepted[0].service();
    let changes = if &target.target.program == program(root) {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
        vec![]
    } else {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 17);
        assert!(overlay.pages().is_empty());
        assert!(overlay.local_program_operands().is_empty());
        assert!(overlay.local_object_operands().is_empty());
        assert!(overlay.local_object_glyph_program_overlays().is_empty());
        assert!(overlay.resource_object_operands().is_empty());
        assert!(overlay.drcs_bitmap_overlays().is_empty());
        let [change] = overlay.resource_object_program_overlays() else {
            panic!("one complete resource target")
        };
        assert_eq!(change.identity(), seed::target(20));
        assert_eq!(change.expected(), program(root));
        assert_eq!(change.replacement(), &target.target.program);
        assert_eq!(
            change.retained_commands(),
            [Some(0), Some(1), Some(2), Some(3), None, None]
        );
        assert!(change.has_invocation_structure_changes());
        vec![change.clone()]
    };
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let before = flow.saved[0].candidate();
    assert_eq!(restored.selection(), before.selection());
    assert_eq!(restored.metadata(), before.metadata());
    assert_eq!(restored.transmissions(), before.transmissions());
    assert_eq!(restored.sources().len(), before.sources().len());
    for (actual, original) in restored.sources().iter().zip(before.sources()) {
        assert_eq!(actual.bytes(), original.bytes());
        assert_eq!(actual.authoring(), original.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        before.producers()[0].original()
    );
    let declared = if changes.is_empty() {
        root.declared().clone()
    } else {
        root.declared()
            .clone()
            .with_resource_object_program_overlays(&changes, policy().encoding.validation.service)
            .unwrap()
    };
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared(), &declared);
    assert_eq!(service.magazine_units(), root.magazine_units());
    assert_eq!(service.inventory(), root.inventory());
    assert_eq!(
        pixels(service, target.target.program.commands().len() == 6),
        target.target.pixels
    );
    assert_units(service, target);
}
async fn lifecycle(case: Case) {
    let added = Box::pin(qualify(case)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        case.original,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_omitted_anchor(&flow, case);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let root = expected(&flow, false);
    assert_ne!(root.target.pixels, added.target.pixels);
    let inserted = import(
        &fixture,
        upload(&flow.exported[0], &flow.receipts[0], 1, &added.target),
    )
    .await;
    assert_eq!(inserted.head().revision, 2);
    Box::pin(assert_concept(&fixture, &flow, &inserted, &added)).await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &inserted).await;
    assert_eq!(pixels(day.service(), true), added.target.pixels);
    assert_units(day.service(), &added);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &inserted, &added)).await;
    let exported = drcs_bitmap::reexport(&fixture, &inserted).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(&fixture, upload(&exported, &latest, 2, &root.target)).await;
    assert_eq!(undone.head().revision, 3);
    Box::pin(assert_concept(&fixture, &flow, &undone, &root)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &undone, &root)).await;
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(&fixture, upload(&exported, &latest, 3, &added.target)).await;
    assert_eq!(repeated.head().revision, 4);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &repeated, &added)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
#[tokio::test]
async fn v8_hidden_resource_positions_survive_body_callers_publication_reopen_root_undo_and_repeat()
{
    for case in CASES {
        Box::pin(lifecycle(case)).await;
    }
}
fn generated(theme: &str, page: u16, subcode: u16, adjacent: bool) -> Value {
    let mut value = response(theme, page, subcode, adjacent, false);
    let page = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|page| page["page"]["page"] == 125)
        .unwrap();
    let zone = page["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "position-glyph-zone")
        .unwrap();
    zone["kind"] = json!("generated");
    value
}
fn leading_generated(theme: &str, page: u16, subcode: u16) -> Value {
    generated(theme, page, subcode, false)
}
fn adjacent_generated(theme: &str, page: u16, subcode: u16) -> Value {
    generated(theme, page, subcode, true)
}
#[tokio::test]
async fn v8_hidden_resource_positions_cannot_bypass_generated_shared_body_permissions() {
    for (case, factory) in CASES.into_iter().zip([
        leading_generated as Response,
        adjacent_generated as Response,
    ]) {
        let target = Box::pin(qualify(case)).await;
        let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
            factory,
            Fixture::new_persistent().await,
            seed::encoding,
            authoring_metadata::write_compact,
        ))
        .await;
        assert_omitted_anchor(&flow, case);
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            upload(&flow.exported[0], &flow.receipts[0], 1, &target.target),
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Effects(error) = error else {
            panic!("body permissions must reject atomically: {error:?}")
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(125, 32)));
        assert_eq!(error.position, Some((1, 14)));
        assert_unchanged(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        assert!(fixture.playout.try_recv().is_err());
        fixture.state.pool.close().await;
    }
}

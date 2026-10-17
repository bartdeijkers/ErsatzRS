//! Existing resource calls: independently compiled full targets precede import.
use super::*;

#[path = "calls/actual_editor.rs"]
mod actual_editor;
#[path = "calls/context_scalars.rs"]
mod context_scalars;
#[path = "calls/lifecycle.rs"]
mod lifecycle;
#[path = "calls/negatives.rs"]
mod negatives;
#[path = "calls/same_pixels.rs"]
mod same_pixels;

fn response_with_offset(theme: &str, page: u16, subcode: u16, offset: u8) -> Value {
    let mut value = super::response(theme, page, subcode);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let target = objects
        .iter()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap()["identity"]
        .clone();
    let active = objects
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 20)
        .unwrap();
    assert_eq!(active["object_type"], "active");
    let commands = active["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 4);
    commands.push(
        json!({"kind":"invoke_object","target":target,"object_type":"passive",
        "offset":{"row":0,"column":offset}}),
    );
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        page["regions"].as_array_mut().unwrap().push(json!({
            "id":"nested-call-zone","kind":"zone","area":"main",
            "row":5,"column":11,"width":2,"height":1
        }));
    }
    value
}
fn original_response(theme: &str, page: u16, subcode: u16) -> Value {
    response_with_offset(theme, page, subcode, 0)
}
fn translated_response(theme: &str, page: u16, subcode: u16) -> Value {
    response_with_offset(theme, page, subcode, 1)
}

fn companion(exported: &ExportedOriginalTeletextDesign) -> &str {
    let resource = protected_file(exported, TeletextResourceKindWire::Gpop, 0xd0);
    &exported.sidecar().project.assets.iter().find(|asset| matches!(&asset.scope,
        TeletextExportAssetScope::ObjectTtiV1 {resource_name,object_id:20,editor_page:256,editor_ordinal:0}
        | TeletextExportAssetScope::ObjectContextTtiV1 {resource_name,object_id:20,editor_page:256,editor_ordinal:0}
        if resource_name == resource)).unwrap().name
}

fn assert_call(service: &EncodedTeletextAuthoredService, offset: u8) {
    let definition = service
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(20))
        .unwrap();
    assert_eq!(definition.object_type, TeletextObjectType::Active);
    assert_eq!(definition.program.commands().len(), 5);
    assert_eq!(
        definition.program.commands()[4],
        TeletextEnhancementCommand::InvokeObject {
            target: seed::target(21),
            object_type: TeletextObjectType::Passive,
            offset: Some(ersatzrs_domain::TeletextObjectOffset::new(0, offset).unwrap()),
        }
    );
}

fn translated_pixels(service: &EncodedTeletextAuthoredService, offset: u8) -> Vec<TeletextRaster> {
    assert_call(service, offset);
    let mask = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    (124..127).map(|page| {
        let raster = external_editor_probe::preview_page(service,page);
        assert_eq!((raster.width(),raster.height()),(672,250));
        // Parent AP(1,10), explicit origin offset(0,x), then Passive's own(4,1).
        // These are distinct from the unchanged slot3 default P at Main(4,1).
        for column in 15..=16 {
            for (row,bits) in mask.into_iter().enumerate() {
                for x in 0..12 {
                    let rgb = if column == 15 + usize::from(offset) && bits & (1 << (11-x)) != 0 {
                        [0,15,0]
                    } else { [0,0,0] };
                    assert!(matches!(raster.pixel(column*12+x,50+row),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==rgb),
                        "fixed nested Passive P at row5/Main column11+offset");
                }
            }
        }
        raster
    }).collect()
}

async fn qualified_target() -> (Vec<TeletextTtiRecord>, Vec<TeletextRaster>) {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        translated_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let pixels = translated_pixels(flow.accepted[0].service(), 1);
    let name = companion(&flow.exported[0]);
    let file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    fixture.state.pool.close().await;
    (records, pixels)
}

fn request(flow: &Flow, target: &[TeletextTtiRecord]) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let name = companion(&flow.exported[0]);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let words = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: 0,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    // Original compound call at command4: prefix first word7, invocation word8.
    // Only OriginModifier data0->1 changes; target/type/pointer stay exact.
    assert_eq!(words[7], 0x28 | (16 << 6));
    words[7] = 0x28 | (16 << 6) | (1 << 11);
    assert_eq!(
        records, target,
        "one literal offset edit equals an independently compiled full target"
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    for original in flow.exported[0]
        .files()
        .iter()
        .filter(|file| file.name != name)
    {
        assert_eq!(
            request
                .files
                .iter()
                .find(|file| file.name == original.name)
                .unwrap(),
            original,
            "native resources, displays and other editor companions remain byte exact"
        );
    }
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
    );
    request
}

#[tokio::test]
async fn existing_gpop_to_pop_call_offset_imports_after_independent_native_target_validation() {
    let (target, expected_pixels) = Box::pin(qualified_target()).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_ne!(
        translated_pixels(flow.accepted[0].service(), 0),
        expected_pixels
    );
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request(&flow, &target),
        policy(),
        designs::deadline(),
    )
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let TeletextDesignMutation::Saved(concept) = result
        .expect("valid existing resource call offset must import after full native validation")
    else {
        panic!("current-head resource invocation concept must save")
    };
    assert_eq!(concept.head().revision, 2);
    let stored = drcs_bitmap::stored(&fixture, &concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    assert_eq!(decoded.editorial_overlay().unwrap().schema_version(), 16);
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        translated_pixels(restored.encoded().unwrap(), 1),
        expected_pixels
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

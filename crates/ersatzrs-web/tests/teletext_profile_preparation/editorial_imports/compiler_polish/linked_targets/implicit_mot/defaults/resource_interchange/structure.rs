//! Positive native resource structure probes; complete targets independently compile.
use super::*;
#[path = "structure/editor.rs"]
mod editor;
#[path = "structure/legacy.rs"]
mod legacy;
#[path = "structure/lifecycle.rs"]
mod lifecycle;

fn changed_response(theme: &str, page: u16, subcode: u16, insert: bool) -> Value {
    let mut value = response(theme, page, subcode);
    let object = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap();
    let commands = object["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 5);
    let glyph = json!({"kind":"at_column","column":1,"operation":{"kind":"g0","code":80}});
    assert_eq!(commands[4], glyph);
    if insert {
        commands.insert(4, glyph);
    } else {
        commands.remove(4);
    }
    value
}
fn inserted(theme: &str, page: u16, subcode: u16) -> Value {
    changed_response(theme, page, subcode, true)
}
fn deleted(theme: &str, page: u16, subcode: u16) -> Value {
    changed_response(theme, page, subcode, false)
}
fn companion(exported: &ExportedOriginalTeletextDesign) -> &str {
    let resource = protected_file(exported, TeletextResourceKindWire::Pop, 0xc0);
    &exported.sidecar().project.assets.iter().find(|asset| matches!(&asset.scope,
        TeletextExportAssetScope::ObjectTtiV1 {resource_name,object_id:21,editor_page:256,editor_ordinal:0}
        if resource_name == resource)).unwrap().name
}

async fn qualified_target(insert: bool) -> (Vec<TeletextTtiRecord>, Vec<TeletextRaster>) {
    let factory = if insert { inserted } else { deleted };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let definition = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(21))
        .unwrap();
    assert_eq!(definition.identity, seed::target(21));
    assert_eq!(definition.object_type, TeletextObjectType::Passive);
    assert_eq!(
        definition.program.commands().len(),
        if insert { 6 } else { 4 }
    );
    let name = companion(&flow.exported[0]);
    let bytes = &flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    let records = parse_teletext_tti(bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let pixels = (124..127)
        .map(|page| external_editor_probe::preview_page(flow.accepted[0].service(), page))
        .collect();
    fixture.state.pool.close().await;
    (records, pixels)
}

fn request(
    flow: &Flow,
    insert: bool,
    qualified: &[TeletextTtiRecord],
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let name = companion(&flow.exported[0]);
    let untouched: Vec<_> = request
        .files
        .iter()
        .filter(|file| file.name != name)
        .map(|file| (file.name.clone(), file.bytes.clone()))
        .collect();
    let sidecar = request.sidecar.clone();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let packet = records
        .iter_mut()
        .find(|record| {
            matches!(
                record,
                TeletextTtiRecord::Packet {
                    row: 26,
                    designation: 0,
                    ..
                }
            )
        })
        .unwrap();
    let TeletextTtiRecord::Packet { words, .. } = packet else {
        unreachable!()
    };
    let glyph = 1 | (9 << 6) | (80 << 11);
    let terminator = 63 | (31 << 6) | (5 << 11);
    assert_eq!(
        (words[7], words[8], words[12]),
        (glyph, terminator, terminator)
    );
    let mut edited = words.to_vec();
    if insert {
        edited.insert(7, glyph);
        assert_eq!(edited.pop(), Some(terminator));
    } else {
        assert_eq!(edited.remove(7), glyph);
        edited.push(terminator);
    }
    *words = edited.try_into().unwrap();
    assert_eq!(
        records, qualified,
        "fixed edit exactly matches the independently compiled full native target"
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    for (name, bytes) in untouched {
        assert_eq!(
            request
                .files
                .iter()
                .find(|file| file.name == name)
                .unwrap()
                .bytes,
            bytes
        );
    }
    request
}

async fn run(insert: bool) {
    let (qualified, expected_pixels) = Box::pin(qualified_target(insert)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let original = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(21))
        .unwrap();
    assert_eq!(original.program.commands().len(), 5);
    if insert {
        assert_eq!(
            expected_pixels,
            (124..127)
                .map(|page| external_editor_probe::preview_page(flow.accepted[0].service(), page))
                .collect::<Vec<_>>(),
            "same-cell Insert Copy retains the independent original P pixels on every consumer"
        );
    }
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request(&flow, insert, &qualified),
        policy(),
        designs::deadline(),
    )
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let TeletextDesignMutation::Saved(concept) =
        result.expect("legal original resource glyph insertion/deletion must import")
    else {
        panic!("current-head structural concept must save");
    };
    assert_eq!(concept.head().revision, 2);
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        (124..127)
            .map(|page| external_editor_probe::preview_page(restored.encoded().unwrap(), page))
            .collect::<Vec<_>>(),
        expected_pixels
    );
    assert_eq!(
        restored
            .encoded()
            .unwrap()
            .declared()
            .semantic()
            .objects()
            .definition(seed::target(21))
            .unwrap()
            .program
            .commands()
            .len(),
        if insert { 6 } else { 4 }
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn original_pop_glyph_insert_copy_imports_after_independent_full_native_target_validation() {
    Box::pin(run(true)).await;
}
#[tokio::test]
async fn original_pop_glyph_delete_imports_after_independent_full_native_target_validation() {
    Box::pin(run(false)).await;
}

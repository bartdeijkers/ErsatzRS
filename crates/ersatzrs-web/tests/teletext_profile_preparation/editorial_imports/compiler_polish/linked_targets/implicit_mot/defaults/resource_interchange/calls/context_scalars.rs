//! Sibling context snapshots stay original when their editable definition changes.
use super::*;

fn replacement(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_response(theme, page, subcode);
    let passive = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["identity"]["object_id"] == 21)
        .unwrap();
    assert_eq!(passive["program"]["commands"][4]["operation"]["code"], 80);
    passive["program"]["commands"][4]["operation"]["code"] = json!(81);
    value
}

fn rasters(service: &EncodedTeletextAuthoredService) -> Vec<TeletextRaster> {
    (124..127)
        .map(|page| external_editor_probe::preview_page(service, page))
        .collect()
}

#[tokio::test]
async fn context_scalar_edit_preserves_original_readonly_snapshots_in_sibling_companions() {
    let (target_fixture, target) = Box::pin(setup_with_fixture_policy_and_writer(
        replacement,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let target_pixels = rasters(target.accepted[0].service());
    target_fixture.state.pool.close().await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_eq!(
        flow.exported[0].sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
    assert_ne!(rasters(flow.accepted[0].service()), target_pixels);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let sidecar = request.sidecar.clone();
    let originals = request.files.clone();
    let resource = protected_file(&flow.exported[0], TeletextResourceKindWire::Pop, 0xc0);
    let name = flow.exported[0].sidecar().project.assets.iter().find(|a| matches!(&a.scope,
        TeletextExportAssetScope::ObjectContextTtiV1 { resource_name, object_id:21, .. } if resource_name == resource)).unwrap().name.as_str();
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
    assert_eq!(words[7], 1 | (0x09 << 6) | (0x50 << 11));
    words[7] = 1 | (0x09 << 6) | (0x51 << 11);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    for original in originals.iter().filter(|f| f.name != name) {
        assert_eq!(
            request
                .files
                .iter()
                .find(|f| f.name == original.name)
                .unwrap(),
            original,
            "sibling context copies of Passive21 remain byte-identical originals"
        );
    }
    let concept = import(&fixture, request).await;
    let stored = drcs_bitmap::stored(&fixture, &concept).await;
    let docs = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = docs.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 14);
    assert_eq!(overlay.resource_object_operands().len(), 1);
    assert!(overlay.resource_object_program_overlays().is_empty());
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(rasters(restored.encoded().unwrap()), target_pixels);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

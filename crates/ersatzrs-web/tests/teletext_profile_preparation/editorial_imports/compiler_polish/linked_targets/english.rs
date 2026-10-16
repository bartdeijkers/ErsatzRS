//! Prepared English7C/two-pair editor concept; no actual editor/stream/client proof.

use super::*;
use ersatzrs_addon_contract::TeletextResourceKindWire;

#[path = "english/oracle.rs"]
mod checks;

pub(super) const ENGLISH_7C: [u16; 10] =
    [0, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0x198, 0];

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = input::response(theme, page, subcode);
    for page in value["service"]["pages"].as_array_mut().unwrap() {
        page["national_option"] = json!(0);
    }
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        metadata["presentation"]["primary"] = json!(0);
        metadata["local_program"]["commands"].as_array_mut().unwrap().extend(
            json!([
                {"kind":"set_position","position":{"row":4,"column":12}},
                {"kind":"at_column","column":12,"operation":{"kind":"designate_charset","designation":55}},
                {"kind":"at_column","column":12,"operation":{"kind":"g0","code":65}},
                {"kind":"at_column","column":13,"operation":{"kind":"designate_charset","designation":0}}
            ]).as_array().unwrap().iter().cloned());
    }
    assert!(
        serde_json::to_vec(&value).unwrap().len() <= input::encoding().input.max_response_bytes
    );
    value
}

fn request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = "Prepared exact English national double vertical".into();
    assert_eq!(flow.exported[0].sidecar().project.pages.len(), 3);
    for page in &flow.exported[0].sidecar().project.pages {
        edit_cell(&mut request.files, &page.name, 1, 7, 0x7c);
    }
    assert_eq!(request.files.len(), flow.exported[0].files().len());
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
    );
    for before in flow.exported[0].files() {
        let after = request
            .files
            .iter()
            .find(|file| file.name == before.name)
            .unwrap();
        let mut expected = vec![before.clone()];
        if flow.exported[0]
            .sidecar()
            .project
            .pages
            .iter()
            .any(|page| page.name == before.name)
        {
            edit_cell(&mut expected, &before.name, 1, 7, 0x7c);
        }
        assert_eq!(after, &expected[0]);
    }
    request
}

fn expected_overlays(flow: &Flow) -> Vec<TeletextPhysicalBaseOverlay> {
    super::expected_overlays(flow)
        .into_iter()
        .map(|overlay| {
            let mut rows = overlay.rows().to_vec();
            assert_eq!(rows.len(), 1);
            let mut cells = *rows[0].cells();
            assert_eq!(cells[7], 0x5b);
            cells[7] = 0x7c;
            rows[0] = TeletextPhysicalBaseRow::new(1, cells).unwrap();
            TeletextPhysicalBaseOverlay::new(overlay.owner(), rows).unwrap()
        })
        .collect()
}

pub(super) async fn restored(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
) -> Vec<TeletextRaster> {
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (before, after) in original.sources().iter().zip(restored.sources()) {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    checks::assert_service(flow, restored.encoded().unwrap());
    checks::previews(restored.encoded().unwrap(), 0x7c)
}

fn changed_generated(
    exported: &ExportedOriginalTeletextDesign,
    fresh: &TeletextDesignExportReceipt,
) -> NativeTeletextEditorialImportRequest {
    let identity = normal_identity(124, 1);
    let assets = &exported.sidecar().project.assets;
    let raw = assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope,
        TeletextExportAssetScope::Resource {identity:wire,..}
        if wire.kind == TeletextResourceKindWire::Drcs
            && wire.page.magazine == identity.page().magazine()
            && wire.page.page == identity.page().page() && wire.subpage == 1)
        })
        .unwrap();
    let mapping = assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope,
        TeletextExportAssetScope::DrcsTtiSet {resource_names}
            if resource_names.get(1) == Some(&Some(raw.name.clone())))
        })
        .unwrap();
    let mut request = drcs_bitmap::from_receipt(fresh, 2);
    let before = request.files.clone();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == mapping.name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let (mut selected, mut changed) = (false, false);
    for record in &mut records {
        match record {
            TeletextTtiRecord::PageNumber {
                page,
                editor_ordinal,
            } => {
                assert_eq!(*page, 0x2a0);
                selected = *editor_ordinal == 1;
            }
            TeletextTtiRecord::BaseRow { row: 5, cells } if selected => {
                let mask: [u16; 10] = std::array::from_fn(|row| {
                    (u16::from(cells[row * 2] & 63) << 6) | u16::from(cells[row * 2 + 1] & 63)
                });
                assert_eq!(mask, ENGLISH_7C);
                assert!(cells[..20].iter().all(|cell| cell & 0x40 != 0));
                cells[1] ^= 1;
                let mut replacement = ENGLISH_7C;
                replacement[0] ^= 1;
                TeletextDrcsBitmap::new(replacement).unwrap();
                assert!(!changed);
                changed = true;
            }
            _ => {}
        }
    }
    assert!(changed, "exact full Normal2A0/S1=1/slot8 PTU row5 required");
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(before.len(), request.files.len());
    for (old, new) in before.iter().zip(&request.files) {
        assert_eq!(old.name, new.name);
        if old.name == mapping.name {
            assert_ne!(old.bytes, new.bytes);
        } else {
            assert_eq!(old, new);
        }
    }
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap()
    );
    request
}

async fn assert_state(
    fixture: &Fixture,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
    concept: &SavedEditorialTeletextDesign,
    day: &AcceptedTeletextOutputCandidate,
    fresh: &TeletextDesignExportReceipt,
) {
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(concept.head().id)
            .await
            .unwrap()
            .unwrap(),
        *concept.head()
    );
    assert_pointers(fixture, day, &flow.accepted[1]).await;
    retained.assert_unchanged(fixture, flow).await;
    assert_eq!(
        qtm_carrier::stored_receipt(fixture, fresh.metadata.id).await,
        *fresh
    );
    checks::assert_service(flow, recover(fixture, 1).await.service());
    Box::pin(restored(fixture, flow, concept)).await;
}

#[tokio::test]
async fn english_two_pair_concept_previews_publishes_protects_slot8_and_reopens() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        input::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    checks::assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, request(&flow)).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let previews = Box::pin(restored(&fixture, &flow, &concept)).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(
        Box::pin(restored(&fixture, &flow, &concept)).await,
        previews
    );
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    checks::assert_service(&flow, day.service());
    assert_eq!(checks::previews(day.service(), 0x7c), previews);
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    oracle::assert_mapping(&exported);
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        changed_generated(&exported, &fresh),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Tti(error) = error else {
        panic!("factory-owned slot8 must remain protected: {error:?}")
    };
    assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Structure);
    assert_eq!(error.resource, Some(normal_identity(124, 1)));
    assert_eq!((error.page, error.subcode, error.row), (0x2a0, 1, Some(5)));
    Box::pin(assert_state(
        &fixture, &flow, &retained, &concept, &day, &fresh,
    ))
    .await;
    fixture = fixture.reopen().await;
    Box::pin(assert_state(
        &fixture, &flow, &retained, &concept, &day, &fresh,
    ))
    .await;
    assert_eq!(
        checks::previews(recover(&fixture, 1).await.service(), 0x7c),
        previews
    );
    assert_eq!(
        checks::previews(recover(&fixture, 2).await.service(), b'A'),
        checks::previews(flow.accepted[1].service(), b'A')
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

pub(super) fn assert_originals(flow: &Flow) {
    checks::assert_originals(flow);
}

pub(super) fn assert_service(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    checks::assert_service(flow, service);
}

pub(super) fn previews(service: &EncodedTeletextAuthoredService) -> Vec<TeletextRaster> {
    checks::previews(service, 0x7c)
}

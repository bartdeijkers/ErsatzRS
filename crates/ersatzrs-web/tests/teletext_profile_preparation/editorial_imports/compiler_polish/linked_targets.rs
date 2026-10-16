//! Prepared three-owner edits exercise distinct already-linked compiler banks.
//! This native flow does not claim actual editor, stream or client acceptance.

use super::*;
use ersatzrs_ffmpeg::teletext::{TeletextResourceFullUpdate, TeletextServiceResourceTransmission};

#[path = "linked_targets/fixture.rs"]
mod input;
#[path = "linked_targets/oracle.rs"]
mod oracle;

fn address(page: u16) -> TeletextPageAddress {
    channel_documents::address(page, 32)
}

fn normal_identity(page: u16, subpage: u8) -> TeletextResourceIdentity {
    assert!((124..127).contains(&page));
    TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(2, [0xa0, 0xa2, 0xa3][usize::from(page - 124)]).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}

fn authored_identities() -> Vec<TeletextResourceIdentity> {
    let mut identities = (124..127)
        .map(|page| normal_identity(page, 0))
        .collect::<Vec<_>>();
    identities.push(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Gdrcs,
            TeletextRawDataPageNumber::new(1, 0xa1).unwrap(),
            TeletextResourceSubpageIndex::ZERO,
        )
        .unwrap(),
    );
    identities
}

fn prepared_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = "Prepared three linked-target Polish concept".into();
    assert_eq!(flow.exported[0].sidecar().project.pages.len(), 3);
    for page in &flow.exported[0].sidecar().project.pages {
        edit_cell(&mut request.files, &page.name, 1, 7, 0x5b);
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
            edit_cell(&mut expected, &before.name, 1, 7, 0x5b);
        }
        assert_eq!(after, &expected[0]);
    }
    request
}

fn expected_overlays(flow: &Flow) -> Vec<TeletextPhysicalBaseOverlay> {
    flow.exported[0]
        .sidecar()
        .project
        .pages
        .iter()
        .map(|page| {
            let file = flow.exported[0]
                .files()
                .iter()
                .find(|file| file.name == page.name)
                .unwrap();
            let tti = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
            let mut cells = *tti
                .records()
                .iter()
                .find_map(|record| match record {
                    TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
                    _ => None,
                })
                .unwrap();
            assert_eq!(cells[7], b'A');
            cells[7] = 0x5b;
            TeletextPhysicalBaseOverlay::new(
                address(page.page.page),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()
        })
        .collect()
}

async fn assert_restored(
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
    oracle::assert_service(flow, restored.encoded().unwrap());
    oracle::previews(restored.encoded().unwrap(), 0x5b)
}

#[tokio::test]
async fn three_linked_targets_prepared_concept_previews_publishes_exports_and_reopens() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        input::response,
        Fixture::new_persistent().await,
        input::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    oracle::assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, prepared_request(&flow)).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let preview = Box::pin(assert_restored(&fixture, &flow, &concept)).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        Box::pin(assert_restored(&fixture, &flow, &concept)).await,
        preview
    );
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    oracle::assert_service(&flow, day.service());
    assert_eq!(oracle::previews(day.service(), 0x5b), preview);
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    oracle::assert_mapping(&exported);
    fixture = fixture.reopen().await;
    oracle::assert_service(&flow, recover(&fixture, 1).await.service());
    assert_eq!(
        oracle::previews(recover(&fixture, 1).await.service(), 0x5b),
        preview
    );
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(
        qtm_carrier::stored_receipt(&fixture, fresh.metadata.id).await,
        fresh
    );
    assert_eq!(
        oracle::previews(recover(&fixture, 2).await.service(), b'A'),
        oracle::previews(flow.accepted[1].service(), b'A')
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[path = "linked_targets/rejections.rs"]
mod rejections;

#[path = "linked_targets/genuine.rs"]
mod genuine;

#[path = "linked_targets/implicit_mot.rs"]
mod implicit_mot;

#[path = "linked_targets/english.rs"]
mod english;

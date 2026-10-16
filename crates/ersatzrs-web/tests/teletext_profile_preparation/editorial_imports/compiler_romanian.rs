//! Exact Romanian Base edits through installed profiles and native publication.
//! Actual editor and TCP evidence belong only to the opt-in genuine case.

use super::*;
use ersatzrs_app::teletext_export_project::TeletextExportAssetScope;
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use serde_json::{json, Value};

#[path = "compiler_romanian/genuine.rs"]
mod genuine;
#[path = "compiler_romanian/fixture.rs"]
mod input;
#[path = "compiler_romanian/inputs.rs"]
mod inputs;
#[path = "compiler_romanian/oracle.rs"]
mod oracle;

fn address(page: u16) -> TeletextPageAddress {
    channel_documents::address(page, 32)
}

fn variant(page: u16) -> u8 {
    assert!((124..128).contains(&page));
    u8::try_from(page - 124).unwrap()
}

fn code(page: u16) -> u8 {
    if variant(page) < 2 {
        0x5d
    } else {
        0x7d
    }
}

fn slot(page: u16) -> u8 {
    4 + variant(page)
}

fn resource_identity() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(1, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap()
}

fn prepared_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = "Prepared four Romanian national variants".into();
    assert_eq!(flow.exported[0].sidecar().project.pages.len(), 4);
    for page in &flow.exported[0].sidecar().project.pages {
        edit_cell(&mut request.files, &page.name, 1, 7, code(page.page.page));
    }
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
    );
    assert_eq!(request.files.len(), flow.exported[0].files().len());
    for before in flow.exported[0].files() {
        let mut expected = vec![before.clone()];
        if let Some(page) = flow.exported[0]
            .sidecar()
            .project
            .pages
            .iter()
            .find(|page| page.name == before.name)
        {
            edit_cell(&mut expected, &before.name, 1, 7, code(page.page.page));
        }
        assert_eq!(
            request
                .files
                .iter()
                .find(|file| file.name == before.name)
                .unwrap(),
            &expected[0]
        );
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
            let parsed = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
            let mut cells = *parsed
                .records()
                .iter()
                .find_map(|record| match record {
                    TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
                    _ => None,
                })
                .unwrap();
            assert_eq!(cells[7], b'A');
            cells[7] = code(page.page.page);
            TeletextPhysicalBaseOverlay::new(
                address(page.page.page),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()
        })
        .collect()
}

async fn restored_previews(
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
    oracle::previews(restored.encoded().unwrap(), true)
}

struct Published {
    fixture: Fixture,
    concept: SavedEditorialTeletextDesign,
    accepted: [AcceptedTeletextOutputCandidate; 2],
    exported: ExportedOriginalTeletextDesign,
    receipt: TeletextDesignExportReceipt,
}

async fn publish(
    mut fixture: Fixture,
    flow: &Flow,
    request: NativeTeletextEditorialImportRequest,
) -> Published {
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, flow).await;
    retained.assert_unchanged(&fixture, flow).await;
    let previews = Box::pin(restored_previews(&fixture, flow, &concept)).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, flow).await;
    assert_eq!(
        Box::pin(restored_previews(&fixture, flow, &concept)).await,
        previews
    );
    let day = publish_and_accept(&fixture, flow, &concept).await;
    let accepted = [day, recover(&fixture, 2).await];
    oracle::assert_accepted(flow, &accepted);
    assert_eq!(oracle::previews(accepted[0].service(), true), previews);
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let receipt = receipt(&fixture, &exported).await;
    oracle::assert_mapping(&exported);
    fixture = fixture.reopen().await;
    compiler_polish::assert_pointers(&fixture, &accepted[0], &accepted[1]).await;
    retained.assert_unchanged(&fixture, flow).await;
    assert_eq!(
        Box::pin(restored_previews(&fixture, flow, &concept)).await,
        previews
    );
    assert_eq!(
        oracle::previews(recover(&fixture, 1).await.service(), true),
        previews
    );
    assert_eq!(
        qtm_carrier::stored_receipt(&fixture, receipt.metadata.id).await,
        receipt
    );
    Published {
        fixture,
        concept,
        accepted,
        exported,
        receipt,
    }
}

async fn assert_retained(
    published: &Published,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
) {
    let fixture = &published.fixture;
    retained.assert_unchanged(fixture, flow).await;
    compiler_polish::assert_pointers(fixture, &published.accepted[0], &published.accepted[1]).await;
    oracle::assert_accepted(flow, &published.accepted);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(published.concept.head().id)
            .await
            .unwrap()
            .unwrap(),
        *published.concept.head()
    );
    assert_eq!(
        qtm_carrier::stored_receipt(fixture, published.receipt.metadata.id).await,
        published.receipt
    );
    oracle::assert_service(
        flow,
        drcs_bitmap::restored(fixture, &published.concept)
            .await
            .encoded()
            .unwrap(),
    );
}

#[tokio::test]
async fn prepared_four_romanian_variants_preview_publish_reexport_and_reopen() {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        input::response,
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    oracle::assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let request = prepared_request(&flow);
    let mut published = Box::pin(publish(fixture, &flow, request)).await;
    oracle::reject_generated_slots(&published, &flow, &retained).await;
    published.fixture = published.fixture.reopen().await;
    Box::pin(assert_retained(&published, &flow, &retained)).await;
    assert!(published.fixture.playout.try_recv().is_err());
    published.fixture.state.pool.close().await;
}

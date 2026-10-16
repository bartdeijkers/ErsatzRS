//! Prepared eight-file edits through installed profiles and native publication.
//! Exported inspection inputs are not evidence of actual editor use.

use super::*;
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextResourceFullUpdate, TeletextServiceResourceTransmission,
};
use serde_json::{json, Value};
use std::path::PathBuf;

#[path = "compiler_scopes/delivery.rs"]
mod delivery;
#[path = "compiler_scopes/genuine.rs"]
mod genuine;
#[path = "compiler_scopes/oracle.rs"]
mod oracle;
#[path = "compiler_scopes/rejections.rs"]
mod rejections;
#[path = "compiler_scopes/fixture.rs"]
mod scope_fixture;

fn address(page: u16) -> TeletextPageAddress {
    channel_documents::address(page, 32)
}

fn slot(page: u16) -> u8 {
    assert!((124..132).contains(&page));
    u8::try_from((page - 124) % 4).unwrap()
}

fn code(page: u16) -> u8 {
    if slot(page) < 2 {
        0x5b
    } else {
        0x23
    }
}

fn authored_bank() -> TeletextDrcsSubtable {
    TeletextDrcsSubtable::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Drcs,
            TeletextRawDataPageNumber::new(2, 0xa2).unwrap(),
            TeletextResourceSubpageIndex::new(3).unwrap(),
        )
        .unwrap(),
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(5).unwrap(),
            bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
        }],
    )
    .unwrap()
}

fn prepared_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = "Prepared eight-page national scope concept".into();
    for page in &flow.exported[0].sidecar().project.pages {
        edit_cell(&mut request.files, &page.name, 1, 7, code(page.page.page));
    }
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
    );
    assert_eq!(request.files.len(), flow.exported[0].files().len());
    for before in flow.exported[0].files() {
        let after = request
            .files
            .iter()
            .find(|file| file.name == before.name)
            .unwrap();
        if let Some(page) = flow.exported[0]
            .sidecar()
            .project
            .pages
            .iter()
            .find(|page| page.name == before.name)
        {
            let mut expected = vec![before.clone()];
            edit_cell(&mut expected, &before.name, 1, 7, code(page.page.page));
            assert_eq!(after.bytes, expected[0].bytes);
        } else {
            assert_eq!(after.bytes, before.bytes, "every companion remains exact");
        }
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
            cells[7] = code(page.page.page);
            TeletextPhysicalBaseOverlay::new(
                address(page.page.page),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()
        })
        .collect()
}

fn assert_concept(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_physical_base_overlays(
            expected_overlays(flow),
            scope_fixture::encoding_policy().validation.service,
        )
        .unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(
        service.magazine_units(),
        flow.accepted[0].service().magazine_units()
    );
    oracle::assert_generated(flow, service);
}

async fn assert_restored(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
) -> Vec<TeletextRaster> {
    let restored = drcs_bitmap::restored(fixture, concept).await;
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    assert_eq!(restored.metadata(), flow.saved[0].candidate().metadata());
    assert_eq!(
        restored.transmissions(),
        flow.saved[0].candidate().transmissions()
    );
    assert_eq!(
        restored.sources().len(),
        flow.saved[0].candidate().sources().len()
    );
    for (before, after) in flow.saved[0]
        .candidate()
        .sources()
        .iter()
        .zip(restored.sources())
    {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    assert_concept(flow, restored.encoded().unwrap());
    oracle::previews(restored.encoded().unwrap())
}

#[expect(
    clippy::disallowed_methods,
    reason = "Optional create-new inspection export for the separately authorized genuine-editor checkpoint"
)]
fn write_inspection_export(flow: &Flow) {
    let Some(directory) = std::env::var_os("ERSATZRS_QTM_DUAL_SCOPE_DIR") else {
        return;
    };
    let directory = PathBuf::from(directory);
    std::fs::create_dir(&directory).expect("dual-scope inspection destination must be new");
    let original = directory.join("native-original-1");
    std::fs::create_dir(&original).unwrap();
    for file in flow.exported[0].files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    live_client::write_new(
        &original.join("project.ersatzrs.json"),
        &write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap(),
    )
    .unwrap();
    polish_delivery::write_json(
        &directory,
        "inspection-only.json",
        &json!({"probe_version":1,
        "scope":"prepared fixture export for inspection; this closed fixture cannot authenticate a later genuine-editor flow",
        "genuine_editor_proven":false,"ordinary_delivery_proven":false,"actual_client_proven":false}),
    );
}

#[tokio::test]
async fn prepared_eight_page_dual_scope_concept_previews_publishes_and_reopens() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        scope_fixture::response,
        Fixture::new_persistent().await,
        scope_fixture::encoding_policy,
        authoring_metadata::write_compact,
    ))
    .await;
    oracle::assert_originals(&flow);
    write_inspection_export(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, prepared_request(&flow)).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let preview = Box::pin(assert_restored(&fixture, &flow, &concept)).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(
        Box::pin(assert_restored(&fixture, &flow, &concept)).await,
        preview
    );
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_concept(&flow, day.service());
    assert_eq!(oracle::previews(day.service()), preview);
    compiler_polish::assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    let reopened = recover(&fixture, 1).await;
    assert_concept(&flow, reopened.service());
    assert_eq!(oracle::previews(reopened.service()), preview);
    compiler_polish::assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(
        oracle::previews(recover(&fixture, 2).await.service()),
        oracle::previews(flow.accepted[1].service())
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

//! Literal legacy carrier receipt recovery, independently of the current mapper.
//! Fixture-only repository seeding recreates a retained pre-v2 server baseline;
//! this does not grant uploaded metadata export or publication authority.

use super::*;
use ersatzrs_app::teletext_export_project::{
    parse_teletext_export_receipt_body, write_teletext_export_receipt_body,
    TeletextExportReceiptBody, TeletextExportSidecar,
};
use ersatzrs_core::repositories::{TeletextDesignExportSave, TeletextDesignRepository as _};
use ersatzrs_infra::repositories::SqliteTeletextDesignRepository;
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

const LEGACY_PROFILE: &str = "qteletextmaker-1.0";
const CURRENT_PROFILE: &str = "qteletextmaker-1.0-carrier-v2";
const LEGACY_PS: u16 = 0xC0C0;
const CURRENT_PS: u16 = 0xC240;

fn hash(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(text, "{byte:02x}").unwrap();
    }
    text
}

fn assert_status(files: &[TeletextExportFile], name: &str, status: u16) {
    let bytes = &files.iter().find(|file| file.name == name).unwrap().bytes;
    assert!(parse_teletext_tti(bytes, policy().tti)
        .unwrap()
        .records()
        .contains(&TeletextTtiRecord::PageStatus(status)));
    let literal = format!("PS,{status:04X}\r\n");
    assert!(bytes
        .windows(literal.len())
        .any(|bytes| bytes == literal.as_bytes()));
}

fn assert_current_export(flow: &Flow) {
    let exported = &flow.exported[0];
    assert_eq!(exported.sidecar().project.editor_profile, CURRENT_PROFILE);
    let page = &exported.sidecar().project.pages[0];
    assert_eq!(page.header_controls, 0x0481);
    let primary = flow.saved[0]
        .candidate()
        .encoded()
        .unwrap()
        .declared()
        .semantic()
        .effective_presentation(channel_documents::address(124, 32))
        .unwrap()
        .primary_designation()
        .unwrap();
    assert_eq!(primary.get(), 1);
    // Literal oracle: native compact fallback bits are not QTM PS bit positions.
    assert_status(exported.files(), &page.name, CURRENT_PS);
}

fn legacy_body(flow: &Flow) -> TeletextExportReceiptBody {
    let mut body =
        parse_teletext_export_receipt_body(&flow.receipts[0].receipt, policy().project).unwrap();
    LEGACY_PROFILE.clone_into(&mut body.project.editor_profile);
    for page in &mut body.project.pages {
        assert_eq!(page.header_controls, 0x0481);
        let file = body
            .files
            .iter_mut()
            .find(|file| file.name == page.name)
            .unwrap();
        let mut records = parse_teletext_tti(&file.bytes, policy().tti)
            .unwrap()
            .records()
            .to_vec();
        let status = records
            .iter_mut()
            .find(|record| matches!(record, TeletextTtiRecord::PageStatus(_)))
            .unwrap();
        // Independent old vector, deliberately never produced by the v2 mapper.
        *status = TeletextTtiRecord::PageStatus(LEGACY_PS);
        file.bytes = write_teletext_tti(
            &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
            policy().tti,
        )
        .unwrap();
        page.tti_bytes = file.bytes.len();
        page.tti_sha256 = hash(&file.bytes);
        assert_status(&body.files, &page.name, LEGACY_PS);
    }
    body
}

async fn stored_receipt(fixture: &Fixture, id: i64) -> TeletextDesignExportReceipt {
    fixture
        .state
        .application
        .teletext_design_export_receipt(
            id,
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

async fn seed_legacy_receipt(fixture: &Fixture, flow: &Flow) -> TeletextDesignExportReceipt {
    let body = legacy_body(flow);
    let bytes = write_teletext_export_receipt_body(&body, policy().project).unwrap();
    let original = &flow.originals[0];
    // This controlled test-only repository insert models a previously qualified
    // retained server receipt. Production native export remains the sole factory.
    let saved = SqliteTeletextDesignRepository(fixture.state.pool.clone())
        .create_export(
            &TeletextDesignExportSave {
                design_id: original.design_id,
                revision: original.revision,
                design_digest: original.digest.clone(),
                expected_design_head: 1,
                receipt_version: 1,
                receipt: bytes.clone(),
            },
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap();
    let TeletextDesignMutation::Saved(metadata) = saved else {
        panic!("controlled legacy server baseline must be retained");
    };
    assert_eq!(metadata.digest, hash(&bytes));
    let receipt = stored_receipt(fixture, metadata.id).await;
    assert_eq!(receipt.receipt, bytes);
    assert_eq!(receipt.metadata, metadata);
    receipt
}

fn legacy_request(receipt: &TeletextDesignExportReceipt) -> NativeTeletextEditorialImportRequest {
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    assert_eq!(body.project.editor_profile, LEGACY_PROFILE);
    let page = &body.project.pages[0];
    assert_status(&body.files, &page.name, LEGACY_PS);
    let mut files = body.files;
    edit_cell(&mut files, &page.name, 2, 8, b'Q');
    assert_status(&files, &page.name, LEGACY_PS);
    NativeTeletextEditorialImportRequest {
        name: "Synthetic retained legacy carrier concept".to_owned(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: 1,
        sidecar: write_teletext_export_sidecar(
            &TeletextExportSidecar {
                schema_version: 1,
                receipt_id: receipt.metadata.id,
                receipt_digest: receipt.metadata.digest.clone(),
                project: body.project,
            },
            policy().project,
        )
        .unwrap(),
        files,
    }
}

#[tokio::test]
async fn literal_legacy_carrier_concept_survives_preview_publication_and_reopen() {
    let (mut fixture, flow) = setup().await;
    assert_current_export(&flow);
    let legacy = seed_legacy_receipt(&fixture, &flow).await;
    let edited = import(&fixture, legacy_request(&legacy)).await;
    assert_eq!(edited.head().id, flow.saved[0].head().id);
    assert_eq!(edited.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    let raster = assert_concept_preview(&fixture, &flow, &edited, 2).await;
    assert_eq!(stored_receipt(&fixture, legacy.metadata.id).await, legacy);
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        assert_concept_preview(&fixture, &flow, &edited, 2).await,
        raster
    );
    assert_eq!(stored_receipt(&fixture, legacy.metadata.id).await, legacy);
    let accepted = publish_and_accept(&fixture, &flow, &edited).await;
    assert_eq!(generations::preview(accepted.service()).raster(), &raster);
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert_eq!(stored_receipt(&fixture, legacy.metadata.id).await, legacy);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

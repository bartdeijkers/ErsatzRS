//! One grouped native import boundary for the four enhanced spacing controls.
//! This does not substitute for genuine external-editor or fallback-client proof.

use super::*;

#[tokio::test]
async fn enhanced_spacing_bundle_import_preview_reexport_reopen_preserves_publication() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        held_mosaics::response,
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut cells = [b' '; 40];
    cells[..17].copy_from_slice(&[
        0x00, b'A', 0x07, 0x10, 0x66, 0x07, 0x0e, b'B', b' ', 0x0c, 0x0f, b'C', b' ', 0x0c, 0x07,
        b' ', b'Z',
    ]);
    let target = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 4, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    *target = cells;
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    let expected_files = request.files.clone();
    let concept = import(&fixture, request).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (before, after) in original.sources().iter().zip(restored.sources()) {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    assert!(preview.diagnostics().is_empty());
    assert_ne!(
        preview,
        external_editor_probe::preview_page(flow.accepted[0].service(), 124)
    );
    assert_eq!(
        external_editor_probe::preview_page(restored.encoded().unwrap(), 125),
        external_editor_probe::preview_page(flow.accepted[0].service(), 125)
    );
    let reexport = drcs_bitmap::reexport(&fixture, &concept).await;
    national_editor::assert_files(reexport.files(), &expected_files);
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    fixture = fixture.reopen().await;
    let reopened = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(reopened.metadata(), restored.metadata());
    assert_eq!(reopened.sources().len(), restored.sources().len());
    for (before, after) in restored.sources().iter().zip(reopened.sources()) {
        assert_eq!(before.bytes(), after.bytes());
        assert_eq!(before.authoring(), after.authoring());
    }
    assert_eq!(
        external_editor_probe::preview_page(reopened.encoded().unwrap(), 124),
        preview
    );
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

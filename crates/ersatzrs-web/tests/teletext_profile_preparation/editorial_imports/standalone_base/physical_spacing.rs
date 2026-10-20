//! One grouped native import boundary for the four enhanced spacing controls.
//! This does not substitute for genuine external-editor or fallback-client proof.

use super::*;

#[tokio::test]
async fn enhanced_spacing_bundle_import_preview_reexport_reopen_preserves_publication() {
    Box::pin(run(None)).await;
}

#[tokio::test]
#[ignore = "requires pinned headed QTM save in fresh ERSATZRS_TELETEXT_SPACING_EDITOR_DIR"]
async fn actual_qtm_enhanced_spacing_menus_import_preview_publish_reopen() {
    let root = std::path::PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_SPACING_EDITOR_DIR",
    ));
    Box::pin(run(Some(&root))).await;
}

fn expected_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
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
    request
}

async fn genuine_edit(directory: &Path, flow: &Flow) -> NativeTeletextEditorialImportRequest {
    qtm::inputs_for_layout(
        directory,
        "enhanced-physical-spacing",
        flow,
        [0, 0, 0],
        || {
            json!({"kind":"enhanced_spacing","editor_level":"2.5","row":4,"consumer_visual_columns":[0,3,6,10],
            "codes":[0,16,14,15],"columns":[0,3,6,10],"no_optional_resource_banks":true})
        },
    );
    qtm::completion(directory).await;
    let bytes =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    qtm::actual_request(directory, flow, &bytes)
}

async fn run(editor: Option<&Path>) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        held_mosaics::response,
        Fixture::new_persistent().await,
        channel_documents::policy,
        authoring_metadata::write_compact,
    ))
    .await;
    let expected = expected_request(&flow);
    let request = if let Some(directory) = editor {
        genuine_edit(directory, &flow).await
    } else {
        expected_request(&flow)
    };
    national_editor::assert_files(&request.files, &expected.files);
    assert_eq!(request.sidecar, expected.sidecar);
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
    national_editor::assert_files(reexport.files(), &expected.files);
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
    if let Some(directory) = editor {
        let accepted = publish_and_accept(&fixture, &flow, &concept).await;
        assert_ne!(
            designs::units(accepted.service()),
            designs::units(flow.accepted[0].service())
        );
        assert_eq!(
            external_editor_probe::preview_page(accepted.service(), 124),
            preview
        );
        fixture = fixture.reopen().await;
        generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 480);
        generations::assert_same_output_width(&recover(&fixture, 2).await, &flow.accepted[1], 480);
        assert_retained_storage(&fixture, &flow).await;
        qtm::json_new(
            directory,
            "success.json",
            &json!({
                "scope":"Genuine pinned headed native menus/SaveAs -> concept/shared preview/reexport/reopen -> explicit publication/cold recovery. No ordinary HTTP/client/full9A claim.",
                "codes":[0,16,14,15],"row":4,"columns":[0,3,6,10],
                "design_id":concept.head().id,"revision":concept.head().revision,
                "generation_id":accepted.generation().id,"night_unchanged":true,
                "originals_metadata_receipts_preserved":true,"resource_assets":0
            }),
        );
    }
    fixture.state.pool.close().await;
}

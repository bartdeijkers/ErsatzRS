//! EB2: one plain complete page/subpage set, without optional enhanced resources.
use super::super::super::repeated_editorial_imports as repeated;
use super::*;
use std::path::{Path, PathBuf};

const OWNERS: [(u16, u16); 3] = [(124, 32), (124, 33), (125, 32)];

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = repeated::two_subpages(theme, page, subcode);
    let third = authoring_metadata::response(theme, page + 1, subcode);
    for key in ["pages", "page_enhancements", "mot_selections"] {
        value["service"][key]
            .as_array_mut()
            .unwrap()
            .extend(third["service"][key].as_array().unwrap().iter().cloned());
    }
    value["authoring"]["pages"].as_array_mut().unwrap().extend(
        third["authoring"]["pages"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    value["service"]["magazines"] = json!([]);
    for enhancement in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["presentation"] = Value::Null;
        enhancement["local_program"] = Value::Null;
    }
    for (ordinal, display) in value["service"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        display["national_option"] = json!(0);
        display["rows"][0]["runs"][0]["content"]["text"] = json!(format!(
            "PAGE {} SUB {}",
            OWNERS[ordinal].0, OWNERS[ordinal].1
        ));
        let (target, subpage) = OWNERS[(ordinal + 1) % OWNERS.len()];
        display["navigation"][0]["target"] = json!({"page":target,"subcode":subpage});
    }
    value
}

fn expected(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    for page in &flow.exported[0].sidecar().project.pages {
        edit_cell(&mut request.files, &page.name, 2, 8, b'Q');
    }
    request
}

async fn actual_set(directory: &Path, flow: &Flow) -> NativeTeletextEditorialImportRequest {
    std::fs::create_dir(directory).unwrap();
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    for &(page, subcode) in &OWNERS {
        let owner = channel_documents::address(page, subcode);
        let case = directory.join(format!("page-{page}-{subcode:04x}"));
        qtm::inputs_for_address(&case, "plain-base-set", flow, [0, 0, 8], owner, || {
            json!({"kind":"base_set_zone_q","row":2,"column":8,
                "consumer_visual_columns":[8],"no_optional_resource_banks":true})
        });
        let _ = qtm::completion(&case).await;
        let saved =
            live_client::read_bounded(&case.join("edited.tti"), policy().tti.max_bytes).unwrap();
        let name = &flow.exported[0]
            .sidecar()
            .project
            .pages
            .iter()
            .find(|entry| entry.page.page == page && entry.page.subcode == subcode)
            .unwrap()
            .name;
        request
            .files
            .iter_mut()
            .find(|file| &file.name == name)
            .unwrap()
            .bytes = saved;
    }
    request
}

fn rasters(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
) -> Vec<TeletextRaster> {
    OWNERS
        .iter()
        .map(|&(page, subcode)| {
            let preview = project_teletext_preview(
                service,
                channel_documents::address(page, subcode),
                TeletextColourPixelOptions {
                    reveal_concealed: false,
                    flash_phase: TeletextFlashPhase::Original,
                },
                preview_budget(),
            )
            .unwrap();
            let wanted = service
                .declared()
                .semantic()
                .base()
                .pages
                .iter()
                .find(|entry| entry.address == channel_documents::address(page, subcode))
                .unwrap();
            assert_eq!(preview.navigation(), wanted.navigation);
            assert!(preview.raster().diagnostics().is_empty());
            assert_eq!(
                (preview.raster().width(), preview.raster().height()),
                (480, 250)
            );
            preview.raster().clone()
        })
        .collect()
}

async fn verify_reexport(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    request: &NativeTeletextEditorialImportRequest,
) {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    national_editor::assert_files(exported.files(), &request.files);
    assert_eq!(exported.sidecar().project.assets.len(), 0);
    let addresses = exported
        .sidecar()
        .project
        .pages
        .iter()
        .map(|page| (page.page.page, page.page.subcode))
        .collect::<Vec<_>>();
    assert_eq!(addresses, OWNERS);
}

async fn run(editor: Option<PathBuf>) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        repeated::two_subpage_policy,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_eq!(flow.exported[0].sidecar().project.pages.len(), 3);
    assert!(flow.exported[0].sidecar().project.assets.is_empty());
    assert!(flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definitions()
        .is_empty());
    let wanted = expected(&flow);
    let request = if let Some(directory) = editor.as_deref() {
        actual_set(directory, &flow).await
    } else {
        expected(&flow)
    };
    national_editor::assert_files(&request.files, &wanted.files);
    assert_eq!(request.sidecar, wanted.sidecar);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    let before = rasters(flow.accepted[0].service());
    let after = rasters(restored.encoded().unwrap());
    assert_preview_changes(&before, &after);
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    verify_reexport(&fixture, &concept, &wanted).await;
    fixture = fixture.reopen().await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(rasters(restored.encoded().unwrap()), after);
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(rasters(accepted.service()), after);
    assert_retained_storage(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 480);
    generations::assert_same_output_width(&recover(&fixture, 2).await, &flow.accepted[1], 480);
    verify_reexport(&fixture, &concept, &wanted).await;
    assert!(fixture.playout.try_recv().is_err());
    if let Some(directory) = editor {
        qtm::json_new(
            &directory,
            "success.json",
            &json!({
                "criterion":"EB2","pages":OWNERS,"all_saved_files_imported":true,
                "raw_subcodes_navigation_and_sidecar_preserved":true,
                "concept_preview_explicit_publication_cold_reopen":true,
                "no_optional_resource_banks":true,"other_channel_and_package_unchanged":true,
                "editor":"QTeletextMaker1.0/cac2167/af7cb",
                "scope":"Plain complete-base-set native/editor proof; no new stream/client/platform acceptance"
            }),
        );
    }
    fixture.state.pool.close().await;
}

fn assert_preview_changes(before: &[TeletextRaster], after: &[TeletextRaster]) {
    assert_eq!(before.len(), 3);
    assert_eq!(after.len(), 3);
    for (original, changed) in before.iter().zip(after) {
        assert_ne!(original, changed);
        for y in 0..250 {
            for x in 0..480 {
                if !(20..30).contains(&y) || !(96..108).contains(&x) {
                    assert_eq!(original.pixel(x, y), changed.pixel(x, y));
                }
            }
        }
    }
}

#[tokio::test]
async fn complete_plain_base_set_preserves_subcodes_navigation_and_publication() {
    Box::pin(run(None)).await;
}

#[tokio::test]
#[ignore = "requires actual pinned headed QTM saves in fresh ERSATZRS_TELETEXT_BASE_SET_EDITOR_DIR"]
async fn actual_qtm_complete_plain_base_set_import_preview_publish_reopen() {
    let directory = PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_BASE_SET_EDITOR_DIR",
    ));
    Box::pin(run(Some(directory))).await;
}

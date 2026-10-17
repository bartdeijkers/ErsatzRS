//! One genuine pinned resource Insert Copy save joins the existing native flow.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use std::path::Path;
const CASE: &str = "original-pop-glyph-insert-copy";

fn wide_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let region = page["regions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["id"] == "guide-content")
            .unwrap();
        assert_eq!(region["width"], 1);
        region["width"] = json!(2);
    }
    value
}
fn wide_inserted(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = wide_response(theme, page, subcode);
    let object = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["identity"]["object_id"] == 21)
        .unwrap();
    let commands = object["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 5);
    commands.push(json!({"kind":"at_column","column":2,"operation":{"kind":"g0","code":80}}));
    value
}
async fn qualified_wide_target() -> (Vec<TeletextTtiRecord>, Vec<TeletextRaster>) {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        wide_inserted,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let exported = &flow.exported[0];
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == companion(exported))
        .unwrap();
    let records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let pixels = (124..127)
        .map(|page| external_editor_probe::preview_page(flow.accepted[0].service(), page))
        .collect();
    fixture.state.pool.close().await;
    (records, pixels)
}
fn assert_added_p(flow: &Flow, pixels: &[TeletextRaster]) {
    let mask = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    for (page, raster) in (124..127).zip(pixels) {
        let original = external_editor_probe::preview_page(flow.accepted[0].service(), page);
        assert_eq!((raster.width(), raster.height()), (672, 250));
        for y in 0..250 {
            for x in 0..672 {
                if (72..84).contains(&x) && (40..50).contains(&y) {
                    let rgb = if mask[y - 40] & (1 << (11 - (x - 72))) == 0 {
                        [0, 0, 0]
                    } else {
                        [0, 15, 0]
                    };
                    assert!(
                        matches!(raster.pixel(x,y),Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==rgb)
                    );
                } else {
                    assert_eq!(
                        raster.pixel(x, y),
                        original.pixel(x, y),
                        "outside the one newly authored P cell"
                    );
                }
            }
        }
    }
}
#[tokio::test]
async fn original_pop_zone_insert_adds_visible_p_and_preserves_every_other_pixel() {
    let (target, pixels) = Box::pin(qualified_wide_target()).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        wide_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_added_p(&flow, &pixels);
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request
        .files
        .iter_mut()
        .find(|file| file.name == companion(&flow.exported[0]))
        .unwrap()
        .bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(target, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    let concept = import(&fixture, request).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        (124..127)
            .map(|p| external_editor_probe::preview_page(restored.encoded().unwrap(), p))
            .collect::<Vec<_>>(),
        pixels
    );
    assert_original_pointers(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

fn stage(directory: &Path, flow: &Flow) -> Value {
    std::fs::create_dir(directory).expect("fresh resource editor case");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let exported = &flow.exported[0];
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    let name = companion(exported);
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let ready = json!({"probe_version":1,"case":CASE,"editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&file.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|file|json!({"name":file.name,"bytes":file.bytes.len(),"sha256":drcs_bitmap::hash(&file.bytes)})).collect::<Vec<_>>(),
        "page":{"page":0x100,"subcode":0},"editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":{"kind":"resource_glyph_insert_copy","identity":seed::target(21),"source_command":4,"editor_word":7,"column":1,"copy_editor_word":8,"copy_column":2,"mode":9,"code":80,"consumers":[124,125,126]},
        "save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898","binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "publication":"unpublished concept first; explicit publication follows native validation"});
    editor::json_new(directory, "ready.json", &ready);
    ready
}
fn actual_request(
    directory: &Path,
    flow: &Flow,
    saved: &[u8],
    target: &[TeletextTtiRecord],
) -> NativeTeletextEditorialImportRequest {
    let exported = &flow.exported[0];
    let original = directory.join("original");
    assert_eq!(
        live_client::read_bounded(
            &original.join("project.ersatzrs.json"),
            policy().project.max_bytes
        )
        .unwrap(),
        write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap()
    );
    for file in exported.files() {
        assert_eq!(
            live_client::read_bounded(&original.join(&file.name), policy().project.max_file_bytes)
                .unwrap(),
            file.bytes
        );
    }
    let mut actual = parse_teletext_tti(saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(actual.len(), target.len());
    for expected in target {
        let i = actual
            .iter()
            .position(|r| r == expected)
            .expect("genuine save must match independently compiled resource insertion target");
        actual.swap_remove(i);
    }
    assert!(actual.is_empty());
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    "Actual QTM original resource Insert Copy".clone_into(&mut request.name);
    request
        .files
        .iter_mut()
        .find(|f| f.name == companion(exported))
        .unwrap()
        .bytes = saved.to_vec();
    request
}
#[tokio::test]
#[ignore = "requires pinned headed QTM resource structural driver and a fresh explicit proof directory"]
async fn actual_qtm_resource_insert_copy_save_import_preview_publish_reopen() {
    let root = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh proof root");
    let directory = root.join(CASE);
    let (target, pixels) = Box::pin(qualified_wide_target()).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        wide_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_added_p(&flow, &pixels);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let ready = stage(&directory, &flow);
    let completion = editor::completion(&directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let concept = import(&fixture, actual_request(&directory, &flow, &saved, &target)).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    let stored = drcs_bitmap::stored(&fixture, &concept).await;
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 15);
    assert_eq!(overlay.resource_object_program_overlays().len(), 1);
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        (124..127)
            .map(|p| external_editor_probe::preview_page(restored.encoded().unwrap(), p))
            .collect::<Vec<_>>(),
        pixels
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        (124..127)
            .map(|p| external_editor_probe::preview_page(accepted.service(), p))
            .collect::<Vec<_>>(),
        pixels
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        (124..127)
            .map(|p| external_editor_probe::preview_page(restored.encoded().unwrap(), p))
            .collect::<Vec<_>>(),
        pixels
    );
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    editor::json_new(
        &directory,
        "success.json",
        &json!({"probe_version":1,"case":CASE,"editor":completion,"ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&ready).unwrap()),"saved_sha256":drcs_bitmap::hash(&saved),"schema_version":15,"concept_revision":2,"all_three_shared_consumers_exact":true,"import_did_not_publish":true,"explicit_publication_and_reopen":true,"protected_artifacts_and_other_channel_unchanged":true,"ordinary_stream_or_client_acceptance_claim":false}),
    );
    fixture.state.pool.close().await;
}

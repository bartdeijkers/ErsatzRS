//! Genuine pinned-editor first-body edit, with exact retained input associations.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use std::path::Path;

const CASE: &str = "new-empty-definition-position";

fn stage(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
) -> Value {
    std::fs::create_dir(directory).expect("fresh first-empty-definition editor case");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    assert_eq!(receipt.metadata.revision, 2);
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v6"
    );
    let name = object_name(exported);
    let input = exported
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let asset = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .unwrap();
    let words = file_records(exported, name);
    assert_eq!(
        [word(&words, 3), word(&words, 4), word(&words, 5)],
        [TERMINATION; 3]
    );
    let ready = json!({"probe_version":1,"case":CASE,
        "editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":receipt.metadata.id,"receipt_digest":receipt.metadata.digest,"design_revision":2,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&input.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|file|json!({"name":file.name,"bytes":file.bytes.len(),
            "sha256":drcs_bitmap::hash(&file.bytes)})).collect::<Vec<_>>(),
        "resource_scope":asset.scope,"page":{"page":256,"subcode":0},
        "editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":{"kind":"resource_first_body_position","identity":identity(0),
            "source_commands_before":0,"typed_commands_after":2,"triplets":[
                {"semantic_word":3,"address":41,"mode":4,"data":0,
                    "expected_word":TERMINATION,"replacement_word":POSITION},
                {"semantic_word":4,"address":8,"mode":9,"data":80,
                    "expected_word":TERMINATION,"replacement_word":glyph(b'P')}],
            "final_termination_word":5,"consumers":[]},
        "save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0",
            "source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898",
            "binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "setup":"explicit HTTP Add2 into an actually empty original POP; no invented NoOp or call retarget",
        "publication":"unpublished import3 and reopen before separate explicit publication"});
    editor::json_new(directory, "ready.json", &ready);
    ready
}

fn actual_request(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    saved: &[u8],
    expected: &BodyExpected,
) -> NativeTeletextEditorialImportRequest {
    let original = directory.join("original");
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    assert_eq!(
        live_client::read_bounded(
            &original.join("project.ersatzrs.json"),
            policy().project.max_bytes
        )
        .unwrap(),
        sidecar
    );
    for file in exported.files() {
        assert_eq!(
            live_client::read_bounded(&original.join(&file.name), policy().project.max_file_bytes)
                .unwrap(),
            file.bytes
        );
    }
    let mut records = parse_teletext_tti(saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(records.len(), expected.records.len());
    for expected in &expected.records {
        let index = records
            .iter()
            .position(|record| record == expected)
            .expect("actual Save As must match entire independently encoded first-body companion");
        records.swap_remove(index);
    }
    assert!(records.is_empty());
    let mut request = drcs_bitmap::from_receipt(receipt, 2);
    "Actual QTM first empty definition row position".clone_into(&mut request.name);
    request
        .files
        .iter_mut()
        .find(|file| file.name == object_name(exported))
        .unwrap()
        .bytes = saved.to_vec();
    assert_eq!(request.sidecar, sidecar);
    for file in exported
        .files()
        .iter()
        .filter(|file| file.name != object_name(exported))
    {
        assert_eq!(
            request
                .files
                .iter()
                .find(|after| after.name == file.name)
                .unwrap(),
            file
        );
    }
    assert_eq!(
        request
            .files
            .iter()
            .find(|file| file.name == object_name(exported))
            .unwrap()
            .bytes,
        saved
    );
    request
}

async fn reopen_publish(
    mut fixture: Fixture,
    flow: &Flow,
    before: &Retained,
    concept: &SavedEditorialTeletextDesign,
    expected: &BodyExpected,
) -> Fixture {
    fixture = fixture.reopen().await;
    let stored = Box::pin(assert_body(&fixture, flow, concept, expected)).await;
    unchanged(&fixture, flow, before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(
        file_records(&exported, object_name(&exported)),
        expected.records
    );
    let accepted = publish_and_accept(&fixture, flow, concept).await;
    assert_eq!(
        accepted
            .service()
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect::<Vec<_>>(),
        expected.units
    );
    fixture = fixture.reopen().await;
    Box::pin(assert_body(&fixture, flow, concept, expected)).await;
    generations::assert_same_output_width(&recover(&fixture, 1).await, &accepted, 40 * 12);
    generations::assert_same_output_width(&recover(&fixture, 2).await, &flow.accepted[1], 40 * 12);
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(accepted.generation().clone())
    );
    assert_retained_storage(&fixture, flow).await;
    assert_eq!(retained(&fixture).await, *before);
    assert!(fixture.playout.is_empty());
    fixture
}

#[tokio::test]
#[ignore = "requires pinned headed QTM row-position driver and a fresh explicit proof directory"]
async fn actual_qtm_first_empty_definition_position_save_import_preview_publish_reopen() {
    let root = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh actual first-body position proof root");
    let directory = root.join(CASE);
    let empty = Box::pin(qualify_empty(1)).await;
    let expected = Box::pin(qualify_body(b'P')).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (fixture, flow) = Box::pin(setup_empty(fixture)).await;
    let before = retained(&fixture).await;
    assert_eq!(
        submit(&fixture, &fields(&flow.originals[0], 0)).await["saved"],
        "draft"
    );
    let added = Box::pin(assert_concept(&fixture, &flow, &empty)).await;
    let exported = Box::pin(export_stored(&fixture, &added)).await;
    let latest = receipt(&fixture, &exported).await;
    let ready = stage(&directory, &exported, &latest);
    let completion = editor::completion(&directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let request = actual_request(&directory, &exported, &latest, &saved, &expected);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 3);
    Box::pin(assert_body(&fixture, &flow, &concept, &expected)).await;
    unchanged(&fixture, &flow, &before).await;
    let fixture = Box::pin(reopen_publish(fixture, &flow, &before, &concept, &expected)).await;
    editor::json_new(
        &directory,
        "success.json",
        &json!({"probe_version":1,"case":CASE,
        "editor":completion,"ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&ready).unwrap()),
        "saved_sha256":drcs_bitmap::hash(&saved),"schema_version":18,"setup_add_revision":2,
        "actual_editor_import_revision":3,"complete_added_body_folded":true,
        "typed_set_position_row1_column0_then_g0_p_column8":true,
        "complete_native_units_and_unchanged_display_preview_exact":true,
        "fresh_export_then_unpublished_cold_reopen_before_explicit_publication":true,
        "accepted_cold_reopen_and_protected_night_profiles_package_unchanged":true,
        "actual_qtm_call_retarget_or_visible_display_glyph_claim":false,
        "ordinary_stream_or_client_acceptance_claim":false,"full_engine_acceptance_claim":false}),
    );
    fixture.state.pool.close().await;
}

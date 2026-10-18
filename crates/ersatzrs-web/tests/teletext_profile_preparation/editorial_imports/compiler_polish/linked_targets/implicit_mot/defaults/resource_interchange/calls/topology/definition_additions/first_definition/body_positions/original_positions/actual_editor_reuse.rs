//! Reuse one genuine pinned Save As only for a byte-identical fresh input.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use std::path::{Path, PathBuf};

const CASE: &str = "added-definition-interior-row";
const INPUT_SHA256: &str = "e5c5ff7e6f92d7f2e5b38c4765819b48d7d4bccbc826cc8ebfd689acc687436f";
const SAVED_SHA256: &str = "e9d9425c14d2183c536d1df221e88c4e9bc23faec0beda22e93230ebf4acdddb";

fn read_json(directory: &Path, name: &str) -> Value {
    serde_json::from_slice(&live_client::read_bounded(&directory.join(name), 64 * 1024).unwrap())
        .unwrap()
}
fn pins(value: &Value) {
    assert_eq!(value["editor_name"], "QTeletextMaker");
    assert_eq!(value["editor_version"], "1.0");
    assert_eq!(
        value["editor_source_commit"],
        "cac2167bac933a6160cca45a45d5d4ee95355898"
    );
    assert_eq!(
        value["editor_binary_sha256"],
        "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"
    );
}
fn original_files(directory: &Path, ready: &Value) {
    let files = ready["files"].as_array().unwrap();
    assert_eq!(files.len(), 4);
    for file in files {
        let name = file["name"].as_str().unwrap();
        assert!(name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)));
        let bytes = live_client::read_bounded(
            &directory.join("original").join(name),
            policy().project.max_file_bytes,
        )
        .unwrap();
        assert_eq!(
            u64::try_from(bytes.len()).unwrap(),
            file["bytes"].as_u64().unwrap()
        );
        assert_eq!(drcs_bitmap::hash(&bytes), file["sha256"].as_str().unwrap());
    }
    let sidecar = live_client::read_bounded(
        &directory.join("original/project.ersatzrs.json"),
        policy().project.max_bytes,
    )
    .unwrap();
    assert_eq!(
        drcs_bitmap::hash(&sidecar),
        ready["sidecar_sha256"].as_str().unwrap()
    );
}
async fn qualified_saved(exported: &ExportedOriginalTeletextDesign) -> Vec<u8> {
    let root = PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_EDITOR_DIR",
    ));
    assert!(root.is_absolute() && root.is_dir());
    let directory = root.join(CASE);
    assert!(
        directory.join("editor-saved.json").is_file(),
        "completed proof is required; no new editor trial"
    );
    let ready = read_json(&directory, "ready.json");
    assert_eq!(ready["probe_version"], 1);
    assert_eq!(ready["case"], CASE);
    assert_eq!(ready["design_revision"], 3);
    assert_eq!(ready["input_sha256"], INPUT_SHA256);
    assert_eq!(
        ready["edit"]["identity"],
        serde_json::to_value(identity(0)).unwrap()
    );
    let completion = serde_json::to_value(editor::completion(&directory).await).unwrap();
    pins(&completion);
    let success = read_json(&directory, "success.json");
    assert_eq!(success["case"], CASE);
    assert_eq!(success["schema_version"], 18);
    assert_eq!(success["editor"], completion);
    assert_eq!(success["saved_sha256"], SAVED_SHA256);
    assert_eq!(
        success["ready_sha256"],
        drcs_bitmap::hash(
            &live_client::read_bounded(&directory.join("ready.json"), 64 * 1024).unwrap()
        )
    );
    let gui = read_json(&directory, "editor-gui.json");
    pins(&gui);
    assert_eq!(
        gui["actual_gui_insert_before_interior_row_glyph_and_save_as"],
        true
    );
    assert_eq!(gui["all_original_files_unchanged"], true);
    assert_eq!(gui["input_sha256"], INPUT_SHA256);
    assert_eq!(gui["saved_sha256"], SAVED_SHA256);
    original_files(&directory, &ready);
    let input = live_client::read_bounded(
        &directory.join("gui/native-input.tti"),
        policy().tti.max_bytes,
    )
    .unwrap();
    assert_eq!(drcs_bitmap::hash(&input), INPUT_SHA256);
    let filename = object_name(exported);
    assert_eq!(ready["input"], format!("original/{filename}"));
    assert_eq!(
        live_client::read_bounded(
            &directory.join("original").join(filename),
            policy().tti.max_bytes
        )
        .unwrap(),
        input
    );
    let fresh = &exported
        .files()
        .iter()
        .find(|file| file.name == filename)
        .unwrap()
        .bytes;
    assert_eq!(drcs_bitmap::hash(fresh),INPUT_SHA256,"fresh ORIGINAL export must be byte-identical to actual QTM input; stop instead of normalizing");
    assert_eq!(*fresh, input);
    let saved =
        live_client::read_bounded(&directory.join("gui/edited.tti"), policy().tti.max_bytes)
            .unwrap();
    assert_eq!(drcs_bitmap::hash(&saved), SAVED_SHA256);
    assert_eq!(
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap(),
        saved
    );
    saved
}
fn actual_request(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    saved: &[u8],
    expected: &BodyExpected,
) -> NativeTeletextEditorialImportRequest {
    assert_eq!(receipt.metadata.revision, 1);
    let mut records = parse_teletext_tti(saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(records.len(), expected.records.len());
    for record in &expected.records {
        let index = records
            .iter()
            .position(|actual| actual == record)
            .expect("entire genuine QTM save matches independent original-position target");
        records.swap_remove(index);
    }
    assert!(records.is_empty());
    let mut request = drcs_bitmap::from_receipt(receipt, 1);
    "Reuse genuine QTM interior row in original resource".clone_into(&mut request.name);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == object_name(exported))
        .unwrap();
    saved.clone_into(&mut file.bytes);
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap()
    );
    for file in exported
        .files()
        .iter()
        .filter(|file| file.name != object_name(exported))
    {
        assert_eq!(
            request
                .files
                .iter()
                .find(|actual| actual.name == file.name)
                .unwrap(),
            file
        );
    }
    let uploaded = &request
        .files
        .iter()
        .find(|file| file.name == object_name(exported))
        .unwrap()
        .bytes;
    assert_eq!(uploaded, saved);
    assert_eq!(drcs_bitmap::hash(uploaded), SAVED_SHA256);
    request
}

#[tokio::test]
#[ignore = "requires ERSATZRS_TELETEXT_RESOURCE_EDITOR_DIR pointing to the successful native-editor4 proof root"]
async fn actual_qtm_saved_interior_row_bytes_reuse_original_position_role_and_publish_reopen() {
    let expected = Box::pin(independent_inserted_original()).await;
    let fixture = Fixture::new_persistent().await;
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_outer_rows,
        fixture,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let before = retained(&fixture).await;
    let original =
        decode_teletext_design_documents(&flow.originals[0].document, designs::budget()).unwrap();
    assert!(
        original.editorial_overlay().is_none(),
        "no Add authority or previous editorial intent"
    );
    let exported = &flow.exported[0];
    let saved_bytes = Box::pin(qualified_saved(exported)).await;
    let fresh = receipt(&fixture, exported).await;
    let saved = import(
        &fixture,
        actual_request(exported, &fresh, &saved_bytes, &expected),
    )
    .await;
    assert_eq!(saved.head().revision, 2);
    let stored = Box::pin(support::assert_saved(
        &fixture, &flow, &saved, &expected, true,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    let exported = Box::pin(export_stored(&fixture, &stored)).await;
    assert_eq!(
        file_records(&exported, object_name(&exported)),
        expected.records
    );
    fixture = fixture.reopen().await;
    Box::pin(support::assert_saved(
        &fixture, &flow, &saved, &expected, true,
    ))
    .await;
    unchanged(&fixture, &flow, &before).await;
    Box::pin(support::accept_reopen(
        fixture, &flow, &before, &saved, &expected,
    ))
    .await;
}

//! Pinned editor bytes retain the populated saved Add and its fresh receipt.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use std::path::{Path, PathBuf};

const CASE: &str = "added-definition-interior-row";

pub(super) struct Proof {
    directory: PathBuf,
    ready: Value,
    completion: Value,
    saved_sha256: String,
}
impl Proof {
    pub(super) fn success(&self, concept: &SavedEditorialTeletextDesign) {
        assert_eq!(concept.head().revision, 4);
        editor::json_new(
            &self.directory,
            "success.json",
            &json!({
                "probe_version":1,"case":CASE,"editor":self.completion,
                "ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&self.ready).unwrap()),
                "saved_sha256":self.saved_sha256,"schema_version":18,
                "setup_add_revision":2,"populated_revision":3,"actual_editor_import_revision":4,
                "original_row1_p_and_row3_q_exact":true,"inserted_row2_r_column6":true,
                "entire_saved_companion_records_match_independent_native_original":true,
                "sidecar_sibling_files_and_original_files_immutable":true,
                "complete_native_units_and_unchanged_display_preview_exact":true,
                "fresh_export_and_unpublished_cold_reopen_before_explicit_publication":true,
                "accepted_cold_reopen_and_night_profiles_package_unchanged":true,
                "actual_invoked_display_rendering_claim":false,
                "ordinary_stream_or_client_acceptance_claim":false,
                "full_engine_acceptance_claim":false
            }),
        );
    }
}

fn stage(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
) -> Value {
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    assert_eq!(receipt.metadata.revision, 3);
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
    let records = file_records(exported, name);
    let original_words = [
        POSITION,
        glyph(b'P'),
        0x2b | (4 << 6),
        4 | (9 << 6) | (81 << 11),
        TERMINATION,
    ];
    assert_eq!(
        (3..8)
            .map(|index| word(&records, index))
            .collect::<Vec<_>>(),
        original_words
    );
    let ready = json!({
        "probe_version":1,"case":CASE,"mode":CASE,
        "editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":receipt.metadata.id,"receipt_digest":receipt.metadata.digest,
        "design_revision":3,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&input.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|file|json!({
            "name":file.name,"bytes":file.bytes.len(),"sha256":drcs_bitmap::hash(&file.bytes)
        })).collect::<Vec<_>>(),
        "resource_scope":asset.scope,"page":{"page":256,"subcode":0},
        "editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":{"kind":"resource_body_inserted_row","identity":identity(0),
            "source_commands_before":4,"typed_commands_after":6,
            "triplet":5,"column":6,"row":2,"code":82,
            "original_words":original_words,
            "expected_words":edits(true).iter().map(|(_,word)|*word).collect::<Vec<_>>(),
            "final_termination_word":9,"consumers":[]},
        "save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0",
            "source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898",
            "binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "setup":"HTTP Add2 then native import3 with exact original row1 P and row3 Q",
        "publication":"actual editor import4 remains unpublished until preview and cold reopen"
    });
    editor::json_new(directory, "ready.json", &ready);
    ready
}

fn original_unchanged(directory: &Path, exported: &ExportedOriginalTeletextDesign) {
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
}

pub(super) async fn request(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    expected: &BodyExpected,
) -> (NativeTeletextEditorialImportRequest, Proof) {
    let root = PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh explicit interior-row editor proof root");
    let directory = root.join(CASE);
    std::fs::create_dir(&directory).unwrap();
    let ready = stage(&directory, exported, receipt);
    let completion = serde_json::to_value(editor::completion(&directory).await).unwrap();
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    original_unchanged(&directory, exported);
    let mut records = parse_teletext_tti(&saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(records.len(), expected.records.len());
    for expected in &expected.records {
        let index = records.iter().position(|record| record == expected).expect(
            "actual Save As must match the entire independently encoded inserted-row companion",
        );
        records.swap_remove(index);
    }
    assert!(records.is_empty());
    let mut request = drcs_bitmap::from_receipt(receipt, 3);
    "Actual QTM populated added definition interior row".clone_into(&mut request.name);
    request
        .files
        .iter_mut()
        .find(|file| file.name == object_name(exported))
        .unwrap()
        .bytes
        .clone_from(&saved);
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
    (
        request,
        Proof {
            directory,
            ready,
            completion,
            saved_sha256: drcs_bitmap::hash(&saved),
        },
    )
}

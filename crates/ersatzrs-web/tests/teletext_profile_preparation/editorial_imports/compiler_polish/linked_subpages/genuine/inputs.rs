//! Existing closed-marker protocol and exact native source authority.

use super::*;

fn sidecar(flow: &Flow) -> Vec<u8> {
    write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
}

fn assert_inputs(flow: &Flow, directory: &Path) {
    let original = directory.join("native-original-1");
    assert_eq!(
        std::fs::read_dir(&original).unwrap().count(),
        flow.exported[0].files().len() + 1
    );
    for file in flow.exported[0].files() {
        assert_eq!(
            drcs_bitmap::read_retained_editor_file(&original.join(&file.name)),
            file.bytes
        );
    }
    assert_eq!(
        drcs_bitmap::read_retained_editor_file(&original.join("project.ersatzrs.json")),
        sidecar(flow)
    );
}

pub(super) async fn await_editor(fixture: &Fixture, flow: &Flow, directory: &Path) {
    let original = directory.join("native-original-1");
    std::fs::create_dir(&original).unwrap();
    for file in flow.exported[0].files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar(flow)).unwrap();
    let page = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let files:Vec<_>=flow.exported[0].files().iter().map(|file|json!({"name":file.name,
        "path":format!("native-original-1/{}",file.name),"bytes":file.bytes.len(),"sha256":live_client::hash(&file.bytes)})).collect();
    polish_delivery::write_json(
        directory,
        "actual-ready.json",
        &json!({"probe_version":1,
        "scope":"same live original and receipt; one genuine QTM Save As before import/publication",
        "files":files,"sidecar_file":"native-original-1/project.ersatzrs.json","sidecar_sha256":live_client::hash(&sidecar(flow)),
        "design_id":flow.saved[0].head().id,"receipt_id":flow.exported[0].metadata().id,
        "edits":[{"page":124,"subcode":32,"row":1,"column":7,"before":65,"after":91,
            "original_file":"native-original-1/page-124-0020.tti","original_sha256":live_client::hash(&page.bytes),
            "save_as":"editor-1/124/edited.tti","proof":"editor-1/124/editor-proof.json"}],
        "completion":"editor-complete.json","timeout_seconds":1200}),
    );
    let marker: Value = tokio::time::timeout(
        Duration::from_mins(20),
        resource_bank_admin::marker(directory, "editor-complete.json"),
    )
    .await
    .expect("actual editor must complete within1200 seconds")
    .unwrap();
    assert_eq!(marker["probe_version"], 1);
    let proof =
        live_client::read_bounded(&directory.join("editor-1/124/editor-proof.json"), 16 * 1024)
            .unwrap();
    assert_eq!(marker["proof_sha256"], live_client::hash(&proof));
    assert_original_pointers(fixture, flow).await;
    assert_inputs(flow, directory);
}

pub(super) fn request(
    flow: &Flow,
    directory: &Path,
) -> (NativeTeletextEditorialImportRequest, Value) {
    assert_inputs(flow, directory);
    let name = "page-124-0020.tti";
    let original = &flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    let edited = drcs_bitmap::read_retained_editor_file(&directory.join("editor-1/124/edited.tti"));
    let proof = actual_editor_proof_at(
        &directory.join("editor-1/124/editor-proof.json"),
        original,
        &edited,
    );
    assert_actual_tti_edit(original, &edited, 0x5b);
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = "Actual QTM occupied-role linked-S1 concept".into();
    request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes = edited;
    assert_eq!(request.sidecar, sidecar(flow));
    for file in flow.exported[0]
        .files()
        .iter()
        .filter(|file| file.name != name)
    {
        assert_eq!(
            request
                .files
                .iter()
                .find(|candidate| candidate.name == file.name)
                .unwrap(),
            file
        );
    }
    (request, proof)
}

pub(super) fn hashes(flow: &Flow, directory: &Path) -> Vec<(String, String)> {
    assert_inputs(flow, directory);
    let mut names: Vec<_> = flow.exported[0]
        .files()
        .iter()
        .map(|file| format!("native-original-1/{}", file.name))
        .collect();
    names.extend(
        [
            "native-original-1/project.ersatzrs.json",
            "editor-1/124/edited.tti",
            "editor-1/124/editor-proof.json",
            "editor-complete.json",
        ]
        .map(str::to_owned),
    );
    names
        .into_iter()
        .map(|name| {
            let bytes = drcs_bitmap::read_retained_editor_file(&directory.join(&name));
            (name, live_client::hash(&bytes))
        })
        .collect()
}

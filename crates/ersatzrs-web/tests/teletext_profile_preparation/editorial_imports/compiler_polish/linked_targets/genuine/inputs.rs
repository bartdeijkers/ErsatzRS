//! Three closed saves retain one live export's entire fresh authority.

use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    probe_version: u8,
    proofs: Vec<PageProof>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageProof {
    page: u16,
    sha256: String,
}

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
            live_client::read_bounded(&original.join(&file.name), policy().project.max_file_bytes)
                .unwrap(),
            file.bytes
        );
    }
    assert_eq!(
        live_client::read_bounded(
            &original.join("project.ersatzrs.json"),
            policy().project.max_bytes
        )
        .unwrap(),
        sidecar(flow)
    );
}

fn ready(case: Case, flow: &Flow) -> Value {
    let files: Vec<_> = flow.exported[0]
        .files()
        .iter()
        .map(|file| {
            json!({
        "name":file.name,"path":format!("native-original-1/{}",file.name),
        "bytes":file.bytes.len(),"sha256":live_client::hash(&file.bytes)})
        })
        .collect();
    let edits: Vec<_> = (124..127).map(|page| {
        let name = format!("page-{page}-0020.tti");
        let original = flow.exported[0].files().iter().find(|file|file.name == name).unwrap();
        let mut edit = json!({"page":page,"subcode":32,"row":1,"column":7,"before":65,"after":91,
            "primary":8,"underlined":false,"scope":"normal","s1":1,"slot":0,
            "original_file":format!("native-original-1/{name}"),"original_sha256":live_client::hash(&original.bytes),
            "save_as":format!("editor-1/{page}/edited.tti"),"proof":format!("editor-1/{page}/editor-proof.json")});
        if matches!(case, Case::EnglishTwoPairs) {
            edit["after"] = json!(124);
            edit["primary"] = json!(0);
            edit["slot"] = json!(8);
        }
        edit
    }).collect();
    json!({"probe_version":1,
        "scope":"same owned live fixture, three actual QTM saves before import/publication",
        "design_id":flow.saved[0].head().id,"revision":1,"original_digest":flow.saved[0].digest(),
        "receipt_id":flow.exported[0].metadata().id,"receipt_digest":flow.exported[0].metadata().digest,
        "files":files,"sidecar_file":"native-original-1/project.ersatzrs.json","sidecar_sha256":live_client::hash(&sidecar(flow)),
        "edits":edits,"completion":"editor-complete.json","timeout_seconds":1200,
        "completion_schema":{"probe_version":1,"proofs":[{"page":124,"sha256":"SHA256 of closed per-page proof; exactly124,125,126 in ascending order"}]}})
}

pub(super) async fn await_editor(case: Case, fixture: &Fixture, flow: &Flow, directory: &Path) {
    let original = directory.join("native-original-1");
    std::fs::create_dir(&original).unwrap();
    for file in flow.exported[0].files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar(flow)).unwrap();
    polish_delivery::write_json(directory, "actual-ready.json", &ready(case, flow));
    let marker: Completion = tokio::time::timeout(
        Duration::from_mins(20),
        resource_bank_admin::marker(directory, "editor-complete.json"),
    )
    .await
    .expect("three actual QTM saves must finish within1200 seconds")
    .unwrap();
    validate_completion(&marker);
    assert_original_pointers(fixture, flow).await;
    assert_inputs(flow, directory);
}

fn validate_completion(completed: &Completion) {
    assert_eq!(completed.probe_version, 1);
    assert_eq!(completed.proofs.len(), 3);
    for (page, proof) in (124..127).zip(&completed.proofs) {
        assert_eq!(proof.page, page);
        assert_eq!(proof.sha256.len(), 64);
        assert!(proof.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}

fn editor_proof(
    flow: &Flow,
    directory: &Path,
    closed: &PageProof,
    original: &[u8],
    edited: &[u8],
) -> Value {
    let path = directory.join(format!("editor-1/{}/editor-proof.json", closed.page));
    let bytes = live_client::read_bounded(&path, 16 * 1024).unwrap();
    assert_eq!(live_client::hash(&bytes), closed.sha256);
    let proof = actual_editor_proof_at(&path, original, edited);
    assert_eq!(proof["page"], closed.page);
    let process = &proof["actual_process_receipt"];
    assert_eq!(process["actual_gui_exit"], 0);
    assert_eq!(process["input_sha256_unchanged"], true);
    assert_eq!(process["launch"]["source_mount_readonly"], true);
    let hashes = process["launch"]["input_sha256"].as_object().unwrap();
    assert_eq!(hashes.len(), flow.exported[0].files().len() + 1);
    for file in flow.exported[0].files() {
        assert_eq!(hashes[&file.name], live_client::hash(&file.bytes));
    }
    assert_eq!(
        hashes["project.ersatzrs.json"],
        live_client::hash(&sidecar(flow))
    );
    proof
}

pub(super) fn request(
    case: Case,
    flow: &Flow,
    directory: &Path,
) -> (NativeTeletextEditorialImportRequest, Value) {
    assert_inputs(flow, directory);
    let bytes = live_client::read_bounded(&directory.join("editor-complete.json"), 4096).unwrap();
    let completed: Completion = serde_json::from_slice(&bytes).unwrap();
    validate_completion(&completed);
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = if matches!(case, Case::EnglishTwoPairs) {
        "Actual QTM three linked-target English two-pair concept".into()
    } else {
        "Actual QTM three linked-target Polish concept".into()
    };
    let mut proofs = Vec::new();
    for closed in &completed.proofs {
        let name = format!("page-{}-0020.tti", closed.page);
        let file = request
            .files
            .iter_mut()
            .find(|file| file.name == name)
            .unwrap();
        let edited = live_client::read_bounded(
            &directory.join(format!("editor-1/{}/edited.tti", closed.page)),
            policy().tti.max_bytes,
        )
        .unwrap();
        let proof = editor_proof(flow, directory, closed, &file.bytes, &edited);
        assert_actual_tti_edit(&file.bytes, &edited, case.code());
        file.bytes = edited;
        proofs.push(json!({"page":closed.page,"proof":proof}));
    }
    assert_eq!(request.sidecar, sidecar(flow));
    assert_eq!(request.files.len(), flow.exported[0].files().len());
    for file in flow.exported[0].files() {
        if !flow.exported[0]
            .sidecar()
            .project
            .pages
            .iter()
            .any(|page| page.name == file.name)
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
    }
    (request, json!(proofs))
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
            "actual-ready.json",
            "editor-complete.json",
        ]
        .map(str::to_owned),
    );
    for page in 124..127 {
        names.push(format!("editor-1/{page}/edited.tti"));
        names.push(format!("editor-1/{page}/editor-proof.json"));
    }
    names
        .into_iter()
        .map(|name| {
            let limit = if name.ends_with("editor-proof.json") {
                16 * 1024
            } else if name == "editor-complete.json" {
                4096
            } else if name == "native-original-1/project.ersatzrs.json" {
                policy().project.max_bytes
            } else {
                policy().project.max_file_bytes
            };
            let bytes = live_client::read_bounded(&directory.join(&name), limit).unwrap();
            (name, live_client::hash(&bytes))
        })
        .collect()
}

//! Eight actual saves remain bound to one live fixture's entire fresh export.
//! The reported editor pins require root's separate owned GUI/image inspection.

use super::*;
use serde::Deserialize;
use std::time::Duration;

const EDITOR_TIMEOUT: Duration = Duration::from_mins(80);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Completion {
    probe_version: u8,
    proof_sha256_by_page: Vec<PageProofHash>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageProofHash {
    page: u16,
    sha256: String,
}

fn sidecar(flow: &Flow) -> Vec<u8> {
    write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
}

fn write_inputs(flow: &Flow, directory: &Path) {
    std::fs::create_dir(directory).expect("actual eight-page editor root must be new");
    let original = directory.join("native-original-1");
    std::fs::create_dir(&original).unwrap();
    for file in flow.exported[0].files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar(flow)).unwrap();
}

fn ready(flow: &Flow) -> Value {
    let files: Vec<_> = flow.exported[0]
        .files()
        .iter()
        .map(|file| {
            json!({"name":file.name,"path":format!("native-original-1/{}",file.name),
            "bytes":file.bytes.len(),"sha256":live_client::hash(&file.bytes)})
        })
        .collect();
    let edits: Vec<_> = flow.exported[0]
        .sidecar()
        .project
        .pages
        .iter()
        .map(|mapping| {
            let page = mapping.page.page;
            let original = flow.exported[0]
                .files()
                .iter()
                .find(|file| file.name == mapping.name)
                .unwrap();
            json!({"page":page,"subcode":32,"row":1,"column":7,"before":65,"after":code(page),
            "primary":if slot(page)<2 {8} else {22},"underlined":slot(page)%2==1,
            "scope":if page<128 {"normal"} else {"global"},"slot":slot(page),
            "original_file":format!("native-original-1/{}",mapping.name),
            "original_sha256":live_client::hash(&original.bytes),
            "save_as":format!("editor-1/{page}/edited.tti"),
            "proof":format!("editor-1/{page}/editor-proof.json")})
        })
        .collect();
    json!({"probe_version":1,"scope":"same owned live fixture, eight actual QTM saves before import/publication",
        "design_id":flow.saved[0].head().id,"revision":1,"original_digest":flow.saved[0].digest(),
        "receipt_id":flow.exported[0].metadata().id,"receipt_digest":flow.exported[0].metadata().digest,
        "files":files,"sidecar_file":"native-original-1/project.ersatzrs.json",
        "sidecar_sha256":live_client::hash(&sidecar(flow)),"edits":edits,
        "completion":"editor-complete.json","timeout_seconds":EDITOR_TIMEOUT.as_secs(),
        "completion_schema":{"probe_version":1,"proof_sha256_by_page":[{"page":124,"sha256":"SHA256 of closed per-page editor-proof.json; all eight in ascending page order"}]},
        "publication":"only after exact whole fresh sidecar, all companions, pinned editor/hash and strict fixed-record oracle"})
}

async fn await_editor(fixture: &Fixture, flow: &Flow, directory: &Path) -> Completion {
    let retained = resource_bank_admin::Retained::capture(fixture).await;
    write_inputs(flow, directory);
    polish_delivery::write_json(directory, "actual-ready.json", &ready(flow));
    let completed = tokio::time::timeout(
        EDITOR_TIMEOUT,
        resource_bank_admin::marker::<Completion>(directory, "editor-complete.json"),
    )
    .await;
    assert_original_pointers(fixture, flow).await;
    retained.assert_unchanged(fixture, flow).await;
    completed
        .expect("eight actual QTM saves must finish within 4800 seconds")
        .expect("editor completion must be valid closed JSON no larger than 4KiB")
}

fn assert_fresh_inputs(flow: &Flow, directory: &Path) {
    let original = directory.join("native-original-1");
    assert_eq!(
        std::fs::read_dir(&original).unwrap().count(),
        flow.exported[0].files().len() + 1
    );
    for file in flow.exported[0].files() {
        assert_eq!(drcs_bitmap::read_retained_editor_file(&original.join(&file.name)), file.bytes,
            "all original display and authored companion bytes must equal this live fixture's export");
    }
    assert_eq!(
        drcs_bitmap::read_retained_editor_file(&original.join("project.ersatzrs.json")),
        sidecar(flow),
        "every fresh receipt, installation, original, profile and project field must remain exact"
    );
}

fn editor_proof(
    directory: &Path,
    page: u16,
    original: &[u8],
    edited: &[u8],
    closed_hash: &str,
) -> Value {
    let bytes = live_client::read_bounded(
        &directory.join(format!("editor-1/{page}/editor-proof.json")),
        16 * 1024,
    )
    .unwrap();
    assert_eq!(
        live_client::hash(&bytes),
        closed_hash,
        "completion binds each closed proof"
    );
    let proof: Value = serde_json::from_slice(&bytes).unwrap();
    for (field, expected) in [
        ("editor_name", "QTeletextMaker"),
        ("editor_version", "1.0"),
        (
            "editor_source_commit",
            "cac2167bac933a6160cca45a45d5d4ee95355898",
        ),
        (
            "editor_binary_sha256",
            "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a",
        ),
    ] {
        assert_eq!(proof[field].as_str(), Some(expected));
    }
    assert_eq!(
        proof["original_sha256"].as_str(),
        Some(live_client::hash(original).as_str())
    );
    assert_eq!(
        proof["saved_sha256"].as_str(),
        Some(live_client::hash(edited).as_str())
    );
    proof
}

fn actual_request(
    flow: &Flow,
    directory: &Path,
    completed: &Completion,
) -> (NativeTeletextEditorialImportRequest, Value) {
    assert_fresh_inputs(flow, directory);
    assert_eq!(completed.probe_version, 1);
    assert_eq!(completed.proof_sha256_by_page.len(), 8);
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = "Actual QTM eight-page national dual-scope concept".into();
    let mut proofs = Vec::new();
    for (page, closed) in (124..132).zip(&completed.proof_sha256_by_page) {
        assert_eq!(
            closed.page, page,
            "each exact page occurs once in ascending order"
        );
        let mapping = flow.exported[0]
            .sidecar()
            .project
            .pages
            .iter()
            .find(|mapping| mapping.page.page == page)
            .unwrap();
        let file = request
            .files
            .iter_mut()
            .find(|file| file.name == mapping.name)
            .unwrap();
        let edited = drcs_bitmap::read_retained_editor_file(
            &directory.join(format!("editor-1/{page}/edited.tti")),
        );
        let proof = editor_proof(directory, page, &file.bytes, &edited, &closed.sha256);
        compiler_polish::assert_actual_tti_edit(&file.bytes, &edited, code(page));
        file.bytes = edited;
        proofs.push(json!({"page":page,"proof":proof}));
    }
    assert_eq!(request.sidecar, sidecar(flow));
    assert_eq!(request.files.len(), flow.exported[0].files().len());
    for original in flow.exported[0].files() {
        if !flow.exported[0]
            .sidecar()
            .project
            .pages
            .iter()
            .any(|page| page.name == original.name)
        {
            assert_eq!(request.files.iter().find(|file| file.name==original.name).unwrap(),original,
                "genuine fixture permits only eight display edits; authored companions remain exact");
        }
    }
    (request, json!(proofs))
}

fn input_hashes(flow: &Flow, directory: &Path) -> Vec<(String, String)> {
    let mut names: Vec<_> = flow.exported[0]
        .files()
        .iter()
        .map(|file| format!("native-original-1/{}", file.name))
        .collect();
    names.extend([
        "native-original-1/project.ersatzrs.json".into(),
        "actual-ready.json".into(),
        "editor-complete.json".into(),
    ]);
    for page in 124..132 {
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

#[tokio::test]
#[ignore = "exports one fresh live eight-page original, waits up to 4800 seconds for pinned actual QTM saves, then verifies native concept/publication/reopen; no ordinary/client claim"]
async fn actual_qtm_eight_page_dual_scope_concept_previews_publishes_and_reopens() {
    let directory = PathBuf::from(client_capture::selected("ERSATZRS_QTM_DUAL_SCOPE_DIR"));
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        scope_fixture::response,
        Fixture::new_persistent().await,
        scope_fixture::encoding_policy,
        authoring_metadata::write_compact,
    ))
    .await;
    oracle::assert_originals(&flow);
    let completed = Box::pin(await_editor(&fixture, &flow, &directory)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let hashes = input_hashes(&flow, &directory);
    let (request, proofs) = actual_request(&flow, &directory, &completed);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let preview = Box::pin(assert_restored(&fixture, &flow, &concept)).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(
        Box::pin(assert_restored(&fixture, &flow, &concept)).await,
        preview
    );
    assert_eq!(input_hashes(&flow, &directory), hashes);
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_concept(&flow, day.service());
    assert_eq!(oracle::previews(day.service()), preview);
    compiler_polish::assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    let reopened = recover(&fixture, 1).await;
    assert_concept(&flow, reopened.service());
    assert_eq!(oracle::previews(reopened.service()), preview);
    compiler_polish::assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(input_hashes(&flow, &directory), hashes);
    assert_eq!(
        oracle::previews(recover(&fixture, 2).await.service()),
        oracle::previews(flow.accepted[1].service())
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "native-proof.json",
        &json!({"probe_version":1,
        "scope":"same live fresh export, eight reported pinned QTM saves, native receipt-bound concept/shared preview/explicit publication/reopen",
        "editor_proofs":proofs,"input_hashes":hashes,"design_id":concept.head().id,"revision":concept.head().revision,"digest":concept.digest(),
        "generation_id":day.generation().id,"request_id":day.generation().request.id,
        "original_and_authored_companions_fresh_and_exact":true,"both_generated_banks_all_four_fixed_masks_ptus_and_bindings_verified":true,
        "all_eight_shared_previews_equal_published_and_reopened":true,"night_original_profiles_bindings_package_receipts_config_unchanged":true,
        "ordinary_delivery_proven":false,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false}),
    );
}

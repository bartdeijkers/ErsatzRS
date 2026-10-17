//! Genuine V7 saves retain native call addresses; QTM does not render POP calls.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use ersatzrs_core::repositories::TeletextPublicationMutation;
use std::path::{Path, PathBuf};

const ADD: &str = "v7-native-resource-call-add";
const DELETE: &str = "v7-native-resource-call-delete";

fn assert_staged(directory: &Path, exported: &ExportedOriginalTeletextDesign) {
    assert_eq!(
        live_client::read_bounded(
            &directory.join("original/project.ersatzrs.json"),
            policy().project.max_bytes,
        )
        .unwrap(),
        write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap()
    );
    for file in exported.files() {
        assert_eq!(
            live_client::read_bounded(
                &directory.join("original").join(&file.name),
                policy().project.max_file_bytes,
            )
            .unwrap(),
            file.bytes
        );
    }
}

fn stage(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    from: &Expected,
    target: &Expected,
    native_selector: u32,
) -> Value {
    std::fs::create_dir(directory).expect("fresh actual V7 native call editor stage");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    let asset = selected(&exported.sidecar().project);
    let file = exported
        .files()
        .iter()
        .find(|f| f.name == asset.name)
        .unwrap();
    let input = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    assert_eq!(input.records(), from.fragment);
    let original_words = words(input.records());
    let expected_words = words(&target.fragment);
    let expected_editor = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(target.fragment.clone(), policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    live_client::write_new(&directory.join("expected-editor.tti"), &expected_editor).unwrap();
    let edit = json!({
        "kind":"native_resource_fragment_calls","action":if target.state == Calls::Added {"insert"} else {"delete"},
        "identity":seed::target(20),"target":seed::target(21),"object_type":"passive",
        "asset_scope":asset.scope,"source_command":CALL,
        "from_calls":from.state.offsets().len(),"to_calls":target.state.offsets().len(),
        "original_words":original_words,"expected_words":expected_words,
        "original_padded_words":original_words.len().div_ceil(13)*13,
        "expected_padded_words":expected_words.len().div_ceil(13)*13,
        "native_selector":native_selector,
        "native_selector_tuple":{"address":native_selector&63,"mode":(native_selector>>6)&31,"data":native_selector>>11},
        "origin_word":0x28|(16<<6),"consumers":[124,125,126],
        "protected_context":true,"qtm_nested_render_claim":false
    });
    let ready = json!({
        "probe_version":1,"case":if target.state == Calls::Added {ADD} else {DELETE},
        "editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":receipt.metadata.id,"receipt_digest":receipt.metadata.digest,
        "input":format!("original/{}",asset.name),"input_sha256":drcs_bitmap::hash(&file.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|f|json!({"name":f.name,"bytes":f.bytes.len(),"sha256":drcs_bitmap::hash(&f.bytes)})).collect::<Vec<_>>(),
        "protected_files":exported.files().iter().filter(|f|f.name!=asset.name).map(|f|f.name.as_str()).collect::<Vec<_>>(),
        "page":{"page":256,"subcode":0},"editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":edit,"expected_editor":"expected-editor.tti","expected_editor_sha256":drcs_bitmap::hash(&expected_editor),
        "save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898",
            "binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "publication":"unchanged actual save imports as concept; reopen before separate explicit publication"
    });
    editor::json_new(directory, "ready.json", &ready);
    ready
}

fn saved_request(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    saved: &[u8],
    target: &Expected,
) -> NativeTeletextEditorialImportRequest {
    assert_staged(directory, exported);
    let mut records = parse_teletext_tti(saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(records.len(), target.fragment.len());
    for expected in &target.fragment {
        let index = records.iter().position(|r| r == expected).expect(
            "actual QTM save must match independently compiled complete native fragment target",
        );
        records.swap_remove(index);
    }
    assert!(records.is_empty(), "only record ordering may differ");
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let sidecar = request.sidecar.clone();
    let originals = request.files.clone();
    let name = &selected(&exported.sidecar().project).name;
    request
        .files
        .iter_mut()
        .find(|f| &f.name == name)
        .unwrap()
        .bytes = saved.to_vec();
    assert_eq!(request.sidecar, sidecar);
    for (original, upload) in originals.iter().zip(&request.files) {
        assert_eq!(original.name, upload.name);
        if &original.name != name {
            assert_eq!(original.bytes, upload.bytes);
        }
    }
    request
}

struct EditorOutput {
    ready: Value,
    completion: Value,
    saved: Vec<u8>,
    concept: SavedEditorialTeletextDesign,
}
async fn run_stage(
    fixture: &Fixture,
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    transition: (&Expected, &Expected),
    native_selector: u32,
) -> EditorOutput {
    let (from, target) = transition;
    // Synthetic rewriting qualifies the target only. Its bytes never become an upload.
    let expectation = request(exported, receipt, head, from, target, native_selector);
    let name = &selected(&exported.sidecar().project).name;
    let expected_file = expectation.files.iter().find(|f| &f.name == name).unwrap();
    assert_eq!(
        parse_teletext_tti(&expected_file.bytes, policy().tti)
            .unwrap()
            .records(),
        target.fragment,
    );
    let ready = stage(directory, exported, receipt, from, target, native_selector);
    let completion = serde_json::to_value(editor::completion(directory).await).unwrap();
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let concept = import(
        fixture,
        saved_request(directory, exported, receipt, head, &saved, target),
    )
    .await;
    assert_eq!(concept.head().revision, head + 1);
    EditorOutput {
        ready,
        completion,
        saved,
        concept,
    }
}

async fn publish_current(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    current: &AcceptedTeletextOutputCandidate,
) -> AcceptedTeletextOutputCandidate {
    let mutation = request_original_teletext_design_publication(
        &fixture.state.application,
        concept.head().id,
        concept.head().revision,
        concept.digest().to_owned(),
        concept.head().revision,
        Some(current.generation().request.id),
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(pending) = mutation else {
        panic!("explicit current-head publication must admit")
    };
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        Some(current.generation().clone())
    );
    generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: pending.request().clone(),
                expected_design_head: concept.head().revision,
                expected_generation_id: Some(current.generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    )
}

async fn publish_reopen(
    mut fixture: Fixture,
    flow: &Flow,
    output: &EditorOutput,
    target: &Expected,
    current: &AcceptedTeletextOutputCandidate,
) -> (Fixture, AcceptedTeletextOutputCandidate) {
    Box::pin(assert_concept(&fixture, flow, &output.concept, target)).await;
    assert_pointers(&fixture, current, &flow.accepted[1]).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, flow, &output.concept, target)).await;
    assert_pointers(&fixture, current, &flow.accepted[1]).await;
    let accepted = Box::pin(publish_current(&fixture, &output.concept, current)).await;
    assert_eq!(
        pixels(accepted.service(), target.state.occupied()),
        target.pixels
    );
    assert_units(accepted.service(), target);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, flow, &output.concept, target)).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    let recovered = recover(&fixture, 1).await;
    generations::assert_same_output(&recovered, &accepted);
    assert_units(recovered.service(), target);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    (fixture, accepted)
}

#[tokio::test]
#[ignore = "requires pinned headed QTM V7 native call driver and a fresh explicit proof directory"]
async fn actual_qtm_v7_native_fragment_call_add_delete_import_preview_publish_reopen() {
    let root = PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_V7_CALL_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh actual V7 native call proof root");
    let added = Box::pin(qualify(Calls::Added)).await;
    let removed = Box::pin(qualify(Calls::Removed)).await;
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original,
        Fixture::new_persistent().await,
        fragments::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let original = expected(&flow, Calls::Original);
    let native_selector = selector(&flow);
    assert_ne!(original.pixels, added.pixels);
    assert_ne!(original.pixels, removed.pixels);
    let first = Box::pin(run_stage(
        &fixture,
        &root.join(ADD),
        &flow.exported[0],
        &flow.receipts[0],
        1,
        (&original, &added),
        native_selector,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    let (fixture, day) = Box::pin(publish_reopen(
        fixture,
        &flow,
        &first,
        &added,
        &flow.accepted[0],
    ))
    .await;
    let exported = drcs_bitmap::reexport(&fixture, &first.concept).await;
    let latest = receipt(&fixture, &exported).await;
    let second = Box::pin(run_stage(
        &fixture,
        &root.join(DELETE),
        &exported,
        &latest,
        2,
        (&added, &removed),
        native_selector,
    ))
    .await;
    let (mut fixture, _) = Box::pin(publish_reopen(fixture, &flow, &second, &removed, &day)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_staged(&root.join(ADD), &flow.exported[0]);
    assert_staged(&root.join(DELETE), &exported);
    for (case, output) in [(ADD, &first), (DELETE, &second)] {
        assert_eq!(
            live_client::read_bounded(&root.join(case).join("edited.tti"), policy().tti.max_bytes)
                .unwrap(),
            output.saved
        );
    }
    assert!(fixture.playout.try_recv().is_err());
    editor::json_new(
        &root,
        "success.json",
        &json!({
            "probe_version":1,"add_editor":first.completion,"delete_editor":second.completion,
            "add_ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&first.ready).unwrap()),
            "delete_ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&second.ready).unwrap()),
            "add_saved_sha256":drcs_bitmap::hash(&first.saved),"delete_saved_sha256":drcs_bitmap::hash(&second.saved),
            "schema_version":17,"add_revision":2,"delete_revision":3,
            "all_three_shared_previews_and_all_native_units_exact":true,
            "both_concepts_reopened_before_explicit_publication":true,
            "both_explicit_publications_accepted_and_cold_reopened":true,
            "other_channel_and_original_catalogue_unchanged":true,
            "qtm_nested_render_claim":false,"ordinary_stream_or_client_acceptance_claim":false
        }),
    );
    fixture.state.pool.close().await;
}

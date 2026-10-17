//! One actual pinned Save As combines glyph/style shape and a resource call.
use super::super::topology::structure as native;
use super::*;

const CASE: &str = "existing-gpop-call-insert-structure";

fn stage(directory: &Path, flow: &Flow, target: &native::Expected) -> Value {
    std::fs::create_dir(directory).expect("fresh actual compound resource stage");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let exported = &flow.exported[0];
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    let expected = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(target.records().to_vec(), policy().tti)
            .unwrap(),
        policy().tti,
    )
    .unwrap();
    live_client::write_new(&directory.join("expected-editor.tti"), &expected).unwrap();
    let before = mixed::meaningful(&editor_records(exported));
    let after = mixed::meaningful(target.records());
    assert_eq!((before.len(), after.len()), (22, 26));
    assert_eq!(target.program().commands().len(), 8);
    assert_eq!(
        &before[3..9],
        &[
            [41, 4, 8],
            [8, 12, 65],
            [8, 9, 65],
            [10, 12, 0],
            [40, 16, 0],
            [40, 19, 18]
        ]
    );
    assert_eq!(
        &after[3..13],
        &[
            [41, 4, 8],
            [8, 12, 65],
            [8, 12, 67],
            [8, 9, 66],
            [9, 9, 65],
            [10, 12, 0],
            [40, 16, 0],
            [40, 19, 22],
            [40, 16, 1],
            [40, 19, 22]
        ]
    );
    assert_eq!((after[13], after[19]), ([40, 22, 16], [40, 23, 22]));
    let name = companion(exported);
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let edit = json!({"kind":"resource_call_topology_structure","action":"insert",
        "identity":seed::target(20),"target":seed::target(21),"object_type":"passive",
        "source_command":4,"style_source_command":1,"glyph_source_command":2,
        "original_pair_words":[7,8],"retained_pair_words":[9,10],"added_pair_words":[11,12],
        "style_original_word":4,"style_inserted_word":5,"style_before":[8,12,65],"style_after":[8,12,67],
        "inserted_glyph_word":6,"inserted_glyph":[8,9,66],"moved_glyph_word":7,"moved_glyph":[9,9,65],
        "from_calls":1,"to_calls":2,"model_before":before.len(),"model_after":after.len(),
        "original_words":before,"expected_words":after,"protected_context":true,"consumers":[124,125,126]});
    let ready = json!({"probe_version":1,"case":CASE,"editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&file.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|file|json!({"name":file.name,"bytes":file.bytes.len(),
            "sha256":drcs_bitmap::hash(&file.bytes)})).collect::<Vec<_>>(),
        "protected_files":exported.files().iter().filter(|file|file.name!=name).map(|file|file.name.as_str()).collect::<Vec<_>>(),
        "expected_file":"expected-editor.tti","expected_sha256":drcs_bitmap::hash(&expected),
        "page":{"page":256,"subcode":0},"editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":edit,"save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898",
            "binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "publication":"import unpublished concept and reopen before separate explicit publication"});
    editor::json_new(directory, "ready.json", &ready);
    ready
}

async fn run_actual(
    fixture: &Fixture,
    flow: &Flow,
    directory: &Path,
    target: &native::Expected,
) -> EditorOutput {
    let ready = stage(directory, flow, target);
    let completion = serde_json::to_value(editor::completion(directory).await).unwrap();
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let concept = import(
        fixture,
        saved_request(
            directory,
            &flow.exported[0],
            &flow.receipts[0],
            1,
            &saved,
            target.records(),
        ),
    )
    .await;
    assert_eq!(concept.head().revision, 2);
    EditorOutput {
        ready,
        completion,
        saved,
        concept,
    }
}

#[tokio::test]
#[ignore = "requires pinned headed QTM compound glyph/style/call driver and fresh explicit proof directory"]
async fn actual_qtm_resource_glyph_style_structure_and_call_insert_publish_and_reopen() {
    let root = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_STRUCTURE_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh actual compound resource proof root");
    let target = Box::pin(native::qualify(true)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        native::original,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let directory = root.join(CASE);
    let output = Box::pin(run_actual(&fixture, &flow, &directory, &target)).await;
    Box::pin(native::assert_concept(
        &fixture,
        &flow,
        &output.concept,
        &target,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(native::assert_concept(
        &fixture,
        &flow,
        &output.concept,
        &target,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &output.concept).await;
    assert_eq!(preview(day.service()), target.pixels());
    native::assert_units(day.service(), &target);
    fixture = fixture.reopen().await;
    Box::pin(native::assert_concept(
        &fixture,
        &flow,
        &output.concept,
        &target,
    ))
    .await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_staged(&directory, &flow.exported[0]);
    assert_eq!(
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap(),
        output.saved
    );
    assert!(fixture.playout.try_recv().is_err());
    editor::json_new(
        &root,
        "success.json",
        &json!({"probe_version":1,"editor":output.completion,
        "ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&output.ready).unwrap()),
        "saved_sha256":drcs_bitmap::hash(&output.saved),"schema_version":17,"concept_revision":2,
        "actual_compound_glyph_style_structure_and_call_save":true,
        "literal_enlarged_b_and_added_p":true,"all_three_native_previews_exact":true,
        "all_native_page_units_equal_independent_target":true,"unpublished_concept_reopened":true,
        "explicit_publication_accepted_then_cold_reopened":true,
        "original_catalogue_profile_package_other_channel_unchanged":true,
        "ordinary_stream_or_client_acceptance_claim":false}),
    );
    fixture.state.pool.close().await;
}

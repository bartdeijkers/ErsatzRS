//! Two actual Save As operations add a call, then restore the immutable root.
use super::*;
use ersatzrs_core::repositories::TeletextPublicationMutation;

const INSERT: &str = "existing-gpop-call-pair-insert";
const DELETE: &str = "existing-gpop-call-pair-delete";

fn extra_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original_response(theme, page, subcode);
    let active = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|o| o["identity"]["object_id"] == 20)
        .unwrap();
    let commands = active["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 5);
    let mut extra = commands[4].clone();
    extra["offset"]["column"] = json!(1);
    commands.push(extra);
    value
}
fn preview(service: &EncodedTeletextAuthoredService) -> Vec<TeletextRaster> {
    (124..127)
        .map(|p| external_editor_probe::preview_page(service, p))
        .collect()
}
fn editor_records(exported: &ExportedOriginalTeletextDesign) -> Vec<TeletextTtiRecord> {
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
    let file = exported
        .files()
        .iter()
        .find(|f| f.name == companion(exported))
        .unwrap();
    parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec()
}
async fn qualified_extra() -> (Vec<TeletextTtiRecord>, Vec<TeletextRaster>) {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        extra_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let program = &flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(20))
        .unwrap()
        .program;
    assert_eq!(program.commands().len(), 6);
    assert_eq!(
        program.commands()[5],
        TeletextEnhancementCommand::InvokeObject {
            target: seed::target(21),
            object_type: TeletextObjectType::Passive,
            offset: Some(ersatzrs_domain::TeletextObjectOffset::new(0, 1).unwrap()),
        }
    );
    let result = (
        editor_records(&flow.exported[0]),
        preview(flow.accepted[0].service()),
    );
    fixture.state.pool.close().await;
    result
}
fn assert_added_p(flow: &Flow, pixels: &[TeletextRaster]) {
    let mask = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    for (page, raster) in (124..127).zip(pixels) {
        let before = external_editor_probe::preview_page(flow.accepted[0].service(), page);
        assert_eq!((raster.width(), raster.height()), (672, 250));
        for y in 0..250 {
            for x in 0..672 {
                if (192..204).contains(&x) && (50..60).contains(&y) {
                    let expected = if mask[y - 50] & (1 << (11 - (x - 192))) != 0 {
                        [0, 15, 0]
                    } else {
                        [0, 0, 0]
                    };
                    assert!(
                        matches!(raster.pixel(x,y),Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) if rgb.get()==expected)
                    );
                } else {
                    assert_eq!(
                        raster.pixel(x, y),
                        before.pixel(x, y),
                        "outside new shared Passive P"
                    );
                }
            }
        }
    }
}

fn stage_export(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    insert: bool,
) -> Value {
    std::fs::create_dir(directory).expect("fresh fixed call topology editor stage");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    let name = companion(exported);
    let file = exported.files().iter().find(|f| f.name == name).unwrap();
    let ready = json!({"probe_version":1,"case":if insert {INSERT} else {DELETE},
        "editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":receipt.metadata.id,"receipt_digest":receipt.metadata.digest,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&file.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|f|json!({"name":f.name,"bytes":f.bytes.len(),"sha256":drcs_bitmap::hash(&f.bytes)})).collect::<Vec<_>>(),
        "protected_files":exported.files().iter().filter(|f|f.name!=name).map(|f|f.name.as_str()).collect::<Vec<_>>(),
        "page":{"page":256,"subcode":0},"editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":{"kind":"resource_call_topology","action":if insert {"insert"} else {"delete"},
            "identity":seed::target(20),"target":seed::target(21),"object_type":"passive","source_command":4,
            "original_pair_words":[7,8],"added_pair_words":[9,10],"editor_word":7,"address":40,"mode":16,
            "from_calls":if insert {1} else {2},"to_calls":if insert {2} else {1},
            "model_before":if insert {21} else {23},"model_after":if insert {23} else {21},
            "protected_context":true,"consumers":[124,125,126]},
        "save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898",
            "binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "publication":"import and reopen first; separate explicit publication; deletion is not GUI Undo"});
    editor::json_new(directory, "ready.json", &ready);
    ready
}
fn assert_staged(directory: &Path, exported: &ExportedOriginalTeletextDesign) {
    assert_eq!(
        live_client::read_bounded(
            &directory.join("original/project.ersatzrs.json"),
            policy().project.max_bytes
        )
        .unwrap(),
        write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap()
    );
    for file in exported.files() {
        assert_eq!(
            live_client::read_bounded(
                &directory.join("original").join(&file.name),
                policy().project.max_file_bytes
            )
            .unwrap(),
            file.bytes
        );
    }
}
fn saved_request(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    saved: &[u8],
    expected: &[TeletextTtiRecord],
) -> NativeTeletextEditorialImportRequest {
    assert_staged(directory, exported);
    let mut records = parse_teletext_tti(saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(records.len(), expected.len());
    for expected in expected {
        let index = records.iter().position(|record| record == expected).expect(
            "genuine saved records must match independently compiled complete native target",
        );
        records.swap_remove(index);
    }
    assert!(records.is_empty());
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let sidecar = request.sidecar.clone();
    let original_files = request.files.clone();
    let name = companion(exported);
    request
        .files
        .iter_mut()
        .find(|f| f.name == name)
        .unwrap()
        .bytes = saved.to_vec();
    assert_eq!(request.sidecar, sidecar);
    for (before, after) in original_files.iter().zip(&request.files) {
        assert_eq!(before.name, after.name);
        if before.name != name {
            assert_eq!(before.bytes, after.bytes);
        }
    }
    request
}

async fn check_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    added: bool,
    pixels: &[TeletextRaster],
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let document = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let original = flow.accepted[0].service();
    let changes = if added {
        let overlay = document.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 17);
        assert!(overlay.pages().is_empty());
        assert!(overlay.local_program_operands().is_empty());
        assert!(overlay.local_object_operands().is_empty());
        assert!(overlay.local_object_glyph_program_overlays().is_empty());
        assert!(overlay.resource_object_operands().is_empty());
        assert!(overlay.drcs_bitmap_overlays().is_empty());
        let changes = overlay.resource_object_program_overlays();
        assert_eq!(changes.len(), 1);
        let change = &changes[0];
        assert_eq!(change.identity(), seed::target(20));
        let before = &original
            .declared()
            .semantic()
            .objects()
            .definition(seed::target(20))
            .unwrap()
            .program;
        assert_eq!(change.expected(), before);
        let mut expected = before.commands().to_vec();
        expected.push(TeletextEnhancementCommand::InvokeObject {
            target: seed::target(21),
            object_type: TeletextObjectType::Passive,
            offset: Some(ersatzrs_domain::TeletextObjectOffset::new(0, 1).unwrap()),
        });
        assert_eq!(change.replacement().commands(), expected);
        assert_eq!(
            change.retained_commands(),
            [Some(0), Some(1), Some(2), Some(3), Some(4), None]
        );
        changes.to_vec()
    } else {
        assert!(document.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
        vec![]
    };
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let root = flow.saved[0].candidate();
    assert_eq!(restored.selection(), root.selection());
    assert_eq!(restored.metadata(), root.metadata());
    assert_eq!(restored.transmissions(), root.transmissions());
    assert_eq!(restored.sources().len(), root.sources().len());
    for (source, before) in restored.sources().iter().zip(root.sources()) {
        assert_eq!(source.bytes(), before.bytes());
        assert_eq!(source.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        root.producers()[0].original()
    );
    let expected = if added {
        original
            .declared()
            .clone()
            .with_resource_object_program_overlays(&changes, policy().encoding.validation.service)
            .unwrap()
    } else {
        original.declared().clone()
    };
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(preview(service).as_slice(), pixels);
    assert_eq!(service.magazine_units(), original.magazine_units());
    for (before, after) in original.pages().iter().zip(service.pages()) {
        if before
            .resource_header()
            .is_none_or(|h| Some(h.identity()) != seed::target(20).resource())
        {
            assert_eq!(before.data_units(), after.data_units());
        }
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
    expected: &[TeletextTtiRecord],
    insert: bool,
) -> EditorOutput {
    let ready = stage_export(directory, exported, receipt, insert);
    let completion = serde_json::to_value(editor::completion(directory).await).unwrap();
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let head = if insert { 1 } else { 2 };
    let concept = import(
        fixture,
        saved_request(directory, exported, receipt, head, &saved, expected),
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
async fn complete_undo(
    mut fixture: Fixture,
    flow: &Flow,
    inserted: &SavedEditorialTeletextDesign,
    root: &Path,
    day: &AcceptedTeletextOutputCandidate,
) -> (Fixture, EditorOutput, ExportedOriginalTeletextDesign) {
    let original_records = editor_records(&flow.exported[0]);
    let original_pixels = translated_pixels(flow.accepted[0].service(), 0);
    let exported = drcs_bitmap::reexport(&fixture, inserted).await;
    let latest = receipt(&fixture, &exported).await;
    let output = Box::pin(run_stage(
        &fixture,
        &root.join(DELETE),
        &exported,
        &latest,
        &original_records,
        false,
    ))
    .await;
    Box::pin(check_concept(
        &fixture,
        flow,
        &output.concept,
        false,
        &original_pixels,
    ))
    .await;
    assert_pointers(&fixture, day, &flow.accepted[1]).await;
    fixture = fixture.reopen().await;
    Box::pin(check_concept(
        &fixture,
        flow,
        &output.concept,
        false,
        &original_pixels,
    ))
    .await;
    assert_pointers(&fixture, day, &flow.accepted[1]).await;
    let accepted = Box::pin(publish_current(&fixture, &output.concept, day)).await;
    assert_eq!(preview(accepted.service()), original_pixels);
    fixture = fixture.reopen().await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    (fixture, output, exported)
}

#[tokio::test]
#[ignore = "requires pinned headed QTM call insertion/deletion driver and fresh explicit proof directory"]
async fn actual_qtm_resource_call_pair_insert_delete_root_undo_and_explicit_publications() {
    let root = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_CALL_TOPOLOGY_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh actual call topology proof root");
    let (expected, pixels) = Box::pin(qualified_extra()).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_added_p(&flow, &pixels);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let insertion = root.join(INSERT);
    let first = Box::pin(run_stage(
        &fixture,
        &insertion,
        &flow.exported[0],
        &flow.receipts[0],
        &expected,
        true,
    ))
    .await;
    Box::pin(check_concept(
        &fixture,
        &flow,
        &first.concept,
        true,
        &pixels,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(check_concept(
        &fixture,
        &flow,
        &first.concept,
        true,
        &pixels,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &first.concept).await;
    assert_eq!(preview(day.service()), pixels);
    let (mut fixture, second, exported) =
        Box::pin(complete_undo(fixture, &flow, &first.concept, &root, &day)).await;
    let deletion = root.join(DELETE);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_staged(&insertion, &flow.exported[0]);
    assert_staged(&deletion, &exported);
    assert_eq!(
        live_client::read_bounded(&insertion.join("edited.tti"), policy().tti.max_bytes).unwrap(),
        first.saved
    );
    assert_eq!(
        live_client::read_bounded(&deletion.join("edited.tti"), policy().tti.max_bytes).unwrap(),
        second.saved
    );
    assert!(fixture.playout.try_recv().is_err());
    editor::json_new(
        &root,
        "success.json",
        &json!({"probe_version":1,
        "insert_editor":first.completion,"delete_editor":second.completion,
        "insert_ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&first.ready).unwrap()),
        "delete_ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&second.ready).unwrap()),
        "insert_saved_sha256":drcs_bitmap::hash(&first.saved),"delete_saved_sha256":drcs_bitmap::hash(&second.saved),
        "schema_version":17,"insert_revision":2,"root_undo_revision":3,"all_three_shared_previews_exact":true,
        "root_undo_document_byte_exact":true,"both_concepts_reopened_before_explicit_publication":true,
        "both_explicit_publications_accepted":true,"other_channel_and_original_catalogue_unchanged":true,
        "gui_undo_used":false,"ordinary_stream_or_client_acceptance_claim":false}),
    );
    fixture.state.pool.close().await;
}

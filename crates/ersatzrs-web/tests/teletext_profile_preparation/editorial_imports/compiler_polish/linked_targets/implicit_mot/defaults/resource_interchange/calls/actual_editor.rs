//! Genuine pinned Origin Modifier Save As bytes, not an editor simulation.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use std::path::Path;

const CASE: &str = "original-gpop-call-origin-column-context";

fn word(records: &[TeletextTtiRecord], index: usize) -> u32 {
    let mut words = records.iter().filter_map(|record| match record {
        TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } if usize::from(*designation) == index / 13 => Some(words[index % 13]),
        _ => None,
    });
    let result = words.next().unwrap();
    assert!(words.next().is_none());
    result
}

fn stage(directory: &Path, flow: &Flow) -> Value {
    std::fs::create_dir(directory).expect("fresh resource call editor case");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let exported = &flow.exported[0];
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
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
    let records = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    assert_eq!(word(records.records(), 7), 0x28 | (0x10 << 6));
    let invocation = word(records.records(), 8);
    assert_eq!((invocation >> 6) & 31, 19);
    let ready = json!({"probe_version":1,"case":CASE,"editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&file.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|f|json!({"name":f.name,"bytes":f.bytes.len(),"sha256":drcs_bitmap::hash(&f.bytes)})).collect::<Vec<_>>(),
        "protected_files":exported.files().iter().filter(|f|f.name!=name).map(|f|f.name.as_str()).collect::<Vec<_>>(),
        "page":{"page":0x100,"subcode":0},"editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":{"kind":"resource_call_origin","identity":seed::target(20),"source_command":4,
            "target":seed::target(21),"object_type":"passive","editor_word":7,"mode":16,"address":40,
            "from":0,"to":1,"invocation_word":8,"invocation_value":invocation,"invocation_unchanged":true,"protected_context":true,"consumers":[124,125,126]},
        "save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898","binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "publication":"unpublished concept and restart first; separate explicit request and acceptance afterwards"});
    editor::json_new(directory, "ready.json", &ready);
    ready
}

fn assert_original_files(directory: &Path, flow: &Flow) {
    let original = directory.join("original");
    assert_eq!(
        live_client::read_bounded(
            &original.join("project.ersatzrs.json"),
            policy().project.max_bytes
        )
        .unwrap(),
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
    );
    for file in flow.exported[0].files() {
        assert_eq!(
            live_client::read_bounded(&original.join(&file.name), policy().project.max_file_bytes)
                .unwrap(),
            file.bytes
        );
    }
}

fn actual_request(
    directory: &Path,
    flow: &Flow,
    saved: &[u8],
    target: &[TeletextTtiRecord],
) -> NativeTeletextEditorialImportRequest {
    assert_original_files(directory, flow);
    let mut actual = parse_teletext_tti(saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(actual.len(), target.len());
    for expected in target {
        let index = actual
            .iter()
            .position(|record| record == expected)
            .expect("actual saved record must match independently compiled full call target");
        actual.swap_remove(index);
    }
    assert!(actual.is_empty());
    let name = companion(&flow.exported[0]);
    let original = &flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    let before = parse_teletext_tti(original, policy().tti).unwrap();
    let after = parse_teletext_tti(saved, policy().tti).unwrap();
    assert_eq!(word(before.records(), 7), 0x28 | (0x10 << 6));
    assert_eq!(word(after.records(), 7), 0x28 | (0x10 << 6) | (1 << 11));
    assert_eq!(word(before.records(), 8), word(after.records(), 8));
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    "Actual QTM contextual GPOP call origin column".clone_into(&mut request.name);
    request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes = saved.to_vec();
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(flow.exported[0].sidecar(), policy().project).unwrap()
    );
    for original in flow.exported[0]
        .files()
        .iter()
        .filter(|file| file.name != name)
    {
        assert_eq!(
            request
                .files
                .iter()
                .find(|file| file.name == original.name)
                .unwrap(),
            original
        );
    }
    assert_eq!(
        &request
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes,
        saved
    );
    request
}

async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    pixels: &[TeletextRaster],
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 16);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.local_object_glyph_program_overlays().is_empty());
    assert!(overlay.resource_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.resource_object_program_overlays().len(), 1);
    let change = &overlay.resource_object_program_overlays()[0];
    assert_eq!(change.identity(), seed::target(20));
    assert!(change.has_invocation_changes());
    let definition = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(20))
        .unwrap();
    assert_eq!(change.expected(), &definition.program);
    let mut expected = definition.program.commands().to_vec();
    expected[4] = TeletextEnhancementCommand::InvokeObject {
        target: seed::target(21),
        object_type: TeletextObjectType::Passive,
        offset: Some(ersatzrs_domain::TeletextObjectOffset::new(0, 1).unwrap()),
    };
    assert_eq!(change.replacement().commands(), expected);
    assert_eq!(
        change.retained_commands(),
        [Some(0), Some(1), Some(2), Some(3), Some(4)]
    );
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (source, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(source.bytes(), before.bytes());
        assert_eq!(source.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    assert_eq!(
        translated_pixels(restored.encoded().unwrap(), 1).as_slice(),
        pixels
    );
}

#[tokio::test]
#[ignore = "requires pinned headed QTM Origin Modifier driver and a fresh explicit proof directory"]
async fn actual_qtm_resource_context_origin_save_import_preview_publish_and_two_reopens() {
    let root = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_CONTEXT_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh resource call editor proof root");
    let directory = root.join(CASE);
    let (target, pixels) = Box::pin(qualified_target()).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original_response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_ne!(translated_pixels(flow.accepted[0].service(), 0), pixels);
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
    Box::pin(assert_concept(&fixture, &flow, &concept, &pixels)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &concept, &pixels)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(translated_pixels(accepted.service(), 1), pixels);
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    Box::pin(assert_concept(&fixture, &flow, &concept, &pixels)).await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_original_files(&directory, &flow);
    assert_eq!(
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap(),
        saved
    );
    assert!(fixture.playout.try_recv().is_err());
    editor::json_new(
        &directory,
        "success.json",
        &json!({"probe_version":1,"case":CASE,
        "editor":completion,"ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&ready).unwrap()),
        "saved_sha256":drcs_bitmap::hash(&saved),"schema_version":16,"concept_revision":2,
        "all_three_shared_consumers_exact":true,"import_did_not_publish":true,
        "concept_reopen_before_explicit_publication":true,"explicit_publication_and_second_reopen":true,
        "protected_artifacts_and_other_channel_unchanged":true,"ordinary_stream_or_client_acceptance_claim":false}),
    );
    fixture.state.pool.close().await;
}

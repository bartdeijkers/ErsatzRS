//! Opt-in genuine pinned QTM Save As of one authenticated resource definition.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use std::path::Path;

const CASE: &str = "original-pop-foreground";

fn companion(flow: &Flow) -> &str {
    let resource = protected_file(&flow.exported[0], TeletextResourceKindWire::Pop, 0xc0);
    &flow.exported[0]
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope, TeletextExportAssetScope::ObjectTtiV1 {
            resource_name, object_id: 21, editor_page: 0x100, editor_ordinal: 0,
        } if resource_name == resource)
        })
        .unwrap()
        .name
}

fn edit(flow: &Flow, bytes: &[u8]) -> Value {
    let target = original_resource_operand(flow).identity();
    let original = flow.accepted[0]
        .service()
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(page)
                if page.header().identity() == target.resource().unwrap() =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let mut positions = original.command_positions().iter().filter(|position| {
        position.source().object == Some(target) && position.source().command == 2
    });
    let position = positions.next().unwrap();
    assert!(positions.next().is_none());
    let parsed = parse_teletext_tti(bytes, policy().tti).unwrap();
    let words: Vec<_> = parsed
        .records()
        .iter()
        .flat_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } => words
                .iter()
                .enumerate()
                .filter_map(|(slot, word)| {
                    (*word == 0x1001).then_some(usize::from(*designation) * 13 + slot)
                })
                .collect(),
            _ => vec![],
        })
        .collect();
    assert_eq!(words.len(), 1);
    json!({"kind":"resource_foreground", "identity":target,"source_command":2,
        "native_first_word":position.first_word(),"native_semantic_word":position.semantic_word(),
        "semantic_word":words[0],"column":1,"mode":0,"from":2,"to":3,
        "expected_word":0x1001,"replacement_word":0x1801,"consumers":[124,125,126]})
}

fn stage(directory: &Path, flow: &Flow) -> Value {
    std::fs::create_dir(directory).expect("fresh editor case directory");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let exported = &flow.exported[0];
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    let name = companion(flow);
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let ready = json!({"probe_version":1,"case":CASE,"editor_profile":exported.sidecar().project.editor_profile,
        "design_id":flow.saved[0].head().id,"design_revision":1,"design_digest":flow.saved[0].digest(),
        "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&file.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|file|json!({"name":file.name,
            "bytes":file.bytes.len(),"sha256":drcs_bitmap::hash(&file.bytes)})).collect::<Vec<_>>(),
        "page":{"page":0x100,"subcode":0},"editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":edit(flow,&file.bytes),"save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0",
            "source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898",
            "binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "publication":"import is an unpublished concept; explicit publication follows native validation"});
    editor::json_new(directory, "ready.json", &ready);
    ready
}

fn actual_request(
    directory: &Path,
    flow: &Flow,
    saved: &[u8],
) -> NativeTeletextEditorialImportRequest {
    let exported = &flow.exported[0];
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
    let name = companion(flow);
    let oracle = resource_request(&flow.receipts[0], 1, 2, 3);
    let expected = &oracle
        .files
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    let mut actual = parse_teletext_tti(saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let expected = parse_teletext_tti(expected, policy().tti).unwrap();
    assert_eq!(actual.len(), expected.records().len());
    for record in expected.records() {
        let index = actual
            .iter()
            .position(|candidate| candidate == record)
            .expect(
            "actual Save As must retain every exact record except the qualified foreground word",
        );
        actual.swap_remove(index);
    }
    assert!(actual.is_empty());
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    "Actual pinned QTM original POP foreground".clone_into(&mut request.name);
    request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes = saved.to_vec();
    request
}

#[tokio::test]
#[ignore = "requires the pinned headed QTM resource controller and a fresh explicit proof directory"]
async fn actual_qtm_resource_foreground_save_as_import_preview_publish_reopen() {
    let root = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("proof root must be create-new");
    let directory = root.join(CASE);
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let ready = stage(&directory, &flow);
    let completion = editor::completion(&directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let concept = import(&fixture, actual_request(&directory, &flow, &saved)).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    let previews = Box::pin(assert_resource_concept(&fixture, &flow, &concept, 3)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_resource_concept(&fixture, &flow, &concept, 3)).await,
        previews
    );
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        (124..127)
            .map(|page| external_editor_probe::preview_page(accepted.service(), page))
            .collect::<Vec<_>>(),
        previews
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    editor::json_new(
        &directory,
        "success.json",
        &json!({"probe_version":1,"case":CASE,
        "editor":completion,"ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&ready).unwrap()),
        "saved_sha256":drcs_bitmap::hash(&saved),"concept_revision":2,"schema_version":14,
        "all_three_shared_consumers_exact":true,"protected_artifacts_and_other_channel_unchanged":true,
        "import_did_not_publish":true,"explicit_publication_and_reopen":true,
        "ordinary_stream_or_client_acceptance_claim":false}),
    );
    fixture.state.pool.close().await;
}

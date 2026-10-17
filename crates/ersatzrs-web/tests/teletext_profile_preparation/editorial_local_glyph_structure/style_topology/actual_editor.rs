//! Actual pinned widget Save As to an unpublished exact-original style concept.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use std::path::{Path, PathBuf};

const CASE: &str = "local-style-structure-zone";

fn metadata(flow: &Flow) -> Value {
    let mut value = super::super::actual_editor_trial::metadata(flow);
    let fields = value.as_object_mut().unwrap();
    fields.remove("deleted_original_command");
    fields.remove("repositioned_original_command");
    value["kind"] = json!("local_style_structure");
    value["replacement_commands"] = json!(inserted_words()
        .iter()
        .map(|word| [word & 63, (word >> 6) & 31, word >> 11])
        .collect::<Vec<_>>());
    value["expected_retained_commands"] = json!(expected(&styles()[..1]).retained_commands());
    value["following_definition"]["replacement_marker"] = json!(22);
    value["following_definition"]["replacement_object_number"] = json!(number(22));
    value["insert_copy"] = json!({"source_command":5,"semantic_word":17,
        "original_tuple":[1,1,33],"mode_changed_tuple":[1,0,1],"saved_tuple":[1,0,19]});
    value
}
fn prepared_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    request_body_words(
        &flow.receipts[0],
        1,
        flow.accepted[0].service(),
        &glyph_words(0),
        inserted_words(),
    )
}
fn assert_saved(flow: &Flow, saved: &[u8]) {
    let expected = prepared_request(flow);
    let name = page_name(&expected);
    let expected = &expected
        .files
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    let packets = |bytes: &[u8]| {
        let mut records = parse_teletext_tti(bytes, policy().tti)
            .unwrap()
            .records()
            .iter()
            .filter_map(|record| match record {
                TeletextTtiRecord::Packet {
                    row,
                    designation,
                    words,
                } => Some((*row, *designation, *words)),
                _ => None,
            })
            .collect::<Vec<_>>();
        records.sort_unstable();
        records
    };
    assert_eq!(packets(saved), packets(expected), "actual Save As preserves exact original packets and one inserted style with relocated marker/pointer");
}
async fn setup() -> (Fixture, Flow) {
    Box::pin(setup_with_fixture_policy(
        zone,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await
}

#[tokio::test]
async fn prepared_local_style_editor_handoff_has_exact_compiled_words_and_original_packets() {
    let (fixture, flow) = Box::pin(setup()).await;
    let edit = metadata(&flow);
    assert_eq!(
        edit["original_semantic_words"],
        json!((12..21).collect::<Vec<_>>())
    );
    assert_eq!(edit["following_definition"]["original_marker"], 21);
    assert_eq!(edit["following_definition"]["replacement_marker"], 22);
    assert_eq!(edit["insert_copy"]["saved_tuple"], json!([1, 0, 19]));
    let request = prepared_request(&flow);
    let name = page_name(&request);
    assert_saved(
        &flow,
        &request
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes,
    );
    fixture.state.pool.close().await;
}

async fn run(directory: &Path) {
    let (mut fixture, flow) = Box::pin(setup()).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let ready = editor::inputs_for_edit(directory, CASE, &flow, || metadata(&flow));
    let marker = editor::completion(directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    assert_saved(&flow, &saved);
    let concept = import(&fixture, editor::actual_request(directory, &flow, &saved)).await;
    assert_eq!(
        (concept.head().id, concept.head().revision),
        (flow.saved[0].head().id, 2)
    );
    let expected = expected(&styles()[..1]);
    let preview = Box::pin(super::assert_concept(&fixture, &flow, &concept, &expected)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(super::assert_concept(&fixture, &flow, &concept, &expected)).await,
        preview
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    let revision = drcs_bitmap::stored(&fixture, &concept).await;
    editor::json_new(
        directory,
        "success.json",
        &json!({"probe_version":1,"case":CASE,
        "editor":marker,"receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
        "original_digest":flow.originals[0].digest,"revision":2,"digest":concept.digest(),"schema_version":13,
        "concept_document_sha256":drcs_bitmap::hash(&revision.document),"saved_sha256":drcs_bitmap::hash(&saved),
        "edit":ready["edit"],"native_preview_geometry":[672,250],"native_raster_unchanged":true,
        "exact_style_intent_and_relocated_pointers":true,"reopen_verified":true,"originals_and_other_channel_unchanged":true,
        "publication":"unchanged","ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
    );
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires fresh pinned headed QTM1.0 style Save As via ERSATZRS_TELETEXT_STYLE_EDITOR_DIR"]
async fn actual_qtm_local_style_structure_save_as_v13_concept_preview_reopen() {
    let root = PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_STYLE_EDITOR_DIR",
    ));
    assert!(
        root.is_absolute(),
        "editor proof path must be an explicit absolute input"
    );
    std::fs::create_dir(&root).expect("actual editor proof root must be create-new");
    Box::pin(run(&root.join(CASE))).await;
}

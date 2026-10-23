//! Reuse the pinned actual-editor handshake and complete native lifecycle.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as qtm;
use std::path::Path;

fn words(flow: &Flow) -> Vec<u32> {
    let file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let mut result = Vec::new();
    for record in parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
    {
        if let TeletextTtiRecord::Packet { row: 26, words, .. } = record {
            result.extend_from_slice(words);
        }
    }
    let end = result.iter().position(|word| *word == 12287).unwrap();
    assert!(result[end..].iter().all(|word| *word == 12287));
    result.truncate(end + 1);
    result
}

pub(super) async fn edit(
    directory: &Path,
    flow: &Flow,
    target: &Flow,
    expected: NativeTeletextEditorialImportRequest,
) -> NativeTeletextEditorialImportRequest {
    let before = words(flow);
    let after = words(target);
    assert_eq!(before.len(), after.len());
    assert!(before.len() <= 208);
    qtm::inputs_for_layout(
        directory,
        "single-page-complete-main-context-greek",
        flow,
        [4, 12, 13],
        || {
            json!({"kind":"complete_main_charset_interleaved_base",
            "before_designation":87,"after_designation":55,
            "consumer_visual_columns":[12,13],"consumer_row":6,
            "original_words":before,"expected_words":after})
        },
    );
    let name = "page-124-0020.tti";
    let wanted = expected
        .files
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    live_client::write_new(&directory.join("expected-edited.tti"), &wanted.bytes).unwrap();
    let _ = qtm::completion(directory).await;
    let bytes =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let actual = qtm::actual_request(directory, flow, &bytes);
    assert_eq!(actual.sidecar, expected.sidecar);
    for original in &expected.files {
        let saved = actual
            .files
            .iter()
            .find(|file| file.name == original.name)
            .unwrap();
        if saved.name == name {
            let parsed = parse_teletext_tti(&saved.bytes, policy().tti).unwrap();
            let expected = parse_teletext_tti(&original.bytes, policy().tti).unwrap();
            // Only the previously qualified QTM grouping of unchanged FL/X28.
            for group in 0..3 {
                let selected = |record: &&TeletextTtiRecord| match record {
                    TeletextTtiRecord::FastLinks(_) => group == 0,
                    TeletextTtiRecord::Packet { row: 28, .. } => group == 1,
                    _ => group == 2,
                };
                assert_eq!(
                    parsed.records().iter().filter(selected).collect::<Vec<_>>(),
                    expected
                        .records()
                        .iter()
                        .filter(selected)
                        .collect::<Vec<_>>()
                );
            }
        } else {
            assert_eq!(saved, original);
        }
    }
    assert_eq!(actual.files.len(), expected.files.len());
    actual
}

fn units(service: &EncodedTeletextAuthoredService) -> Value {
    json!({"owner":{"page":124,"subcode":32},
        "display_units":service.pages().iter().flat_map(EncodedTeletextAuthoredPage::data_units)
            .map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "magazine_units":service.magazine_units().iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()})
}

pub(super) fn published(
    directory: &Path,
    original: &EncodedTeletextAuthoredService,
    wanted: &EncodedTeletextAuthoredService,
    published: &EncodedTeletextAuthoredService,
) {
    qtm::json_new(
        directory,
        "editor-published-units.json",
        &json!({
        "probe_version":1,"complete_catalogue":true,
        "originalBefore":units(original),"editorSaved":units(wanted),
        "newPublished":units(published)}),
    );
}

pub(super) fn completed(directory: &Path, restored: &EncodedTeletextAuthoredService) {
    let mut capture: Value = serde_json::from_slice(
        &live_client::read_bounded(
            &directory.join("editor-published-units.json"),
            policy().project.max_bytes,
        )
        .unwrap(),
    )
    .unwrap();
    capture["undoRestored"] = units(restored);
    assert_eq!(capture["originalBefore"], capture["undoRestored"]);
    assert_eq!(capture["editorSaved"], capture["newPublished"]);
    qtm::json_new(directory, "emitted-units.json", &capture);
    qtm::json_new(
        directory,
        "success.json",
        &json!({"probe_version":1,
        "actual_editor_concept_publication_and_exact_root_undo":true,
        "complete_catalogue_default_local_identities":true,
        "ordinary_stream_client_proven":false}),
    );
}

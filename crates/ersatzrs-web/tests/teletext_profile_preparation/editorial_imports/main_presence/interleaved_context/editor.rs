//! Pinned editor handoff and actual published-unit capture for both contexts.
use super::super::super::local_object_glyphs::actual_editor as qtm;
use super::*;

fn words(expected: &checks::Expected) -> Vec<u32> {
    let mut words: Vec<_> = expected
        .packets
        .iter()
        .flat_map(|record| match record {
            TeletextTtiRecord::Packet { row: 26, words, .. } => words.to_vec(),
            _ => unreachable!(),
        })
        .collect();
    assert!(words.len() > expected.body_words);
    assert!(words[expected.body_words..]
        .iter()
        .all(|word| *word == 0x3fff));
    words.truncate(expected.body_words + 1);
    words
}

fn assert_actual(
    request: &NativeTeletextEditorialImportRequest,
    expected: &NativeTeletextEditorialImportRequest,
    target: &checks::Expected,
) {
    assert_eq!(request.sidecar, expected.sidecar);
    assert_eq!(request.files.len(), expected.files.len());
    for wanted in &expected.files {
        let actual = &request
            .files
            .iter()
            .find(|file| file.name == wanted.name)
            .unwrap()
            .bytes;
        if wanted.name != target.name {
            assert_eq!(actual, &wanted.bytes);
            continue;
        }
        let a = parse_teletext_tti(actual, policy().tti).unwrap();
        let b = parse_teletext_tti(&wanted.bytes, policy().tti).unwrap();
        let packets: Vec<_> = a
            .records()
            .iter()
            .filter(|record| matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
            .cloned()
            .collect();
        assert_eq!(packets, target.packets);
        // Permit only the already qualified QTM grouping of unchanged records.
        for group in 0..3 {
            let selected = |record: &&TeletextTtiRecord| {
                let actual = match record {
                    TeletextTtiRecord::FastLinks(_) => 0,
                    TeletextTtiRecord::Packet { row: 28, .. } => 1,
                    _ => 2,
                };
                actual == group
            };
            assert_eq!(
                a.records().iter().filter(selected).collect::<Vec<_>>(),
                b.records().iter().filter(selected).collect::<Vec<_>>()
            );
        }
    }
}

pub(super) async fn edit(
    directory: &std::path::Path,
    flow: &Flow,
    proposed: &NativeTeletextEditorialImportRequest,
    expected: (&checks::Expected, &checks::Expected),
    designation: u8,
) -> NativeTeletextEditorialImportRequest {
    let (original, target) = expected;
    let original_words = words(original);
    let expected_words = words(target);
    assert_eq!(original_words.len(), 7);
    assert_eq!(expected_words.len(), if designation == 0 { 5 } else { 7 });
    let mode = if designation == 0 {
        "single-page-main-context-latin"
    } else {
        "single-page-main-context-greek"
    };
    qtm::inputs_for_layout(directory, mode, flow, [4, 12, 13], || {
        json!({
            "kind":"main_charset_interleaved_base",
            "before_designation":87,"after_designation":designation,
            "original_words":original_words,"expected_words":expected_words,
            "consumer_visual_columns":[12,13]
        })
    });
    let target_file = proposed
        .files
        .iter()
        .find(|file| file.name == target.name)
        .unwrap();
    live_client::write_new(&directory.join("expected-edited.tti"), &target_file.bytes).unwrap();
    let _ = qtm::completion(directory).await;
    let bytes =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let actual = qtm::actual_request(directory, flow, &bytes);
    assert_actual(&actual, proposed, target);
    actual
}

pub(super) async fn published(
    directory: &std::path::Path,
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    let restored = Box::pin(drcs_bitmap::restored(fixture, saved)).await;
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(accepted.service())
    );
    assert_eq!(
        restored.encoded().unwrap().magazine_units(),
        accepted.service().magazine_units()
    );
    qtm::json_new(
        directory,
        "editor-published-units.json",
        &json!({
            "probe_version":1,"editor_saved_revision":saved.head().revision,
            "originalBefore":encoded_units(flow.accepted[0].service()),
            "editorSaved":encoded_units(restored.encoded().unwrap()),
            "newPublished":encoded_units(accepted.service())
        }),
    );
}

pub(super) async fn completed(
    directory: &std::path::Path,
    fixture: &Fixture,
    flow: &Flow,
    undone: &SavedEditorialTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    let restored = Box::pin(drcs_bitmap::restored(fixture, undone)).await;
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(flow.accepted[0].service())
    );
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(accepted.service())
    );
    let bytes = live_client::read_bounded(
        &directory.join("editor-published-units.json"),
        policy().tti.max_bytes,
    )
    .unwrap();
    let mut capture: Value = serde_json::from_slice(&bytes).unwrap();
    capture["undoRestored"] = encoded_units(accepted.service());
    qtm::json_new(directory, "emitted-units.json", &capture);
    qtm::json_new(
        directory,
        "success.json",
        &json!({
            "probe_version":1,"editor_saved_revision":2,"unpublished_repeat_revision":3,
            "root_undo_revision":undone.head().revision,"exact_root_document":true,
            "explicit_publication_current_cas":true,"ordinary_stream_client_proven":false
        }),
    );
}

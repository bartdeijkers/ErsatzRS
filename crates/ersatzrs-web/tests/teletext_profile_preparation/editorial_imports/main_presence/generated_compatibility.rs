//! First authored Main must preserve source-owned compatibility enhancements.
use super::*;

#[path = "generated_compatibility/editor.rs"]
mod editor;

fn with_generated_base(mut value: Value) -> Value {
    // Latin supplementary Ł cannot be represented by the original national G0.
    // It is protected source content on row1, outside the free row2 Main edit.
    value["service"]["pages"][0]["rows"][0]["runs"][0]["content"]["text"] = json!("Ł");
    value
}

fn original(theme: &str, page: u16, subcode: u16) -> Value {
    with_generated_base(editable_absent(theme, page, subcode))
}

fn authored(theme: &str, page: u16, subcode: u16) -> Value {
    with_generated_base(created(theme, page, subcode))
}

fn main_words(bytes: &[u8]) -> Vec<TeletextTtiRecord> {
    parse_teletext_tti(bytes, policy().tti)
        .unwrap()
        .records()
        .iter()
        .filter(|record| matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
        .cloned()
        .collect()
}

#[tokio::test]
async fn first_main_preserves_generated_base_words_and_original_source_identity() {
    Box::pin(run(None)).await;
}

#[tokio::test]
#[ignore = "requires an actual pinned headed QTM1.0 generated-Base first-Main save in fresh ERSATZRS_TELETEXT_GENERATED_MAIN_EDITOR_DIR"]
async fn actual_qtm_generated_base_first_main_save_import_repeat_publish_exact_undo_and_reopen() {
    let directory = std::path::PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_GENERATED_MAIN_EDITOR_DIR",
    ));
    Box::pin(run(Some(&directory))).await;
}

async fn run(editor: Option<&std::path::Path>) {
    let target = ExpectedMain::prepare(authored).await;
    let (fixture, flow) = setup_with_response(original).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert!(program(flow.accepted[0].service()).is_none());
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let sidecar = request.sidecar.clone();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == target.name)
        .unwrap();
    let baseline_words = main_words(&file.bytes);
    assert!(
        !baseline_words.is_empty(),
        "fixture must contain generated X26"
    );
    let final_words = main_words(&target.target_file);
    assert!(!final_words.is_empty());
    assert_ne!(baseline_words, final_words);
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .iter()
        .filter(|record| !matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
        .cloned()
        .collect::<Vec<_>>();
    let insertion = records
        .iter()
        .position(|record| matches!(record, TeletextTtiRecord::FastLinks(_)))
        .unwrap_or(records.len());
    records.splice(insertion..insertion, final_words);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    let request = if let Some(directory) = editor {
        editor::edit(directory, &flow, &request, &target.target_program).await
    } else {
        request
    };
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await;
    let TeletextDesignMutation::Saved(concept) = result
        .expect("first Main must preserve generated Base without adopting it as authored intent")
    else {
        panic!("first Main must save an unpublished concept");
    };
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    let service = restored.encoded().unwrap();
    assert_eq!(program(service), Some(&target.target_program));
    assert_eq!(pixels(service), target.target_pixels);
    assert_eq!(designs::units(service), target.units);
    assert_eq!(service.magazine_units(), target.magazine_units);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.is_empty());
    Box::pin(finish(fixture, flow, target, concept, retained, editor)).await;
}

async fn finish(
    mut fixture: Fixture,
    flow: Flow,
    target: ExpectedMain,
    concept: SavedEditorialTeletextDesign,
    retained: resource_bank_admin::Retained,
    editor: Option<&std::path::Path>,
) {
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(program(accepted.service()), Some(&target.target_program));
    assert_eq!(pixels(accepted.service()), target.target_pixels);
    assert_eq!(designs::units(accepted.service()), target.units);
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    let pending = fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap();
    let changed = repeat_created(
        &fixture,
        &concept,
        &target.name,
        &target.target_program,
        &target.target_pixels,
    )
    .await;
    let undone = undo_created(&fixture, &flow, &changed, &target.name, pending.as_ref()).await;
    let restored = drcs_bitmap::restored(&fixture, &undone).await;
    assert!(program(restored.encoded().unwrap()).is_none());
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(flow.accepted[0].service())
    );
    // The compatibility packets remain after restoring genuine Main absence.
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == target.name)
        .unwrap();
    assert!(!main_words(&file.bytes).is_empty());
    fixture = fixture.reopen().await;
    assert!(program(
        drcs_bitmap::restored(&fixture, &undone)
            .await
            .encoded()
            .unwrap()
    )
    .is_none());
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        pending
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(2)
            .await
            .unwrap(),
        Some(flow.pending[1].clone())
    );
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    if let Some(directory) = editor {
        emitted_units(directory, &fixture, &flow, &accepted, &undone).await;
        completion(directory, &concept, &changed, &undone, &accepted);
    }
    assert!(fixture.playout.is_empty());
    fixture.state.pool.close().await;
}

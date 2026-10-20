//! Authored charset edits must not adopt generated Base restoration commands.
use super::*;
use ersatzrs_domain::{TeletextDeclaredService, TeletextEnhancementProgram};
use ersatzrs_ffmpeg::teletext::{TeletextBaseCellCommandKind, TeletextMainCompositionSource};
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
};

#[path = "interleaved_context/checks.rs"]
mod checks;

#[path = "interleaved_context/editor.rs"]
mod editor;

type Response = fn(&str, u16, u16) -> Value;

fn response(theme: &str, page: u16, subcode: u16, designation: u8, code: u8) -> Value {
    let mut value = editable_absent(theme, page, subcode);
    value["service"]["pages"][0]["rows"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "row":2,"runs":[{"column":8,
            "style":{"foreground":"white","background":"black","height":"normal",
                "boxed":false,"flash":false,"conceal":false},
            "content":{"kind":"text","text":"é"}}]
        }));
    value["service"]["page_enhancements"][0]["local_program"] = json!({"commands":[
        {"kind":"set_position","position":{"row":2,"column":0}},
        {"kind":"at_column","column":0,"operation":{"kind":"designate_charset","designation":designation}},
        {"kind":"at_column","column":9,"operation":{"kind":"g0","code":code}}
    ]});
    value
}
fn root(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0x57, b'A')
}
fn edited(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0x37, b'A')
}
fn repeated(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0x37, b'B')
}
fn edited_latin(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0, b'A')
}
fn repeated_latin(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, 0, b'B')
}

fn request(
    receipt: &TeletextDesignExportReceipt,
    target: &checks::Expected,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, receipt.metadata.revision);
    let original = request.files.clone();
    let sidecar = request.sidecar.clone();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == target.name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .iter()
        .filter(|record| !matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
        .cloned()
        .collect::<Vec<_>>();
    let position = records
        .iter()
        .position(|record| matches!(record, TeletextTtiRecord::FastLinks(_)))
        .unwrap_or(records.len());
    records.splice(position..position, target.packets.iter().cloned());
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    for before in original.iter().filter(|file| file.name != target.name) {
        assert_eq!(
            request.files.iter().find(|file| file.name == before.name),
            Some(before)
        );
    }
    request
}

async fn reject_wrong_restore(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    target: &checks::Expected,
    accepted: Option<&AcceptedTeletextOutputCandidate>,
    mixed_base_row: bool,
) {
    let mut invalid = request(receipt, target);
    let file = invalid
        .files
        .iter_mut()
        .find(|file| file.name == target.name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut changed = 0;
    for record in &mut records {
        if let TeletextTtiRecord::Packet { row: 26, words, .. } = record {
            for word in words {
                if *word == 8 | (8 << 6) | (0x57 << 11) {
                    *word = 8 | (8 << 6) | (0x37 << 11);
                    changed += 1;
                }
            }
        }
    }
    assert_eq!(
        changed, 1,
        "changed authored setter57 must not accept derived restoration37"
    );
    if mixed_base_row {
        let row = records
            .iter_mut()
            .find_map(|record| match record {
                TeletextTtiRecord::BaseRow { row: 2, cells } => Some(cells),
                _ => None,
            })
            .unwrap();
        assert_eq!(row[12], b' ');
        row[12] = b'X';
    }
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        invalid,
        policy(),
        designs::deadline(),
    )
    .await;
    let Err(NativeTeletextEditorialImportError::Tti(error)) = result else {
        panic!("mismatched generated restoration must fail TTI validation, not admission");
    };
    assert!(
        matches!(
            error.failure,
            NativeTeletextEditorialTtiFailure::Packet
                | NativeTeletextEditorialTtiFailure::Structure
        ),
        "{error:?}"
    );
    assert_eq!((error.page, error.subcode), (124, 32));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(saved.head().id)
            .await
            .unwrap(),
        Some(saved.head().clone())
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(saved.head().id, 3, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    if let Some(accepted) = accepted {
        checks::published(fixture, accepted, &flow.accepted[1]).await;
    } else {
        assert_original_pointers(fixture, flow).await;
    }
}

async fn undo_and_publish(
    mut fixture: Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    expected: (&checks::Expected, &checks::Expected),
    retained: &resource_bank_admin::Retained,
    editor: Option<(&std::path::Path, AcceptedTeletextOutputCandidate)>,
) {
    let (expected, original_expected) = expected;
    let (accepted, directory) = if let Some((directory, accepted)) = editor {
        (accepted, Some(directory))
    } else {
        let accepted = checks::publish_after(&fixture, saved, &flow.accepted[0]).await;
        checks::service(accepted.service(), expected);
        (accepted, None)
    };
    fixture = fixture.reopen().await;
    checks::published(&fixture, &accepted, &flow.accepted[1]).await;
    let exported = drcs_bitmap::reexport(&fixture, saved).await;
    let receipt = receipt(&fixture, &exported).await;
    let mut undo = drcs_bitmap::from_receipt(&receipt, 3);
    let original = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == expected.name)
        .unwrap();
    undo.files
        .iter_mut()
        .find(|file| file.name == expected.name)
        .unwrap()
        .bytes
        .clone_from(&original.bytes);
    let undone = import(&fixture, undo).await;
    assert_eq!(undone.head().revision, 4);
    Box::pin(checks::saved(&fixture, flow, &undone, original_expected)).await;
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    checks::published(&fixture, &accepted, &flow.accepted[1]).await;
    let restored = checks::publish_after(&fixture, &undone, &accepted).await;
    checks::service(restored.service(), original_expected);
    fixture = fixture.reopen().await;
    Box::pin(checks::saved(&fixture, flow, &undone, original_expected)).await;
    checks::published(&fixture, &restored, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, flow).await;
    assert_retained_storage(&fixture, flow).await;
    assert!(fixture.playout.is_empty());
    if let Some(directory) = directory {
        Box::pin(editor::completed(
            directory, &fixture, flow, &undone, &restored,
        ))
        .await;
    }
    fixture.state.pool.close().await;
}

async fn reject_restore_variants(
    fixture: &Fixture,
    flow: &Flow,
    saved: &SavedEditorialTeletextDesign,
    latest: &TeletextDesignExportReceipt,
    original: &checks::Expected,
    accepted: Option<&AcceptedTeletextOutputCandidate>,
    designation: u8,
) {
    for mixed_row in [false, true]
        .into_iter()
        .take(if designation == 0x37 { 2 } else { 1 })
    {
        Box::pin(reject_wrong_restore(
            fixture, flow, saved, latest, original, accepted, mixed_row,
        ))
        .await;
    }
}

#[tokio::test]
async fn authored_main_charset_recompiles_interleaved_base_restore_repeat_and_exact_undo() {
    Box::pin(run(0x37, None)).await;
}

#[tokio::test]
async fn authored_main_charset_repacking_preserves_base_repeat_and_exact_root_undo() {
    Box::pin(run(0, None)).await;
}

#[tokio::test]
#[ignore = "requires pinned headed QTM1.0 and fresh ERSATZRS_TELETEXT_MAIN_CONTEXT_GREEK_DIR"]
async fn actual_qtm_main_context_greek_edit_publish_repeat_exact_undo_and_reopen() {
    let directory = std::path::PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_MAIN_CONTEXT_GREEK_DIR",
    ));
    Box::pin(run(0x37, Some(&directory))).await;
}

#[tokio::test]
#[ignore = "requires pinned headed QTM1.0 and fresh ERSATZRS_TELETEXT_MAIN_CONTEXT_LATIN_DIR"]
async fn actual_qtm_main_context_latin_edit_publish_repeat_exact_undo_and_reopen() {
    let directory = std::path::PathBuf::from(super::super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_MAIN_CONTEXT_LATIN_DIR",
    ));
    Box::pin(run(0, Some(&directory))).await;
}

async fn run(designation: u8, editor: Option<&std::path::Path>) {
    let (edit, repeat_source): (Response, Response) = match designation {
        0x37 => (edited, repeated),
        0 => (edited_latin, repeated_latin),
        _ => panic!("only the two qualified context-dependency boundaries"),
    };
    let original = Box::pin(checks::qualify(root, 0x57, b'A')).await;
    let target = Box::pin(checks::qualify(edit, designation, b'A')).await;
    let repeat = Box::pin(checks::qualify(repeat_source, designation, b'B')).await;
    checks::unchanged_base(&original, &target);
    checks::unchanged_base(&original, &repeat);
    assert_eq!(
        original.body_words,
        target.body_words + usize::from(designation == 0) * 2,
        "Latin context removes only generated charset and restoration words"
    );
    assert_eq!(target.body_words, repeat.body_words);
    let (fixture, flow) = Box::pin(setup_with_response(root)).await;
    checks::service(flow.accepted[0].service(), &original);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let proposed = request(&flow.receipts[0], &target);
    let proposed = if let Some(directory) = editor {
        Box::pin(editor::edit(
            directory,
            &flow,
            &proposed,
            (&original, &target),
            designation,
        ))
        .await
    } else {
        proposed
    };
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        proposed,
        policy(),
        designs::deadline(),
    )
    .await;
    let TeletextDesignMutation::Saved(saved)=result.expect("authored Main charset must recompile its interleaved Base restore without adopting generated commands") else {
        panic!("valid existing-Main context edit must save an unpublished concept");
    };
    assert_eq!(saved.head().revision, 2);
    Box::pin(checks::saved(&fixture, &flow, &saved, &target)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let published_editor = if let Some(directory) = editor {
        let accepted = Box::pin(checks::publish_after(&fixture, &saved, &flow.accepted[0])).await;
        checks::service(accepted.service(), &target);
        Box::pin(editor::published(
            directory, &fixture, &flow, &saved, &accepted,
        ))
        .await;
        Some(accepted)
    } else {
        None
    };
    let exported = drcs_bitmap::reexport(&fixture, &saved).await;
    let latest = receipt(&fixture, &exported).await;
    Box::pin(reject_restore_variants(
        &fixture,
        &flow,
        &saved,
        &latest,
        &original,
        published_editor.as_ref(),
        designation,
    ))
    .await;
    let repeated = import(&fixture, request(&latest, &repeat)).await;
    assert_eq!(repeated.head().revision, 3);
    Box::pin(checks::saved(&fixture, &flow, &repeated, &repeat)).await;
    if let Some(accepted) = published_editor.as_ref() {
        checks::published(&fixture, accepted, &flow.accepted[1]).await;
    } else {
        assert_original_pointers(&fixture, &flow).await;
    }
    retained.assert_unchanged(&fixture, &flow).await;
    Box::pin(undo_and_publish(
        fixture,
        &flow,
        &repeated,
        (&repeat, &original),
        &retained,
        editor.zip(published_editor),
    ))
    .await;
}

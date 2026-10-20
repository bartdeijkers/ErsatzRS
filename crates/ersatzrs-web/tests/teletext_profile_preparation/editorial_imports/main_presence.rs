//! A newly authored Main is distinct from an original absent or empty program.
use super::*;
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use serde_json::{json, Value};

#[path = "main_presence/generated_compatibility.rs"]
mod generated_compatibility;

fn absent(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = authoring_metadata::response(theme, page, subcode);
    value["service"]["page_enhancements"][0]["local_program"] = Value::Null;
    value
}

// A new foreground setter (including its white reset) owns the complete row
// tail. This captured template grants that exact row; partial-row imports below
// remain negative rather than borrowing default-writer authority at equal pixels.
fn editable_absent(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = absent(theme, page, subcode);
    value["authoring"]["pages"][0]["regions"][0]["column"] = json!(0);
    value["authoring"]["pages"][0]["regions"][0]["width"] = json!(40);
    value
}

fn created(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = editable_absent(theme, page, subcode);
    value["service"]["page_enhancements"][0]["local_program"] = json!({"commands":[
        {"kind":"set_position","position":{"row":2,"column":8}},
        {"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":18}},
        {"kind":"at_column","column":8,"operation":{"kind":"g0","code":81}},
        {"kind":"at_column","column":9,"operation":{"kind":"foreground","colour":7}}
    ]});
    value
}

fn program(
    service: &EncodedTeletextAuthoredService,
) -> Option<&ersatzrs_domain::TeletextEnhancementProgram> {
    service.declared().semantic().page_enhancements()[&channel_documents::address(124, 32)]
        .local_program
        .as_ref()
}

fn pixels(service: &EncodedTeletextAuthoredService) -> TeletextRaster {
    let preview = project_teletext_preview(
        service,
        channel_documents::address(124, 32),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    preview.raster().clone()
}

fn replace_packets(request: &mut NativeTeletextEditorialImportRequest, name: &str, target: &[u8]) {
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let target = parse_teletext_tti(target, policy().tti).unwrap();
    let packets: Vec<_> = target
        .records()
        .iter()
        .filter(|record| matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
        .cloned()
        .collect();
    assert!(!packets.is_empty());
    assert!(!records
        .iter()
        .any(|record| matches!(record, TeletextTtiRecord::Packet { row: 26, .. })));
    let position = records
        .iter()
        .position(|record| matches!(record, TeletextTtiRecord::FastLinks(_)))
        .unwrap_or(records.len());
    records.splice(position..position, packets);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
}

#[tokio::test]
async fn absent_main_can_be_authored_as_a_concept_without_changing_original_or_broadcast() {
    Box::pin(run(None)).await;
}

#[tokio::test]
#[ignore = "requires an actual pinned headed QTM1.0 first-Main save in fresh ERSATZRS_TELETEXT_MAIN_PRESENCE_DIR"]
async fn actual_qtm_first_main_save_import_repeat_publish_exact_undo_and_reopen() {
    let directory = std::path::PathBuf::from(super::super::client_capture::selected(
        "ERSATZRS_TELETEXT_MAIN_PRESENCE_DIR",
    ));
    Box::pin(run(Some(&directory))).await;
}

struct ExpectedMain {
    target_program: ersatzrs_domain::TeletextEnhancementProgram,
    target_pixels: TeletextRaster,
    name: String,
    target_file: Vec<u8>,
    units: Vec<Vec<[u8; 46]>>,
    magazine_units: Vec<[u8; 46]>,
}

impl ExpectedMain {
    async fn prepare(response: fn(&str, u16, u16) -> Value) -> Self {
        let (target_fixture, target) = setup_with_response(response).await;
        let target_program = program(target.accepted[0].service()).unwrap().clone();
        let target_pixels = pixels(target.accepted[0].service());
        let units = designs::units(target.accepted[0].service());
        let magazine_units = target.accepted[0].service().magazine_units().to_vec();
        let name = target.exported[0].sidecar().project.pages[0].name.clone();
        let target_file = target.exported[0]
            .files()
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes
            .clone();
        target_fixture.state.pool.close().await;

        Self {
            target_program,
            target_pixels,
            name,
            target_file,
            units,
            magazine_units,
        }
    }
}

async fn repeat_created(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    name: &str,
    target_program: &ersatzrs_domain::TeletextEnhancementProgram,
    target_pixels: &TeletextRaster,
) -> SavedEditorialTeletextDesign {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    let latest = receipt(fixture, &exported).await;
    let mut next = drcs_bitmap::from_receipt(&latest, 2);
    let file = next
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut count = 0;
    for record in &mut records {
        if let TeletextTtiRecord::Packet { row: 26, words, .. } = record {
            for word in words {
                if *word == 8 | (9 << 6) | (81 << 11) {
                    *word = 8 | (9 << 6) | (82 << 11);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 1);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    let changed = import(fixture, next).await;
    assert_eq!(changed.head().revision, 3);
    let changed_model = drcs_bitmap::restored(fixture, &changed).await;
    assert_ne!(
        program(changed_model.encoded().unwrap()),
        Some(target_program)
    );
    assert_ne!(pixels(changed_model.encoded().unwrap()), *target_pixels);
    changed
}

async fn undo_created(
    fixture: &Fixture,
    flow: &Flow,
    changed: &SavedEditorialTeletextDesign,
    name: &str,
    pending: Option<&TeletextPublicationRequest>,
) -> SavedEditorialTeletextDesign {
    // An edit to the newly authored Main is folded into its root-None attachment.
    // Fresh export and exact root undo must restore None, never Some(empty).
    let exported = drcs_bitmap::reexport(fixture, changed).await;
    let latest = receipt(fixture, &exported).await;
    let mut undo = drcs_bitmap::from_receipt(&latest, 3);
    let original = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    undo.files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes
        .clone_from(
            &original
                .files
                .iter()
                .find(|file| file.name == name)
                .unwrap()
                .bytes,
        );
    let undone = import(fixture, undo).await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        drcs_bitmap::stored(fixture, &undone).await.document,
        flow.originals[0].document
    );
    let restored = drcs_bitmap::restored(fixture, &undone).await;
    assert!(program(restored.encoded().unwrap()).is_none());
    assert_eq!(
        pixels(restored.encoded().unwrap()),
        pixels(flow.accepted[0].service())
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap()
            .as_ref(),
        pending
    );
    undone
}

fn completion(
    directory: &std::path::Path,
    concept: &SavedEditorialTeletextDesign,
    changed: &SavedEditorialTeletextDesign,
    undone: &SavedEditorialTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    local_object_glyphs::actual_editor::json_new(
        directory,
        "success.json",
        &json!({
            "probe_version":1, "first_main_concept":concept.head().revision,
            "repeat_scalar_concept":changed.head().revision, "root_undo_concept":undone.head().revision,
            "exact_original_bytes_restored":true, "undo_main_is_none":true,
            "explicit_published_generation":accepted.generation().id,
            "published_model_and_pixels_equal_target":true, "night_and_originals_unchanged":true,
            "concept_does_not_publish":true, "published_output_and_concept_survive_reopen":true,
            "full_engine_acceptance":false
        }),
    );
}

async fn run(editor: Option<&std::path::Path>) {
    let ExpectedMain {
        target_program,
        target_pixels,
        name,
        target_file,
        units,
        magazine_units,
    } = ExpectedMain::prepare(created).await;
    let (mut fixture, flow) = setup_with_response(editable_absent).await;
    assert!(program(flow.accepted[0].service()).is_none());
    assert_ne!(pixels(flow.accepted[0].service()), target_pixels);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let sidecar = request.sidecar.clone();
    replace_packets(&mut request, &name, &target_file);
    assert_eq!(request.sidecar, sidecar);
    let request = if let Some(directory) = editor {
        genuine_edit(directory, &flow, &request, &target_program).await
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
    let TeletextDesignMutation::Saved(concept) = result.unwrap() else {
        panic!("an authorized newly authored Main must save a concept");
    };
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    let service = restored.encoded().unwrap();
    assert_eq!(program(service), Some(&target_program));
    assert_eq!(pixels(service), target_pixels);
    assert_eq!(designs::units(service), units);
    assert_eq!(service.magazine_units(), magazine_units);
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(program(flow.accepted[0].service()).is_none());
    assert!(fixture.playout.try_recv().is_err());
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(program(accepted.service()), Some(&target_program));
    assert_eq!(pixels(accepted.service()), target_pixels);
    assert_eq!(designs::units(accepted.service()), units);
    assert_eq!(accepted.service().magazine_units(), magazine_units);
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    let pending = fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap();
    let changed = repeat_created(&fixture, &concept, &name, &target_program, &target_pixels).await;
    let undone = undo_created(&fixture, &flow, &changed, &name, pending.as_ref()).await;
    // Undo is another concept; it does not replace the explicitly published Q.
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
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
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    if let Some(directory) = editor {
        emitted_units(directory, &fixture, &flow, &accepted, &undone).await;
        completion(directory, &concept, &changed, &undone, &accepted);
    }
    fixture.state.pool.close().await;
}

fn encoded_units(service: &EncodedTeletextAuthoredService) -> Value {
    let display = service
        .pages()
        .iter()
        .find(|page| match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page) => {
                page.encoded().page().address() == channel_documents::address(124, 32)
            }
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => {
                page.page().address() == channel_documents::address(124, 32)
            }
            _ => false,
        })
        .unwrap();
    json!({
        "owner":{"page":124,"subcode":32},
        "display_units":display.data_units().iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "magazine_units":service.magazine_units().iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()
    })
}

fn empty(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = absent(theme, page, subcode);
    value["service"]["page_enhancements"][0]["local_program"] = json!({"commands":[]});
    value
}

fn created_at<const ROW: u8>(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = created(theme, page, subcode);
    value["service"]["page_enhancements"][0]["local_program"]["commands"][0]["position"]["row"] =
        json!(ROW);
    value
}

fn malformed_foreground(request: &mut NativeTeletextEditorialImportRequest, name: &str) {
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut count = 0;
    for record in &mut records {
        if let TeletextTtiRecord::Packet { row: 26, words, .. } = record {
            for word in words {
                if *word == 8 | (18 << 11) {
                    *word = 8 | (127 << 11); // Reserved foreground data cannot become a colour.
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 1);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
}

async fn refusals(fixture: &mut Fixture, flow: &Flow) {
    use ersatzrs_web::host::teletext_editorial_designs::{
        NativeTeletextEditorialImportError, TeletextEditorialEffectsFailure,
    };
    let retained = resource_bank_admin::Retained::capture(fixture).await;
    for (row, malformed) in [(2, false), (2, true), (1, false), (4, false)] {
        let target = ExpectedMain::prepare(match row {
            2 => created,
            1 => created_at::<1>,
            4 => created_at::<4>,
            _ => unreachable!(),
        })
        .await;
        let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
        replace_packets(&mut request, &target.name, &target.target_file);
        if malformed {
            malformed_foreground(&mut request, &target.name);
        }
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        if malformed {
            assert!(
                matches!(error, NativeTeletextEditorialImportError::Tti(_)),
                "{error:?}"
            );
        } else {
            let NativeTeletextEditorialImportError::Effects(error) = error else {
                panic!("protected new Main must reach actual display-effects refusal: {error:?}");
            };
            assert_eq!(
                error.reason,
                TeletextEditorialEffectsFailure::ProtectedDisplay
            );
            assert_eq!(error.page, Some(channel_documents::address(124, 32)));
            assert_eq!(error.position, Some((row, if row == 2 { 25 } else { 12 })));
        }
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(flow.saved[0].head().id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            1
        );
        assert_original_pointers(fixture, flow).await;
        retained.assert_unchanged(fixture, flow).await;
        assert!(fixture.playout.try_recv().is_err());
    }
}

#[tokio::test]
async fn absent_main_empty_program_exact_undo_and_invalid_or_protected_creation_are_atomic() {
    let target = ExpectedMain::prepare(empty).await;
    assert!(target.target_program.commands().is_empty());
    let records = parse_teletext_tti(&target.target_file, policy().tti).unwrap();
    let packets: Vec<_> = records
        .records()
        .iter()
        .filter_map(|record| match record {
            TeletextTtiRecord::Packet { row: 26, words, .. } => Some(words),
            _ => None,
        })
        .collect();
    assert_eq!(packets.len(), 1);
    assert!(packets[0].iter().all(|word| *word == 0x3fff));
    let (mut fixture, flow) = setup_with_response(absent).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    refusals(&mut fixture, &flow).await;
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let sidecar = request.sidecar.clone();
    replace_packets(&mut request, &target.name, &target.target_file);
    assert_eq!(request.sidecar, sidecar);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().revision, 2);
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        program(restored.encoded().unwrap()),
        Some(&target.target_program)
    );
    assert_eq!(designs::units(restored.encoded().unwrap()), target.units);
    assert_eq!(
        restored.encoded().unwrap().magazine_units(),
        target.magazine_units
    );
    assert_eq!(
        pixels(restored.encoded().unwrap()),
        pixels(flow.accepted[0].service())
    );
    assert_ne!(
        drcs_bitmap::stored(&fixture, &concept).await.document,
        flow.originals[0].document
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let mut undo = drcs_bitmap::from_receipt(&latest, 2);
    let original = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    undo.files
        .iter_mut()
        .find(|file| file.name == target.name)
        .unwrap()
        .bytes
        .clone_from(
            &original
                .files
                .iter()
                .find(|file| file.name == target.name)
                .unwrap()
                .bytes,
        );
    let undone = import(&fixture, undo).await;
    assert_eq!(undone.head().revision, 3);
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    fixture = fixture.reopen().await;
    let restored = drcs_bitmap::restored(&fixture, &undone).await;
    assert!(program(restored.encoded().unwrap()).is_none());
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(flow.accepted[0].service())
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

async fn emitted_units(
    directory: &std::path::Path,
    fixture: &Fixture,
    flow: &Flow,
    accepted: &AcceptedTeletextOutputCandidate,
    undone: &SavedEditorialTeletextDesign,
) {
    let restored = drcs_bitmap::restored(fixture, undone).await;
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(flow.accepted[0].service())
    );
    assert_eq!(
        restored.encoded().unwrap().magazine_units(),
        flow.accepted[0].service().magazine_units()
    );
    local_object_glyphs::actual_editor::json_new(
        directory,
        "emitted-units.json",
        &json!({
            "probe_version":1,
            "originalBefore":encoded_units(flow.accepted[0].service()),
            "newPublished":encoded_units(accepted.service()),
            "undoRestored":encoded_units(restored.encoded().unwrap())
        }),
    );
}

async fn genuine_edit(
    directory: &std::path::Path,
    flow: &Flow,
    expected: &NativeTeletextEditorialImportRequest,
    program: &ersatzrs_domain::TeletextEnhancementProgram,
) -> NativeTeletextEditorialImportRequest {
    use super::local_object_glyphs::actual_editor as qtm;
    qtm::inputs_for_layout(
        directory,
        "single-page-create-main",
        flow,
        [4, 12, 12],
        || {
            json!({
                "kind":"main_presence", "before":null, "after":program,
                "consumer_visual_columns":[12], "expected_words":[16682, 36872, 166_472, 14345, 16383]
            })
        },
    );
    let name = &flow.exported[0].sidecar().project.pages[0].name;
    let bytes = &expected
        .files
        .iter()
        .find(|file| &file.name == name)
        .unwrap()
        .bytes;
    live_client::write_new(&directory.join("expected-edited.tti"), bytes).unwrap();
    let _ = qtm::completion(directory).await;
    let bytes =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let request = qtm::actual_request(directory, flow, &bytes);
    assert_eq!(request.sidecar, expected.sidecar);
    for wanted in &expected.files {
        let actual = &request
            .files
            .iter()
            .find(|file| file.name == wanted.name)
            .unwrap()
            .bytes;
        if &wanted.name != name {
            assert_eq!(actual, &wanted.bytes);
            continue;
        }
        let a = parse_teletext_tti(actual, policy().tti).unwrap();
        let b = parse_teletext_tti(&wanted.bytes, policy().tti).unwrap();
        // QTM groups the unchanged X28 before X26/base rows and moves FL.
        // Compare those exact records separately; all other order stays strict.
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
    request
}

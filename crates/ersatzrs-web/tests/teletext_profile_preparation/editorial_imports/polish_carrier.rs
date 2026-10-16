//! Native receipt import of a literal national Base edit requiring a generated
//! carrier. The opt-in test uses genuine pinned QTM Save As bytes. Ordinary
//! delivery, headed admin and suitable-client proof remain separate.

use super::*;
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextMainComposition};
use serde_json::{json, Value};

const POLISH_5B_MASK: [u16; 10] = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0];

fn attributes(column: u8, wide: bool, high: bool) -> Value {
    json!({"kind":"at_column","column":column,"operation":{"kind":"attributes",
        "double_height":high,"double_width":wide,"boxed":false,"conceal":false,
        "invert":false,"underline_or_separated_mosaics":false}})
}

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = drcs_bank::explicit_page_response(theme, page, subcode);
    let owner = value["service"]["pages"][0]["address"].clone();
    let local = |id: u16| json!({"scope":"local","owner":owner,"object_id":id});
    let glyph = |column: u8, code: u8| json!({"kind":"at_column","column":column,"operation":{"kind":"g0","code":code}});
    let run = value["service"]["pages"][0]["rows"][0]["runs"][0].clone();
    value["service"]["pages"][0]["rows"] = json!([
        {"row":1,"runs":[{"column":7,"content":{"kind":"text","text":"A"},"style":run["style"]}]},
        {"row":2,"runs":[{"column":8,"content":{"kind":"text","text":"é"},"style":run["style"]}]},
        {"row":3,"runs":[{"column":1,"content":{"kind":"text","text":"B"},"style":run["style"]}]}
    ]);
    value["service"]["page_enhancements"][0]["presentation"]["primary"] = json!(0x08);
    value["service"]["page_enhancements"][0]["local_program"]["commands"] = json!([
        {"kind":"set_position","position":{"row":1,"column":7}},
        {"kind":"invoke_object","target":local(0xffff),"object_type":"active","offset":{"row":0,"column":1}},
        {"kind":"invoke_object","target":local(0xfffe),"object_type":"adaptive","offset":null}
    ]);
    value["service"]["objects"].as_array_mut().unwrap().extend([
        json!({"identity":local(0xffff),"object_type":"active","program":{"commands":[
            attributes(0,true,true),glyph(0,b'A'),attributes(2,false,false)]}}),
        json!({"identity":local(0xfffe),"object_type":"adaptive","program":{"commands":[
            attributes(0,true,false),attributes(2,false,false),glyph(2,b'X')]}}),
    ]);
    let bank = value["service"]["drcs"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|bank| bank["identity"]["subpage"] == 0)
        .unwrap();
    assert_eq!(bank["identity"]["page"], json!({"magazine":1,"page":0xc0}));
    bank["glyphs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"slot":5,"bitmap":POLISH_5B_MASK}));
    let regions = value["authoring"]["pages"][0]["regions"]
        .as_array_mut()
        .unwrap();
    regions.retain(|region| region["id"] != "free-glyph-8");
    regions.push(
        json!({"id":"free-national-base","kind":"free","area":"main",
        "row":1,"column":7,"width":2,"height":1}),
    );
    value
}

fn composition(service: &EncodedTeletextAuthoredService) -> &TeletextMainComposition {
    service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address().page.get() == 124 =>
            {
                Some(page.normalized().original())
            }
            _ => None,
        })
        .unwrap()
}

fn assert_base(composition: &TeletextMainComposition, expected_code: u8) {
    assert_eq!(composition.base_display().primary().get(), 0x08);
    let column = composition.display_geometry().left_width() + 7;
    let display = composition.display_resolution();
    let origin = display.cell(1, column).unwrap().origin().unwrap();
    let candidate_index = display.origins()[origin].candidate_index();
    let candidate = composition.glyph_layers().candidates()[candidate_index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), expected_code);
    let intent = composition.glyph_intent().records()[candidate_index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph})
        if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == expected_code
            && glyph.national_subset() == if expected_code == 0x5b {Some(TeletextNationalSubset::Polish)} else {None})
    );
    let owned: Vec<_> = (0..25)
        .flat_map(|row| (0..display.width()).map(move |col| (row, col)))
        .filter(|&(row, col)| {
            display
                .cell(row, col)
                .and_then(ersatzrs_domain::TeletextDisplayCell::origin)
                == Some(origin)
        })
        .collect();
    assert_eq!(
        owned,
        vec![(1, column), (1, column + 1)],
        "the canonical enlarged origin occupies precisely these two cells"
    );
}

#[allow(
    clippy::disallowed_methods,
    reason = "Manual native editor proof selects a disposable artifact directory; this is not application configuration"
)]
fn write_editor_inputs(flow: &Flow) {
    if let Some(directory) = std::env::var_os("ERSATZRS_TELETEXT_POLISH_EDITOR_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir(&directory).expect("editor input destination must be new");
        let exported = &flow.exported[0];
        for file in exported.files() {
            live_client::write_new(&directory.join(&file.name), &file.bytes).unwrap();
        }
        live_client::write_new(
            &directory.join("project.ersatzrs.json"),
            &write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap(),
        )
        .unwrap();
    }
}

fn native_polish_request(flow: &Flow) -> NativeTeletextEditorialImportRequest {
    let mut request = request(flow, 1, 7, 0x5b);
    request.name = "Synthetic Polish Base concept".into();
    let page_name = &flow.exported[0].sidecar().project.pages[0].name;
    for original in flow.exported[0].files() {
        let edited = request
            .files
            .iter()
            .find(|file| file.name == original.name)
            .unwrap();
        if &original.name == page_name {
            let mut expected = parse_teletext_tti(&original.bytes, policy().tti)
                .unwrap()
                .records()
                .to_vec();
            let cells = expected
                .iter_mut()
                .find_map(|record| match record {
                    TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
                    _ => None,
                })
                .unwrap();
            assert_eq!(cells[7], b'A');
            cells[7] = 0x5b;
            assert_eq!(
                parse_teletext_tti(&edited.bytes, policy().tti)
                    .unwrap()
                    .records(),
                expected
            );
        } else {
            assert_eq!(
                edited.bytes, original.bytes,
                "companions and the other display are unchanged"
            );
        }
    }
    request
}

#[tokio::test]
async fn raw_polish_base_edit_rebuilds_generated_carrier_then_previews_publishes_and_reopens() {
    run_polish_flow(None).await;
}

#[tokio::test]
#[ignore = "requires retained actual QTeletextMaker 1.0 Polish Base Save As artifacts"]
async fn actual_qtm_polish_base_edit_imports_previews_publishes_and_reopens() {
    let directory = client_capture::selected("ERSATZRS_QTM_POLISH_CALIBRATION_DIR");
    run_polish_flow(Some(std::path::Path::new(&directory))).await;
}

fn genuine_polish_request(
    flow: &Flow,
    directory: &std::path::Path,
) -> NativeTeletextEditorialImportRequest {
    let exported = &flow.exported[0];
    for file in exported.files() {
        let retained = drcs_bitmap::read_retained_editor_file(
            &directory.join("native-original-1").join(&file.name),
        );
        assert_eq!(
            retained, file.bytes,
            "every retained actual editor input must equal the fresh authoritative export"
        );
    }
    let name = &exported.sidecar().project.pages[0].name;
    assert_eq!(
        drcs_bitmap::hash(
            &exported
                .files()
                .iter()
                .find(|f| &f.name == name)
                .unwrap()
                .bytes
        ),
        "a1cb39545a3ed73c13ff7cafc14b999b4a677ef39d1604b434aabee3453e9e3a"
    );
    let edited = drcs_bitmap::read_retained_editor_file(&directory.join("editor-1/edited.tti"));
    assert_eq!(
        drcs_bitmap::hash(&edited),
        "962254bbb0427b5e9d1ff4d25f24d59fb5354203f61c3f1c4db2ae63b59f00fe"
    );
    // Use the independently saved file byte-for-byte, including the editor's
    // packet reordering and hexadecimal spelling. Native reconciliation owns
    // parsing and permissions; no test rewrite repairs the genuine output.
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = "Actual QTM Polish Base concept".into();
    request
        .files
        .iter_mut()
        .find(|f| &f.name == name)
        .unwrap()
        .bytes = edited;
    request
}

async fn run_polish_flow(actual: Option<&std::path::Path>) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    assert_base(composition(flow.accepted[0].service()), b'A');
    let request = if let Some(directory) = actual {
        genuine_polish_request(&flow, directory)
    } else {
        write_editor_inputs(&flow);
        native_polish_request(&flow)
    };
    // This intended success must pass native reconstruction and effect checks;
    // no authored X26 operand or resource bitmap was edited in the input TTI.
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        concept.head().id,
        2,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_base(composition(restored.encoded().unwrap()), 0x5b);
    assert_eq!(restored.selection(), flow.saved[0].candidate().selection());
    assert_eq!(
        restored.transmissions(),
        flow.saved[0].candidate().transmissions()
    );
    for (old, new) in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .zip(restored.encoded().unwrap().pages())
    {
        if matches!(old,EncodedTeletextAuthoredPage::CompiledDisplay(page) if page.page().address().page.get() == 124)
        {
            continue;
        }
        assert_eq!(old.data_units(), new.data_units());
    }
    let raster = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    assert_ne!(
        raster,
        external_editor_probe::preview_page(flow.accepted[0].service(), 124)
    );
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_eq!(
        external_editor_probe::preview_page(accepted.service(), 124),
        raster
    );
    assert_base(composition(accepted.service()), 0x5b);
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn protected_fragment_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    let region = value["authoring"]["pages"][0]["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "free-national-base")
        .unwrap();
    region["width"] = json!(1);
    value
}

async fn assert_no_concept_saved(fixture: &Fixture, flow: &Flow) {
    assert_original_pointers(fixture, flow).await;
    for original in &flow.saved {
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(original.head().id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            1
        );
    }
}

#[tokio::test]
async fn polish_base_edit_rejects_changed_enlarged_fragment_outside_free_region() {
    use ersatzrs_web::host::teletext_editorial_designs::{
        NativeTeletextEditorialImportError, TeletextEditorialEffectsFailure,
    };
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        protected_fragment_response,
        Fixture::new_persistent().await,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    let original = composition(flow.accepted[0].service());
    assert_base(original, b'A');
    let protected_column = original.display_geometry().left_width() + 8;
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request(&flow, 1, 7, 0x5b),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("the covered protected fragment must reject the edit: {error:?}");
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(124, 32)));
    assert_eq!(error.position, Some((1, protected_column)));
    assert_no_concept_saved(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_no_concept_saved(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn polish_base_edit_does_not_authorize_mutating_retained_authored_x26() {
    use ersatzrs_web::host::teletext_editorial_designs::{
        NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
    };
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    let page = flow.accepted[0]
        .service()
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address().page.get() == 124 =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    let retained: Vec<_> = page
        .normalized()
        .encoded()
        .command_positions()
        .iter()
        .filter(|position| position.original_local_command() == Some(0))
        .collect();
    assert_eq!(retained.len(), 1);
    let semantic_word = retained[0].semantic_word();
    let designation = u8::try_from(semantic_word / 13).unwrap();
    let slot = semantic_word % 13;
    let mut request = request(&flow, 1, 7, 0x5b);
    let name = &flow.exported[0].sidecar().project.pages[0].name;
    let file = request
        .files
        .iter_mut()
        .find(|file| &file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let words = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation: actual,
                words,
            } if *actual == designation => Some(words),
            _ => None,
        })
        .unwrap();
    // This word is the retained authored SetPosition command, not a generated
    // carrier. Change its valid column operand from seven to six.
    assert_eq!(words[slot] & 0x3f, 41);
    assert_eq!((words[slot] >> 6) & 0x1f, 4);
    assert_eq!(words[slot] >> 11, 7);
    words[slot] ^= 1 << 11;
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Tti(error) = error else {
        panic!("authored X26 command protection must reject the edit: {error:?}");
    };
    assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Packet);
    assert_eq!(error.page, 124);
    assert_eq!(error.subcode, 32);
    assert_eq!(error.row, Some(26));
    assert_eq!(error.designation, Some(designation));
    assert_eq!(error.column, Some(slot));
    assert_no_concept_saved(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_no_concept_saved(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

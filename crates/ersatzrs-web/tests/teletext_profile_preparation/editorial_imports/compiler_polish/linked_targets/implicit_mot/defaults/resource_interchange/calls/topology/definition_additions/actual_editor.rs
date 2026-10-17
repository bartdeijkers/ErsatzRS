//! Genuine editing of a newly added body; call retargeting is native setup only.
use super::*;
use crate::editorial_imports::local_object_glyphs::actual_editor as editor;
use std::path::Path;

const CASE: &str = "new-definition-foreground";

struct Yellow {
    definition: TeletextEnhancementObjectDefinition,
    records: Vec<TeletextTtiRecord>,
    rasters: Vec<TeletextRaster>,
    units: Vec<Vec<[u8; 46]>>,
}
fn yellow(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = called(theme, page, subcode);
    let definition = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 25)
        .unwrap();
    let foreground = &mut definition["program"]["commands"][2]["operation"];
    assert_eq!(foreground["kind"], "foreground");
    assert_eq!(foreground["colour"], 2);
    foreground["colour"] = json!(3);
    value
}
async fn qualify_yellow() -> Yellow {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        yellow,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let target = Yellow {
        definition: service
            .declared()
            .semantic()
            .objects()
            .definition(seed::target(25))
            .unwrap()
            .clone(),
        records: file_records(&flow.exported[0], passive_name(&flow.exported[0])),
        rasters: rasters(service),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    };
    assert_eq!(word(&target.records, 5), 0x1801);
    fixture.state.pool.close().await;
    target
}
fn assert_yellow(before: &[TeletextRaster], after: &[TeletextRaster]) {
    let mask = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];
    assert_eq!(before.len(), 3);
    assert_eq!(after.len(), 3);
    for (original, raster) in before.iter().zip(after) {
        assert_eq!((raster.width(), raster.height()), (672, 250));
        for y in 0..250 {
            for x in 0..672 {
                if (180..192).contains(&x) && (50..60).contains(&y) {
                    let rgb = if mask[y - 50] & (1 << (11 - (x - 180))) == 0 {
                        [0, 0, 0]
                    } else {
                        [15, 15, 0]
                    };
                    assert!(
                        matches!(raster.pixel(x,y),Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==rgb)
                    );
                } else {
                    assert_eq!(
                        raster.pixel(x, y),
                        original.pixel(x, y),
                        "unchanged siblings and all pixels outside new25's nested P"
                    );
                }
            }
        }
    }
}
fn assert_output(service: &EncodedTeletextAuthoredService, before: &Expected, target: &Yellow) {
    let pixels = rasters(service);
    assert_yellow(&before.rasters, &pixels);
    assert_eq!(pixels, target.rasters);
    assert_eq!(service.pages().len(), target.units.len());
    for (page, units) in service.pages().iter().zip(&target.units) {
        assert_eq!(
            page.data_units(),
            units,
            "every independently encoded native unit"
        );
    }
}
fn edit_metadata(service: &EncodedTeletextAuthoredService) -> Value {
    let position = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(page)
                if page.header().identity() == seed::target(25).resource().unwrap() =>
            {
                let mut positions = page.command_positions().iter().filter(|position| {
                    position.source().object == Some(seed::target(25))
                        && position.source().command == 2
                });
                let found = positions.next().unwrap();
                assert!(positions.next().is_none());
                Some((found.first_word(), found.semantic_word()))
            }
            _ => None,
        })
        .unwrap();
    json!({"kind":"resource_foreground","identity":seed::target(25),"source_command":2,
        "native_first_word":position.0,"native_semantic_word":position.1,
        "semantic_word":5,"column":1,"mode":0,"from":2,"to":3,
        "expected_word":0x1001,"replacement_word":0x1801,"consumers":[124,125,126]})
}
fn stage(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    service: &EncodedTeletextAuthoredService,
) -> Value {
    std::fs::create_dir(directory).expect("fresh new-definition editor case");
    let original = directory.join("original");
    std::fs::create_dir(&original).unwrap();
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    live_client::write_new(&original.join("project.ersatzrs.json"), &sidecar).unwrap();
    for file in exported.files() {
        live_client::write_new(&original.join(&file.name), &file.bytes).unwrap();
    }
    let name = passive_name(exported);
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let asset = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .unwrap();
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
    assert_eq!(receipt.metadata.revision, 3);
    assert_eq!(word(&file_records(exported, name), 5), 0x1001);
    let ready = json!({"probe_version":1,"case":CASE,"editor_profile":exported.sidecar().project.editor_profile,
        "receipt_id":receipt.metadata.id,"receipt_digest":receipt.metadata.digest,"design_revision":3,
        "input":format!("original/{name}"),"input_sha256":drcs_bitmap::hash(&file.bytes),
        "sidecar":"original/project.ersatzrs.json","sidecar_sha256":drcs_bitmap::hash(&sidecar),
        "files":exported.files().iter().map(|file|json!({"name":file.name,"bytes":file.bytes.len(),"sha256":drcs_bitmap::hash(&file.bytes)})).collect::<Vec<_>>(),
        "resource_scope":asset.scope,"page":{"page":256,"subcode":0},
        "editor_geometry":{"left_columns":0,"right_columns":0,"total_columns":40},
        "edit":edit_metadata(service),"save_as":"edited.tti","completion":"editor-saved.json","timeout_seconds":600,
        "editor":{"name":"QTeletextMaker","version":"1.0","source_commit":"cac2167bac933a6160cca45a45d5d4ee95355898","binary_sha256":"af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a"},
        "setup":"explicit Add2; authenticated manual call retarget3; no actual-QTM call-retarget claim",
        "publication":"unpublished import4 and reopen before separate explicit publication"});
    editor::json_new(directory, "ready.json", &ready);
    ready
}
fn actual_request(
    directory: &Path,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    saved: &[u8],
    target: &Yellow,
) -> NativeTeletextEditorialImportRequest {
    let original = directory.join("original");
    let sidecar = write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap();
    assert_eq!(
        live_client::read_bounded(
            &original.join("project.ersatzrs.json"),
            policy().project.max_bytes
        )
        .unwrap(),
        sidecar
    );
    for file in exported.files() {
        assert_eq!(
            live_client::read_bounded(&original.join(&file.name), policy().project.max_file_bytes)
                .unwrap(),
            file.bytes
        );
    }
    let mut actual = parse_teletext_tti(saved, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(actual.len(), target.records.len());
    for expected in &target.records {
        let index = actual
            .iter()
            .position(|record| record == expected)
            .expect("genuine save must match independently compiled entire new25 context");
        actual.swap_remove(index);
    }
    assert!(actual.is_empty());
    let name = passive_name(exported);
    let mut request = drcs_bitmap::from_receipt(receipt, 3);
    "Actual QTM newly added definition foreground".clone_into(&mut request.name);
    request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes = saved.to_vec();
    assert_eq!(request.sidecar, sidecar);
    for file in exported.files().iter().filter(|file| file.name != name) {
        assert_eq!(
            request
                .files
                .iter()
                .find(|after| after.name == file.name)
                .unwrap(),
            file
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
    before: &Expected,
    target: &Yellow,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 18);
    let [addition] = overlay.resource_object_definition_additions() else {
        panic!("one retained explicit addition")
    };
    assert_eq!(addition.definition(), &target.definition);
    assert!(
        overlay.resource_object_operands().is_empty(),
        "new foreground folds into complete addition"
    );
    let [call] = overlay.resource_object_program_overlays() else {
        panic!("one retained existing Active20 call retarget")
    };
    assert_eq!(call.identity(), seed::target(20));
    assert_eq!(call.expected(), program(flow.accepted[0].service()));
    assert_eq!(call.replacement(), &before.active);
    assert_eq!(
        call.retained_commands(),
        [Some(0), Some(1), Some(2), Some(3), Some(4)]
    );
    assert!(call.has_invocation_changes());
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    assert_output(restored.encoded().unwrap(), before, target);
}
#[tokio::test]
#[ignore = "requires pinned headed QTM foreground driver and a fresh explicit proof directory"]
async fn actual_qtm_new_definition_foreground_save_import_preview_publish_reopen() {
    let root = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_RESOURCE_EDITOR_DIR",
    ));
    assert!(root.is_absolute());
    std::fs::create_dir(&root).expect("fresh actual new-definition proof root");
    let directory = root.join(CASE);
    let dormant = Box::pin(qualify(State::Dormant)).await;
    let called = Box::pin(qualify(State::Called)).await;
    let target = Box::pin(qualify_yellow()).await;
    assert_yellow(&called.rasters, &target.rasters);
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        super::super::response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let added = add(&fixture, &flow.receipts[0], dormant.definition).await;
    let exported = drcs_bitmap::reexport(&fixture, &added).await;
    let latest = receipt(&fixture, &exported).await;
    // Existing authenticated native retarget is setup, never a claimed GUI edit.
    let selected = import(&fixture, upload(&exported, &latest, 2, &called)).await;
    assert_eq!(selected.head().revision, 3);
    Box::pin(super::assert_concept(&fixture, &flow, &selected, &called)).await;
    let exported = drcs_bitmap::reexport(&fixture, &selected).await;
    let latest = receipt(&fixture, &exported).await;
    let restored = drcs_bitmap::restored(&fixture, &selected).await;
    let ready = stage(&directory, &exported, &latest, restored.encoded().unwrap());
    let completion = editor::completion(&directory).await;
    let saved =
        live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes).unwrap();
    let concept = import(
        &fixture,
        actual_request(&directory, &exported, &latest, &saved, &target),
    )
    .await;
    assert_eq!(concept.head().revision, 4);
    Box::pin(assert_concept(&fixture, &flow, &concept, &called, &target)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &concept, &called, &target)).await;
    assert_original_pointers(&fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_output(accepted.service(), &called, &target);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &concept, &called, &target)).await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    editor::json_new(
        &directory,
        "success.json",
        &json!({"probe_version":1,"case":CASE,"editor":completion,
        "ready_sha256":drcs_bitmap::hash(&serde_json::to_vec_pretty(&ready).unwrap()),"saved_sha256":drcs_bitmap::hash(&saved),
        "schema_version":18,"setup_add_revision":2,"manual_call_retarget_revision":3,"actual_editor_import_revision":4,
        "new_definition_body_folded":true,"all_three_previews_and_native_units_exact":true,"literal_yellow_p_unchanged_siblings":true,
        "unpublished_reopen_then_explicit_publication_and_cold_reopen":true,"protected_artifacts_and_night_unchanged":true,
        "actual_qtm_call_retarget_claim":false,"ordinary_stream_or_client_acceptance_claim":false,"full_engine_acceptance_claim":false}),
    );
    fixture.state.pool.close().await;
}
